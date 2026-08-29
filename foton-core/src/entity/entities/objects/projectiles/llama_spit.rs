//! A llama's spit.
//!
//! Vanilla parity: `net.minecraft.world.entity.projectile.LlamaSpit`. It sits
//! straight on `Projectile` rather than on the throwable or hurting stacks,
//! because it resolves its hit before it moves and dies the moment it is buried
//! in a block or lands in water.

use std::sync::Weak;

use foton_macros::entity_behavior;
use foton_registry::blocks::block_state_ext::BlockStateExt as _;
use foton_registry::entity_type::EntityTypeRef;
use foton_registry::vanilla_damage_types;
use foton_registry::vanilla_entity_data::LlamaSpitEntityData;
use foton_utils::locks::SyncMutex;
use foton_utils::{BlockPos, DowncastType, DowncastTypeKey};
use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;

use crate::entity::damage::DamageSource;
use crate::entity::{
    Entity, EntityBase, EntityBaseLoad, EntitySyncedData, Projectile, ProjectileBase,
    RemovalReason, SharedEntity,
};
use crate::world::{ClipHitResult, World};

/// Vanilla `LlamaSpit.getDefaultGravity`.
const DEFAULT_GRAVITY: f64 = 0.06;

/// Vanilla `LlamaSpit.getAirDrag`.
const AIR_DRAG: f64 = 0.99;

/// What a spit does to whatever it lands on.
///
/// Vanilla parity: the `hurtServer(..., 1.0F)` of `LlamaSpit.onHitEntity`.
const SPIT_DAMAGE: f32 = 1.0;

/// A llama's spit.
#[entity_behavior(class = "LlamaSpit")]
pub struct LlamaSpitEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    entity_data: SyncMutex<LlamaSpitEntityData>,
    projectile_base: ProjectileBase,
}

// SAFETY: This key is owned by Foton and uniquely identifies `LlamaSpitEntity`.
unsafe impl DowncastType for LlamaSpitEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:entity/llama_spit");
}

impl LlamaSpitEntity {
    /// Creates a spit.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self {
            base: EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
            entity_data: SyncMutex::new(LlamaSpitEntityData::new()),
            projectile_base: ProjectileBase::new(),
        }
    }

    /// Creates a spit from saved base data.
    #[must_use]
    pub fn from_saved(entity_type: EntityTypeRef, load: EntityBaseLoad) -> Self {
        Self {
            base: EntityBase::from_load(load, entity_type.dimensions),
            entity_type,
            entity_data: SyncMutex::new(LlamaSpitEntityData::new()),
            projectile_base: ProjectileBase::new(),
        }
    }

    /// Returns whether every block the spit overlaps is solid enough to bury it.
    ///
    /// Vanilla parity: the `getBlockStates(getBoundingBox()).noneMatch(isAir)` of
    /// `LlamaSpit.tick`.
    fn is_buried(&self, world: &World) -> bool {
        let aabb = self.bounding_box();
        let min = BlockPos::new(
            aabb.min_x().floor() as i32,
            aabb.min_y().floor() as i32,
            aabb.min_z().floor() as i32,
        );
        let max = BlockPos::new(
            aabb.max_x().floor() as i32,
            aabb.max_y().floor() as i32,
            aabb.max_z().floor() as i32,
        );
        BlockPos::between_closed(min, max).all(|pos| !world.get_block_state(pos).is_air())
    }
}

impl Entity for LlamaSpitEntity {
    fn base(&self) -> &EntityBase {
        &self.base
    }

    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
    }

    fn get_default_gravity(&self) -> f64 {
        DEFAULT_GRAVITY
    }

    /// Vanilla parity: `LlamaSpit.tick`.
    fn tick(&self) {
        self.set_old_position_to_current();
        self.base().set_old_rotation_to_current();
        self.projectile_base_tick();

        let Some(world) = self.level() else {
            return;
        };

        let movement = self.velocity();
        let hit = self.get_hit_result_on_move_vector();
        if let Some(hit) = hit {
            self.hit_target_or_deflect_self(&hit);
        }
        if self.is_removed() || self.is_world_change_pending() {
            return;
        }

        let next_position = self.position() + movement;
        self.update_rotation();

        if self.is_buried(&world) || self.is_in_water() {
            self.set_removed(RemovalReason::Discarded);
            return;
        }

        self.set_velocity(movement * AIR_DRAG);
        self.apply_gravity();
        if let Err(error) = self.try_set_position(next_position) {
            log::debug!("failed to advance llama spit {}: {error}", self.id());
            self.set_removed(RemovalReason::Discarded);
        }
    }

    fn synced_data(&self) -> Option<&dyn EntitySyncedData> {
        Some(&self.entity_data)
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_projectile(nbt);
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_projectile(nbt);
    }

    fn restore_owner_reference(&self, owner: &SharedEntity) {
        self.cache_owner_entity(owner);
    }
}

impl Projectile for LlamaSpitEntity {
    fn projectile_base(&self) -> &ProjectileBase {
        &self.projectile_base
    }

    /// Vanilla parity: `LlamaSpit.onHitEntity`.
    fn on_hit_entity(&self, entity: &SharedEntity, _location: DVec3) {
        let Some(owner) = self.get_owner() else {
            return;
        };
        if owner.as_living_entity().is_none() {
            return;
        }

        let damage = DamageSource::environment(&vanilla_damage_types::SPIT)
            .with_direct_entity(self.id())
            .with_causing_entity(owner.id());
        if let Some(world) = entity.level() {
            entity.hurt(&world, &damage, SPIT_DAMAGE);
        }
    }

    /// Vanilla parity: `LlamaSpit.onHitBlock`.
    fn on_hit_block(&self, hit: &ClipHitResult) {
        self.projectile_on_hit_block(hit);
        self.set_removed(RemovalReason::Discarded);
    }
}
