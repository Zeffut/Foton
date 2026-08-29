//! The two behaviors only a frog runs: croaking, and the tongue.
//!
//! Vanilla parity: `net.minecraft.world.entity.ai.behavior.Croak` and
//! `net.minecraft.world.entity.animal.frog.ShootTongue`.

use foton_registry::sound_event::SoundEventRef;
use foton_utils::Downcast as _;
use uuid::Uuid;

use super::{BrainContext, MemoryModuleId, MemoryStatus, TimedBehavior};

use crate::entity::ai::brain::memory::{WalkTarget, memory_module_types};
use crate::entity::ai::brain::position_tracker::PositionTracker;
use crate::entity::entities::FrogEntity;
use crate::entity::{EntityPose, PathfinderMob, RemovalReason, SharedEntity};

/// Vanilla parity: `Croak.CROAK_TICKS`.
const CROAK_TICKS: i32 = 60;
/// Vanilla parity: `Croak.TIME_OUT_DURATION`.
const CROAK_TIME_OUT: i32 = 100;

/// Vanilla parity: `ShootTongue.TIME_OUT_DURATION`.
const TONGUE_TIME_OUT: i32 = 100;
/// Vanilla parity: `ShootTongue.CATCH_ANIMATION_DURATION`.
const CATCH_ANIMATION_DURATION: i32 = 6;
/// Vanilla parity: `ShootTongue.TONGUE_ANIMATION_DURATION`.
const TONGUE_ANIMATION_DURATION: i32 = 10;
/// Vanilla parity: `ShootTongue.EATING_DISTANCE`.
const EATING_DISTANCE: f64 = 1.75;
/// Vanilla parity: `ShootTongue.EATING_MOVEMENT_FACTOR`, the pull that drags
/// the prey into the frog's mouth.
const EATING_MOVEMENT_FACTOR: f64 = 0.75;
/// Vanilla parity: `ShootTongue.UNREACHABLE_TONGUE_TARGETS_COOLDOWN_DURATION`.
const UNREACHABLE_COOLDOWN: i64 = 100;
/// Vanilla parity: `ShootTongue.MAX_UNREACHBLE_TONGUE_TARGETS_IN_MEMORY`.
const MAX_UNREACHABLE_TARGETS: usize = 5;
/// How fast the frog closes on what it is about to eat.
///
/// Vanilla parity: the `2.0F` of the `WalkTarget` `ShootTongue` sets.
const APPROACH_SPEED: f64 = 2.0;
/// How often the approach recalculates its path.
const RECALCULATE_PATH_INTERVAL: i32 = 10;
/// Vanilla parity: the `2.0F` volume of the tongue and eat sounds.
const TONGUE_SOUND_VOLUME: f32 = 2.0;

/// Puffs the frog's throat out for three seconds.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.behavior.Croak`.
pub struct Croak {
    croak_counter: i32,
}

/// Vanilla parity: `ImmutableMap.of(WALK_TARGET, VALUE_ABSENT)`.
const CROAK_ENTRY_CONDITION: &[(MemoryModuleId, MemoryStatus)] = &[(
    memory_module_types::WALK_TARGET.id(),
    MemoryStatus::ValueAbsent,
)];

impl Croak {
    /// Vanilla parity: `new Croak()`.
    #[must_use]
    pub const fn new() -> Self {
        Self { croak_counter: 0 }
    }
}

impl Default for Croak {
    fn default() -> Self {
        Self::new()
    }
}

impl TimedBehavior for Croak {
    fn entry_condition(&self) -> &[(MemoryModuleId, MemoryStatus)] {
        CROAK_ENTRY_CONDITION
    }

    fn duration(&self) -> (i32, i32) {
        (CROAK_TIME_OUT, CROAK_TIME_OUT)
    }

    fn check_extra_start_conditions(&mut self, ctx: &BrainContext<'_>) -> bool {
        ctx.mob().pose() == EntityPose::Standing
    }

    fn can_still_use(&mut self, _ctx: &BrainContext<'_>) -> bool {
        self.croak_counter < CROAK_TICKS
    }

    fn start(&mut self, ctx: &BrainContext<'_>) {
        // Vanilla parity: a frog in liquid opens its mouth but the pose never
        // changes, so the counter is not reset either.
        // Vanilla parity: `LivingEntity.isInLiquid`, which is water or lava.
        if !ctx.mob().is_in_water() && !ctx.mob().is_in_lava() {
            ctx.mob().set_pose(EntityPose::Croaking);
            self.croak_counter = 0;
        }
    }

    fn stop(&mut self, ctx: &BrainContext<'_>) {
        ctx.mob().set_pose(EntityPose::Standing);
    }

    fn tick(&mut self, _ctx: &BrainContext<'_>) {
        self.croak_counter += 1;
    }

    fn debug_name(&self) -> &'static str {
        "Croak"
    }
}

/// Where a tongue shot has got to.
///
/// Vanilla parity: `ShootTongue.State`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TongueState {
    MoveToTarget,
    CatchAnimation,
    EatAnimation,
    Done,
}

/// Walks the frog up to its prey, then eats it.
///
/// Vanilla parity: `net.minecraft.world.entity.animal.frog.ShootTongue`. This is
/// the behavior that turns a small magma cube into a froglight: the frog is the
/// killing entity, and the magma cube's loot table reads the frog's variant.
pub struct ShootTongue {
    tongue_sound: SoundEventRef,
    eat_sound: SoundEventRef,
    eat_animation_timer: i32,
    calculate_path_counter: i32,
    state: TongueState,
}

/// Vanilla parity: the `entryCondition` map of `ShootTongue`.
const TONGUE_ENTRY_CONDITION: &[(MemoryModuleId, MemoryStatus)] = &[
    (
        memory_module_types::WALK_TARGET.id(),
        MemoryStatus::ValueAbsent,
    ),
    (
        memory_module_types::LOOK_TARGET.id(),
        MemoryStatus::Registered,
    ),
    (
        memory_module_types::ATTACK_TARGET.id(),
        MemoryStatus::ValuePresent,
    ),
    (
        memory_module_types::IS_PANICKING.id(),
        MemoryStatus::ValueAbsent,
    ),
];

impl ShootTongue {
    /// Vanilla parity: `new ShootTongue(SoundEvent, SoundEvent)`.
    #[must_use]
    pub const fn new(tongue_sound: SoundEventRef, eat_sound: SoundEventRef) -> Self {
        Self {
            tongue_sound,
            eat_sound,
            eat_animation_timer: 0,
            calculate_path_counter: 0,
            state: TongueState::Done,
        }
    }

    /// Vanilla parity: `ShootTongue.canPathfindToTarget`.
    fn can_pathfind_to_target(body: &dyn PathfinderMob, target: &SharedEntity) -> bool {
        body.create_path_to(target.block_position(), 0)
            .is_some_and(|path| f64::from(path.dist_to_target()) < EATING_DISTANCE)
    }

    /// Vanilla parity: `ShootTongue.addUnreachableTargetToMemory`, which keeps
    /// the last five so a frog stops re-targeting prey behind a wall.
    fn add_unreachable_target_to_memory(ctx: &BrainContext<'_>, target_uuid: Uuid) {
        let mut unreachable = ctx
            .brain()
            .get_memory(memory_module_types::UNREACHABLE_TONGUE_TARGETS)
            .unwrap_or_default();

        if unreachable.contains(&target_uuid) {
            return;
        }
        if unreachable.len() == MAX_UNREACHABLE_TARGETS {
            unreachable.remove(0);
        }
        unreachable.push(target_uuid);

        ctx.brain().set_memory_with_expiry(
            memory_module_types::UNREACHABLE_TONGUE_TARGETS,
            unreachable,
            UNREACHABLE_COOLDOWN,
        );
    }

    /// Vanilla parity: `ShootTongue.eatEntity`.
    fn eat_entity(&self, ctx: &BrainContext<'_>) {
        let body = ctx.mob();
        body.play_sound(self.eat_sound, TONGUE_SOUND_VOLUME, 1.0);

        let Some(frog) = body.downcast_ref::<FrogEntity>() else {
            return;
        };
        let Some(target) = frog.tongue_target() else {
            return;
        };
        if !target.is_alive() {
            return;
        }

        let Some(world) = body.level() else {
            return;
        };
        let _hurt = body.do_hurt_target(&world, &target);
        if !target.is_alive() {
            target.set_removed(RemovalReason::Killed);
        }
    }
}

impl TimedBehavior for ShootTongue {
    fn entry_condition(&self) -> &[(MemoryModuleId, MemoryStatus)] {
        TONGUE_ENTRY_CONDITION
    }

    fn duration(&self) -> (i32, i32) {
        (TONGUE_TIME_OUT, TONGUE_TIME_OUT)
    }

    fn check_extra_start_conditions(&mut self, ctx: &BrainContext<'_>) -> bool {
        let Some(target) = ctx
            .brain()
            .get_memory(memory_module_types::ATTACK_TARGET)
            .and_then(|memory| memory.get())
        else {
            return false;
        };

        let body = ctx.mob();
        if !Self::can_pathfind_to_target(body, &target) {
            ctx.brain()
                .erase_memory(memory_module_types::ATTACK_TARGET.id());
            Self::add_unreachable_target_to_memory(ctx, target.uuid());
            return false;
        }

        body.pose() != EntityPose::Croaking
            && target.as_living_entity().is_some_and(FrogEntity::can_eat)
    }

    fn can_still_use(&mut self, ctx: &BrainContext<'_>) -> bool {
        ctx.brain()
            .has_memory_value(memory_module_types::ATTACK_TARGET.id())
            && self.state != TongueState::Done
            && !ctx
                .brain()
                .has_memory_value(memory_module_types::IS_PANICKING.id())
    }

    fn start(&mut self, ctx: &BrainContext<'_>) {
        let Some(target) = ctx
            .brain()
            .get_memory(memory_module_types::ATTACK_TARGET)
            .and_then(|memory| memory.get())
        else {
            return;
        };

        let body = ctx.mob();
        // Vanilla parity: `BehaviorUtils.lookAtEntity`, then a walk target aimed
        // at where the prey is standing now rather than at the prey itself.
        ctx.brain().set_memory(
            memory_module_types::LOOK_TARGET,
            PositionTracker::of_entity(&target, true),
        );
        if let Some(frog) = body.downcast_ref::<FrogEntity>() {
            frog.set_tongue_target(&target);
        }
        ctx.brain().set_memory(
            memory_module_types::WALK_TARGET,
            WalkTarget::of_position(target.position(), APPROACH_SPEED, 0),
        );
        self.calculate_path_counter = RECALCULATE_PATH_INTERVAL;
        self.state = TongueState::MoveToTarget;
    }

    fn stop(&mut self, ctx: &BrainContext<'_>) {
        ctx.brain()
            .erase_memory(memory_module_types::ATTACK_TARGET.id());
        if let Some(frog) = ctx.mob().downcast_ref::<FrogEntity>() {
            frog.erase_tongue_target();
        }
        ctx.mob().set_pose(EntityPose::Standing);
    }

    fn tick(&mut self, ctx: &BrainContext<'_>) {
        let Some(target) = ctx
            .brain()
            .get_memory(memory_module_types::ATTACK_TARGET)
            .and_then(|memory| memory.get())
        else {
            return;
        };

        let body = ctx.mob();
        if let Some(frog) = body.downcast_ref::<FrogEntity>() {
            frog.set_tongue_target(&target);
        }

        match self.state {
            TongueState::MoveToTarget => {
                if target.position().distance(body.position()) < EATING_DISTANCE {
                    body.play_sound(self.tongue_sound, TONGUE_SOUND_VOLUME, 1.0);
                    body.set_pose(EntityPose::UsingTongue);
                    // Vanilla drags the prey toward the frog rather than moving
                    // the frog, which is what makes the tongue read as a pull.
                    let pull = (body.position() - target.position()).normalize_or_zero()
                        * EATING_MOVEMENT_FACTOR;
                    target.set_velocity(pull);
                    self.eat_animation_timer = 0;
                    self.state = TongueState::CatchAnimation;
                } else if self.calculate_path_counter <= 0 {
                    ctx.brain().set_memory(
                        memory_module_types::WALK_TARGET,
                        WalkTarget::of_position(target.position(), APPROACH_SPEED, 0),
                    );
                    self.calculate_path_counter = RECALCULATE_PATH_INTERVAL;
                } else {
                    self.calculate_path_counter -= 1;
                }
            }
            TongueState::CatchAnimation => {
                self.eat_animation_timer += 1;
                if self.eat_animation_timer >= CATCH_ANIMATION_DURATION {
                    self.state = TongueState::EatAnimation;
                    self.eat_entity(ctx);
                }
            }
            TongueState::EatAnimation => {
                if self.eat_animation_timer >= TONGUE_ANIMATION_DURATION {
                    self.state = TongueState::Done;
                } else {
                    self.eat_animation_timer += 1;
                }
            }
            TongueState::Done => {}
        }
    }

    fn debug_name(&self) -> &'static str {
        "ShootTongue"
    }
}
