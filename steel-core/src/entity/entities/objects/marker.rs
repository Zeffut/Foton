//! Marker entity.
//!
//! Vanilla parity: `Marker`. The emptiest entity in the game: it does not
//! tick, cannot be hurt, cannot carry passengers, ignores pistons and pressure
//! plates, and is never sent to a client -- its entity type has a client
//! tracking range of zero, and vanilla's `getAddEntityPacket` throws outright
//! if anything tries. What is left is a position with a UUID and a set of
//! scoreboard tags, which is exactly what data packs use it for.
//!
//! Note for anyone porting from an older version: `Marker` used to persist a
//! free-form `data` compound. In MC 26.2 both `Marker.readAdditionalSaveData`
//! and `Marker.addAdditionalSaveData` are empty, so there is nothing left to
//! round-trip beyond the shared base fields, and Steel matches that.

use std::sync::Weak;

use glam::DVec3;
use steel_macros::entity_behavior;
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::vanilla_entity_data::MarkerEntityData;
use steel_utils::locks::SyncMutex;
use steel_utils::{DowncastType, DowncastTypeKey};
use uuid::Uuid;

use crate::entity::{Entity, EntityBase, EntityBaseLoad, EntitySyncedData};
use crate::world::World;

/// A marker entity.
///
/// Vanilla parity: `Marker`.
#[entity_behavior(class = "Marker")]
pub struct MarkerEntity {
    /// Common entity fields (id, uuid, position, etc.).
    base: EntityBase,
    /// Vanilla entity type registered for this implementation.
    entity_type: EntityTypeRef,
    /// Synced entity data. `Marker.defineSynchedData` adds nothing of its own,
    /// so this is only the shared base layer.
    entity_data: SyncMutex<MarkerEntityData>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `MarkerEntity`.
unsafe impl DowncastType for MarkerEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/marker");
}

impl MarkerEntity {
    /// Creates a new marker entity.
    ///
    /// The `id` should be obtained from `next_entity_id()`.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self {
            base: EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
            entity_data: SyncMutex::new(MarkerEntityData::new()),
        }
    }

    /// Creates a new marker entity with a specific UUID.
    ///
    /// The `id` should be obtained from `next_entity_id()`.
    #[must_use]
    pub fn with_uuid(
        entity_type: EntityTypeRef,
        id: i32,
        position: DVec3,
        uuid: Uuid,
        world: Weak<World>,
    ) -> Self {
        Self {
            base: EntityBase::with_uuid(id, uuid, position, entity_type.dimensions, world),
            entity_type,
            entity_data: SyncMutex::new(MarkerEntityData::new()),
        }
    }

    /// Creates a marker entity from saved data.
    #[must_use]
    pub fn from_saved(entity_type: EntityTypeRef, load: EntityBaseLoad) -> Self {
        Self {
            base: EntityBase::from_load(load, entity_type.dimensions),
            entity_type,
            entity_data: SyncMutex::new(MarkerEntityData::new()),
        }
    }

    /// Gets a reference to the entity data for reading/modifying synced state.
    pub const fn entity_data(&self) -> &SyncMutex<MarkerEntityData> {
        &self.entity_data
    }
}

impl Entity for MarkerEntity {
    fn base(&self) -> &EntityBase {
        &self.base
    }

    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
    }

    fn synced_data(&self) -> Option<&dyn EntitySyncedData> {
        Some(&self.entity_data)
    }

    /// Vanilla parity: `Marker.isIgnoringBlockTriggers`.
    fn is_ignoring_block_triggers(&self) -> bool {
        true
    }

    /// Vanilla parity: `Marker.canAddPassenger`, which always refuses.
    fn can_add_passenger(&self, _passenger: &dyn Entity) -> bool {
        false
    }

    /// Vanilla parity: `Marker.couldAcceptPassenger`, which always refuses.
    fn could_accept_passenger(&self) -> bool {
        false
    }

    // `Marker.tick` is empty and the default non-living tick does nothing, so
    // there is no override here. `Marker.hurtServer` always returns false, which
    // Steel gets for free: `Entity::hurt` only forwards to living entities.
    // `save_additional`/`load_additional` are left at their empty defaults to
    // match the empty vanilla methods.
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use simdnbt::borrow::read_compound;
    use simdnbt::owned::NbtCompound;
    use steel_registry::{init_vanilla_registry, vanilla_entities};

    use super::*;
    use crate::entity::{EntityBaseSaveData, EntityFireFreezeState};

    #[test]
    fn a_marker_saves_nothing_of_its_own_and_reloads_from_the_shared_base_alone() {
        init_vanilla_registry();
        let uuid = Uuid::from_u128(0x00ff_00ff_00ff_00ff_00ff_00ff_00ff_00ff);
        let entity = MarkerEntity::with_uuid(
            &vanilla_entities::MARKER,
            31,
            DVec3::new(8.5, 70.0, -3.5),
            uuid,
            Weak::new(),
        );

        let mut saved = NbtCompound::new();
        entity.save_additional(&mut saved);
        assert!(
            saved.is_empty(),
            "Marker.addAdditionalSaveData writes nothing"
        );

        let mut bytes = Vec::new();
        saved.write(&mut bytes);
        let Ok(borrowed) = read_compound(&mut Cursor::new(bytes.as_slice())) else {
            panic!("saved marker NBT should reborrow");
        };
        let loaded = MarkerEntity::from_saved(
            &vanilla_entities::MARKER,
            EntityBaseLoad {
                id: 32,
                position: DVec3::new(8.5, 70.0, -3.5),
                uuid,
                velocity: DVec3::ZERO,
                rotation: (0.0, 0.0),
                fall_distance: 0.0,
                fire_freeze: EntityFireFreezeState::new(),
                on_ground: false,
                save_data: EntityBaseSaveData::new(),
                world: Weak::new(),
            },
        );
        loaded.load_additional((&borrowed).into());

        assert_eq!(loaded.uuid(), uuid);
        assert_eq!(loaded.position(), DVec3::new(8.5, 70.0, -3.5));
    }

    #[test]
    fn a_marker_refuses_every_passenger() {
        init_vanilla_registry();
        let marker = MarkerEntity::new(&vanilla_entities::MARKER, 31, DVec3::ZERO, Weak::new());
        let rider = MarkerEntity::new(&vanilla_entities::MARKER, 32, DVec3::ZERO, Weak::new());

        assert!(!marker.could_accept_passenger());
        assert!(!marker.can_add_passenger(&rider));
    }
}
