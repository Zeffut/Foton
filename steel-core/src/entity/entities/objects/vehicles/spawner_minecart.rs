//! The spawner minecart.
//!
//! Vanilla parity: `MinecartSpawner`. A rolling monster spawner: the same
//! [`BaseSpawner`] a spawner block carries, ticking at wherever the cart
//! happens to be. Nothing in vanilla generates one -- it exists for map makers
//! and for `/summon`.
//!
//! Everything about rolling is [`super::minecart_common`].

use std::sync::{Arc, Weak};

use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;
use steel_macros::entity_behavior;
use steel_protocol::packets::game::SoundSource;
use steel_registry::entity_type::EntityTypeRef;
use steel_utils::locks::SyncMutex;
use steel_utils::{BlockPos, DowncastType, DowncastTypeKey};

use super::minecart_common::{self, MinecartLike, MinecartState};
use crate::entity::{Entity, EntityBase, EntityBaseLoad, EntityMovementEmission};
use crate::world::World;
use crate::world::base_spawner::{BaseSpawner, SpawnerOwner};

/// Vanilla parity: `AbstractMinecart.getDefaultGravity`.
const MINECART_GRAVITY: f64 = 0.04;

/// And the same in water.
const MINECART_GRAVITY_IN_WATER: f64 = 0.005;

/// A minecart with a spawner in it.
#[entity_behavior(class = "MinecartSpawner")]
pub struct SpawnerMinecartEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    minecart: SyncMutex<MinecartState>,
    spawner: BaseSpawner,
}

// SAFETY: This key is owned by Steel and uniquely identifies `SpawnerMinecartEntity`.
unsafe impl DowncastType for SpawnerMinecartEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/spawner_minecart");
}

impl SpawnerMinecartEntity {
    /// Creates one at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self {
            base: EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
            minecart: SyncMutex::new(MinecartState::default()),
            spawner: BaseSpawner::new(),
        }
    }

    /// Creates one from saved base data.
    #[must_use]
    pub fn from_saved(entity_type: EntityTypeRef, load: EntityBaseLoad) -> Self {
        Self {
            base: EntityBase::from_load(load, entity_type.dimensions),
            entity_type,
            minecart: SyncMutex::new(MinecartState::default()),
            spawner: BaseSpawner::new(),
        }
    }

    /// Returns the spawner this cart carries.
    ///
    /// Vanilla parity: `MinecartSpawner.getSpawner`.
    #[must_use]
    pub const fn spawner(&self) -> &BaseSpawner {
        &self.spawner
    }
}

impl SpawnerOwner for SpawnerMinecartEntity {
    /// Vanilla parity: `MinecartSpawner`'s `broadcastEvent`, which sends the
    /// spawner's event id as an entity event rather than a block event.
    ///
    /// Steel's `EntityStatus` is generated from vanilla's `EntityEvent`
    /// constants, and the spawner's `EVENT_SPAWN` is `1` -- the same wire value
    /// `EntityEvent.JUMP` carries. The client reads it through
    /// `MinecartSpawner.handleEntityEvent`, so the shared number is vanilla's,
    /// not a mistake here. It only resets a client-side delay, so nothing on
    /// the server depends on it and Steel does not send it.
    fn broadcast_spawner_event(&self, _world: &Arc<World>, _pos: BlockPos, _id: i32) {}
}

impl Entity for SpawnerMinecartEntity {
    fn base(&self) -> &EntityBase {
        &self.base
    }

    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
    }

    /// Vanilla parity: `MinecartSpawner.tick`, which rolls first and then ticks
    /// its spawner at wherever it ended up.
    fn tick(&self) {
        minecart_common::tick_minecart(self);
        let Some(world) = self.level() else {
            return;
        };
        self.spawner
            .server_tick(self, &world, self.block_position());
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

    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.spawner.save(nbt);
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.spawner.load(&nbt);
    }
}

impl MinecartLike for SpawnerMinecartEntity {
    fn minecart_state(&self) -> &SyncMutex<MinecartState> {
        &self.minecart
    }
}
