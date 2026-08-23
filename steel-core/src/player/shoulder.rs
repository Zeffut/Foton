//! Entities riding on a player's shoulders.
//!
//! Vanilla parity: the `shoulderEntityLeft`/`shoulderEntityRight` half of
//! `ServerPlayer`, plus `Player.handleShoulderEntities`. Only the parrot uses
//! it, and only because [`crate::entity::entities::ParrotEntity`] asks to sit
//! there; everything about *carrying* one belongs to the player.

use std::sync::Arc;

use glam::DVec3;
use steel_utils::ChunkPos;
use steel_utils::locks::SyncMutex;

use crate::chunk_saver::{ChunkStorage, PersistentEntity};
use steel_utils::Downcast as _;

use steel_registry::vanilla_entities;

use crate::entity::entities::ParrotEntity;
use crate::entity::{Entity, LivingEntity, SharedEntity};

use super::Player;

/// How long a shoulder rider is safe from being shaken off.
///
/// Vanilla parity: the `timeEntitySatOnShoulder + 20L` of
/// `ServerPlayer.removeEntitiesOnShoulder`, which stops a parrot from being
/// dropped in the same second it landed.
const SHOULDER_SETTLE_TICKS: i64 = 20;

/// One chance in this many ticks that a shoulder rider makes a noise.
///
/// Vanilla parity: the `random.nextInt(200) == 0` of
/// `ServerPlayer.playShoulderEntityAmbientSound`.
const AMBIENT_SOUND_CHANCE: i32 = 200;

/// How far above the player a dropped rider reappears.
///
/// Vanilla parity: the `getY() + 0.7F` of `respawnEntityOnShoulder`.
const RESPAWN_Y_OFFSET: f64 = 0.7;

/// Fall distance past which a rider is shaken off.
///
/// Vanilla parity: the `fallDistance > 0.5` of `handleShoulderEntities`.
const SHAKE_OFF_FALL_DISTANCE: f64 = 0.5;

/// Which shoulder a rider is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shoulder {
    Left,
    Right,
}

/// What a player is carrying on each shoulder.
#[derive(Debug, Default)]
pub struct ShoulderEntities {
    left: SyncMutex<Option<PersistentEntity>>,
    right: SyncMutex<Option<PersistentEntity>>,
    settled_at: SyncMutex<i64>,
}

impl ShoulderEntities {
    /// Creates an empty pair of shoulders.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            left: SyncMutex::new(None),
            right: SyncMutex::new(None),
            settled_at: SyncMutex::new(0),
        }
    }

    const fn slot(&self, shoulder: Shoulder) -> &SyncMutex<Option<PersistentEntity>> {
        match shoulder {
            Shoulder::Left => &self.left,
            Shoulder::Right => &self.right,
        }
    }
}

impl Player {
    /// Returns whether a rider is on this shoulder.
    #[must_use]
    pub fn has_shoulder_entity(&self, shoulder: Shoulder) -> bool {
        self.shoulder_entities.slot(shoulder).lock().is_some()
    }

    /// Seats an entity on a free shoulder.
    ///
    /// Vanilla parity: `ServerPlayer.setEntityOnShoulder`. The caller removes
    /// the live entity; this only takes over the saved copy.
    pub fn set_entity_on_shoulder(&self, entity: &SharedEntity) -> bool {
        if self.is_passenger()
            || !self.on_ground()
            || self.is_in_water()
            || self.is_in_powder_snow()
        {
            return false;
        }
        let Some(persistent) = ChunkStorage::entity_tree_to_persistent(entity) else {
            return false;
        };

        let shoulder = if self.shoulder_entities.left.lock().is_none() {
            Shoulder::Left
        } else if self.shoulder_entities.right.lock().is_none() {
            Shoulder::Right
        } else {
            return false;
        };

        *self.shoulder_entities.slot(shoulder).lock() = Some(persistent);
        *self.shoulder_entities.settled_at.lock() =
            self.level().map_or(0, |world| world.game_time());
        self.sync_shoulder_parrot(shoulder, entity.as_ref());
        true
    }

    /// Runs the per-tick half of carrying a rider.
    ///
    /// Vanilla parity: `ServerPlayer.handleShoulderEntities`.
    pub(super) fn handle_shoulder_entities(&self) {
        self.play_shoulder_entity_ambient_sound(Shoulder::Left);
        self.play_shoulder_entity_ambient_sound(Shoulder::Right);

        if self.fall_distance() > SHAKE_OFF_FALL_DISTANCE
            || self.is_in_water()
            || self.abilities.lock().flying
            || self.is_sleeping()
            || self.is_in_powder_snow()
        {
            self.remove_entities_on_shoulder();
        }
    }

    /// Drops both riders back into the world once they have settled.
    ///
    /// Vanilla parity: `ServerPlayer.removeEntitiesOnShoulder`.
    pub fn remove_entities_on_shoulder(&self) {
        let now = self.level().map_or(0, |world| world.game_time());
        if *self.shoulder_entities.settled_at.lock() + SHOULDER_SETTLE_TICKS >= now {
            return;
        }

        self.drop_shoulder_entities();
    }

    /// Drops both riders because the player is leaving.
    ///
    /// **Gap**: vanilla saves the riders inside the player file and gives them
    /// back on the next login. Steel's `PersistentPlayerData` has no field for
    /// them yet, so they are returned to the world instead -- the pet is never
    /// lost, but it is standing where its owner logged out rather than back on
    /// the shoulder.
    pub(crate) fn drop_shoulder_entities_on_disconnect(&self) {
        self.drop_shoulder_entities();
    }

    fn drop_shoulder_entities(&self) {
        for shoulder in [Shoulder::Left, Shoulder::Right] {
            let Some(persistent) = self.shoulder_entities.slot(shoulder).lock().take() else {
                continue;
            };
            self.respawn_entity_on_shoulder(&persistent);
            self.clear_shoulder_parrot(shoulder);
        }
    }

    /// Vanilla parity: `ServerPlayer.respawnEntityOnShoulder`.
    fn respawn_entity_on_shoulder(&self, persistent: &PersistentEntity) {
        let Some(world) = self.level() else {
            return;
        };

        let position = self.position();
        let spawn_position = DVec3::new(position.x, position.y + RESPAWN_Y_OFFSET, position.z);
        let entities = ChunkStorage::persistent_to_entity_tree_at_level(
            persistent,
            ChunkPos::from_entity_pos(spawn_position),
            &Arc::downgrade(&world),
        );

        for entity in entities {
            if let Some(tamable) = entity.as_tamable_animal() {
                tamable.set_owner_uuid(Some(self.uuid()));
            }
            if entity.try_set_position(spawn_position).is_err() {
                continue;
            }
            if let Err(error) = world.try_add_entity(entity) {
                log::error!("failed to drop a shoulder rider back into the world: {error}");
            }
        }
    }

    /// Vanilla parity: `ServerPlayer.playShoulderEntityAmbientSound`.
    fn play_shoulder_entity_ambient_sound(&self, shoulder: Shoulder) {
        let Some(persistent) = self.shoulder_entities.slot(shoulder).lock().clone() else {
            return;
        };
        if persistent.silent || rand::random_range(0..AMBIENT_SOUND_CHANCE) != 0 {
            return;
        }
        if persistent.entity_type != vanilla_entities::PARROT.key {
            return;
        }

        ParrotEntity::imitate_nearby_mobs_or_chirp(self);
    }

    /// Publishes the rider's parrot variant so the client can draw it.
    ///
    /// Vanilla parity: `Player.setShoulderParrotLeft`/`Right`, which in 26.2
    /// sync only the variant rather than the whole entity.
    fn sync_shoulder_parrot(&self, shoulder: Shoulder, entity: &dyn Entity) {
        let Some(parrot) = entity.downcast_ref::<ParrotEntity>() else {
            return;
        };
        let variant = u32::try_from(parrot.variant().id()).ok();
        self.set_shoulder_parrot(shoulder, variant);
    }

    fn clear_shoulder_parrot(&self, shoulder: Shoulder) {
        self.set_shoulder_parrot(shoulder, None);
    }

    fn set_shoulder_parrot(&self, shoulder: Shoulder, variant: Option<u32>) {
        let mut entity_data = self.entity_data.lock();
        match shoulder {
            Shoulder::Left => entity_data.shoulder_parrot_left.set(variant),
            Shoulder::Right => entity_data.shoulder_parrot_right.set(variant),
        }
    }
}
