//! The fox's own goals.
//!
//! Vanilla parity: the inner classes of `Fox`. They are inner classes there
//! because every one of them reads or writes the fox's flag byte, and they stay
//! next to the fox here for the same reason.

use std::f64::consts::TAU;

use foton_registry::blocks::block_state_ext::BlockStateExt as _;
use foton_registry::{sound_events, vanilla_blocks};
use foton_utils::{BlockPos, Downcast as _};
use glam::DVec3;

use super::{FoxEntity, FoxVariant};
use crate::entity::ai::goal::{
    AvoidEntityGoal, BreedGoal, FleeSunGoal, FloatGoal, FollowParentGoal, Goal, GoalControls,
    LookAtPlayerGoal, MeleeAttackGoal, MoveToBlockGoal, NearestAttackableTargetGoal, PanicGoal,
    reduced_tick_delay,
};
use crate::entity::ai::targeting::TargetingConditions;
use crate::entity::entities::{ItemEntity, WolfEntity};
use crate::entity::mob::rotlerp;
use crate::entity::{Entity, LivingEntity, MobBase, PathfinderMob, SharedEntity, is_tamed};
use crate::world::LevelReader;

/// How long a faceplanted fox lies there.
///
/// Vanilla parity: the `adjustedTickDelay(40)` of `Fox.FaceplantGoal.start`.
const FACEPLANT_TICKS: i32 = 40;

/// How far a fox notices something that would keep it awake.
///
/// Vanilla parity: the `range(12.0)` of `Fox.FoxBehaviorGoal`.
const ALERTABLE_RANGE: f64 = 12.0;

/// Vertical reach of the same search.
///
/// Vanilla parity: the `inflate(12.0, 6.0, 12.0)` of `alertable`.
const ALERTABLE_RANGE_Y: f64 = 6.0;

/// Longest a fox waits before it may fall asleep.
///
/// Vanilla parity: `Fox.SleepGoal.WAIT_TIME_BEFORE_SLEEP`.
const WAIT_TIME_BEFORE_SLEEP: i32 = 140;

/// Squared distance a fox starts stalking prey from.
///
/// Vanilla parity: the `distanceToSqr(target) > 36.0` of `Fox.StalkPreyGoal`.
const STALK_DISTANCE_SQR: f64 = 36.0;

/// How long the perch-and-search goal looks in one direction.
///
/// Vanilla parity: the `adjustedTickDelay(80 + nextInt(20))` of
/// `Fox.PerchAndSearchGoal.resetLook`.
const PERCH_LOOK_TICKS: i32 = 80;
const PERCH_LOOK_EXTRA_TICKS: i32 = 20;

/// How close a fox has to get to a berry bush.
///
/// Vanilla parity: `Fox.FoxEatBerriesGoal.acceptedDistance`.
const BERRY_ACCEPTED_DISTANCE: f64 = 2.0;

/// How long a fox waits at a bush before it picks.
///
/// Vanilla parity: `Fox.FoxEatBerriesGoal.WAIT_TICKS`.
const BERRY_WAIT_TICKS: i32 = 40;

/// How often the berry goal re-paths.
///
/// Vanilla parity: the `tryTicks % 100 == 0` of `shouldRecalculatePath`.
const BERRY_RECALCULATE_INTERVAL: i32 = 100;

/// Returns the fox behind a pathfinder mob, if it is one.
fn as_fox(mob: &dyn PathfinderMob) -> Option<&FoxEntity> {
    mob.downcast_ref::<FoxEntity>()
}

/// Returns whether anything nearby would keep a fox awake.
///
/// Vanilla parity: `Fox.FoxBehaviorGoal.alertable`, whose selector is
/// `FoxAlertableEntitiesSelector`.
fn alertable(fox: &FoxEntity) -> bool {
    let Some(world) = fox.level() else {
        return false;
    };

    let targeting = TargetingConditions::for_combat()
        .range(ALERTABLE_RANGE)
        .ignore_line_of_sight()
        .selector(|targeter, target, _| {
            let Some(fox) = targeter.and_then(|entity| entity.downcast_ref::<FoxEntity>()) else {
                return false;
            };
            fox_alertable_entities_selector(fox, target)
        });

    let search =
        fox.bounding_box()
            .inflate_xyz(ALERTABLE_RANGE, ALERTABLE_RANGE_Y, ALERTABLE_RANGE);
    world.has_entity_in_aabb_matching(&search, |entity| {
        entity
            .as_living_entity()
            .is_some_and(|living| targeting.test(world.as_ref(), Some(fox), living))
    })
}

/// Vanilla parity: `Fox.FoxAlertableEntitiesSelector`.
fn fox_alertable_entities_selector(fox: &FoxEntity, target: &dyn LivingEntity) -> bool {
    let target_entity = target.as_entity_event_source();
    if target_entity.downcast_ref::<FoxEntity>().is_some() {
        return false;
    }
    if FoxEntity::is_stalkable_prey(target) || FoxEntity::is_monster(target) {
        return true;
    }
    if target_entity.as_tamable_animal().is_some() {
        return !is_tamed(target_entity);
    }
    if let Some(player) = target_entity.as_player()
        && (target.is_spectator() || player.has_infinite_materials())
    {
        return false;
    }
    if fox.trusts(target_entity) {
        return false;
    }

    !target.is_sleeping() && !target.is_discrete()
}

/// Returns whether a fox has a roof over it.
///
/// Vanilla parity: `Fox.FoxBehaviorGoal.hasShelter`.
fn has_shelter(fox: &FoxEntity) -> bool {
    let Some(world) = fox.level() else {
        return false;
    };

    let position = fox.position();
    let pos = BlockPos::containing(position.x, fox.bounding_box().max_y(), position.z);
    !world.can_see_sky(pos) && fox.get_walk_target_value(pos) >= 0.0
}

/// The fox's float goal, which also wakes it up.
///
/// Vanilla parity: `Fox.FoxFloatGoal`.
pub(super) struct FoxFloatGoal {
    float: FloatGoal,
}

impl FoxFloatGoal {
    pub(super) fn new(mob_base: &MobBase) -> Self {
        Self {
            float: FloatGoal::new(mob_base),
        }
    }
}

impl Goal for FoxFloatGoal {
    fn controls(&self) -> GoalControls {
        self.float.controls()
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        // Vanilla parity: the override reads the water height rather than the
        // plain in-water flag, so a fox paddling in a puddle keeps walking.
        mob.is_in_water() && mob.fluid_contact().water_height() > 0.25 || mob.is_in_lava()
    }

    fn can_continue_to_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        self.float.can_continue_to_use(mob)
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        self.float.start(mob);
        if let Some(fox) = as_fox(mob) {
            fox.clear_states();
        }
    }

    fn stop(&mut self, mob: &dyn PathfinderMob) {
        self.float.stop(mob);
    }

    fn requires_update_every_tick(&self) -> bool {
        self.float.requires_update_every_tick()
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        self.float.tick(mob);
    }
}

/// Keeps a fox face-down in the snow for a moment after a failed pounce.
///
/// Vanilla parity: `Fox.FaceplantGoal`.
pub(super) struct FaceplantGoal {
    countdown: i32,
}

impl FaceplantGoal {
    pub(super) const fn new() -> Self {
        Self { countdown: 0 }
    }
}

impl Goal for FaceplantGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::LOOK | GoalControls::JUMP | GoalControls::MOVE
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        as_fox(mob).is_some_and(FoxEntity::is_faceplanted)
    }

    fn can_continue_to_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        self.can_use(mob) && self.countdown > 0
    }

    fn start(&mut self, _mob: &dyn PathfinderMob) {
        self.countdown = reduced_tick_delay(FACEPLANT_TICKS);
    }

    fn stop(&mut self, mob: &dyn PathfinderMob) {
        if let Some(fox) = as_fox(mob) {
            fox.set_faceplanted(false);
        }
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, _mob: &dyn PathfinderMob) {
        self.countdown -= 1;
    }
}

/// The fox's panic, which a defending fox suppresses.
///
/// Vanilla parity: `Fox.FoxPanicGoal`.
#[must_use]
pub(super) fn fox_panic_goal(speed_modifier: f64) -> PanicGoal {
    PanicGoal::new(speed_modifier)
        .with_panic_filter(|mob| !as_fox(mob).is_some_and(FoxEntity::is_defending))
}

/// Vanilla parity: the three `AvoidEntityGoal`s of `Fox.registerGoals`.
#[must_use]
pub(super) fn fox_avoid_players_goal() -> AvoidEntityGoal {
    AvoidEntityGoal::with_selector(16.0, 1.6, 1.4, |targeter, target, _| {
        let Some(fox) = targeter.and_then(|entity| entity.downcast_ref::<FoxEntity>()) else {
            return false;
        };
        let target_entity = target.as_entity_event_source();
        let Some(player) = target_entity.as_player() else {
            return false;
        };

        !target.is_discrete()
            && !target.is_spectator()
            && !player.has_infinite_materials()
            && !fox.trusts(target_entity)
            && !fox.is_defending()
    })
}

/// Vanilla parity: the wolf-avoiding goal, which only fears untamed wolves.
#[must_use]
pub(super) fn fox_avoid_wolves_goal() -> AvoidEntityGoal {
    AvoidEntityGoal::with_selector(8.0, 1.6, 1.4, |targeter, target, _| {
        let target_entity = target.as_entity_event_source();
        let is_wolf = target_entity.downcast_ref::<WolfEntity>().is_some();
        let fox_is_defending = targeter
            .and_then(|entity| entity.downcast_ref::<FoxEntity>())
            .is_some_and(FoxEntity::is_defending);

        is_wolf && !is_tamed(target_entity) && !fox_is_defending
    })
}

/// The fox's breed goal, which clears both parents' poses first.
///
/// Vanilla parity: `Fox.FoxBreedGoal`.
pub(super) struct FoxBreedGoal {
    breed: BreedGoal,
}

impl FoxBreedGoal {
    pub(super) const fn new(speed_modifier: f64) -> Self {
        Self {
            breed: BreedGoal::new(speed_modifier),
        }
    }
}

impl Goal for FoxBreedGoal {
    fn controls(&self) -> GoalControls {
        self.breed.controls()
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        self.breed.can_use(mob)
    }

    fn can_continue_to_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        self.breed.can_continue_to_use(mob)
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        if let Some(fox) = as_fox(mob) {
            fox.clear_states();
        }
        self.breed.start(mob);
    }

    fn stop(&mut self, mob: &dyn PathfinderMob) {
        self.breed.stop(mob);
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        self.breed.tick(mob);
    }
}

/// Creeps up on a chicken or a rabbit until it is close enough to pounce.
///
/// Vanilla parity: `Fox.StalkPreyGoal`.
pub(super) struct StalkPreyGoal;

impl Goal for StalkPreyGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::MOVE | GoalControls::LOOK
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        let Some(fox) = as_fox(mob) else {
            return false;
        };
        if fox.is_sleeping() {
            return false;
        }
        let Some(target) = mob.target() else {
            return false;
        };
        let Some(target_living) = target.as_living_entity() else {
            return false;
        };

        target.is_alive()
            && FoxEntity::is_stalkable_prey(target_living)
            && mob.position().distance_squared(target.position()) > STALK_DISTANCE_SQR
            && !fox.is_crouching_flag()
            && !fox.is_interested()
            && !mob.is_jumping()
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        let Some(fox) = as_fox(mob) else {
            return;
        };
        fox.set_sitting(false);
        fox.set_faceplanted(false);
    }

    fn stop(&mut self, mob: &dyn PathfinderMob) {
        let Some(fox) = as_fox(mob) else {
            return;
        };
        let Some(target) = mob.target() else {
            fox.set_is_interested(false);
            fox.set_is_crouching(false);
            return;
        };

        if FoxEntity::is_path_clear(fox, target.as_ref()) {
            fox.set_is_interested(true);
            fox.set_is_crouching(true);
            mob.mob_base().navigation().lock().stop();
            look_at_target(mob, &target);
        } else {
            fox.set_is_interested(false);
            fox.set_is_crouching(false);
        }
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        let Some(fox) = as_fox(mob) else {
            return;
        };
        let Some(target) = mob.target() else {
            return;
        };

        look_at_target(mob, &target);
        if mob.position().distance_squared(target.position()) <= STALK_DISTANCE_SQR {
            fox.set_is_interested(true);
            fox.set_is_crouching(true);
            mob.mob_base().navigation().lock().stop();
        } else {
            mob.move_to_pos(target.position(), 1.5);
        }
    }
}

fn look_at_target(mob: &dyn PathfinderMob, target: &SharedEntity) {
    let position = target.position();
    mob.mob_base().controls().lock().look_control.set_look_at(
        DVec3::new(position.x, target.get_eye_y(), position.z),
        mob.max_head_y_rot(),
        mob.max_head_x_rot(),
    );
}

/// The leap a fully crouched fox makes at its prey.
///
/// Vanilla parity: `Fox.FoxPounceGoal`.
pub(super) struct FoxPounceGoal;

impl Goal for FoxPounceGoal {
    fn controls(&self) -> GoalControls {
        // Vanilla parity: `JumpGoal`, which claims MOVE and JUMP.
        GoalControls::MOVE | GoalControls::JUMP
    }

    fn is_interruptable(&self) -> bool {
        false
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        let Some(fox) = as_fox(mob) else {
            return false;
        };
        if !fox.is_fully_crouched() {
            return false;
        }
        let Some(target) = mob.target() else {
            return false;
        };
        if !target.is_alive() {
            return false;
        }
        // Vanilla parity gap: the `getMotionDirection() != getDirection()`
        // guard makes a fox wait until its prey is not running straight at it.
        // Foton tracks no per-entity motion direction, so the guard is skipped
        // and a fox pounces slightly more eagerly.

        let path_clear = FoxEntity::is_path_clear(fox, target.as_ref());
        if !path_clear {
            let _ = mob.create_path_to(target.block_position(), 0);
            fox.set_is_crouching(false);
            fox.set_is_interested(false);
        }
        path_clear
    }

    fn can_continue_to_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        let Some(fox) = as_fox(mob) else {
            return false;
        };
        let Some(target) = mob.target() else {
            return false;
        };
        if !target.is_alive() {
            return false;
        }

        let yd = mob.velocity().y;
        let still_airborne = yd * yd >= 0.05 || mob.rotation().1.abs() >= 15.0 || !mob.on_ground();
        still_airborne && !fox.is_faceplanted()
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        let Some(fox) = as_fox(mob) else {
            return;
        };
        mob.set_jumping(true);
        fox.set_is_pouncing(true);
        fox.set_is_interested(false);

        if let Some(target) = mob.target() {
            let position = target.position();
            mob.mob_base().controls().lock().look_control.set_look_at(
                DVec3::new(position.x, target.get_eye_y(), position.z),
                60.0,
                30.0,
            );
            let leap = (position - mob.position()).normalize_or_zero();
            mob.set_velocity(mob.velocity() + DVec3::new(leap.x * 0.8, 0.9, leap.z * 0.8));
        }

        mob.mob_base().navigation().lock().stop();
    }

    fn stop(&mut self, mob: &dyn PathfinderMob) {
        let Some(fox) = as_fox(mob) else {
            return;
        };
        fox.set_is_crouching(false);
        fox.reset_crouch_amount();
        fox.set_is_interested(false);
        fox.set_is_pouncing(false);
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        let Some(fox) = as_fox(mob) else {
            return;
        };
        let target = mob.target();
        if let Some(target) = &target {
            let position = target.position();
            mob.mob_base().controls().lock().look_control.set_look_at(
                DVec3::new(position.x, target.get_eye_y(), position.z),
                60.0,
                30.0,
            );
        }

        if !fox.is_faceplanted() {
            pitch_toward_flight(mob);
        }

        let Some(target) = target else {
            return;
        };
        let Some(world) = mob.level() else {
            return;
        };

        if mob.position().distance(target.position()) <= 2.0 {
            let _ = mob.do_hurt_target(&world, &target);
            return;
        }

        let landed_in_snow = mob.rotation().1 > 0.0
            && mob.on_ground()
            && mob.velocity().y != 0.0
            && world.get_block_state(mob.block_position()).get_block() == &vanilla_blocks::SNOW;
        if landed_in_snow {
            mob.set_rotation((mob.rotation().0, 60.0));
            mob.set_target(None);
            fox.set_faceplanted(true);
        }
    }
}

/// Vanilla parity: the pitch half of `Fox.FoxPounceGoal.tick`, which is what
/// makes a pouncing fox nose-dive.
fn pitch_toward_flight(mob: &dyn PathfinderMob) {
    let movement = mob.velocity();
    let (yaw, pitch) = mob.rotation();
    if movement.y * movement.y < 0.03 && pitch != 0.0 {
        mob.set_rotation((yaw, rotlerp(pitch, 0.0, 0.2)));
        return;
    }

    let horizontal = movement.x.hypot(movement.z);
    let upward_bias = if mob.is_jumping() && movement.y > 0.0 {
        6.5
    } else {
        1.0
    };
    let biased_y = movement.y * upward_bias;
    let length = horizontal.hypot(biased_y);
    if length <= 1.0e-5 {
        return;
    }

    let rotation = -biased_y.signum() * (horizontal / length).acos().to_degrees();
    mob.set_rotation((yaw, rotation as f32));
}

/// Sends a fox indoors when the sun is up or a storm is coming.
///
/// Vanilla parity: `Fox.SeekShelterGoal`.
pub(super) struct SeekShelterGoal {
    flee_sun: FleeSunGoal,
    interval: i32,
}

impl SeekShelterGoal {
    pub(super) const fn new(speed_modifier: f64) -> Self {
        Self {
            flee_sun: FleeSunGoal::new(speed_modifier),
            interval: reduced_tick_delay(100),
        }
    }
}

impl Goal for SeekShelterGoal {
    fn controls(&self) -> GoalControls {
        self.flee_sun.controls()
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        let Some(fox) = as_fox(mob) else {
            return false;
        };
        if fox.is_sleeping() || mob.target().is_some() {
            return false;
        }
        let Some(world) = mob.level() else {
            return false;
        };

        if world.is_thundering() && world.can_see_sky(mob.block_position()) {
            return self.flee_sun.set_wanted_pos(mob, &world);
        }
        if self.interval > 0 {
            self.interval -= 1;
            return false;
        }

        self.interval = 100;
        // Vanilla parity gap: the vanilla condition also excludes a position
        // inside a village (`ServerLevel.isVillage`). Foton has no village
        // points of interest yet, so a fox hides from daylight anywhere.
        world.is_bright_outside()
            && world.can_see_sky(mob.block_position())
            && self.flee_sun.set_wanted_pos(mob, &world)
    }

    fn can_continue_to_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        self.flee_sun.can_continue_to_use(mob)
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        if let Some(fox) = as_fox(mob) {
            fox.clear_states();
        }
        self.flee_sun.start(mob);
    }
}

/// The fox's bite.
///
/// Vanilla parity: `Fox.FoxMeleeAttackGoal`.
pub(super) struct FoxMeleeAttackGoal {
    melee: MeleeAttackGoal,
}

impl FoxMeleeAttackGoal {
    pub(super) fn new(speed_modifier: f64, track_target: bool) -> Self {
        Self {
            melee: MeleeAttackGoal::new(speed_modifier, track_target).with_attack_override(
                |mob, target| {
                    if let Some(world) = mob.level() {
                        let _ = mob.do_hurt_target(&world, target);
                    }
                    mob.play_sound(&sound_events::ENTITY_FOX_BITE, 1.0, 1.0);
                },
            ),
        }
    }
}

impl Goal for FoxMeleeAttackGoal {
    fn controls(&self) -> GoalControls {
        self.melee.controls()
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        let Some(fox) = as_fox(mob) else {
            return false;
        };
        !fox.is_sitting()
            && !fox.is_sleeping()
            && !fox.is_crouching_flag()
            && !fox.is_faceplanted()
            && self.melee.can_use(mob)
    }

    fn can_continue_to_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        self.melee.can_continue_to_use(mob)
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        if let Some(fox) = as_fox(mob) {
            fox.set_is_interested(false);
        }
        self.melee.start(mob);
    }

    fn stop(&mut self, mob: &dyn PathfinderMob) {
        self.melee.stop(mob);
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        self.melee.tick(mob);
    }
}

/// Puts a fox to sleep in the daylight when nothing is watching.
///
/// Vanilla parity: `Fox.SleepGoal`.
pub(super) struct SleepGoal {
    countdown: i32,
}

impl SleepGoal {
    pub(super) fn new() -> Self {
        Self {
            countdown: rand::random_range(0..reduced_tick_delay(WAIT_TIME_BEFORE_SLEEP)),
        }
    }

    fn can_sleep(&mut self, fox: &FoxEntity) -> bool {
        if self.countdown > 0 {
            self.countdown -= 1;
            return false;
        }

        let Some(world) = fox.level() else {
            return false;
        };
        world.is_bright_outside() && has_shelter(fox) && !alertable(fox) && !fox.is_in_powder_snow()
    }
}

impl Goal for SleepGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::MOVE | GoalControls::LOOK | GoalControls::JUMP
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        let Some(fox) = as_fox(mob) else {
            return false;
        };
        let input = mob.travel_input();
        if input.sideways() != 0.0 || input.vertical() != 0.0 || input.forward() != 0.0 {
            return false;
        }

        self.can_sleep(fox) || fox.is_sleeping()
    }

    fn can_continue_to_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        as_fox(mob).is_some_and(|fox| self.can_sleep(fox))
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        let Some(fox) = as_fox(mob) else {
            return;
        };
        fox.set_sitting(false);
        fox.set_is_crouching(false);
        fox.set_is_interested(false);
        mob.set_jumping(false);
        fox.set_sleeping(true);
        mob.mob_base().navigation().lock().stop();
        mob.set_wanted_position(mob.position(), 0.0);
    }

    fn stop(&mut self, mob: &dyn PathfinderMob) {
        self.countdown = rand::random_range(0..reduced_tick_delay(WAIT_TIME_BEFORE_SLEEP));
        if let Some(fox) = as_fox(mob) {
            fox.clear_states();
        }
    }
}

/// The fox's follow-parent goal, which a defending kit ignores.
///
/// Vanilla parity: `Fox.FoxFollowParentGoal`.
pub(super) struct FoxFollowParentGoal {
    follow: FollowParentGoal,
}

impl FoxFollowParentGoal {
    pub(super) const fn new(speed_modifier: f64) -> Self {
        Self {
            follow: FollowParentGoal::new(speed_modifier),
        }
    }

    fn is_defending(mob: &dyn PathfinderMob) -> bool {
        as_fox(mob).is_some_and(FoxEntity::is_defending)
    }
}

impl Goal for FoxFollowParentGoal {
    fn controls(&self) -> GoalControls {
        self.follow.controls()
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        !Self::is_defending(mob) && self.follow.can_use(mob)
    }

    fn can_continue_to_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        !Self::is_defending(mob) && self.follow.can_continue_to_use(mob)
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        if let Some(fox) = as_fox(mob) {
            fox.clear_states();
        }
        self.follow.start(mob);
    }

    fn stop(&mut self, mob: &dyn PathfinderMob) {
        self.follow.stop(mob);
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        self.follow.tick(mob);
    }
}

/// Sends a fox to a berry bush and lets it pick.
///
/// Vanilla parity: `Fox.FoxEatBerriesGoal`.
pub(super) struct FoxEatBerriesGoal {
    move_to_block: MoveToBlockGoal,
    ticks_waited: i32,
}

impl FoxEatBerriesGoal {
    pub(super) fn new(speed_modifier: f64, search_range: i32, vertical_search_range: i32) -> Self {
        Self {
            move_to_block: MoveToBlockGoal::with_vertical_search_range(
                speed_modifier,
                search_range,
                vertical_search_range,
                is_ripe_berry_bush,
            )
            .with_accepted_distance(BERRY_ACCEPTED_DISTANCE)
            .with_recalculate_path_interval(BERRY_RECALCULATE_INTERVAL),
            ticks_waited: 0,
        }
    }
}

/// Vanilla parity: `Fox.FoxEatBerriesGoal.isValidTarget`.
fn is_ripe_berry_bush(level: &dyn LevelReader, pos: BlockPos) -> bool {
    use foton_registry::blocks::properties::BlockStateProperties;

    let state = level.get_block_state(pos);
    let block = state.get_block();
    if block == &vanilla_blocks::SWEET_BERRY_BUSH {
        return state.get_value(&BlockStateProperties::AGE_3) >= 2;
    }

    // Vanilla parity: `CaveVines.hasGlowBerries`.
    (block == &vanilla_blocks::CAVE_VINES || block == &vanilla_blocks::CAVE_VINES_PLANT)
        && state.get_value(&BlockStateProperties::BERRIES)
}

impl Goal for FoxEatBerriesGoal {
    fn controls(&self) -> GoalControls {
        self.move_to_block.controls()
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        !as_fox(mob).is_some_and(FoxEntity::is_sleeping) && self.move_to_block.can_use(mob)
    }

    fn can_continue_to_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        self.move_to_block.can_continue_to_use(mob)
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        self.ticks_waited = 0;
        if let Some(fox) = as_fox(mob) {
            fox.set_sitting(false);
        }
        self.move_to_block.start(mob);
    }

    fn stop(&mut self, mob: &dyn PathfinderMob) {
        self.move_to_block.stop(mob);
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        if self.move_to_block.is_reached_target() {
            if self.ticks_waited >= BERRY_WAIT_TICKS {
                if let Some(fox) = as_fox(mob) {
                    fox.pick_berries(self.move_to_block.block_pos());
                }
            } else {
                self.ticks_waited += 1;
            }
        } else if rand::random::<f32>() < 0.05 {
            mob.play_sound(&sound_events::ENTITY_FOX_SNIFF, 1.0, 1.0);
        }

        self.move_to_block.tick(mob);
    }
}

/// Sends a fox after a dropped item it could carry.
///
/// Vanilla parity: `Fox.FoxSearchForItemsGoal`.
pub(super) struct FoxSearchForItemsGoal;

impl FoxSearchForItemsGoal {
    fn nearby_item(mob: &dyn PathfinderMob) -> Option<SharedEntity> {
        let world = mob.level()?;
        let search = mob.bounding_box().inflate(8.0);
        world.nearest_entity_in_aabb_matching(&search, mob.position(), |entity| {
            entity
                .downcast_ref::<ItemEntity>()
                .is_some_and(|item| !item.has_pickup_delay() && entity.is_alive())
        })
    }
}

impl Goal for FoxSearchForItemsGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::MOVE
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        let Some(fox) = as_fox(mob) else {
            return false;
        };
        if !fox.mouth_item_is_empty() || mob.target().is_some() || mob.last_hurt_by_mob().is_some()
        {
            return false;
        }
        if !fox.can_move() {
            return false;
        }
        if rand::random_range(0..reduced_tick_delay(10)) != 0 {
            return false;
        }

        Self::nearby_item(mob).is_some()
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        if let Some(item) = Self::nearby_item(mob) {
            mob.move_to_pos(item.position(), 1.2);
        }
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        let Some(fox) = as_fox(mob) else {
            return;
        };
        if !fox.mouth_item_is_empty() {
            return;
        }
        if let Some(item) = Self::nearby_item(mob) {
            mob.move_to_pos(item.position(), 1.2);
        }
    }
}

/// The fox's look-at-player goal, suppressed while it is busy.
///
/// Vanilla parity: `Fox.FoxLookAtPlayerGoal`.
pub(super) struct FoxLookAtPlayerGoal {
    look: LookAtPlayerGoal,
}

impl FoxLookAtPlayerGoal {
    pub(super) fn new(look_distance: f64) -> Self {
        Self {
            look: LookAtPlayerGoal::new(look_distance),
        }
    }

    fn is_busy(mob: &dyn PathfinderMob) -> bool {
        as_fox(mob).is_some_and(|fox| fox.is_faceplanted() || fox.is_interested())
    }
}

impl Goal for FoxLookAtPlayerGoal {
    fn controls(&self) -> GoalControls {
        self.look.controls()
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        self.look.can_use(mob) && !Self::is_busy(mob)
    }

    fn can_continue_to_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        self.look.can_continue_to_use(mob) && !Self::is_busy(mob)
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        self.look.start(mob);
    }

    fn stop(&mut self, mob: &dyn PathfinderMob) {
        self.look.stop(mob);
    }

    fn requires_update_every_tick(&self) -> bool {
        self.look.requires_update_every_tick()
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        self.look.tick(mob);
    }
}

/// Sits a fox down to look around when it has nothing else to do.
///
/// Vanilla parity: `Fox.PerchAndSearchGoal`.
pub(super) struct PerchAndSearchGoal {
    rel_x: f64,
    rel_z: f64,
    look_time: i32,
    looks_remaining: i32,
}

impl PerchAndSearchGoal {
    pub(super) const fn new() -> Self {
        Self {
            rel_x: 0.0,
            rel_z: 0.0,
            look_time: 0,
            looks_remaining: 0,
        }
    }

    fn reset_look(&mut self) {
        let angle = TAU * rand::random::<f64>();
        self.rel_x = angle.cos();
        self.rel_z = angle.sin();
        self.look_time =
            reduced_tick_delay(PERCH_LOOK_TICKS + rand::random_range(0..PERCH_LOOK_EXTRA_TICKS));
    }
}

impl Goal for PerchAndSearchGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::MOVE | GoalControls::LOOK
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        let Some(fox) = as_fox(mob) else {
            return false;
        };

        mob.last_hurt_by_mob().is_none()
            && rand::random::<f32>() < 0.02
            && !fox.is_sleeping()
            && mob.target().is_none()
            && mob.mob_base().navigation().lock().is_done()
            && !alertable(fox)
            && !fox.is_pouncing()
            && !fox.is_crouching_flag()
    }

    fn can_continue_to_use(&mut self, _mob: &dyn PathfinderMob) -> bool {
        self.looks_remaining > 0
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        self.reset_look();
        self.looks_remaining = 2 + rand::random_range(0..3);
        if let Some(fox) = as_fox(mob) {
            fox.set_sitting(true);
        }
        mob.mob_base().navigation().lock().stop();
    }

    fn stop(&mut self, mob: &dyn PathfinderMob) {
        if let Some(fox) = as_fox(mob) {
            fox.set_sitting(false);
        }
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        self.look_time -= 1;
        if self.look_time <= 0 {
            self.looks_remaining -= 1;
            self.reset_look();
        }

        let position = mob.position();
        mob.mob_base().controls().lock().look_control.set_look_at(
            DVec3::new(
                position.x + self.rel_x,
                mob.get_eye_y(),
                position.z + self.rel_z,
            ),
            mob.max_head_y_rot(),
            mob.max_head_x_rot(),
        );
    }
}

/// Sends a fox after whoever hurt someone it trusts.
///
/// Vanilla parity: `Fox.DefendTrustedTargetGoal`.
pub(super) struct DefendTrustedTargetGoal {
    nearest: NearestAttackableTargetGoal,
    trusted_last_hurt_by: Option<SharedEntity>,
    timestamp: i32,
}

impl DefendTrustedTargetGoal {
    pub(super) fn new() -> Self {
        Self {
            nearest: NearestAttackableTargetGoal::new_with_interval(
                10,
                false,
                false,
                |targeter, target, _| {
                    let Some(fox) = targeter.and_then(|entity| entity.downcast_ref::<FoxEntity>())
                    else {
                        return false;
                    };
                    FoxEntity::is_recent_aggressor(target)
                        && !fox.trusts(target.as_entity_event_source())
                },
            ),
            trusted_last_hurt_by: None,
            timestamp: 0,
        }
    }
}

impl Goal for DefendTrustedTargetGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::TARGET
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        if rand::random_range(0..reduced_tick_delay(10)) != 0 {
            return false;
        }
        let Some(fox) = as_fox(mob) else {
            return false;
        };
        let Some(trusted) = fox.first_trusted_entity() else {
            return false;
        };
        let Some(trusted_living) = trusted.as_living_entity() else {
            return false;
        };

        self.trusted_last_hurt_by = trusted_living.last_hurt_by_mob();
        let timestamp = trusted_living.last_hurt_by_mob_timestamp();
        timestamp != self.timestamp && self.trusted_last_hurt_by.is_some()
    }

    fn can_continue_to_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        self.nearest.can_continue_to_use(mob)
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        let _ = mob.set_target(self.trusted_last_hurt_by.as_ref());
        self.nearest.set_target(self.trusted_last_hurt_by.clone());

        if let Some(fox) = as_fox(mob) {
            if let Some(trusted) = fox.first_trusted_entity()
                && let Some(living) = trusted.as_living_entity()
            {
                self.timestamp = living.last_hurt_by_mob_timestamp();
            }
            mob.play_sound(&sound_events::ENTITY_FOX_AGGRO, 1.0, 1.0);
            fox.set_defending(true);
            fox.set_sleeping(false);
        }

        self.nearest.start(mob);
    }

    fn stop(&mut self, mob: &dyn PathfinderMob) {
        self.nearest.stop(mob);
        self.trusted_last_hurt_by = None;
    }
}

/// Builds the target goal a fox uses to hunt one kind of prey.
///
/// Vanilla parity: the `landTargetGoal`, `turtleEggTargetGoal` and
/// `fishTargetGoal` of `Fox.registerGoals`.
#[must_use]
pub(super) fn fox_prey_target_goal(
    random_interval: i32,
    prey: fn(&dyn LivingEntity) -> bool,
) -> NearestAttackableTargetGoal {
    NearestAttackableTargetGoal::new_with_interval(
        random_interval,
        false,
        false,
        move |_, target, _| prey(target),
    )
}

/// Returns the two prey goals a fox of this variant registers, with their
/// priorities.
///
/// Vanilla parity: `Fox.setTargetGoals`, which swaps the priorities of the land
/// and the fish goal by variant.
#[must_use]
pub(super) fn prey_goal_priorities(variant: FoxVariant) -> (i32, i32) {
    if variant == FoxVariant::Red {
        (4, 6)
    } else {
        (6, 4)
    }
}
