//! Boats and rafts that carry a chest.
//!
//! Vanilla parity: `ChestBoat` and `ChestRaft`, which are `AbstractChestBoat`
//! -- a boat with twenty-seven slots and one seat instead of two. Everything
//! about floating is [`super::boat_common`], the same code an ordinary boat
//! runs; what is here is the chest and the rule for which of the two a
//! right-click means.

use std::sync::{Arc, Weak};

use glam::DVec3;
use simdnbt::ToNbtTag as _;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::{NbtCompound, NbtList, NbtTag};
use steel_macros::entity_behavior;
use steel_protocol::packets::game::SoundSource;
use steel_registry::entity_type::{EntityDimensions, EntityTypeRef};
use steel_registry::item_stack::ItemStack;
use steel_registry::vanilla_entity_data::AbstractBoatEntityData;
use steel_registry::vanilla_game_events;
use steel_utils::locks::{Shared, SyncMutex};
use steel_utils::types::InteractionHand;
use steel_utils::{DowncastType, DowncastTypeKey};

use super::boat_common::{
    self, BOAT_GRAVITY, BOAT_RIDE_HEIGHT, BoatLike, BoatState, RAFT_RIDE_HEIGHT,
};
use crate::behavior::InteractionResult;
use crate::block_entity::ContainerLoot;
use crate::entity::{Entity, EntityBase, EntityBaseLoad, EntityMovementEmission, EntitySyncedData};
use crate::inventory::container::{Container as _, SimpleContainer};
use crate::inventory::lock::{ContainerRef, SharedContainer};
use crate::inventory::menu::kinds::chest;
use crate::inventory::slot_ranges::container_slot_item;
use crate::player::Player;
use crate::world::World;
use crate::world::game_event::GameEventContext;

/// Slots a chest boat carries.
///
/// Vanilla parity: `AbstractChestBoat.CONTAINER_SIZE`.
const CHEST_BOAT_SLOTS: usize = 27;

/// Rows the menu shows.
const CHEST_BOAT_ROWS: usize = 3;

/// How many riders a chest boat carries.
///
/// Vanilla parity: `AbstractChestBoat.getMaxPassengers`. The chest takes the
/// second seat, which is the only reason this differs from a plain boat.
const CHEST_BOAT_PASSENGERS: usize = 1;

/// Declares one chest boat shape.
///
/// The struct is written out rather than produced by the macro so the entity
/// codegen can see it: a behavior it cannot see is silently never registered.
macro_rules! chest_boat_body {
    ($name:ident, $ride_height:expr, $key:literal) => {
        // SAFETY: This key is owned by Steel and uniquely identifies the type.
        unsafe impl DowncastType for $name {
            const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new($key);
        }

        impl $name {
            /// Creates one at runtime.
            #[must_use]
            pub fn new(
                entity_type: EntityTypeRef,
                id: i32,
                position: DVec3,
                world: Weak<World>,
            ) -> Self {
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
                    Arc::new(SyncMutex::new(SimpleContainer::new(CHEST_BOAT_SLOTS)));
                let shared: SharedContainer = container.clone();
                Self {
                    base,
                    entity_type,
                    entity_data: SyncMutex::new(AbstractBoatEntityData::new()),
                    boat: SyncMutex::new(BoatState::default()),
                    container_ref: ContainerRef::from(shared),
                    container,
                    loot: Arc::new(ContainerLoot::new()),
                }
            }

            /// Rolls a still-packed loot table into the boat.
            ///
            /// Vanilla parity: `AbstractChestBoat.unpackLootTable`, which is
            /// `ContainerEntity.unpackChestVehicleLootTable`.
            fn unpack_loot_table(&self, player: Option<&Player>) {
                let Some(world) = self.level() else {
                    return;
                };
                let container: SharedContainer = self.container.clone();
                self.loot
                    .unpack_at(&world, self.position(), &container, player);
            }
        }

        impl Entity for $name {
            fn base(&self) -> &EntityBase {
                &self.base
            }

            fn entity_type(&self) -> EntityTypeRef {
                self.entity_type
            }

            /// Vanilla parity: `ContainerEntity.getChestVehicleSlot`, which reads
            /// through `getChestVehicleItem` and so rolls a still-packed loot table
            /// before answering. A chest vehicle that came out of a structure
            /// has to report what a player opening it would find, not the
            /// empty container it is until then.
            fn slot_item(&self, slot: i32) -> Option<ItemStack> {
                self.unpack_loot_table(None);
                container_slot_item(&*self.container.lock(), slot)
            }

            fn synced_data(&self) -> Option<&dyn EntitySyncedData> {
                Some(&self.entity_data)
            }

            fn tick(&self) {
                boat_common::tick_boat(self);
            }

            fn get_default_gravity(&self) -> f64 {
                BOAT_GRAVITY
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

            /// Vanilla parity: `AbstractBoat.canAddPassenger` with the chest
            /// boat's single seat.
            fn can_add_passenger(&self, _passenger: &dyn Entity) -> bool {
                self.passengers().len() < CHEST_BOAT_PASSENGERS && !self.is_eye_in_water()
            }

            fn passenger_attachment_point(&self, _passenger: &dyn Entity) -> DVec3 {
                DVec3::new(0.0, self.ride_height(self.entity_type.dimensions), 0.0)
            }

            /// Rides it, or opens it.
            ///
            /// Vanilla parity: `AbstractChestBoat.interact`. A plain
            /// right-click boards while there is a seat free; sneaking, or
            /// clicking a boat somebody is already sitting in, opens the chest
            /// instead. That is why a plain boat must pass on a sneak rather
            /// than board: otherwise there would be no gesture left for this.
            fn interact(
                &self,
                player: &Player,
                _hand: InteractionHand,
                _location: DVec3,
            ) -> InteractionResult {
                if self.can_add_passenger(player) && !player.is_secondary_use_active() {
                    return boat_common::interact_boat(self, player);
                }
                self.open_chest(player)
            }

            /// Vanilla parity: `ContainerEntity.addChestVehicleSaveData`,
            /// which writes the loot table instead of the items.
            fn save_additional(&self, nbt: &mut NbtCompound) {
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
                // Vanilla parity: `ContainerEntity.readChestVehicleSaveData`,
                // which reads the items only when no table is packed.
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
                    if slot < CHEST_BOAT_SLOTS
                        && let Some(item) = ItemStack::from_borrowed_compound(&compound)
                    {
                        container.items_mut()[slot] = item;
                    }
                }
            }
        }

        impl BoatLike for $name {
            fn boat_state(&self) -> &SyncMutex<BoatState> {
                &self.boat
            }

            fn ride_height(&self, dimensions: EntityDimensions) -> f64 {
                f64::from(dimensions.height) * $ride_height
            }
        }
    };
}

/// A boat with a chest in it.
#[entity_behavior(class = "ChestBoat")]
pub struct ChestBoatEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    entity_data: SyncMutex<AbstractBoatEntityData>,
    boat: SyncMutex<BoatState>,
    container: Shared<SimpleContainer>,
    container_ref: ContainerRef,
    /// Vanilla parity: the `lootTable`/`lootTableSeed` pair of
    /// `AbstractChestBoat`.
    loot: Arc<ContainerLoot>,
}

/// A raft with a chest on it.
#[entity_behavior(class = "ChestRaft")]
pub struct ChestRaftEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    entity_data: SyncMutex<AbstractBoatEntityData>,
    boat: SyncMutex<BoatState>,
    container: Shared<SimpleContainer>,
    container_ref: ContainerRef,
    /// Vanilla parity: the `lootTable`/`lootTableSeed` pair of
    /// `AbstractChestBoat`.
    loot: Arc<ContainerLoot>,
}

chest_boat_body!(ChestBoatEntity, BOAT_RIDE_HEIGHT, "steel:entity/chest_boat");
chest_boat_body!(ChestRaftEntity, RAFT_RIDE_HEIGHT, "steel:entity/chest_raft");

/// Opens the chest for `player`.
///
/// Written once rather than in the macro because it is the same for both
/// shapes and needs no per-type constant.
macro_rules! open_chest_impl {
    ($name:ident) => {
        impl $name {
            fn open_chest(&self, player: &Player) -> InteractionResult {
                // Vanilla parity: `AbstractChestBoat.createMenu`.
                self.unpack_loot_table(Some(player));
                let inventory = player.inventory.clone();
                let container = self.container_ref.clone();
                player.open_menu(self.name(), move |context| {
                    chest(inventory, context.container_id, container, CHEST_BOAT_ROWS)
                });

                // Vanilla parity: the `gameEvent(CONTAINER_OPEN, player)` of
                // `AbstractChestBoat.interact`, credited to the player rather
                // than the boat, which is what a sculk sensor listens for.
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
    };
}

open_chest_impl!(ChestBoatEntity);
open_chest_impl!(ChestRaftEntity);
