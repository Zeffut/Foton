//! Wither skull.
//!
//! Vanilla parity: `WitherSkull`, on `AbstractHurtingProjectile`. It comes in
//! two forms. The plain skull is what the wither's side heads spit; the
//! dangerous one is the blue skull the middle head charges up, and it differs in
//! two ways: it keeps less of its speed each tick, so it travels slower and
//! visibly homes less, and it eats through blocks a plain skull cannot.
//!
//! Both forms wither what they hit, and a kill heals the wither that threw it,
//! which is the whole reason the fight has a healing phase.

use std::sync::Weak;

use foton_macros::entity_behavior;
use foton_registry::entity_type::EntityTypeRef;
use foton_registry::vanilla_entity_data::WitherSkullEntityData;
use foton_registry::vanilla_game_rules::{MOB_EXPLOSION_DROP_DECAY, MOB_GRIEFING};
use foton_registry::{vanilla_damage_types, vanilla_mob_effects};
use foton_utils::locks::SyncMutex;
use foton_utils::types::Difficulty;
use foton_utils::{DowncastType, DowncastTypeKey};
use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;

use crate::entity::damage::DamageSource;
use crate::entity::{
    Entity, EntityBase, EntityBaseLoad, EntitySyncedData, HurtingProjectile, HurtingProjectileBase,
    MobEffectInstance, Projectile, ProjectileBase, ProjectileHit, RemovalReason, SharedEntity,
};
use crate::world::World;
use crate::world::explosion::{ExplosionBlockInteraction, ExplosionSpec};

/// Velocity a charged skull keeps each tick.
///
/// Vanilla parity: `WitherSkull.getInertia` for a dangerous skull. Lower than
/// the 0.95 of everything else on this branch, so the blue skull crawls.
const DANGEROUS_INERTIA: f64 = 0.73;

/// Damage a skull thrown by a living shooter does.
///
/// Vanilla parity: the `8.0F` of `WitherSkull.onHitEntity`.
const OWNED_DAMAGE: f32 = 8.0;

/// Damage an ownerless skull does.
///
/// Vanilla parity: the `5.0F` magic fallback of `WitherSkull.onHitEntity`, used
/// when the wither that threw it is gone.
const UNOWNED_DAMAGE: f32 = 5.0;

/// Health the shooter gets back for a kill.
///
/// Vanilla parity: the `livingOwner.heal(5.0F)` of `WitherSkull.onHitEntity`.
const KILL_HEAL: f32 = 5.0;

/// Seconds of wither a skull inflicts on normal difficulty.
const NORMAL_WITHER_SECONDS: i32 = 10;

/// Seconds of wither a skull inflicts on hard difficulty.
const HARD_WITHER_SECONDS: i32 = 40;

/// Amplifier of the wither effect a skull inflicts.
///
/// Vanilla parity: the `1` of the `MobEffectInstance` in `onHitEntity`.
const WITHER_AMPLIFIER: i32 = 1;

/// Radius of the blast a skull leaves.
///
/// Vanilla parity: the `1.0F` of `WitherSkull.onHit`, the same for both forms.
const EXPLOSION_RADIUS: f32 = 1.0;

/// A wither's skull.
#[entity_behavior(class = "WitherSkull")]
pub struct WitherSkullEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    entity_data: SyncMutex<WitherSkullEntityData>,
    projectile_base: ProjectileBase,
    hurting_projectile_base: HurtingProjectileBase,
}

// SAFETY: This key is owned by Foton and uniquely identifies `WitherSkullEntity`.
unsafe impl DowncastType for WitherSkullEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:entity/wither_skull");
}

impl WitherSkullEntity {
    /// Creates a plain wither skull.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self {
            base: EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
            entity_data: SyncMutex::new(WitherSkullEntityData::new()),
            projectile_base: ProjectileBase::new(),
            hurting_projectile_base: HurtingProjectileBase::new(),
        }
    }

    /// Creates a wither skull from saved base data.
    #[must_use]
    pub fn from_saved(entity_type: EntityTypeRef, load: EntityBaseLoad) -> Self {
        Self {
            base: EntityBase::from_load(load, entity_type.dimensions),
            entity_type,
            entity_data: SyncMutex::new(WitherSkullEntityData::new()),
            projectile_base: ProjectileBase::new(),
            hurting_projectile_base: HurtingProjectileBase::new(),
        }
    }

    /// Returns whether this is the charged blue skull.
    ///
    /// Vanilla parity: `WitherSkull.isDangerous`.
    #[must_use]
    pub fn is_dangerous(&self) -> bool {
        *self.entity_data.lock().dangerous.get()
    }

    /// Charges or discharges the skull.
    ///
    /// Vanilla parity: `WitherSkull.setDangerous`, called by the wither's middle
    /// head when it has finished charging.
    pub fn set_dangerous(&self, dangerous: bool) {
        self.entity_data.lock().dangerous.set(dangerous);
    }

    /// Withers a living target for as long as the difficulty says.
    ///
    /// Vanilla parity: the difficulty branch of `WitherSkull.onHitEntity`. Easy
    /// and peaceful inflict nothing at all, which is why a wither on easy is a
    /// damage fight rather than an attrition one.
    fn apply_wither(world: &World, target: &SharedEntity) {
        let seconds = match world.difficulty() {
            Difficulty::Normal => NORMAL_WITHER_SECONDS,
            Difficulty::Hard => HARD_WITHER_SECONDS,
            Difficulty::Peaceful | Difficulty::Easy => return,
        };
        let Some(living) = target.as_living_entity() else {
            return;
        };
        living.add_mob_effect(MobEffectInstance::with_duration(
            vanilla_mob_effects::WITHER,
            seconds * 20,
            WITHER_AMPLIFIER,
        ));
    }
}

impl Entity for WitherSkullEntity {
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

    /// Vanilla parity: `WitherSkull.isOnFire` returns false, so the client never
    /// draws flames on a skull even while it is over lava.
    fn is_on_fire(&self) -> bool {
        false
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
        nbt.insert("dangerous", i8::from(self.is_dangerous()));
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_projectile(nbt);
        self.load_hurting_projectile(nbt);
        self.set_dangerous(nbt.byte("dangerous").is_some_and(|value| value != 0));
    }
}

impl Projectile for WitherSkullEntity {
    fn projectile_base(&self) -> &ProjectileBase {
        &self.projectile_base
    }

    fn on_deflection(&self, by_attack: bool) {
        self.hurting_projectile_on_deflection(by_attack);
    }

    /// Vanilla parity: `WitherSkull.onHitEntity`.
    fn on_hit_entity(&self, entity: &SharedEntity, _location: DVec3) {
        let Some(world) = self.level() else {
            return;
        };

        let owner = self.get_owner();
        let living_owner = owner
            .as_ref()
            .filter(|owner| owner.as_living_entity().is_some());

        let was_hurt = if let Some(owner) = living_owner {
            let source = DamageSource::environment(&vanilla_damage_types::WITHER_SKULL)
                .with_direct_entity(self.id())
                .with_causing_entity(owner.id());
            let hurt = entity.hurt(&world, &source, OWNED_DAMAGE);
            if hurt && !Entity::is_alive(entity.as_ref()) {
                // Vanilla parity: the kill heals the wither, not the skull.
                if let Some(living) = owner.as_living_entity() {
                    living.heal(KILL_HEAL);
                }
            }
            // TODO: on a hit that did not kill, vanilla also runs
            // `EnchantmentHelper.doPostAttackEffects`; Foton has no post-attack
            // enchantment dispatch for projectiles yet.
            hurt
        } else {
            entity.hurt(
                &world,
                &DamageSource::environment(&vanilla_damage_types::MAGIC),
                UNOWNED_DAMAGE,
            )
        };

        if was_hurt {
            Self::apply_wither(&world, entity);
        }
    }

    /// Vanilla parity: `WitherSkull.onHit`.
    ///
    /// Deviation: vanilla also overrides `getBlockExplosionResistance` so a
    /// charged skull caps the resistance of anything `WitherBoss.canDestroy`
    /// allows at 0.8, which is how the blue skull chews through obsidian-grade
    /// blocks a plain one bounces off. Foton's `World::explode` has no
    /// per-source resistance hook, so both forms here break exactly the same
    /// blocks.
    fn on_hit(&self, hit: &ProjectileHit) {
        self.projectile_on_hit(hit);

        let Some(world) = self.level() else {
            return;
        };
        if self.is_removed() {
            return;
        }

        // Vanilla passes `ExplosionInteraction.MOB`, which the level resolves
        // through the `mobGriefing` rule; the fire flag is hard false, so a
        // skull never leaves flames the way a ghast's fireball does.
        let interaction = if world.get_game_rule(&MOB_GRIEFING) {
            world.explosion_destroy_type(&MOB_EXPLOSION_DROP_DECAY)
        } else {
            ExplosionBlockInteraction::Keep
        };
        world.explode(
            ExplosionSpec::new(
                Some(self.id()),
                self.get_owner().map(|owner| owner.id()),
                None,
                EXPLOSION_RADIUS,
                false,
                interaction,
            ),
            self.position(),
        );
        self.set_removed(RemovalReason::Discarded);
    }
}

impl HurtingProjectile for WitherSkullEntity {
    fn hurting_projectile_base(&self) -> &HurtingProjectileBase {
        &self.hurting_projectile_base
    }

    /// Vanilla parity: `WitherSkull.getInertia`.
    fn get_inertia(&self) -> f64 {
        if self.is_dangerous() {
            DANGEROUS_INERTIA
        } else {
            0.95
        }
    }

    /// Vanilla parity: `WitherSkull.shouldBurn` returns false.
    fn should_burn(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Weak};

    use foton_registry::{init_vanilla_registry, vanilla_entities, vanilla_mob_effects};
    use foton_utils::ChunkPos;
    use foton_utils::types::Difficulty;
    use glam::DVec3;

    use crate::behavior::init_behaviors;
    use crate::entity::entities::PigEntity;
    use crate::entity::{Entity, HurtingProjectile, Projectile, SharedEntity, next_entity_id};
    use crate::test_support::{fresh_test_world, insert_ready_full_chunk};
    use crate::world::World;

    use super::{DANGEROUS_INERTIA, WitherSkullEntity};

    fn skull(world: Weak<World>) -> WitherSkullEntity {
        init_vanilla_registry();
        WitherSkullEntity::new(
            &vanilla_entities::WITHER_SKULL,
            next_entity_id(),
            DVec3::new(8.5, 64.0, 8.5),
            world,
        )
    }

    #[test]
    fn a_charged_skull_gives_up_more_speed_each_tick_than_a_plain_one() {
        let skull = skull(Weak::new());

        assert!((skull.get_inertia() - 0.95).abs() < 1.0e-9);

        skull.set_dangerous(true);
        assert!((skull.get_inertia() - DANGEROUS_INERTIA).abs() < 1.0e-9);
    }

    #[test]
    fn a_skull_never_shows_flames_even_though_it_flies_out_of_the_nether() {
        assert!(!skull(Weak::new()).is_on_fire());
        assert!(!skull(Weak::new()).should_burn());
    }

    #[test]
    fn a_hit_on_normal_difficulty_withers_the_target_for_ten_seconds() {
        init_vanilla_registry();
        init_behaviors();

        let world = fresh_test_world("test_wither_skull_effect");
        insert_ready_full_chunk(&world, ChunkPos::new(0, 0));
        let target: SharedEntity = Arc::new(PigEntity::new(
            &vanilla_entities::PIG,
            next_entity_id(),
            DVec3::new(8.5, 64.0, 8.5),
            Arc::downgrade(&world),
        ));
        world
            .try_add_entity(Arc::clone(&target))
            .expect("the test chunk is loaded");

        let skull = skull(Arc::downgrade(&world));
        skull.on_hit_entity(&target, target.position());

        let living = target.as_living_entity().expect("a pig is a living entity");
        let effect = living
            .mob_effect(vanilla_mob_effects::WITHER)
            .expect("a wither skull withers what it hits");
        assert_eq!(effect.duration(), 200);
        assert_eq!(effect.amplifier(), 1);
    }

    #[test]
    fn hard_difficulty_withers_four_times_as_long() {
        init_vanilla_registry();
        init_behaviors();

        let world = fresh_test_world("test_wither_skull_hard");
        world.set_difficulty(Difficulty::Hard);
        insert_ready_full_chunk(&world, ChunkPos::new(0, 0));
        let position = DVec3::new(8.5, 64.0, 8.5);
        let target: SharedEntity = Arc::new(PigEntity::new(
            &vanilla_entities::PIG,
            next_entity_id(),
            position,
            Arc::downgrade(&world),
        ));
        world
            .try_add_entity(Arc::clone(&target))
            .expect("the test chunk is loaded");

        let skull = WitherSkullEntity::new(
            &vanilla_entities::WITHER_SKULL,
            next_entity_id(),
            position,
            Arc::downgrade(&world),
        );
        skull.on_hit_entity(&target, position);

        let living = target.as_living_entity().expect("a pig is a living entity");
        let effect = living
            .mob_effect(vanilla_mob_effects::WITHER)
            .expect("a wither skull withers what it hits");
        assert_eq!(effect.duration(), 800);
    }
}
