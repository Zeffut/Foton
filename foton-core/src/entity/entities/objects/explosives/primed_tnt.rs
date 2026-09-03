//! Primed TNT entity.
//!
//! Vanilla parity: `PrimedTnt`. Lit TNT becomes an entity that falls, skids on
//! landing, and detonates when its fuse runs out.

use std::sync::{Arc, Weak};

use foton_macros::entity_behavior;
use foton_registry::entity_type::EntityTypeRef;
use foton_registry::vanilla_entity_data::PrimedTntEntityData;
use foton_registry::vanilla_game_rules::TNT_EXPLOSION_DROP_DECAY;
use foton_registry::{vanilla_blocks, vanilla_entities};
use foton_utils::locks::SyncMutex;
use foton_utils::{BlockPos, BlockStateId, DowncastType, DowncastTypeKey};
use glam::DVec3;

use crate::entity::{
    Entity, EntityBase, EntityBaseLoad, EntitySyncedData, RemovalReason, next_entity_id,
};
use crate::physics::MoverType;
use crate::world::World;
use crate::world::explosion::ExplosionSpec;

/// Ticks a fuse burns for when TNT is lit normally.
///
/// Vanilla parity: `PrimedTnt.DEFAULT_FUSE_TIME`.
pub const DEFAULT_FUSE_TIME: i32 = 80;

/// Blast radius of a single TNT.
const DEFAULT_EXPLOSION_POWER: f32 = 4.0;

/// Vanilla parity: `PrimedTnt.getDefaultGravity`.
const DEFAULT_GRAVITY: f64 = 0.04;

/// Velocity kept each tick while airborne.
const AIR_DRAG: f64 = 0.98;

/// Horizontal velocity kept on the tick TNT touches the ground.
const LANDED_HORIZONTAL_DRAG: f64 = 0.7;

/// Vertical velocity kept on landing, which makes TNT hop slightly.
const LANDED_VERTICAL_BOUNCE: f64 = -0.5;

/// Fraction of the entity's height the blast originates from.
const EXPLOSION_HEIGHT_FRACTION: f64 = 0.062_5;

/// State that is not mirrored to clients.
struct PrimedTntState {
    explosion_power: f32,
    owner_id: Option<i32>,
}

/// Lit TNT waiting to go off.
#[entity_behavior(class = "PrimedTnt")]
pub struct PrimedTntEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    entity_data: SyncMutex<PrimedTntEntityData>,
    state: SyncMutex<PrimedTntState>,
}

// SAFETY: This Foton-owned key uniquely identifies `PrimedTntEntity`.
unsafe impl DowncastType for PrimedTntEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:entity/primed_tnt");
}

impl PrimedTntEntity {
    /// Returns the entity id that directly primed this TNT, when known.
    pub fn source_entity_id(&self) -> Option<i32> {
        self.state.lock().owner_id
    }

    /// Creates primed TNT with the default fuse.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        let mut entity_data = PrimedTntEntityData::new();
        entity_data.fuse.set(DEFAULT_FUSE_TIME);
        entity_data
            .block_state
            .set(vanilla_blocks::TNT.default_state());
        Self {
            base: EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
            entity_data: SyncMutex::new(entity_data),
            state: SyncMutex::new(PrimedTntState {
                explosion_power: DEFAULT_EXPLOSION_POWER,
                owner_id: None,
            }),
        }
    }

    /// Creates primed TNT from saved base data.
    #[must_use]
    pub fn from_saved(entity_type: EntityTypeRef, load: EntityBaseLoad) -> Self {
        Self {
            base: EntityBase::from_load(load, entity_type.dimensions),
            entity_type,
            entity_data: SyncMutex::new(PrimedTntEntityData::new()),
            state: SyncMutex::new(PrimedTntState {
                explosion_power: DEFAULT_EXPLOSION_POWER,
                owner_id: None,
            }),
        }
    }

    /// Lights TNT at `pos` and adds the entity to the world.
    ///
    /// Vanilla parity: `TntBlock.explode`. The entity spawns at the center of the
    /// block with a small upward nudge and a randomized short fuse offset.
    pub fn prime(world: &Arc<World>, pos: BlockPos, owner_id: Option<i32>) -> Arc<Self> {
        let position = DVec3::new(
            f64::from(pos.x()) + 0.5,
            f64::from(pos.y()),
            f64::from(pos.z()) + 0.5,
        );
        let entity = Arc::new(Self::new(
            &vanilla_entities::TNT,
            next_entity_id(),
            position,
            Arc::downgrade(world),
        ));
        entity.state.lock().owner_id = owner_id;

        if let Err(error) = world.try_add_entity(Arc::clone(&entity) as Arc<dyn Entity>) {
            log::error!("failed to add primed tnt entity: {error}");
        }
        entity
    }

    /// Rolls a shortened fuse for explosives caught in a blast.
    ///
    /// Vanilla parity: `PrimedTnt.getRandomShortFuse`, which is what makes a
    /// chain of TNT go off in a ragged burst rather than all at once.
    #[must_use]
    pub fn random_short_fuse(fuse: i32) -> i32 {
        rand::random_range(0..(fuse / 4).max(1)) + fuse / 8
    }

    /// Returns the ticks left before detonation.
    #[must_use]
    pub fn fuse(&self) -> i32 {
        *self.entity_data.lock().fuse.get()
    }

    /// Sets the ticks left before detonation.
    pub fn set_fuse(&self, fuse: i32) {
        self.entity_data.lock().fuse.set(fuse);
    }

    /// Returns the block state this TNT renders as.
    #[must_use]
    pub fn block_state(&self) -> BlockStateId {
        *self.entity_data.lock().block_state.get()
    }

    /// Detonates.
    ///
    /// Vanilla parity: `PrimedTnt.explode`. The blast starts slightly above the
    /// entity's feet and leaves no fire.
    fn explode(&self, world: &Arc<World>) {
        let power = self.state.lock().explosion_power;
        let position = self.position();
        let center = DVec3::new(
            position.x,
            f64::from(self.entity_type.dimensions.height)
                .mul_add(EXPLOSION_HEIGHT_FRACTION, position.y),
            position.z,
        );
        world.explode(
            ExplosionSpec::new(
                Some(self.id()),
                self.state.lock().owner_id,
                None,
                power,
                false,
                world.explosion_destroy_type(&TNT_EXPLOSION_DROP_DECAY),
            ),
            center,
        );
    }
}

impl Entity for PrimedTntEntity {
    fn base(&self) -> &EntityBase {
        &self.base
    }

    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
    }

    fn tick(&self) {
        self.apply_gravity();
        let _ = self.move_entity(MoverType::SelfMovement, self.velocity());
        self.apply_effects_from_blocks();
        self.set_velocity(self.velocity() * AIR_DRAG);

        if self.on_ground() {
            let velocity = self.velocity();
            self.set_velocity(DVec3::new(
                velocity.x * LANDED_HORIZONTAL_DRAG,
                velocity.y * LANDED_VERTICAL_BOUNCE,
                velocity.z * LANDED_HORIZONTAL_DRAG,
            ));
        }

        let fuse = self.fuse() - 1;
        self.set_fuse(fuse);
        if fuse > 0 {
            return;
        }

        self.set_removed(RemovalReason::Discarded);
        if let Some(world) = self.level() {
            self.explode(&world);
        }
    }

    fn get_default_gravity(&self) -> f64 {
        DEFAULT_GRAVITY
    }

    fn synced_data(&self) -> Option<&dyn EntitySyncedData> {
        Some(&self.entity_data)
    }

    fn is_pickable(&self) -> bool {
        false
    }
}
