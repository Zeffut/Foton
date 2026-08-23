//! The chest minecart.
//!
//! Vanilla parity: `MinecartChest` and `AbstractMinecartContainer`. A rolling
//! chest: it runs on rails like any cart, and a right-click opens it rather
//! than seating anybody, because there is nowhere to sit.
//!
//! Mineshafts already generate these with a loot table attached, and until now
//! there was no way to open one. Steel has no loot system, so a generated cart
//! opens empty rather than full -- but it opens, and it keeps what is put in
//! it.

use std::str::FromStr;
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
use crate::entity::{
    Entity, EntityBase, EntityBaseLoad, EntityMovementEmission,
    reset_forward_direction_of_relative_portal_position,
};
use crate::inventory::container::{
    Container as _, SimpleContainer, calculate_redstone_signal_from_container,
};
use crate::inventory::lock::{ContainerRef, SharedContainer};
use crate::inventory::menu::kinds::chest;
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
}

// SAFETY: This key is owned by Steel and uniquely identifies `ChestMinecartEntity`.
unsafe impl DowncastType for ChestMinecartEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/chest_minecart");
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ChestMinecartState {
    first_tick: bool,
    loot_table: Option<Identifier>,
    loot_table_seed: i64,
}

impl ChestMinecartState {
    const fn new(first_tick: bool) -> Self {
        Self {
            first_tick,
            loot_table: None,
            loot_table_seed: 0,
        }
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
        }
    }

    /// Opens the chest for `player`.
    ///
    /// Vanilla parity: `MinecartChest.interact`, which goes straight to the
    /// container -- there is no seat to compete with, so no sneak is needed.
    fn open_chest(&self, player: &Player) -> InteractionResult {
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
    pub fn set_loot_table(&self, loot_table: Identifier, seed: i64) {
        let mut state = self.state.lock();
        state.loot_table = Some(loot_table);
        state.loot_table_seed = seed;
    }

    const fn nbt_bool(value: bool) -> i8 {
        if value { 1 } else { 0 }
    }
}

impl Entity for ChestMinecartEntity {
    fn base(&self) -> &EntityBase {
        &self.base
    }

    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
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

        // Vanilla parity: the `Items` tag of `ContainerEntity`.
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
        drop(container);
        nbt.insert("Items", NbtList::Compound(items));

        let state = self.state.lock();
        nbt.insert("HasTicked", Self::nbt_bool(state.first_tick));

        if let Some(loot_table) = state.loot_table.as_ref() {
            nbt.insert("LootTable", loot_table.to_string());
            if state.loot_table_seed != 0 {
                nbt.insert("LootTableSeed", NbtTag::Long(state.loot_table_seed));
            }
        }
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        let loot_table = nbt
            .string("LootTable")
            .and_then(|value| Identifier::from_str(&value.to_string()).ok());
        let mut state = self.state.lock();
        if let Some(first_tick) = nbt.byte("HasTicked") {
            state.first_tick = first_tick != 0;
        }
        state.loot_table = loot_table;
        state.loot_table_seed = nbt.long("LootTableSeed").unwrap_or(0);
        drop(state);

        if let Some(flipped) = nbt.byte("FlippedRotation") {
            self.minecart.lock().flipped = flipped != 0;
        }

        let mut container = self.container.lock();
        container.items_mut().fill(ItemStack::empty());
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
        if self.state.lock().loot_table.is_none() {
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
    use super::*;
    use steel_registry::vanilla_entities;

    #[test]
    fn chest_minecart_saves_structure_loot_table_state() {
        let minecart = ChestMinecartEntity::new(
            &vanilla_entities::CHEST_MINECART,
            1,
            DVec3::new(1.5, 2.5, 3.5),
            Weak::new(),
        );
        minecart.set_loot_table(
            Identifier::new_static("minecraft", "chests/abandoned_mineshaft"),
            42,
        );

        let mut nbt = NbtCompound::new();
        minecart.save_additional(&mut nbt);

        assert_eq!(
            nbt.string("LootTable").map(ToString::to_string),
            Some("minecraft:chests/abandoned_mineshaft".to_owned())
        );
        assert_eq!(nbt.long("LootTableSeed"), Some(42));
        assert_eq!(nbt.byte("HasTicked"), Some(1));
        assert_eq!(nbt.byte("FlippedRotation"), Some(0));
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
