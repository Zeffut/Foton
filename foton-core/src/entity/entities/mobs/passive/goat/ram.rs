//! The goat's ram: `RamTarget` and `PrepareRamNearestTarget`.
//!
//! Both are `Behavior<Goat>` in vanilla -- `PrepareRamNearestTarget` is generic
//! over `PathfinderMob` there, but the goat is its only user, so both live here
//! rather than in the shared behavior module.

use foton_registry::blocks::block_state_ext::BlockStateExt as _;
use foton_registry::sound_event::SoundEventRef;
use foton_registry::vanilla_block_tags::BlockTag;
use foton_registry::vanilla_game_rules::MOB_GRIEFING;
use foton_registry::{
    sound_events, vanilla_attributes, vanilla_damage_types, vanilla_entities, vanilla_mob_effects,
};
use foton_utils::entity_events::EntityStatus;
use foton_utils::{BlockPos, Downcast as _};
use glam::DVec3;

use super::GoatEntity;
use crate::entity::SharedEntity;
use crate::entity::ai::brain::behavior::{BrainContext, TimedBehavior};
use crate::entity::ai::brain::memory::{
    MemoryModuleId, MemoryStatus, WalkTarget, memory_module_types,
};
use crate::entity::ai::brain::position_tracker::PositionTracker;
use crate::entity::damage::DamageSource;
use crate::entity::mob::PathfinderMob;
use crate::entity::{AgeableMob, Entity, LivingEntity};
use crate::world::World;
use std::sync::Arc;

/// Vanilla parity: `RamTarget.TIME_OUT_DURATION`.
const RAM_TIME_OUT_DURATION: i32 = 200;

/// Vanilla parity: `RamTarget.RAM_SPEED_FORCE_FACTOR`.
const RAM_SPEED_FORCE_FACTOR: f64 = 1.65;

/// Vanilla parity: the `0.2F` and `3.0F` the ram's speed factor is clamped to.
const RAM_SPEED_FACTOR_MIN: f64 = 0.2;

/// Vanilla parity: the upper half of the same clamp.
const RAM_SPEED_FACTOR_MAX: f64 = 3.0;

/// Vanilla parity: the `0.25F` each level of Speed or Slowness is worth.
const RAM_SPEED_PER_EFFECT_LEVEL: f64 = 0.25;

/// Vanilla parity: the `0.25` a walk target must be within to count as reached.
const RAM_TARGET_REACHED_DISTANCE: f64 = 0.25;

/// Vanilla parity: the `2.5` an adult goat rams with.
const RAM_KNOCKBACK_ADULT: f64 = 2.5;

/// Vanilla parity: the `1.0` a kid rams with.
const RAM_KNOCKBACK_BABY: f64 = 1.0;

/// Vanilla parity: the `3.0F` speed of `new RamTarget(..., 3.0F, ...)`.
const RAM_WALK_SPEED: f64 = 3.0;

/// Charges the position in `RAM_TARGET`, hurting whatever it runs into.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.behavior.RamTarget`.
pub struct RamTarget {
    entry_condition: [(MemoryModuleId, MemoryStatus); 2],
    /// Vanilla parity: the `getTimeBetweenRams` sampled into `RAM_COOLDOWN_TICKS`.
    time_between_rams: fn(&GoatEntity) -> (i32, i32),
    /// Vanilla parity: the `ramDirection` field, set in `start`.
    ram_direction: DVec3,
}

impl RamTarget {
    /// Vanilla parity: `new RamTarget(...)` as `GoatAi` builds it.
    #[must_use]
    pub fn new(time_between_rams: fn(&GoatEntity) -> (i32, i32)) -> Self {
        Self {
            entry_condition: [
                (
                    memory_module_types::RAM_COOLDOWN_TICKS.id(),
                    MemoryStatus::ValueAbsent,
                ),
                (
                    memory_module_types::RAM_TARGET.id(),
                    MemoryStatus::ValuePresent,
                ),
            ],
            time_between_rams,
            ram_direction: DVec3::ZERO,
        }
    }

    /// Vanilla parity: `RamTarget.hasRammedHornBreakingBlock`.
    fn has_rammed_horn_breaking_block(ctx: &BrainContext<'_>, goat: &GoatEntity) -> bool {
        let horizontal = (goat.velocity() * DVec3::new(1.0, 0.0, 1.0)).normalize_or_zero();
        let ahead = goat.position() + horizontal;
        let facing = BlockPos::containing(ahead.x, ahead.y, ahead.z);
        let world = ctx.world();
        [facing, facing.above()].into_iter().any(|pos| {
            world
                .get_block_state(pos)
                .get_block()
                .has_tag(&BlockTag::SNAPS_GOAT_HORN)
        })
    }

    /// Vanilla parity: `RamTarget.finishRam`.
    fn finish_ram(&self, ctx: &BrainContext<'_>, goat: &GoatEntity) {
        goat.broadcast_entity_event(EntityStatus::EndRam);
        let (min, max) = (self.time_between_rams)(goat);
        let cooldown = if min >= max {
            min
        } else {
            min + rand::random_range(0..=(max - min))
        };
        ctx.brain()
            .set_memory(memory_module_types::RAM_COOLDOWN_TICKS, cooldown);
        ctx.brain()
            .erase_memory(memory_module_types::RAM_TARGET.id());
    }
}

impl TimedBehavior for RamTarget {
    fn debug_name(&self) -> &'static str {
        "RamTarget"
    }

    fn entry_condition(&self) -> &[(MemoryModuleId, MemoryStatus)] {
        &self.entry_condition
    }

    fn duration(&self) -> (i32, i32) {
        (RAM_TIME_OUT_DURATION, RAM_TIME_OUT_DURATION)
    }

    fn check_extra_start_conditions(&mut self, ctx: &BrainContext<'_>) -> bool {
        ctx.brain()
            .has_memory_value(memory_module_types::RAM_TARGET.id())
    }

    fn can_still_use(&mut self, ctx: &BrainContext<'_>) -> bool {
        ctx.brain()
            .has_memory_value(memory_module_types::RAM_TARGET.id())
    }

    fn start(&mut self, ctx: &BrainContext<'_>) {
        let Some(goat) = ctx.mob().downcast_ref::<GoatEntity>() else {
            return;
        };
        let Some(target) = ctx.brain().get_memory(memory_module_types::RAM_TARGET) else {
            return;
        };
        let here = goat.position();
        self.ram_direction =
            DVec3::new(here.x - target.x, 0.0, here.z - target.z).normalize_or_zero();
        ctx.brain().set_memory(
            memory_module_types::WALK_TARGET,
            WalkTarget::of_position(target, RAM_WALK_SPEED, 0),
        );
    }

    fn tick(&mut self, ctx: &BrainContext<'_>) {
        let Some(goat) = ctx.mob().downcast_ref::<GoatEntity>() else {
            return;
        };
        let hit = ctx
            .world()
            .get_entities_in_aabb_matching(&goat.bounding_box(), |entity| {
                entity.id() != goat.id() && entity.as_living_entity().is_some()
            })
            .into_iter()
            .next();

        if let Some(target) = hit {
            let Some(living) = target.as_living_entity() else {
                return;
            };
            #[expect(
                clippy::cast_possible_truncation,
                reason = "an attack-damage attribute, immediately used as damage"
            )]
            let damage = goat
                .attributes()
                .lock()
                .required_value(vanilla_attributes::ATTACK_DAMAGE) as f32;
            let source = DamageSource::environment(&vanilla_damage_types::MOB_ATTACK_NO_AGGRO);
            target.hurt(ctx.world(), &source, damage);

            // Vanilla parity: the Speed/Slowness term of `RamTarget.tick`.
            let speed_levels = goat
                .mob_effect(vanilla_mob_effects::SPEED)
                .map_or(0, |effect| effect.amplifier() + 1);
            let slow_levels = goat
                .mob_effect(vanilla_mob_effects::SLOWNESS)
                .map_or(0, |effect| effect.amplifier() + 1);
            let boost = RAM_SPEED_PER_EFFECT_LEVEL * f64::from(speed_levels - slow_levels);
            let factor = (f64::from(LivingEntity::get_speed(goat)) * RAM_SPEED_FORCE_FACTOR)
                .clamp(RAM_SPEED_FACTOR_MIN, RAM_SPEED_FACTOR_MAX)
                + boost;
            let force = if AgeableMob::is_baby(goat) {
                RAM_KNOCKBACK_BABY
            } else {
                RAM_KNOCKBACK_ADULT
            };
            // MISSING FOUNDATION: vanilla hands `knockback` the damage source and
            // the damage so `applyItemBlocking` can halve it. Foton's takes only
            // the power and the direction, so a raised shield does not soften a
            // ram the way it does in vanilla.
            living.knockback(factor * force, self.ram_direction.x, self.ram_direction.z);
            self.finish_ram(ctx, goat);
            goat.play_sound(impact_sound(goat), 1.0, 1.0);
        } else if Self::has_rammed_horn_breaking_block(ctx, goat) {
            goat.play_sound(impact_sound(goat), 1.0, 1.0);
            if goat.drop_horn() {
                goat.play_sound(&sound_events::ENTITY_GOAT_HORN_BREAK, 1.0, 1.0);
            }
            self.finish_ram(ctx, goat);
        } else {
            let walk = ctx.brain().get_memory(memory_module_types::WALK_TARGET);
            let ram = ctx.brain().get_memory(memory_module_types::RAM_TARGET);
            let lost_or_reached = match (walk, ram) {
                (Some(walk), Some(ram)) => walk
                    .target()
                    .current_position()
                    .is_none_or(|at| at.distance(ram) < RAM_TARGET_REACHED_DISTANCE),
                _ => true,
            };
            if lost_or_reached {
                self.finish_ram(ctx, goat);
            }
        }
    }
}

/// Vanilla parity: the impact sound `GoatAi` picks by goat kind.
fn impact_sound(goat: &GoatEntity) -> SoundEventRef {
    if goat.is_screaming_goat() {
        &sound_events::ENTITY_GOAT_SCREAMING_RAM_IMPACT
    } else {
        &sound_events::ENTITY_GOAT_RAM_IMPACT
    }
}

/// Vanilla parity: `PrepareRamNearestTarget.TIME_OUT_DURATION`.
const PREPARE_TIME_OUT_DURATION: i32 = 160;

/// Vanilla parity: the `0.5` half-block offset of `getEdgeOfBlock`.
const RAM_EDGE_OFFSET: f64 = 0.5;

/// What the goat has picked to charge, and where it will start from.
///
/// Vanilla parity: `PrepareRamNearestTarget.RamCandidate`.
struct RamCandidate {
    start_position: BlockPos,
    target_position: BlockPos,
    target: SharedEntity,
}

/// Walks to a run-up spot, waits there, then writes `RAM_TARGET`.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.behavior.PrepareRamNearestTarget`.
/// Vanilla makes it generic over `PathfinderMob`; the goat is its only user, so
/// this one is written for the goat.
pub struct PrepareRamNearestTarget {
    entry_condition: [(MemoryModuleId, MemoryStatus); 4],
    /// Vanilla parity: `getCooldownOnFail`, sampled into `RAM_COOLDOWN_TICKS`.
    cooldown_on_fail: fn(&GoatEntity) -> i32,
    /// Vanilla parity: `minRamDistance` and `maxRamDistance`.
    min_ram_distance: i32,
    max_ram_distance: i32,
    /// Vanilla parity: `walkSpeed`.
    walk_speed: f64,
    /// Vanilla parity: `ramPrepareTime`, the ticks spent standing still.
    ram_prepare_time: i64,
    /// Vanilla parity: `reachedRamPositionTimestamp`.
    reached_ram_position_at: Option<i64>,
    /// Vanilla parity: `ramCandidate`.
    candidate: Option<RamCandidate>,
}

impl PrepareRamNearestTarget {
    /// Vanilla parity: `new PrepareRamNearestTarget(...)` as `GoatAi` builds it.
    #[must_use]
    pub fn new(
        cooldown_on_fail: fn(&GoatEntity) -> i32,
        min_ram_distance: i32,
        max_ram_distance: i32,
        walk_speed: f64,
        ram_prepare_time: i64,
    ) -> Self {
        Self {
            entry_condition: [
                (
                    memory_module_types::LOOK_TARGET.id(),
                    MemoryStatus::Registered,
                ),
                (
                    memory_module_types::RAM_COOLDOWN_TICKS.id(),
                    MemoryStatus::ValueAbsent,
                ),
                (
                    memory_module_types::NEAREST_VISIBLE_LIVING_ENTITIES.id(),
                    MemoryStatus::ValuePresent,
                ),
                (
                    memory_module_types::RAM_TARGET.id(),
                    MemoryStatus::ValueAbsent,
                ),
            ],
            cooldown_on_fail,
            min_ram_distance,
            max_ram_distance,
            walk_speed,
            ram_prepare_time,
            reached_ram_position_at: None,
            candidate: None,
        }
    }

    /// Vanilla parity: `GoatAi.RAM_TARGET_CONDITIONS`.
    ///
    /// The world-border clause is left out: Foton's border is not reachable
    /// from a brain behavior, the same gap the warden's target search carries.
    fn is_rammable(world: &Arc<World>, target: &dyn LivingEntity) -> bool {
        if target.entity_type() == &vanilla_entities::GOAT {
            return false;
        }
        if target.entity_type() == &vanilla_entities::ARMOR_STAND {
            return world.get_game_rule(&MOB_GRIEFING);
        }
        LivingEntity::is_alive(target)
    }

    /// Vanilla parity: `PrepareRamNearestTarget.getEdgeOfBlock`.
    fn edge_of_block(start: BlockPos, target: BlockPos) -> DVec3 {
        let x_offset = RAM_EDGE_OFFSET * f64::from((target.x() - start.x()).signum());
        let z_offset = RAM_EDGE_OFFSET * f64::from((target.z() - start.z()).signum());
        DVec3::new(
            f64::from(target.x()) + RAM_EDGE_OFFSET + x_offset,
            f64::from(target.y()),
            f64::from(target.z()) + RAM_EDGE_OFFSET + z_offset,
        )
    }

    /// Vanilla parity: `PrepareRamNearestTarget.isWalkableBlock`.
    fn is_walkable_block(goat: &GoatEntity, pos: BlockPos) -> bool {
        goat.is_stable_destination(pos)
    }

    /// Vanilla parity: `PrepareRamNearestTarget.calculateRammingStartPosition`.
    fn calculate_ramming_start_position(
        &self,
        goat: &GoatEntity,
        target: &SharedEntity,
    ) -> Option<BlockPos> {
        let target_pos = BlockPos::containing(
            target.position().x,
            target.position().y,
            target.position().z,
        );
        if !Self::is_walkable_block(goat, target_pos) {
            return None;
        }

        let mut candidates = Vec::new();
        for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let mut furthest = target_pos;
            for _ in 0..self.max_ram_distance {
                let next = BlockPos::new(furthest.x() + dx, furthest.y(), furthest.z() + dz);
                if !Self::is_walkable_block(goat, next) {
                    break;
                }
                furthest = next;
            }
            let manhattan = (furthest.x() - target_pos.x()).abs()
                + (furthest.y() - target_pos.y()).abs()
                + (furthest.z() - target_pos.z()).abs();
            if manhattan >= self.min_ram_distance {
                candidates.push(furthest);
            }
        }

        let here = goat.position();
        candidates.sort_by(|a, b| {
            let da = DVec3::new(f64::from(a.x()), f64::from(a.y()), f64::from(a.z()))
                .distance_squared(here);
            let db = DVec3::new(f64::from(b.x()), f64::from(b.y()), f64::from(b.z()))
                .distance_squared(here);
            da.total_cmp(&db)
        });
        candidates.into_iter().find(|pos| {
            goat.create_path_to(*pos, 0)
                .is_some_and(|path| path.can_reach())
        })
    }

    /// Vanilla parity: `PrepareRamNearestTarget.chooseRamPosition`.
    fn choose_ram_position(&mut self, goat: &GoatEntity, target: &SharedEntity) {
        self.reached_ram_position_at = None;
        self.candidate = self
            .calculate_ramming_start_position(goat, target)
            .map(|start| RamCandidate {
                start_position: start,
                target_position: BlockPos::containing(
                    target.position().x,
                    target.position().y,
                    target.position().z,
                ),
                target: target.clone(),
            });
    }
}

impl TimedBehavior for PrepareRamNearestTarget {
    fn debug_name(&self) -> &'static str {
        "PrepareRamNearestTarget"
    }

    fn entry_condition(&self) -> &[(MemoryModuleId, MemoryStatus)] {
        &self.entry_condition
    }

    fn duration(&self) -> (i32, i32) {
        (PREPARE_TIME_OUT_DURATION, PREPARE_TIME_OUT_DURATION)
    }

    fn start(&mut self, ctx: &BrainContext<'_>) {
        let Some(goat) = ctx.mob().downcast_ref::<GoatEntity>() else {
            return;
        };
        let Some(visible) = ctx
            .brain()
            .get_memory(memory_module_types::NEAREST_VISIBLE_LIVING_ENTITIES)
        else {
            return;
        };
        let world = ctx.world();
        let closest = visible.find_closest(|living| Self::is_rammable(world, living));
        if let Some(target) = closest {
            self.choose_ram_position(goat, &target);
        }
    }

    fn can_still_use(&mut self, _ctx: &BrainContext<'_>) -> bool {
        self.candidate.as_ref().is_some_and(|candidate| {
            candidate
                .target
                .as_living_entity()
                .is_some_and(LivingEntity::is_alive)
        })
    }

    fn tick(&mut self, ctx: &BrainContext<'_>) {
        let Some(goat) = ctx.mob().downcast_ref::<GoatEntity>() else {
            return;
        };
        let Some(candidate) = self.candidate.as_ref() else {
            return;
        };
        let start_position = candidate.start_position;
        let target_position = candidate.target_position;
        let target = candidate.target.clone();

        ctx.brain().set_memory(
            memory_module_types::WALK_TARGET,
            WalkTarget::of_block(start_position, self.walk_speed, 0),
        );
        ctx.brain().set_memory(
            memory_module_types::LOOK_TARGET,
            PositionTracker::of_entity(&target, true),
        );

        let now = BlockPos::containing(
            target.position().x,
            target.position().y,
            target.position().z,
        );
        if now != target_position {
            goat.broadcast_entity_event(EntityStatus::EndRam);
            goat.mob_base.navigation().lock().stop();
            self.choose_ram_position(goat, &target);
            return;
        }

        let goat_pos =
            BlockPos::containing(goat.position().x, goat.position().y, goat.position().z);
        if goat_pos != start_position {
            return;
        }

        goat.broadcast_entity_event(EntityStatus::StartRam);
        let reached_at = *self
            .reached_ram_position_at
            .get_or_insert_with(|| ctx.game_time());
        if ctx.game_time() - reached_at >= self.ram_prepare_time {
            ctx.brain().set_memory(
                memory_module_types::RAM_TARGET,
                Self::edge_of_block(start_position, target_position),
            );
            goat.play_sound(prepare_ram_sound(goat), 1.0, goat.voice_pitch());
            self.candidate = None;
        }
    }

    fn stop(&mut self, ctx: &BrainContext<'_>) {
        let Some(goat) = ctx.mob().downcast_ref::<GoatEntity>() else {
            return;
        };
        if ctx
            .brain()
            .has_memory_value(memory_module_types::RAM_TARGET.id())
        {
            return;
        }
        goat.broadcast_entity_event(EntityStatus::EndRam);
        ctx.brain().set_memory(
            memory_module_types::RAM_COOLDOWN_TICKS,
            (self.cooldown_on_fail)(goat),
        );
    }
}

/// Vanilla parity: the prepare sound `GoatAi` picks by goat kind.
fn prepare_ram_sound(goat: &GoatEntity) -> SoundEventRef {
    if goat.is_screaming_goat() {
        &sound_events::ENTITY_GOAT_SCREAMING_PREPARE_RAM
    } else {
        &sound_events::ENTITY_GOAT_PREPARE_RAM
    }
}
