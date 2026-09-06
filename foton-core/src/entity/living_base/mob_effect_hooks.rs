//! Vanilla `MobEffect.onMobHurt` and `MobEffect.onMobRemoved`.
//!
//! `applyEffectTick` was the only effect hook Foton had, which left four
//! effects inert: Infested spawned no silverfish when its carrier was hurt,
//! Wind Charged burst nothing on death, Weaving laid no cobwebs and Oozing
//! dropped no slimes. Those four are the whole payoff of the trial-chamber and
//! ominous-vault loop, so without them the reward is decorative.
//!
//! The hurt hook takes neither the damage source nor the amount, which vanilla
//! passes: `InfestedMobEffect` is the only implementation and it reads neither.
//! A future effect that needs them can widen the signature -- the call site has
//! both in hand.

use std::f64::consts::FRAC_PI_2;
use std::ops::RangeInclusive;
use std::sync::Arc;

use foton_protocol::packets::game::SoundSource;
use foton_registry::blocks::block_state_ext::BlockStateExt as _;
use foton_registry::entity_type::EntityTypeRef;
use foton_registry::particle_type::ParticleData;
use foton_registry::{
    sound_events, vanilla_blocks, vanilla_entities, vanilla_game_rules, vanilla_mob_effects,
    vanilla_particle_types,
};
use foton_utils::random::weighted_list::WeightedList;
use foton_utils::types::UpdateFlags;
use foton_utils::{BlockPos, Direction, Downcast as _};
use glam::DVec3;

use crate::entity::{ENTITIES, entities::SlimeEntity};
use crate::entity::{LivingEntity, RemovalReason, SharedEntity, next_entity_id};
use crate::world::explosion::{ExplosionBlockInteraction, ExplosionSpec};
use crate::world::{LevelAccessor as _, LevelReader as _, World};

use super::MobEffectInstance;

/// `InfestedMobEffect`'s `chanceToSpawn`, from its `MobEffects` registration.
const INFESTED_CHANCE_TO_SPAWN: f32 = 0.1;
/// `InfestedMobEffect`'s `spawnedCount`: `Mth.randomBetweenInclusive(random, 1, 2)`.
const INFESTED_SPAWN_COUNT: RangeInclusive<i32> = 1..=2;
/// The `0.3F` and `1.0F, 1.5F, 1.0F` of `InfestedMobEffect.spawnSilverfish`.
const INFESTED_LAUNCH_SPEED: f64 = 0.3;
const INFESTED_LAUNCH_VERTICAL_BOOST: f64 = 1.5;

/// `WeavingMobEffect`'s `maxCobwebs`: `Mth.randomBetweenInclusive(random, 2, 3)`.
const WEAVING_MAX_COBWEBS: RangeInclusive<i32> = 2..=3;
/// The `15` and `1` of `BlockPos.randomInCube(random, 15, pos, 1)`.
const WEAVING_CANDIDATES: usize = 15;
const WEAVING_CUBE_RADIUS: i32 = 1;
/// `LevelEvent.PARTICLES_AND_SOUND_WAX_ON`'s neighbour in vanilla's numbering:
/// the `3018` of `WeavingMobEffect.spawnCobwebsRandomlyAround`.
const WEAVING_COBWEB_LEVEL_EVENT: i32 = 3018;

/// `OozingMobEffect`'s `spawnedCount`, which is the constant 2.
const OOZING_SLIMES_REQUESTED: i32 = 2;
/// `OozingMobEffect.RADIUS_TO_CHECK_SLIMES` and `SLIME_SIZE`.
const OOZING_SLIME_SEARCH_RADIUS: f64 = 2.0;
const OOZING_SLIME_SIZE: i32 = 2;

impl MobEffectInstance {
    /// Vanilla parity: `MobEffectInstance.onMobHurt`.
    pub(crate) fn on_mob_hurt<E: LivingEntity + ?Sized>(&self, world: &Arc<World>, entity: &E) {
        if self.effect == vanilla_mob_effects::INFESTED {
            infested_spawn_silverfish(world, entity);
        }
    }

    /// Vanilla parity: `MobEffectInstance.onMobRemoved`.
    ///
    /// All three implementations start by testing the reason, so the test lives
    /// here once rather than three times.
    pub(crate) fn on_mob_removed<E: LivingEntity + ?Sized>(
        &self,
        world: &Arc<World>,
        entity: &E,
        reason: RemovalReason,
    ) {
        if reason != RemovalReason::Killed {
            return;
        }

        if self.effect == vanilla_mob_effects::WIND_CHARGED {
            wind_charged_burst(world, entity);
        }
        if self.effect == vanilla_mob_effects::WEAVING {
            weaving_spawn_cobwebs(world, entity);
        }
        if self.effect == vanilla_mob_effects::OOZING {
            oozing_spawn_slimes(world, entity);
        }
    }
}

/// The middle of the carrier's height, which all three spawn points share.
fn effect_origin<E: LivingEntity + ?Sized>(entity: &E) -> DVec3 {
    let position = entity.position();
    DVec3::new(
        position.x,
        position.y + f64::from(entity.bounding_box().height() as f32) / 2.0,
        position.z,
    )
}

/// Vanilla parity: `InfestedMobEffect.onMobHurt` and its `spawnSilverfish`.
fn infested_spawn_silverfish<E: LivingEntity + ?Sized>(world: &Arc<World>, entity: &E) {
    if rand::random::<f32>() > INFESTED_CHANCE_TO_SPAWN {
        return;
    }

    let origin = effect_origin(entity);
    let count = rand::random_range(INFESTED_SPAWN_COUNT);
    for _ in 0..count {
        // Vanilla throws each one along the carrier's look, turned by up to a
        // quarter turn either way and given half again as much lift, so a
        // hurt mob sprays them forward rather than stacking them.
        let yaw_offset = rand::random_range(-FRAC_PI_2..=FRAC_PI_2);
        let look = entity.look_angle() * INFESTED_LAUNCH_SPEED;
        let lifted = DVec3::new(look.x, look.y * INFESTED_LAUNCH_VERTICAL_BOOST, look.z);
        let (sin, cos) = yaw_offset.sin_cos();
        let velocity = DVec3::new(
            lifted.x * cos + lifted.z * sin,
            lifted.y,
            lifted.z * cos - lifted.x * sin,
        );

        let Some(silverfish) = spawn_at(world, &vanilla_entities::SILVERFISH, origin, |_| ())
        else {
            return;
        };
        silverfish.set_velocity(velocity);
        world.play_sound_at(
            &sound_events::ENTITY_SILVERFISH_HURT,
            SoundSource::Hostile,
            silverfish.position(),
            1.0,
            1.0,
            None,
        );
    }
}

/// Vanilla parity: `WindChargedMobEffect.onMobRemoved`.
fn wind_charged_burst<E: LivingEntity + ?Sized>(world: &Arc<World>, entity: &E) {
    // `3.0F + random.nextFloat() * 2.0F`.
    let gust_strength = 3.0_f32 + rand::random::<f32>() * 2.0;
    world.explode_sparing(
        ExplosionSpec {
            direct_entity_id: Some(entity.id()),
            causing_entity_id: None,
            // Vanilla passes a null damage source here, as the wind charge does.
            damage_source: None,
            radius: gust_strength,
            fire: false,
            interaction: ExplosionBlockInteraction::Keep,
            // `SimpleExplosionDamageCalculator(true, false, ...)`: it shoves
            // what it reaches and hurts none of it.
            damages_entities: false,
            knockback_multiplier: 1.0,
            small_particle: ParticleData::simple(&vanilla_particle_types::GUST_EMITTER_SMALL),
            large_particle: ParticleData::simple(&vanilla_particle_types::GUST_EMITTER_LARGE),
            // Vanilla parity: `WeightedList.of()`. A gust breaks nothing, so it
            // has no debris to throw.
            block_particles: WeightedList::empty(),
            sound: &sound_events::ENTITY_BREEZE_WIND_BURST,
        },
        effect_origin(entity),
        &|_pos| true,
    );
}

/// Vanilla parity: `WeavingMobEffect.onMobRemoved` and `spawnCobwebsRandomlyAround`.
fn weaving_spawn_cobwebs<E: LivingEntity + ?Sized>(world: &Arc<World>, entity: &E) {
    // Vanilla lets a player weave regardless of the game rule, and a mob only
    // when it is allowed to grief.
    let is_player = entity.entity_type() == &vanilla_entities::PLAYER;
    if !is_player && !world.get_game_rule(&vanilla_game_rules::MOB_GRIEFING) {
        return;
    }

    let origin = entity.block_position();
    let wanted = rand::random_range(WEAVING_MAX_COBWEBS);
    let mut chosen: Vec<BlockPos> = Vec::new();

    for _ in 0..WEAVING_CANDIDATES {
        let candidate = BlockPos::new(
            origin.x() + rand::random_range(-WEAVING_CUBE_RADIUS..=WEAVING_CUBE_RADIUS),
            origin.y() + rand::random_range(-WEAVING_CUBE_RADIUS..=WEAVING_CUBE_RADIUS),
            origin.z() + rand::random_range(-WEAVING_CUBE_RADIUS..=WEAVING_CUBE_RADIUS),
        );
        if chosen.contains(&candidate) {
            continue;
        }

        let below = candidate.below();
        if !world.get_block_state(candidate).is_replaceable() {
            continue;
        }
        if !world.is_face_sturdy(world.get_block_state(below), below, Direction::Up) {
            continue;
        }

        chosen.push(candidate);
        if i32::try_from(chosen.len()).is_ok_and(|count| count >= wanted) {
            break;
        }
    }

    for pos in chosen {
        world.set_block_state(
            pos,
            vanilla_blocks::COBWEB.default_state(),
            UpdateFlags::UPDATE_ALL,
        );
        world.level_event(WEAVING_COBWEB_LEVEL_EVENT, pos, 0, None);
    }
}

/// Vanilla parity: `OozingMobEffect.onMobRemoved` and `numberOfSlimesToSpawn`.
fn oozing_spawn_slimes<E: LivingEntity + ?Sized>(world: &Arc<World>, entity: &E) {
    let max_cramming = world.get_game_rule(&vanilla_game_rules::MAX_ENTITY_CRAMMING);
    let nearby = if max_cramming < 1 {
        // Vanilla never counts when cramming is off, and neither does this:
        // the search is the expensive half.
        0
    } else {
        nearby_slime_count(world, entity, max_cramming)
    };
    let count = number_of_slimes_to_spawn(max_cramming, nearby, OOZING_SLIMES_REQUESTED);

    let position = entity.position();
    let origin = DVec3::new(position.x, position.y + 0.5, position.z);
    for _ in 0..count {
        // Vanilla sizes the slime before it is positioned and added, so the
        // bounding box it lands with is the one it keeps.
        let spawned = spawn_at(world, &vanilla_entities::SLIME, origin, |slime| {
            if let Some(slime) = slime.downcast_ref::<SlimeEntity>() {
                slime.set_spawn_size(OOZING_SLIME_SIZE);
            }
        });
        if spawned.is_none() {
            return;
        }
    }
}

/// Vanilla parity: `OozingMobEffect.numberOfSlimesToSpawn`.
///
/// Kept separate because vanilla keeps it separate, and for the same reason:
/// it is the one part of the effect that can be checked without a world.
///
/// `Mth.clamp(0, maxEntityCramming - nearbySlimes, numberRequested)` is
/// `min(max(0, difference), requested)`, which is what `clamp` spells here --
/// worth stating because the Java call reads as though the room left were the
/// bound rather than the value.
const fn number_of_slimes_to_spawn(
    max_entity_cramming: i32,
    nearby_slimes: i32,
    requested: i32,
) -> i32 {
    if max_entity_cramming < 1 {
        return requested;
    }
    let room_left = max_entity_cramming - nearby_slimes;
    if room_left < 0 {
        return 0;
    }
    if room_left > requested {
        return requested;
    }
    room_left
}

/// Vanilla parity: `OozingMobEffect.NearbySlimes.closeTo`, capped at `maxResults`.
fn nearby_slime_count<E: LivingEntity + ?Sized>(
    world: &Arc<World>,
    entity: &E,
    max_results: i32,
) -> i32 {
    let search = entity.bounding_box().inflate(OOZING_SLIME_SEARCH_RADIUS);
    let own_id = entity.id();
    let found = world
        .entity_manager()
        .get_entities_in_aabb_matching(&search, |candidate| {
            candidate.id() != own_id && candidate.entity_type() == &vanilla_entities::SLIME
        });
    i32::try_from(found.len())
        .unwrap_or(i32::MAX)
        .min(max_results)
}

/// Creates one entity at `position`, facing a random way, and puts it in the world.
///
/// Vanilla parity: the `create(level, EntitySpawnReason.TRIGGERED)`,
/// `snapTo(x, y, z, level.getRandom().nextFloat() * 360.0F, 0.0F)` and
/// `addFreshEntity` that both spawn helpers share. `configure` runs between
/// creation and insertion, which is where vanilla puts `slime.setSize`.
///
/// Neither spawner calls `finalizeSpawn`, and that is not an omission:
/// finalizing a slime rolls its size at random, which would throw away the
/// `setSize(2, true)` the effect exists to perform.
fn spawn_at(
    world: &Arc<World>,
    entity_type: EntityTypeRef,
    position: DVec3,
    configure: impl FnOnce(&SharedEntity),
) -> Option<SharedEntity> {
    let entity = ENTITIES.create(
        entity_type,
        next_entity_id(),
        position,
        Arc::downgrade(world),
    )?;
    configure(&entity);
    entity.set_rotation((rand::random::<f32>() * 360.0, 0.0));
    world.try_add_entity(Arc::clone(&entity)).ok()?;
    Some(entity)
}

#[cfg(test)]
mod tests {
    use super::{OOZING_SLIMES_REQUESTED, number_of_slimes_to_spawn};

    #[test]
    fn oozing_slime_count_matches_vanillas_clamp() {
        // `Mth.clamp(0, maxEntityCramming - nearbySlimes, numberRequested)` is
        // `min(max(0, difference), requested)`. Reading the Java call as though
        // the room left were a bound would give 24 slimes on an empty default
        // world instead of two, so the shape is pinned here.
        let requested = OOZING_SLIMES_REQUESTED;

        // Cramming off: vanilla does not clamp at all.
        assert_eq!(number_of_slimes_to_spawn(0, 99, requested), requested);
        assert_eq!(number_of_slimes_to_spawn(-1, 0, requested), requested);

        // Plenty of room: still only what was asked for.
        assert_eq!(number_of_slimes_to_spawn(24, 0, requested), requested);

        // Room for exactly one.
        assert_eq!(number_of_slimes_to_spawn(24, 23, requested), 1);

        // Full, and over-full.
        assert_eq!(number_of_slimes_to_spawn(24, 24, requested), 0);
        assert_eq!(number_of_slimes_to_spawn(24, 30, requested), 0);
    }
}
