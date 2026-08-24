//! The skeleton trap that a thunderstorm leaves lying around.
//!
//! Vanilla parity: `net.minecraft.world.entity.animal.equine.SkeletonTrapGoal`.
//! A lone skeleton horse standing in a storm is bait: walk within ten blocks and
//! it calls down a bolt, tames itself, and four mounted skeletons ride out.

use std::sync::Arc;

use glam::DVec3;
use steel_registry::item_stack::ItemStack;
use steel_registry::{vanilla_entities, vanilla_items};
use steel_utils::Downcast as _;
use steel_utils::random::{Random as _, legacy_random::LegacyRandom};

use crate::entity::ai::goal::{Goal, GoalControls};
use crate::entity::entities::LightningBoltEntity;
use crate::entity::entities::mobs::passive::equine::SkeletonHorseEntity;
use crate::entity::{
    AbstractHorse, AgeableMob, ENTITIES, Entity, EntitySpawnReason, LivingEntity, PathfinderMob,
    SharedEntity, next_entity_id,
};
use crate::inventory::equipment::EquipmentSlot;
use crate::world::World;

/// How close a player has to come before the trap springs.
///
/// Vanilla parity: the `hasNearbyAlivePlayer(..., 10.0)` of `canUse`.
const TRIGGER_RANGE: f64 = 10.0;

/// How many extra horses ride out beside the one that was the bait.
///
/// Vanilla parity: the `for (int i = 0; i < 3; i++)` of `tick`.
const EXTRA_HORSES: usize = 3;

/// Ticks the trap's spawns cannot be hurt for.
///
/// Vanilla parity: the `invulnerableTime = 60` of `createHorse`/`createSkeleton`.
const SPAWN_INVULNERABILITY_TICKS: i32 = 60;

/// How far the extra horses are shoved apart.
///
/// Vanilla parity: the `random.triangle(0.0, 1.1485)` of `tick`.
const SCATTER_PUSH: f64 = 1.148_5;

/// Springs a lightning trap when a player walks close.
///
/// Vanilla parity: `SkeletonTrapGoal`.
pub struct SkeletonTrapGoal;

impl SkeletonTrapGoal {
    #[must_use]
    pub(crate) const fn new() -> Self {
        Self
    }

    /// Vanilla parity: `SkeletonTrapGoal.createHorse`.
    fn create_horse(world: &Arc<World>, at: DVec3) -> Option<SharedEntity> {
        let entity = ENTITIES.create(
            &vanilla_entities::SKELETON_HORSE,
            next_entity_id(),
            at,
            Arc::downgrade(world),
        )?;
        let mob = entity.as_mob()?;
        mob.finalize_spawn(world, EntitySpawnReason::Triggered, None);
        let living = entity.as_living_entity()?;
        living
            .living_base()
            .set_invulnerable_time(SPAWN_INVULNERABILITY_TICKS);
        mob.set_persistence_required();

        let horse = entity.as_abstract_horse()?;
        horse.set_tamed(true);
        entity.as_ageable_mob()?.set_age(0);
        Some(entity)
    }

    /// Vanilla parity: `SkeletonTrapGoal.createSkeleton`.
    ///
    /// MISSING FOUNDATION: vanilla re-rolls the skeleton's enchantments from the
    /// `MOB_SPAWN_EQUIPMENT` provider here. Steel has no enchantment provider
    /// registry, so the rider keeps whatever `finalizeSpawn` already gave it and
    /// only the iron helmet is added.
    fn create_skeleton(world: &Arc<World>, at: DVec3) -> Option<SharedEntity> {
        let entity = ENTITIES.create(
            &vanilla_entities::SKELETON,
            next_entity_id(),
            at,
            Arc::downgrade(world),
        )?;
        let mob = entity.as_mob()?;
        mob.finalize_spawn(world, EntitySpawnReason::Triggered, None);
        let living = entity.as_living_entity()?;
        living
            .living_base()
            .set_invulnerable_time(SPAWN_INVULNERABILITY_TICKS);
        mob.set_persistence_required();

        if !living.has_item_in_slot(EquipmentSlot::Head) {
            living.living_base().equipment().lock().set(
                EquipmentSlot::Head,
                ItemStack::new(&vanilla_items::IRON_HELMET),
            );
        }
        Some(entity)
    }

    /// Adds a horse and its rider to the world and seats one on the other.
    ///
    /// Vanilla parity: `Level.addFreshEntityWithPassengers`, which Steel has no
    /// equivalent of; boarding needs both entities present, so they are added
    /// first and mounted after.
    fn add_mounted_pair(world: &Arc<World>, horse: SharedEntity, skeleton: SharedEntity) {
        let skeleton_id = skeleton.id();
        if let Err(error) = world.try_add_entity(horse.clone()) {
            log::debug!("skeleton trap could not add its horse: {error}");
            return;
        }
        if let Err(error) = world.try_add_entity(skeleton) {
            log::debug!("skeleton trap could not add its rider: {error}");
            return;
        }
        let Some(skeleton) = world.get_entity_by_id(skeleton_id) else {
            return;
        };
        skeleton.start_riding(&horse);
    }
}

impl Goal for SkeletonTrapGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::EMPTY
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        let Some(horse) = mob.downcast_ref::<SkeletonHorseEntity>() else {
            return false;
        };
        if !horse.is_trap() {
            return false;
        }
        let Some(world) = mob.level() else {
            return false;
        };

        world
            .nearest_player(mob.position(), TRIGGER_RANGE, |player| {
                !player.is_spectator() && LivingEntity::is_alive(player)
            })
            .is_some()
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        let Some(horse) = mob.downcast_ref::<SkeletonHorseEntity>() else {
            return;
        };
        let Some(world) = mob.level() else {
            return;
        };

        horse.set_trap(false);
        horse.set_tamed(true);
        horse.set_age(0);

        let position = mob.position();
        let bolt = Arc::new(LightningBoltEntity::new(
            &vanilla_entities::LIGHTNING_BOLT,
            next_entity_id(),
            position,
            Arc::downgrade(&world),
        ));
        bolt.set_visual_only(true);
        let bolt_entity: SharedEntity = bolt;
        if let Err(error) = world.try_add_entity(bolt_entity) {
            log::debug!("skeleton trap could not call down its bolt: {error}");
            return;
        }

        let Some(rider) = Self::create_skeleton(&world, position) else {
            return;
        };
        let Some(bait) = world.get_entity_by_id(mob.id()) else {
            return;
        };
        let rider_id = rider.id();
        if let Err(error) = world.try_add_entity(rider) {
            log::debug!("skeleton trap could not add its first rider: {error}");
            return;
        }
        if let Some(rider) = world.get_entity_by_id(rider_id) {
            rider.start_riding(&bait);
        }

        let mut random = LegacyRandom::from_seed(rand::random());
        for _ in 0..EXTRA_HORSES {
            let Some(other_horse) = Self::create_horse(&world, position) else {
                continue;
            };
            let Some(other_skeleton) = Self::create_skeleton(&world, position) else {
                continue;
            };
            other_horse.push_impulse(DVec3::new(
                random.triangle(0.0, SCATTER_PUSH),
                0.0,
                random.triangle(0.0, SCATTER_PUSH),
            ));
            Self::add_mounted_pair(&world, other_horse, other_skeleton);
        }
    }
}
