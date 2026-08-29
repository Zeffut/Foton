//! Dragon fireball.
//!
//! Vanilla parity: `DragonFireball`, on `AbstractHurtingProjectile`. It does no
//! damage of its own at all -- no `onHitEntity` override, no explosion. What it
//! does is leave a cloud of dragon's breath where it lands, and the cloud is the
//! weapon. That is also why the dragon can fire one at its own feet: the ball
//! passes straight through the entity that shot it.

use std::sync::{Arc, Weak};

use foton_macros::entity_behavior;
use foton_registry::entity_type::EntityTypeRef;
use foton_registry::level_events::PARTICLES_DRAGON_FIREBALL_SPLASH;
use foton_registry::vanilla_entities;
use foton_registry::vanilla_entity_data::DragonFireballEntityData;
use foton_utils::locks::SyncMutex;
use foton_utils::{DowncastType, DowncastTypeKey};
use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;

use crate::entity::damage::DamageSource;
use crate::entity::entities::AreaEffectCloudEntity;
use crate::entity::{
    Entity, EntityBase, EntityBaseLoad, EntitySyncedData, HurtingProjectile, HurtingProjectileBase,
    Projectile, ProjectileBase, ProjectileHit, RemovalReason, SharedEntity, next_entity_id,
};
use crate::world::World;

/// How far out the cloud looks for somebody to settle on.
///
/// Vanilla parity: `DragonFireball.SPLASH_RANGE`, used as the horizontal half
/// width of the `inflate(4.0, 2.0, 4.0)` search box.
const SPLASH_RANGE: f64 = 4.0;

/// How far up and down that search box reaches.
const SPLASH_HEIGHT: f64 = 2.0;

/// Squared distance within which the cloud is moved onto a candidate.
///
/// Vanilla parity: the `dist < 16.0` of `DragonFireball.onHit`, which is four
/// blocks even though the box it searched is wider than that.
const CLOUD_SNAP_RANGE_SQR: f64 = 16.0;

/// A dragon's fireball.
#[entity_behavior(class = "DragonFireball")]
pub struct DragonFireballEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    entity_data: SyncMutex<DragonFireballEntityData>,
    projectile_base: ProjectileBase,
    hurting_projectile_base: HurtingProjectileBase,
}

// SAFETY: This key is owned by Foton and uniquely identifies `DragonFireballEntity`.
unsafe impl DowncastType for DragonFireballEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:entity/dragon_fireball");
}

impl DragonFireballEntity {
    /// Creates a dragon fireball.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self {
            base: EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
            entity_data: SyncMutex::new(DragonFireballEntityData::new()),
            projectile_base: ProjectileBase::new(),
            hurting_projectile_base: HurtingProjectileBase::new(),
        }
    }

    /// Creates a dragon fireball from saved base data.
    #[must_use]
    pub fn from_saved(entity_type: EntityTypeRef, load: EntityBaseLoad) -> Self {
        Self {
            base: EntityBase::from_load(load, entity_type.dimensions),
            entity_type,
            entity_data: SyncMutex::new(DragonFireballEntityData::new()),
            projectile_base: ProjectileBase::new(),
            hurting_projectile_base: HurtingProjectileBase::new(),
        }
    }

    /// Picks where the cloud settles.
    ///
    /// Vanilla parity: the loop at the end of `DragonFireball.onHit`. It walks
    /// the living entities in the splash box and drops the cloud on the first
    /// one within four blocks, so a breath aimed at a player's feet lands on the
    /// player rather than where the ball happened to clip a wall.
    fn cloud_position(&self, world: &Arc<World>) -> DVec3 {
        let position = self.position();
        let search = self
            .bounding_box()
            .inflate_xyz(SPLASH_RANGE, SPLASH_HEIGHT, SPLASH_RANGE);

        for entity in world.get_entities_in_aabb(&search) {
            if entity.as_living_entity().is_none() {
                continue;
            }
            if entity.position().distance_squared(position) < CLOUD_SNAP_RANGE_SQR {
                return entity.position();
            }
        }

        position
    }

    /// Leaves the cloud of dragon's breath.
    ///
    /// Vanilla parity: the cloud construction of `DragonFireball.onHit`.
    fn leave_cloud(&self, world: &Arc<World>) {
        let cloud = Arc::new(AreaEffectCloudEntity::new(
            &vanilla_entities::AREA_EFFECT_CLOUD,
            next_entity_id(),
            self.cloud_position(world),
            Arc::downgrade(world),
        ));
        cloud.configure_as_dragon_breath();

        let entity: SharedEntity = cloud;
        if let Err(error) = world.try_add_entity(entity) {
            log::debug!("failed to spawn dragon breath cloud: {error}");
        }
    }
}

impl Entity for DragonFireballEntity {
    fn base(&self) -> &EntityBase {
        &self.base
    }

    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
    }

    fn tick(&self) {
        self.hurting_projectile_tick();
    }

    fn synced_data(&self) -> Option<&dyn EntitySyncedData> {
        Some(&self.entity_data)
    }

    fn hurt(&self, _world: &World, _source: &DamageSource, _amount: f32) -> bool {
        // Vanilla parity: `AbstractHurtingProjectile.hurtServer` returns false.
        false
    }

    fn restore_owner_reference(&self, owner: &SharedEntity) {
        self.cache_owner_entity(owner);
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_projectile(nbt);
        self.save_hurting_projectile(nbt);
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_projectile(nbt);
        self.load_hurting_projectile(nbt);
    }
}

impl Projectile for DragonFireballEntity {
    fn projectile_base(&self) -> &ProjectileBase {
        &self.projectile_base
    }

    fn on_deflection(&self, by_attack: bool) {
        self.hurting_projectile_on_deflection(by_attack);
    }

    /// Vanilla parity: `DragonFireball.onHit`.
    fn on_hit(&self, hit: &ProjectileHit) {
        self.projectile_on_hit(hit);

        // Vanilla parity: the ball ignores its own dragon, which is what lets
        // the dragon breathe on the ground it is standing over.
        if let ProjectileHit::Entity(entity_hit) = hit
            && self.owned_by(entity_hit.entity.as_ref())
        {
            return;
        }

        let Some(world) = self.level() else {
            return;
        };
        if self.is_removed() {
            return;
        }

        self.leave_cloud(&world);
        // Vanilla parity: the -1 tells the client to skip the sound, which is
        // what a silenced dragon fireball wants.
        let sound_flag = if self.is_silent() { -1 } else { 1 };
        world.level_event(
            PARTICLES_DRAGON_FIREBALL_SPLASH,
            self.block_position(),
            sound_flag,
            None,
        );
        self.set_removed(RemovalReason::Discarded);
    }
}

impl HurtingProjectile for DragonFireballEntity {
    fn hurting_projectile_base(&self) -> &HurtingProjectileBase {
        &self.hurting_projectile_base
    }

    /// Vanilla parity: `DragonFireball.shouldBurn` returns false -- the ball is
    /// purple breath, not fire, and is never drawn alight.
    fn should_burn(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Weak};

    use foton_registry::{init_vanilla_registry, vanilla_entities};
    use foton_utils::{BlockPos, ChunkPos, Direction};
    use glam::DVec3;

    use crate::behavior::init_behaviors;
    use crate::entity::entities::PigEntity;
    use crate::entity::{
        Entity, HurtingProjectile, Projectile, ProjectileHit, RemovalReason, SharedEntity,
        next_entity_id,
    };
    use crate::test_support::{fresh_test_world, insert_ready_full_chunk};
    use crate::world::{ClipHitResult, World};

    use super::DragonFireballEntity;

    fn fireball(world: Weak<World>) -> DragonFireballEntity {
        init_vanilla_registry();
        DragonFireballEntity::new(
            &vanilla_entities::DRAGON_FIREBALL,
            next_entity_id(),
            DVec3::new(8.5, 64.0, 8.5),
            world,
        )
    }

    fn block_hit(at: DVec3) -> ProjectileHit {
        ProjectileHit::Block {
            location: at,
            hit: ClipHitResult {
                location: at,
                direction: Direction::Up,
                block_pos: BlockPos::new(8, 63, 8),
                miss: false,
                inside: false,
                world_border_hit: false,
            },
        }
    }

    #[test]
    fn a_dragon_fireball_never_catches_fire_in_flight() {
        assert!(!fireball(Weak::new()).should_burn());
    }

    #[test]
    fn landing_leaves_a_cloud_behind_and_spends_the_fireball() {
        init_vanilla_registry();
        init_behaviors();

        let world = fresh_test_world("test_dragon_fireball_cloud");
        insert_ready_full_chunk(&world, ChunkPos::new(0, 0));
        let fireball = fireball(Arc::downgrade(&world));

        fireball.on_hit(&block_hit(fireball.position()));

        assert_eq!(fireball.removal_reason(), Some(RemovalReason::Discarded));
        let clouds = world
            .get_entities_in_aabb(&fireball.bounding_box().inflate(8.0))
            .into_iter()
            .filter(|entity| entity.entity_type() == &vanilla_entities::AREA_EFFECT_CLOUD)
            .count();
        assert_eq!(clouds, 1);
    }

    #[test]
    fn the_cloud_settles_on_a_nearby_victim_rather_than_where_the_ball_stopped() {
        init_vanilla_registry();
        init_behaviors();

        let world = fresh_test_world("test_dragon_fireball_snaps_to_victim");
        insert_ready_full_chunk(&world, ChunkPos::new(0, 0));
        let victim: SharedEntity = Arc::new(PigEntity::new(
            &vanilla_entities::PIG,
            next_entity_id(),
            DVec3::new(10.5, 64.0, 8.5),
            Arc::downgrade(&world),
        ));
        world
            .try_add_entity(Arc::clone(&victim))
            .expect("the test chunk is loaded");

        let fireball = fireball(Arc::downgrade(&world));
        fireball.on_hit(&block_hit(fireball.position()));

        let cloud = world
            .get_entities_in_aabb(&fireball.bounding_box().inflate(8.0))
            .into_iter()
            .find(|entity| entity.entity_type() == &vanilla_entities::AREA_EFFECT_CLOUD)
            .expect("the fireball should have left a cloud");
        assert_eq!(cloud.position(), victim.position());
    }
}
