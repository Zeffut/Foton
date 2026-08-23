//! The TNT minecart.
//!
//! Vanilla parity: `MinecartTNT`. A cart that carries a charge: an activator
//! rail lights the fuse, and eighty ticks later it goes off. Running into a
//! wall at speed sets it off immediately, and the faster it was going the
//! bigger the blast -- which is what makes a TNT cart worth more than the TNT
//! in it.
//!
//! Everything about rolling is [`super::minecart_common`]; the explosion is
//! [`crate::world::World::explode`]. Both already existed, and neither had a
//! cart to connect them.

use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Arc, Weak};

use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;
use steel_macros::entity_behavior;
use steel_protocol::packets::game::SoundSource;
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::vanilla_block_tags::BlockTag;
use steel_registry::vanilla_game_rules::TNT_EXPLOSION_DROP_DECAY;
use steel_registry::{REGISTRY, TaggedRegistryExt as _, sound_events};
use steel_utils::locks::SyncMutex;
use steel_utils::{BlockPos, DowncastType, DowncastTypeKey};

use super::minecart_common::{self, MinecartLike, MinecartState};
use crate::entity::{Entity, EntityBase, EntityBaseLoad, EntityMovementEmission, RemovalReason};
use crate::world::explosion::ExplosionSpec;
use crate::world::{LevelReader as _, World};

/// Vanilla parity: `AbstractMinecart.getDefaultGravity`.
const MINECART_GRAVITY: f64 = 0.04;

/// And the same in water.
const MINECART_GRAVITY_IN_WATER: f64 = 0.005;

/// The value a fuse that has not been lit holds.
///
/// Vanilla parity: the `fuse = -1` of `MinecartTNT`.
const FUSE_UNLIT: i32 = -1;

/// How long the fuse burns once lit.
///
/// Vanilla parity: the `this.fuse = 80` of `primeFuse`.
const FUSE_TICKS: i32 = 80;

/// Vanilla parity: `MinecartTNT.DEFAULT_EXPLOSION_POWER_BASE`.
const EXPLOSION_POWER_BASE: f64 = 4.0;

/// Vanilla parity: `MinecartTNT.DEFAULT_EXPLOSION_SPEED_FACTOR`.
const EXPLOSION_SPEED_FACTOR: f64 = 1.0;

/// How much speed can add to the blast.
///
/// Vanilla parity: the `Math.min(Math.sqrt(speedSqr), 5.0)` of `explode`.
const MAX_SPEED_CONTRIBUTION: f64 = 5.0;

/// How much a unit of speed is worth to the blast.
///
/// Vanilla parity: the `* 1.5 *` of the same line.
const SPEED_TO_POWER: f64 = 1.5;

/// The speed a wall has to be hit at to set the charge off.
///
/// Vanilla parity: the `speedSqr >= 0.01F` of `MinecartTNT.tick`.
const CRASH_SPEED_SQUARED: f64 = 0.01;

/// A TNT minecart.
#[entity_behavior(class = "MinecartTNT")]
pub struct TntMinecartEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    minecart: SyncMutex<MinecartState>,
    /// Ticks left on the fuse, or [`FUSE_UNLIT`].
    fuse: AtomicI32,
}

// SAFETY: This key is owned by Steel and uniquely identifies `TntMinecartEntity`.
unsafe impl DowncastType for TntMinecartEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/tnt_minecart");
}

impl TntMinecartEntity {
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
        Self {
            base,
            entity_type,
            minecart: SyncMutex::new(MinecartState::default()),
            fuse: AtomicI32::new(FUSE_UNLIT),
        }
    }

    /// Returns whether the fuse is burning.
    ///
    /// Vanilla parity: `MinecartTNT.isPrimed`.
    #[must_use]
    pub fn is_primed(&self) -> bool {
        self.fuse.load(Ordering::Relaxed) > FUSE_UNLIT
    }

    /// Returns the ticks left on the fuse.
    #[must_use]
    pub fn fuse(&self) -> i32 {
        self.fuse.load(Ordering::Relaxed)
    }

    /// Lights the fuse.
    ///
    /// Vanilla parity: `MinecartTNT.primeFuse`. Lighting an already-lit fuse
    /// would restart it, which is why every caller checks first.
    pub fn prime_fuse(&self) {
        self.fuse.store(FUSE_TICKS, Ordering::Relaxed);
        let Some(world) = self.level() else {
            return;
        };
        world.play_sound_at(
            &sound_events::ENTITY_TNT_PRIMED,
            SoundSource::Blocks,
            self.position(),
            1.0,
            1.0,
            None,
        );
    }

    /// Sets the charge off.
    ///
    /// Vanilla parity: `MinecartTNT.explode`. The blast grows with the speed
    /// the cart was carrying, capped so that a cart fired down a long rail
    /// cannot level a chunk.
    fn explode(&self, speed_squared: f64) {
        let Some(world) = self.level() else {
            return;
        };
        let speed = speed_squared.sqrt().min(MAX_SPEED_CONTRIBUTION);
        let power = EXPLOSION_SPEED_FACTOR.mul_add(
            rand::random::<f64>() * SPEED_TO_POWER * speed,
            EXPLOSION_POWER_BASE,
        );

        // Vanilla parity: `MinecartTNT.shouldBlockExplode`, which spares a rail
        // and anything with a rail on top of it. A cart that took its own track
        // with it could only ever be used once.
        let spare_rails = |pos: BlockPos| {
            !is_rail(&world, pos) && !is_rail(&world, BlockPos::new(pos.x(), pos.y() + 1, pos.z()))
        };
        world.explode_sparing(
            ExplosionSpec::new(
                Some(self.id()),
                None,
                None,
                power as f32,
                false,
                world.explosion_destroy_type(&TNT_EXPLOSION_DROP_DECAY),
            ),
            self.position(),
            &spare_rails,
        );
        self.set_removed(RemovalReason::Discarded);
    }

    /// The squared horizontal speed, which is what decides the blast size.
    ///
    /// Vanilla parity: `Vec3.horizontalDistanceSqr`.
    fn horizontal_speed_squared(&self) -> f64 {
        let velocity = self.velocity();
        velocity.x.mul_add(velocity.x, velocity.z * velocity.z)
    }
}

impl Entity for TntMinecartEntity {
    fn base(&self) -> &EntityBase {
        &self.base
    }

    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
    }

    /// Vanilla parity: `MinecartTNT.tick`.
    fn tick(&self) {
        minecart_common::tick_minecart(self);

        let fuse = self.fuse.load(Ordering::Relaxed);
        if fuse > 0 {
            self.fuse.store(fuse - 1, Ordering::Relaxed);
        } else if fuse == 0 {
            self.explode(self.horizontal_speed_squared());
            return;
        }

        // Vanilla parity: a cart that runs into something at speed goes off
        // whether or not its fuse was ever lit.
        if self.horizontal_collision() {
            let speed_squared = self.horizontal_speed_squared();
            if speed_squared >= CRASH_SPEED_SQUARED {
                self.explode(speed_squared);
            }
        }
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
        nbt.insert("fuse", self.fuse.load(Ordering::Relaxed));
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        if let Some(fuse) = nbt.int("fuse") {
            self.fuse.store(fuse, Ordering::Relaxed);
        }
    }
}

impl MinecartLike for TntMinecartEntity {
    fn minecart_state(&self) -> &SyncMutex<MinecartState> {
        &self.minecart
    }

    /// Vanilla parity: `MinecartTNT.activateMinecart`. This is the ordinary way
    /// a TNT cart is set off: it rolls over a powered activator rail.
    fn activate_minecart(&self, _world: &Arc<World>, _pos: BlockPos, powered: bool) {
        if powered && !self.is_primed() {
            self.prime_fuse();
        }
    }
}

/// Returns whether the block at `pos` is a rail.
///
/// Vanilla parity: the `BlockTags.RAILS` check of
/// `MinecartTNT.shouldBlockExplode`.
fn is_rail(world: &Arc<World>, pos: BlockPos) -> bool {
    use steel_registry::blocks::block_state_ext::BlockStateExt as _;

    REGISTRY
        .blocks
        .is_in_tag(world.get_block_state(pos).get_block(), &BlockTag::RAILS)
}
