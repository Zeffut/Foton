//! The chest minecart.
//!
//! Vanilla parity: `MinecartChest` and `AbstractMinecartContainer`. A rolling
//! chest: it runs on rails like any cart, and a right-click opens it rather
//! than seating anybody, because there is nowhere to sit.
//!
//! Mineshafts generate these with a loot table attached. The table is rolled
//! the first time somebody opens the cart, matching
//! `ContainerEntity.unpackChestVehicleLootTable`.

use std::sync::{Arc, Weak};

use glam::DVec3;
use simdnbt::ToNbtTag as _;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::{NbtCompound, NbtList, NbtTag};
use steel_macros::entity_behavior;
use steel_protocol::packets::game::SoundSource;
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::item_stack::ItemStack;
use steel_registry::vanilla_game_events;
use steel_utils::axis::Axis;
use steel_utils::block_util::FoundRectangle;
use steel_utils::locks::{Shared, SyncMutex};
use steel_utils::types::InteractionHand;
use steel_utils::{DowncastType, DowncastTypeKey, Identifier};

use super::minecart_common::{self, MinecartLike, MinecartState};
use crate::behavior::InteractionResult;
use crate::block_entity::ContainerLoot;
use crate::entity::{
    Entity, EntityBase, EntityBaseLoad, EntityMovementEmission,
    reset_forward_direction_of_relative_portal_position,
};
use crate::inventory::container::{
    Container as _, SimpleContainer, calculate_redstone_signal_from_container,
};
use crate::inventory::lock::{ContainerRef, SharedContainer};
use crate::inventory::menu::kinds::chest;
use crate::inventory::slot_ranges::container_slot_item;
use crate::player::Player;
use crate::portal::portal_shape::PortalShape;
use crate::world::World;
use crate::world::game_event::GameEventContext;

/// Slots a chest minecart carries.
///
/// Vanilla parity: `MinecartChest.getContainerSize`.
const CHEST_MINECART_SLOTS: usize = 27;

/// Rows the menu shows.
const CHEST_MINECART_ROWS: usize = 3;

/// Vanilla parity: `AbstractMinecart.getDefaultGravity`.
const MINECART_GRAVITY: f64 = 0.04;

/// And the same in water.
const MINECART_GRAVITY_IN_WATER: f64 = 0.005;

/// How much speed a loaded cart keeps each tick before its cargo is counted.
///
/// Vanilla parity: the `0.98F` of `AbstractMinecartContainer.applyNaturalSlowdown`.
const LOADED_SLOWDOWN: f32 = 0.98;

/// How much each unfilled step of the container gives back.
///
/// Vanilla parity: the `emptiness * 0.001F` of the same method: an empty chest
/// cart rolls noticeably further than a full one.
const EMPTINESS_BONUS: f32 = 0.001;

/// The largest comparator reading a container can give.
const FULL_SIGNAL: i32 = 15;

/// Extra drag while under water.
const WATER_SLOWDOWN: f32 = 0.95;

/// A minecart with a chest in it.
#[entity_behavior(class = "MinecartChest")]
pub struct ChestMinecartEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    state: SyncMutex<ChestMinecartState>,
    minecart: SyncMutex<MinecartState>,
    container: Shared<SimpleContainer>,
    container_ref: ContainerRef,
    /// Vanilla parity: the `lootTable`/`lootTableSeed` pair of
    /// `AbstractMinecartContainer`.
    loot: Arc<ContainerLoot>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `ChestMinecartEntity`.
unsafe impl DowncastType for ChestMinecartEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/chest_minecart");
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ChestMinecartState {
    first_tick: bool,
}

impl ChestMinecartState {
    const fn new(first_tick: bool) -> Self {
        Self { first_tick }
    }
}

impl ChestMinecartEntity {
    /// Creates a new chest minecart entity.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::with_state(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
            ChestMinecartState::new(true),
        )
    }

    /// Creates a chest minecart entity from saved data.
    #[must_use]
    pub fn from_saved(entity_type: EntityTypeRef, load: EntityBaseLoad) -> Self {
        Self::with_state(
            EntityBase::from_load(load, entity_type.dimensions),
            entity_type,
            ChestMinecartState::new(false),
        )
    }

    fn with_state(base: EntityBase, entity_type: EntityTypeRef, state: ChestMinecartState) -> Self {
        let container: Shared<SimpleContainer> =
            Arc::new(SyncMutex::new(SimpleContainer::new(CHEST_MINECART_SLOTS)));
        let shared: SharedContainer = container.clone();
        Self {
            base,
            entity_type,
            state: SyncMutex::new(state),
            minecart: SyncMutex::new(MinecartState::default()),
            container_ref: ContainerRef::from(shared),
            container,
            loot: Arc::new(ContainerLoot::new()),
        }
    }

    /// Rolls a still-packed loot table into the cart.
    ///
    /// Vanilla parity: `ContainerEntity.unpackChestVehicleLootTable`, whose
    /// `ORIGIN` is the cart's own position because a cart moves.
    fn unpack_loot_table(&self, player: Option<&Player>) {
        let Some(world) = self.level() else {
            return;
        };
        let container: SharedContainer = self.container.clone();
        self.loot
            .unpack_at(&world, self.position(), &container, player);
    }

    /// Opens the chest for `player`.
    ///
    /// Vanilla parity: `MinecartChest.interact`, which goes straight to the
    /// container -- there is no seat to compete with, so no sneak is needed.
    fn open_chest(&self, player: &Player) -> InteractionResult {
        // Vanilla parity: `AbstractMinecartContainer.createMenu` unpacks
        // with the opening player, whose luck the roll uses.
        self.unpack_loot_table(Some(player));
        let inventory = player.inventory.clone();
        let container = self.container_ref.clone();
        player.open_menu(self.name(), move |context| {
            chest(
                inventory,
                context.container_id,
                container,
                CHEST_MINECART_ROWS,
            )
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

    /// Sets the deferred loot table used when the container is first opened.
    ///
    /// Vanilla parity: `AbstractMinecartContainer.setLootTable`.
    pub fn set_loot_table(&self, loot_table: Identifier, seed: i64) {
        self.loot.set_loot_table(loot_table, seed);
    }

    const fn nbt_bool(value: bool) -> i8 {
        if value { 1 } else { 0 }
    }
}

impl Entity for ChestMinecartEntity {
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

    fn is_pickable(&self) -> bool {
        !self.is_removed()
    }

    fn is_pushable(&self) -> bool {
        true
    }

    fn blocks_building(&self) -> bool {
        true
    }

    fn tick(&self) {
        minecart_common::tick_minecart(self);
    }

    /// Vanilla parity: `AbstractMinecart.getDefaultGravity`.
    fn get_default_gravity(&self) -> f64 {
        if self.is_in_water() {
            MINECART_GRAVITY_IN_WATER
        } else {
            MINECART_GRAVITY
        }
    }

    fn movement_emission(&self) -> EntityMovementEmission {
        EntityMovementEmission::Events
    }

    fn sound_source(&self) -> SoundSource {
        SoundSource::Neutral
    }

    /// Vanilla parity: `MinecartChest.interact`.
    fn interact(
        &self,
        player: &Player,
        _hand: InteractionHand,
        _location: DVec3,
    ) -> InteractionResult {
        self.open_chest(player)
    }

    fn dimension_changing_delay(&self) -> i32 {
        10
    }

    fn get_relative_portal_position(&self, axis: Axis, portal_area: FoundRectangle) -> DVec3 {
        reset_forward_direction_of_relative_portal_position(PortalShape::get_relative_position(
            portal_area,
            axis,
            self.position(),
            self.dimensions_for_pose(self.pose()),
        ))
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        nbt.insert(
            "FlippedRotation",
            Self::nbt_bool(self.minecart.lock().flipped),
        );

        nbt.insert("HasTicked", Self::nbt_bool(self.state.lock().first_tick));

        // Vanilla parity: `ContainerEntity.addChestVehicleSaveData`, which
        // writes the loot table *instead of* the items when one is packed.
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
        {
            let mut state = self.state.lock();
            if let Some(first_tick) = nbt.byte("HasTicked") {
                state.first_tick = first_tick != 0;
            }
        }

        if let Some(flipped) = nbt.byte("FlippedRotation") {
            self.minecart.lock().flipped = flipped != 0;
        }

        // Vanilla parity: `ContainerEntity.readChestVehicleSaveData` clears
        // the slots, then reads the items only when no table is packed.
        let packed = self.loot.try_load_loot_table(&nbt);
        let mut container = self.container.lock();
        container.items_mut().fill(ItemStack::empty());
        if packed {
            return;
        }
        let Some(items_list) = nbt.list("Items") else {
            return;
        };
        let Some(compounds) = items_list.compounds() else {
            return;
        };
        for compound in compounds {
            let Some(slot) = compound.byte("Slot") else {
                continue;
            };
            let slot = slot as usize;
            if slot < CHEST_MINECART_SLOTS
                && let Some(item) = ItemStack::from_borrowed_compound(&compound)
            {
                container.items_mut()[slot] = item;
            }
        }
    }
}

impl MinecartLike for ChestMinecartEntity {
    fn minecart_state(&self) -> &SyncMutex<MinecartState> {
        &self.minecart
    }

    /// Vanilla parity: `AbstractMinecartContainer.applyNaturalSlowdown`.
    ///
    /// A loaded cart rolls less far than an empty one, measured by what a
    /// comparator would read off the container. It is a small effect -- an
    /// empty cart keeps 0.995 of its speed and a full one 0.98 -- but over a
    /// long line it is the difference between arriving and stopping short.
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

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use simdnbt::borrow::read_compound as read_borrowed_compound;
    use steel_registry::{init_vanilla_registry, vanilla_entities, vanilla_items};

    use super::*;

    const MINESHAFT: &str = "minecraft:chests/abandoned_mineshaft";

    fn test_minecart() -> ChestMinecartEntity {
        init_vanilla_registry();
        ChestMinecartEntity::new(
            &vanilla_entities::CHEST_MINECART,
            1,
            DVec3::new(1.5, 2.5, 3.5),
            Weak::new(),
        )
    }

    fn load_from_owned_nbt(minecart: &ChestMinecartEntity, nbt: &NbtCompound) {
        let mut bytes = Vec::new();
        nbt.write(&mut bytes);
        let base = read_borrowed_compound(&mut Cursor::new(bytes.as_slice()))
            .expect("test nbt should reborrow");
        minecart.load_additional((&base).into());
    }

    #[test]
    fn chest_minecart_saves_structure_loot_table_state() {
        let minecart = test_minecart();
        minecart.set_loot_table(
            Identifier::new_static("minecraft", "chests/abandoned_mineshaft"),
            42,
        );

        let mut nbt = NbtCompound::new();
        minecart.save_additional(&mut nbt);

        assert_eq!(
            nbt.string("LootTable").map(ToString::to_string),
            Some(MINESHAFT.to_owned())
        );
        assert_eq!(nbt.long("LootTableSeed"), Some(42));
        assert_eq!(nbt.byte("HasTicked"), Some(1));
        assert_eq!(nbt.byte("FlippedRotation"), Some(0));
        assert!(
            nbt.list("Items").is_none(),
            "vanilla writes the table instead of the items, never both"
        );
    }

    /// A mineshaft cart comes back off disk still packed, and the stale `Items`
    /// list a hand-edited save might carry does not become free loot on top.
    #[test]
    fn a_packed_chest_minecart_reloads_its_table_and_ignores_stale_items() {
        let minecart = test_minecart();
        let mut nbt = NbtCompound::new();
        nbt.insert("LootTable", MINESHAFT);
        nbt.insert("LootTableSeed", 1234_i64);
        let mut stale = NbtCompound::new();
        if let NbtTag::Compound(mut item) = ItemStack::new(&vanilla_items::DIAMOND).to_nbt_tag() {
            item.insert("Slot", 0_i8);
            stale = item;
        }
        nbt.insert("Items", NbtList::Compound(vec![stale]));

        load_from_owned_nbt(&minecart, &nbt);

        assert!(minecart.loot.is_packed());
        assert!(
            minecart
                .container
                .lock()
                .items()
                .iter()
                .all(ItemStack::is_empty),
            "a packed cart must not also carry the items it saved before"
        );

        let mut saved = NbtCompound::new();
        minecart.save_additional(&mut saved);
        assert_eq!(saved.long("LootTableSeed"), Some(1234));
    }

    #[test]
    fn chest_minecart_is_pickable_and_pushable_like_vanilla() {
        let minecart = ChestMinecartEntity::new(
            &vanilla_entities::CHEST_MINECART,
            1,
            DVec3::new(1.5, 2.5, 3.5),
            Weak::new(),
        );

        assert!(minecart.is_pickable());
        assert!(minecart.is_pushable());
        assert!(minecart.blocks_building());
    }

    #[test]
    fn chest_minecart_relative_portal_position_resets_forward_offset() {
        let minecart = ChestMinecartEntity::new(
            &vanilla_entities::CHEST_MINECART,
            1,
            DVec3::new(12.0, 66.0, 20.75),
            Weak::new(),
        );
        let portal_area = FoundRectangle {
            min_corner: steel_utils::BlockPos::new(10, 64, 20),
            axis1_size: 4,
            axis2_size: 5,
        };

        assert!(
            minecart
                .get_relative_portal_position(Axis::X, portal_area)
                .z
                .abs()
                < f64::EPSILON
        );
    }
}
