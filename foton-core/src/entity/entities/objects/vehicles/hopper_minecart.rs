//! The hopper minecart.
//!
//! Vanilla parity: `MinecartHopper`. A hopper on wheels: five slots that suck
//! up whatever they roll under or over, and an activator rail that switches the
//! sucking off rather than on -- a powered rail disables it, which is how a
//! collection line is gated.
//!
//! It only takes in. Pushing out into containers below is the hopper *block*'s
//! job, and vanilla's cart does not do it either.
//!
//! Everything about rolling is [`super::minecart_common`]; the sucking is the
//! hopper block entity's, reached through
//! [`crate::block_entity::entities::suck_into_at`].

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Weak};

use foton_macros::entity_behavior;
use foton_protocol::packets::game::SoundSource;
use foton_registry::entity_type::EntityTypeRef;
use foton_registry::item_stack::ItemStack;
use foton_registry::vanilla_game_events;
use foton_utils::locks::{Shared, SyncMutex};
use foton_utils::types::InteractionHand;
use foton_utils::{BlockPos, DowncastType, DowncastTypeKey};
use glam::DVec3;
use simdnbt::ToNbtTag as _;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::{NbtCompound, NbtList, NbtTag};

use super::minecart_common::{self, MinecartLike, MinecartState};
use crate::behavior::InteractionResult;
use crate::block_entity::ContainerLoot;
use crate::block_entity::entities::suck_into_at;
use crate::entity::{Entity, EntityBase, EntityBaseLoad, EntityMovementEmission};
use crate::inventory::container::{
    Container as _, SimpleContainer, calculate_redstone_signal_from_container,
};
use crate::inventory::lock::{ContainerRef, SharedContainer};
use crate::inventory::menu::kinds::hopper;
use crate::inventory::slot_ranges::container_slot_item;
use crate::player::Player;
use crate::world::World;
use crate::world::game_event::GameEventContext;

/// Slots a hopper minecart carries.
///
/// Vanilla parity: `MinecartHopper.getContainerSize`.
const HOPPER_MINECART_SLOTS: usize = 5;

/// Vanilla parity: `AbstractMinecart.getDefaultGravity`.
const MINECART_GRAVITY: f64 = 0.04;

/// And the same in water.
const MINECART_GRAVITY_IN_WATER: f64 = 0.005;

/// How high above the cart's feet its hopper mouth sits.
///
/// Vanilla parity: the `getY() + 0.5` of `MinecartHopper.getLevelY`.
const MOUTH_HEIGHT: f64 = 0.5;

/// How much speed a loaded cart keeps each tick before its cargo is counted.
///
/// Vanilla parity: the `0.98F` of `AbstractMinecartContainer.applyNaturalSlowdown`.
const LOADED_SLOWDOWN: f32 = 0.98;

/// How much each unfilled step of the container gives back.
const EMPTINESS_BONUS: f32 = 0.001;

/// A full comparator reading.
const FULL_SIGNAL: i32 = 15;

/// Extra drag while under water.
const WATER_SLOWDOWN: f32 = 0.95;

/// A minecart with a hopper in it.
#[entity_behavior(class = "MinecartHopper")]
pub struct HopperMinecartEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    minecart: SyncMutex<MinecartState>,
    container: Shared<SimpleContainer>,
    container_ref: ContainerRef,
    /// Vanilla parity: the `lootTable`/`lootTableSeed` pair of
    /// `AbstractMinecartContainer`.
    loot: Arc<ContainerLoot>,
    /// Whether it is allowed to suck. An activator rail switches this *off*.
    enabled: AtomicBool,
    /// Guards against sucking twice in one tick, once from the tick itself and
    /// once from a step along the track.
    consumed_this_tick: AtomicBool,
}

// SAFETY: This key is owned by Foton and uniquely identifies `HopperMinecartEntity`.
unsafe impl DowncastType for HopperMinecartEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:entity/hopper_minecart");
}

impl HopperMinecartEntity {
    /// Creates one at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Creates one from saved base data.
    #[must_use]
    pub fn from_saved(entity_type: EntityTypeRef, load: EntityBaseLoad) -> Self {
        Self::new_with_base(
            EntityBase::from_load(load, entity_type.dimensions),
            entity_type,
        )
    }

    fn new_with_base(base: EntityBase, entity_type: EntityTypeRef) -> Self {
        let container: Shared<SimpleContainer> =
            Arc::new(SyncMutex::new(SimpleContainer::new(HOPPER_MINECART_SLOTS)));
        let shared: SharedContainer = container.clone();
        Self {
            base,
            entity_type,
            minecart: SyncMutex::new(MinecartState::default()),
            container_ref: ContainerRef::from(shared),
            container,
            loot: Arc::new(ContainerLoot::new()),
            enabled: AtomicBool::new(true),
            consumed_this_tick: AtomicBool::new(false),
        }
    }

    /// Rolls a still-packed loot table into the cart.
    ///
    /// Vanilla parity: `ContainerEntity.unpackChestVehicleLootTable`.
    fn unpack_loot_table(&self, player: Option<&Player>) {
        let Some(world) = self.level() else {
            return;
        };
        let container: SharedContainer = self.container.clone();
        self.loot
            .unpack_at(&world, self.position(), &container, player);
    }

    /// Returns whether the cart is allowed to suck items in.
    #[must_use]
    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    /// The point the hopper measures its reach from.
    ///
    /// Vanilla parity: `getLevelX` / `getLevelY` / `getLevelZ`, which put the
    /// mouth half a block above the cart's feet.
    fn mouth(&self) -> DVec3 {
        let position = self.position();
        DVec3::new(position.x, position.y + MOUTH_HEIGHT, position.z)
    }

    /// Takes one item in, if there is one and the cart is switched on.
    ///
    /// Vanilla parity: `MinecartHopper.tryConsumeItems`. It runs both from the
    /// tick and from each step along the track, and the flag is what stops a
    /// fast cart swallowing several items per tick.
    fn try_consume_items(&self) {
        if !self.is_alive() || !self.is_enabled() || self.consumed_this_tick.load(Ordering::Relaxed)
        {
            return;
        }
        let Some(world) = self.level() else {
            return;
        };
        if suck_into_at(&world, self.mouth(), &self.container_ref) {
            self.consumed_this_tick.store(true, Ordering::Relaxed);
        }
    }

    /// Opens the hopper for `player`.
    fn open_hopper(&self, player: &Player) -> InteractionResult {
        // Vanilla parity: `AbstractMinecartContainer.createMenu`.
        self.unpack_loot_table(Some(player));
        let inventory = player.inventory.clone();
        let container = self.container_ref.clone();
        player.open_menu(self.name(), move |context| {
            hopper(inventory, context.container_id, container)
        });

        if let Some(world) = self.level() {
            world.game_event_at(
                &vanilla_game_events::CONTAINER_OPEN,
                self.position(),
                &GameEventContext::new(Some(player as &dyn Entity), None),
            );
        }

        InteractionResult::Success
    }
}

impl Entity for HopperMinecartEntity {
    fn is_minecart(&self) -> bool {
        true
    }

    fn base(&self) -> &EntityBase {
        &self.base
    }

    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
    }

    /// Vanilla parity: `ContainerEntity.getChestVehicleSlot`, which reads
    /// through `getChestVehicleItem` and so rolls a still-packed loot table
    /// before answering. A chest vehicle that came out of a structure has to
    /// report what a player opening it would find, not the empty container it
    /// is until then.
    fn slot_item(&self, slot: i32) -> Option<ItemStack> {
        self.unpack_loot_table(None);
        container_slot_item(&*self.container.lock(), slot)
    }

    /// Vanilla parity: `MinecartHopper.tick`.
    fn tick(&self) {
        self.consumed_this_tick.store(false, Ordering::Relaxed);
        minecart_common::tick_minecart(self);
        self.try_consume_items();
    }

    fn get_default_gravity(&self) -> f64 {
        if self.is_in_water() {
            MINECART_GRAVITY_IN_WATER
        } else {
            MINECART_GRAVITY
        }
    }

    fn blocks_building(&self) -> bool {
        true
    }

    fn is_pushable(&self) -> bool {
        true
    }

    fn is_pickable(&self) -> bool {
        !self.is_removed()
    }

    fn movement_emission(&self) -> EntityMovementEmission {
        EntityMovementEmission::Events
    }

    fn sound_source(&self) -> SoundSource {
        SoundSource::Neutral
    }

    /// Vanilla parity: `AbstractMinecartContainer.interact`, which opens the
    /// container -- there is no seat to compete with, so no sneak is needed.
    fn interact(
        &self,
        player: &Player,
        _hand: InteractionHand,
        _location: DVec3,
    ) -> InteractionResult {
        self.open_hopper(player)
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        nbt.insert("Enabled", i8::from(self.is_enabled()));

        // Vanilla parity: `ContainerEntity.addChestVehicleSaveData`.
        if self.loot.try_save_loot_table(nbt) {
            return;
        }
        let container = self.container.lock();
        let mut items: Vec<NbtCompound> = Vec::new();
        for (slot, item) in container.items().iter().enumerate() {
            if !item.is_empty()
                && let NbtTag::Compound(mut item_nbt) = item.clone().to_nbt_tag()
            {
                item_nbt.insert("Slot", slot as i8);
                items.push(item_nbt);
            }
        }
        nbt.insert("Items", NbtList::Compound(items));
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.enabled.store(
            nbt.byte("Enabled").is_none_or(|value| value != 0),
            Ordering::Relaxed,
        );

        // Vanilla parity: `ContainerEntity.readChestVehicleSaveData`.
        let packed = self.loot.try_load_loot_table(&nbt);
        let mut container = self.container.lock();
        for slot in 0..HOPPER_MINECART_SLOTS {
            container.set_item(slot, ItemStack::empty());
        }
        if !packed
            && let Some(items) = nbt.list("Items")
            && let Some(compounds) = items.compounds()
        {
            for compound in compounds {
                if let Some(slot) = compound.byte("Slot") {
                    let slot = slot as usize;
                    if slot < HOPPER_MINECART_SLOTS
                        && let Some(item) = ItemStack::from_borrowed_compound(&compound)
                    {
                        container.set_item(slot, item);
                    }
                }
            }
        }
    }
}

impl MinecartLike for HopperMinecartEntity {
    fn minecart_state(&self) -> &SyncMutex<MinecartState> {
        &self.minecart
    }

    /// Vanilla parity: `MinecartHopper.activateMinecart`, which is the one cart
    /// where a powered rail turns the cart *off*.
    fn activate_minecart(&self, _world: &Arc<World>, _pos: BlockPos, powered: bool) {
        self.enabled.store(!powered, Ordering::Relaxed);
    }

    /// Vanilla parity: `AbstractMinecartContainer.applyNaturalSlowdown`, which
    /// is the same rule the chest cart follows: a fuller cart rolls less far.
    fn apply_natural_slowdown(&self, movement: DVec3) -> DVec3 {
        let mut keep = LOADED_SLOWDOWN;
        // Vanilla skips the cargo bonus while a loot table is still packed,
        // because the cart does not yet know what it is carrying.
        if !self.loot.is_packed() {
            let filled = calculate_redstone_signal_from_container(&*self.container.lock());
            keep += (FULL_SIGNAL - filled) as f32 * EMPTINESS_BONUS;
        }
        if self.is_in_water() {
            keep *= WATER_SLOWDOWN;
        }
        let keep = f64::from(keep);
        DVec3::new(movement.x * keep, 0.0, movement.z * keep)
    }
}
