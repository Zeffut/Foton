//! Boats and rafts.
//!
//! Vanilla parity: `Boat` and `Raft`, which between them are eighteen entity
//! types and two lines of code -- everything else is [`super::boat_common`].
//! The two differ only in how high a rider sits: a raft has no hull to sit
//! down inside.
//!
//! One struct serves all nine woods because the entity type is a constructor
//! argument, and every wood's synced data is the same `AbstractBoatEntityData`
//! under a different name.

use std::sync::Weak;

use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;
use steel_macros::entity_behavior;
use steel_protocol::packets::game::SoundSource;
use steel_registry::entity_type::{EntityDimensions, EntityTypeRef};
use steel_registry::vanilla_entity_data::AbstractBoatEntityData;
use steel_utils::locks::SyncMutex;
use steel_utils::{DowncastType, DowncastTypeKey};

use steel_utils::types::InteractionHand;

use super::boat_common::{
    self, BOAT_GRAVITY, BOAT_RIDE_HEIGHT, BoatLike, BoatState, MAX_PASSENGERS, RAFT_RIDE_HEIGHT,
};
use crate::behavior::InteractionResult;
use crate::entity::{Entity, EntityBase, EntityBaseLoad, EntityMovementEmission, EntitySyncedData};
use crate::player::Player;
use crate::world::World;

/// Declares one boat shape.
///
/// The struct is written out rather than produced by the macro so the entity
/// codegen can see it: a behavior it cannot see is silently never registered,
/// which is how every furnace ended up without one.
macro_rules! boat_body {
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
                Self {
                    base,
                    entity_type,
                    entity_data: SyncMutex::new(AbstractBoatEntityData::new()),
                    boat: SyncMutex::new(BoatState::default()),
                }
            }
        }

        impl Entity for $name {
            fn base(&self) -> &EntityBase {
                &self.base
            }

            fn entity_type(&self) -> EntityTypeRef {
                self.entity_type
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

            /// Vanilla parity: the `blocksBuilding = true` of the constructor.
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

            /// Vanilla parity: `AbstractBoat.interact`, the right-click that
            /// puts a player in the boat. Without it a boat floats, drifts and
            /// accepts a steering packet it will never be sent.
            fn interact(
                &self,
                player: &Player,
                _hand: InteractionHand,
                _location: DVec3,
            ) -> InteractionResult {
                boat_common::interact_boat(self, player)
            }

            /// Vanilla parity: `AbstractBoat.canAddPassenger`, which refuses a
            /// rider once the boat is full or its deck is under water.
            fn can_add_passenger(&self, _passenger: &dyn Entity) -> bool {
                self.passengers().len() < MAX_PASSENGERS && !self.is_eye_in_water()
            }

            /// Vanilla parity: `AbstractBoat.getPassengerAttachmentPoint`.
            ///
            /// TODO: vanilla seats a second rider behind the first, and nudges
            /// an animal forward again. Steel positions every rider on the
            /// center line until the multi-seat offsets are ported.
            fn passenger_attachment_point(&self, _passenger: &dyn Entity) -> DVec3 {
                DVec3::new(0.0, self.ride_height(self.entity_type.dimensions), 0.0)
            }

            fn save_additional(&self, _nbt: &mut NbtCompound) {}

            fn load_additional(&self, _nbt: BorrowedNbtCompoundView<'_, '_>) {}
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

/// A boat.
#[entity_behavior(class = "Boat")]
pub struct BoatEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    entity_data: SyncMutex<AbstractBoatEntityData>,
    boat: SyncMutex<BoatState>,
}

/// A raft.
#[entity_behavior(class = "Raft")]
pub struct RaftEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    entity_data: SyncMutex<AbstractBoatEntityData>,
    boat: SyncMutex<BoatState>,
}

boat_body!(BoatEntity, BOAT_RIDE_HEIGHT, "steel:entity/boat");
boat_body!(RaftEntity, RAFT_RIDE_HEIGHT, "steel:entity/raft");

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use steel_registry::{init_vanilla_registry, vanilla_entities};
    use steel_utils::ChunkPos;
    use steel_utils::types::UpdateFlags;

    use super::*;
    use crate::behavior::init_behaviors;
    use crate::test_support::{fresh_test_world, insert_ready_full_chunk};
    use steel_registry::vanilla_blocks;
    use steel_utils::BlockPos;

    fn boat_in(world: &Arc<World>, position: DVec3) -> BoatEntity {
        BoatEntity::new(
            &vanilla_entities::OAK_BOAT,
            1,
            position,
            Arc::downgrade(world),
        )
    }

    #[test]
    fn a_raft_seats_its_rider_higher_than_a_boat() {
        init_vanilla_registry();
        let boat = BoatEntity::new(
            &vanilla_entities::OAK_BOAT,
            1,
            DVec3::ZERO,
            Weak::<World>::new(),
        );
        let raft = RaftEntity::new(
            &vanilla_entities::BAMBOO_RAFT,
            2,
            DVec3::ZERO,
            Weak::<World>::new(),
        );

        let dimensions = vanilla_entities::OAK_BOAT.dimensions;
        assert!(raft.ride_height(dimensions) > boat.ride_height(dimensions));
    }

    /// A boat dropped over water stops falling and floats.
    ///
    /// This is the whole entity: without it a boat sinks through the surface
    /// and keeps going, which is what an unimplemented boat does.
    #[test]
    fn a_boat_over_water_stops_falling() {
        init_vanilla_registry();
        init_behaviors();
        let world = fresh_test_world("boat_floats");
        insert_ready_full_chunk(&world, ChunkPos::new(0, 0));

        // A pool: water at y=64, stone underneath so nothing drains away.
        for x in 4..12 {
            for z in 4..12 {
                let _ = world.set_block(
                    BlockPos::new(x, 63, z),
                    vanilla_blocks::STONE.default_state(),
                    UpdateFlags::UPDATE_CLIENTS,
                );
                let _ = world.set_block(
                    BlockPos::new(x, 64, z),
                    vanilla_blocks::WATER.default_state(),
                    UpdateFlags::UPDATE_CLIENTS,
                );
            }
        }

        let boat = boat_in(&world, DVec3::new(8.5, 65.0, 8.5));
        boat.set_velocity(DVec3::new(0.0, -0.5, 0.0));

        for _ in 0..40 {
            boat_common::tick_boat(&boat);
        }

        let y = boat.position().y;
        assert!(
            y > 63.0,
            "the boat sank through the pool floor and reached {y}"
        );
        assert!(
            boat.velocity().y > -0.2,
            "the boat is still falling at {}",
            boat.velocity().y
        );
    }

    /// A boat over nothing keeps falling, so the float step is not a hover.
    #[test]
    fn a_boat_over_air_still_falls() {
        init_vanilla_registry();
        init_behaviors();
        let world = fresh_test_world("boat_falls");
        insert_ready_full_chunk(&world, ChunkPos::new(0, 0));

        let boat = boat_in(&world, DVec3::new(8.5, 120.0, 8.5));
        let start = boat.position().y;

        for _ in 0..20 {
            boat_common::tick_boat(&boat);
        }

        assert!(
            boat.position().y < start,
            "a boat in the air should fall, but it stayed at {start}"
        );
    }
}
