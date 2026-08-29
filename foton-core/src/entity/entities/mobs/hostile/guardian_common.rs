//! What a guardian and an elder guardian share.
//!
//! Vanilla parity: `Guardian`, minus the handful of overrides `ElderGuardian`
//! makes. Foton has no entity class hierarchy, so the whole of the guardian --
//! its goal set, its beam, its thorns, the way it flops on land and swims in
//! water -- lives here as free functions over [`GuardianLike`], and the two
//! mobs differ only in the values they answer.

use std::mem;
use std::sync::Arc;

use foton_registry::blocks::block_state_ext::BlockStateExt as _;
use foton_registry::sound_event::SoundEventRef;
use foton_registry::vanilla_damage_type_tags::DamageTypeTag;
use foton_registry::vanilla_fluid_tags::FluidTag;
use foton_registry::{vanilla_blocks, vanilla_damage_types, vanilla_entities};
use foton_utils::locks::SyncMutex;
use foton_utils::types::Difficulty;
use foton_utils::{BlockPos, Downcast as _, DowncastType};
use glam::DVec3;

use crate::entity::ai::control::GuardianMoveControl;
use crate::entity::ai::goal::{
    Goal, GoalControls, LookAtPlayerGoal, MoveTowardsRestrictionGoal, NearestAttackableTargetGoal,
    RandomLookAroundGoal, RandomStrollGoal,
};
use crate::entity::damage::DamageSource;
use crate::entity::{
    EntitySpawnReason, EntityStatus, LivingEntity, Mob, MobBase, PathfinderMob, SharedEntity,
};
use crate::physics::{MoveResult, MoverType};
use crate::world::{LevelReader as _, World};

/// Ticks a guardian charges its beam for.
///
/// Vanilla parity: `Guardian.ATTACK_TIME`, overridden to `60` by the elder.
pub(super) const ATTACK_TIME: i32 = 80;

/// Experience either guardian drops.
///
/// Vanilla parity: the `this.xpReward = 10` of the `Guardian` constructor.
pub(super) const XP_REWARD: i32 = 10;

/// A guardian tracks a target through a full half-circle of pitch.
///
/// Vanilla parity: `Guardian.getMaxHeadXRot`.
pub(super) const MAX_HEAD_X_ROT: f32 = 180.0;

/// How often a guardian makes an idle noise.
///
/// Vanilla parity: `Guardian.getAmbientSoundInterval`.
pub(super) const AMBIENT_SOUND_INTERVAL: i32 = 160;

/// Ticks between two wander rolls.
///
/// Vanilla parity: `new RandomStrollGoal(this, 1.0, 80)`, which the elder slows
/// to `400`.
pub(super) const STROLL_INTERVAL_TICKS: i32 = 80;

/// Air a guardian is topped back up to whenever it is in water.
///
/// Vanilla parity: the `setAirSupply(300)` of `Guardian.aiStep`.
const WATER_AIR_SUPPLY: i32 = 300;

/// Damage a guardian's spikes do to whatever hit it.
///
/// Vanilla parity: the `2.0F` of the thorns branch in `Guardian.hurtServer`.
const THORNS_DAMAGE: f32 = 2.0;

/// Extra beam damage on hard difficulty.
const HARD_DIFFICULTY_BONUS: f32 = 2.0;

/// Extra beam damage an elder guardian does.
const ELDER_BEAM_BONUS: f32 = 2.0;

/// Base beam damage.
const BASE_BEAM_DAMAGE: f32 = 1.0;

/// Speed multiplier of every one of a guardian's movement goals.
const MOVE_SPEED_MODIFIER: f64 = 1.0;

/// Distance at which a guardian watches a player.
const LOOK_AT_PLAYER_RANGE: f64 = 8.0;

/// Distance at which a guardian watches another guardian.
const LOOK_AT_GUARDIAN_RANGE: f64 = 12.0;

/// How often a guardian bothers to watch another guardian.
///
/// Vanilla parity: the `0.01F` of
/// `new LookAtPlayerGoal(this, Guardian.class, 12.0F, 0.01F)`.
const LOOK_AT_GUARDIAN_PROBABILITY: f32 = 0.01;

/// How often the target goal rescans.
const TARGET_SCAN_INTERVAL: i32 = 10;

/// Squared distance a target has to be beyond before a guardian takes it.
///
/// Vanilla parity: the `distanceToSqr(this.guardian) > 9.0` of
/// `GuardianAttackSelector`, which is why a guardian ignores whatever is
/// pressed against it.
const TARGET_MIN_RANGE_SQR: f64 = 9.0;

/// Ticks the beam spends winding up before the client is told about it.
///
/// Vanilla parity: the `this.attackTime = -10` of `GuardianAttackGoal.start`.
const ATTACK_WIND_UP_TICKS: i32 = -10;

/// How far a guardian's head turns each tick while it is firing.
const ATTACK_LOOK_TURN_RATE: f32 = 90.0;

/// Walk-target value of a water block.
///
/// Vanilla parity: the `10.0F` of `Guardian.getWalkTargetValue`.
const WATER_WALK_TARGET_VALUE: f32 = 10.0;

/// How much a stranded guardian flops sideways.
///
/// Vanilla parity: the `* 0.4F` of the flop in `Guardian.aiStep`.
const FLOP_HORIZONTAL_IMPULSE: f64 = 0.4;

/// How high a stranded guardian flops.
const FLOP_VERTICAL_IMPULSE: f64 = 0.5;

/// How hard a guardian pushes against the water.
///
/// Vanilla parity: the `moveRelative(0.1F, input)` of `Guardian.travelInWater`.
const WATER_TRAVEL_SPEED: f32 = 0.1;

/// How much of its speed a guardian keeps each tick in water.
const WATER_TRAVEL_DRAG: f64 = 0.9;

/// How fast an idle guardian sinks.
const IDLE_SINK_RATE: f64 = 0.005;

/// One in this many spawn attempts survives when the sky is visible.
///
/// Vanilla parity: the `random.nextInt(20) == 0` of `checkGuardianSpawnRules`.
const SPAWN_SKY_ROLL: i32 = 20;

/// The mutable guardian state that is neither synchronized nor saved.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(super) struct GuardianState {
    /// Whether the wander goal has been told to re-roll immediately.
    ///
    /// Vanilla parity: `RandomStrollGoal.forceTrigger`, which the guardian
    /// reaches into directly through the reference it keeps. Foton's goals live
    /// behind the selector, so the guardian raises a flag here and
    /// `GuardianRandomStrollGoal` takes it on its next poll.
    pub trigger_stroll: bool,
}

/// What the shared guardian code needs from the mob it is running on.
pub(super) trait GuardianLike: Mob {
    fn guardian_state(&self) -> &SyncMutex<GuardianState>;

    /// Vanilla parity: `guardian instanceof ElderGuardian`.
    fn is_elder(&self) -> bool;

    /// Vanilla parity: `Guardian.getAttackDuration`.
    fn attack_duration(&self) -> i32;

    /// Vanilla parity: `Guardian.isMoving`.
    fn is_moving(&self) -> bool;

    /// Vanilla parity: `Guardian.setMoving`.
    fn set_moving(&self, moving: bool);

    /// Vanilla parity: the `DATA_ID_ATTACK_TARGET` accessor, read back by
    /// `Guardian.hasActiveAttackTarget`.
    fn active_attack_target_id(&self) -> i32;

    /// Vanilla parity: `Guardian.setActiveAttackTarget`.
    fn set_active_attack_target(&self, entity_id: i32);
}

/// The concrete-type callbacks the shared goals need.
///
/// A goal is handed a `&dyn PathfinderMob` and has to recover the guardian; the
/// same trick `cube_common` uses for the slime and the magma cube.
#[derive(Debug, Clone, Copy)]
pub(super) struct GuardianHooks {
    /// Asks the wander goal to re-roll on its next poll.
    pub trigger_stroll: fn(&dyn PathfinderMob),
    /// Consumes a pending re-roll, returning whether there was one.
    pub take_stroll_trigger: fn(&dyn PathfinderMob) -> bool,
    /// Publishes the beam's target to the client, or `0` to clear it.
    pub set_active_attack_target: fn(&dyn PathfinderMob, i32),
    /// Returns how long the beam takes to charge.
    pub attack_duration: fn(&dyn PathfinderMob) -> i32,
    /// Returns whether this is an elder guardian.
    pub is_elder: fn(&dyn PathfinderMob) -> bool,
}

/// Builds the hook table for one concrete guardian type.
pub(super) fn hooks_for<G>() -> GuardianHooks
where
    G: GuardianLike + DowncastType + 'static,
{
    GuardianHooks {
        trigger_stroll: |mob| {
            if let Some(guardian) = mob.downcast_ref::<G>() {
                guardian.guardian_state().lock().trigger_stroll = true;
            }
        },
        take_stroll_trigger: |mob| {
            mob.downcast_ref::<G>().is_some_and(|guardian| {
                let mut state = guardian.guardian_state().lock();
                mem::take(&mut state.trigger_stroll)
            })
        },
        set_active_attack_target: |mob, id| {
            if let Some(guardian) = mob.downcast_ref::<G>() {
                guardian.set_active_attack_target(id);
            }
        },
        attack_duration: |mob| {
            mob.downcast_ref::<G>()
                .map_or(ATTACK_TIME, GuardianLike::attack_duration)
        },
        is_elder: |mob| mob.downcast_ref::<G>().is_some_and(GuardianLike::is_elder),
    }
}

/// Registers the goal set both guardians share.
///
/// Vanilla parity: `Guardian.registerGoals`, including the two `setFlags` calls
/// it makes afterward to give the restriction and stroll goals the look control
/// as well as the move control.
pub(super) fn register_goals<G>(mob_base: &MobBase, stroll_interval: i32)
where
    G: GuardianLike + DowncastType + 'static,
{
    let hooks = hooks_for::<G>();

    {
        let mut goals = mob_base.goal_selector().lock();
        goals.add_goal(4, GuardianAttackGoal::new(hooks));
        goals.add_goal(
            5,
            GuardianMoveTowardsRestrictionGoal::new(MOVE_SPEED_MODIFIER),
        );
        goals.add_goal(
            7,
            GuardianRandomStrollGoal::new(hooks, MOVE_SPEED_MODIFIER, stroll_interval),
        );
        goals.add_goal(8, LookAtPlayerGoal::new(LOOK_AT_PLAYER_RANGE));
        goals.add_goal(
            8,
            LookAtPlayerGoal::new_for_living_entities(
                LOOK_AT_GUARDIAN_RANGE,
                LOOK_AT_GUARDIAN_PROBABILITY,
                |_, target, _| is_guardian(target),
            ),
        );
        goals.add_goal(9, RandomLookAroundGoal::new());
    }

    {
        let mut targets = mob_base.target_selector().lock();
        // Vanilla parity: `Guardian.GuardianAttackSelector`. A guardian shoots
        // players, squid and axolotls, and only ones it is not touching.
        targets.add_goal(
            1,
            NearestAttackableTargetGoal::new_with_interval(
                TARGET_SCAN_INTERVAL,
                true,
                false,
                |targeter, target, _| {
                    let is_prey = target.as_player().is_some()
                        || target.entity_type() == &vanilla_entities::SQUID
                        || target.entity_type() == &vanilla_entities::GLOW_SQUID
                        || target.entity_type() == &vanilla_entities::AXOLOTL;
                    is_prey
                        && targeter.is_some_and(|guardian| {
                            target.position().distance_squared(guardian.position())
                                > TARGET_MIN_RANGE_SQR
                        })
                },
            ),
        );
    }
}

/// Returns whether `target` is one of the two guardians.
fn is_guardian(target: &dyn LivingEntity) -> bool {
    target.entity_type() == &vanilla_entities::GUARDIAN
        || target.entity_type() == &vanilla_entities::ELDER_GUARDIAN
}

/// Vanilla parity: the server half of `Guardian.aiStep`.
///
/// A guardian in water never drowns; a guardian on the ground throws itself
/// about at random until it finds some again.
pub(super) fn ai_step<G: GuardianLike + ?Sized>(guardian: &G) {
    if !LivingEntity::is_alive(guardian) {
        return;
    }

    if guardian.is_in_water() {
        guardian.set_air_supply(WATER_AIR_SUPPLY);
    } else if guardian.on_ground() {
        let flop = || f64::from(rand::random::<f32>().mul_add(2.0, -1.0)) * FLOP_HORIZONTAL_IMPULSE;
        guardian
            .set_velocity(guardian.velocity() + DVec3::new(flop(), FLOP_VERTICAL_IMPULSE, flop()));
        let (_, pitch) = guardian.rotation();
        guardian.set_rotation((rand::random::<f32>() * 360.0, pitch));
        guardian.set_on_ground(false);
        guardian.mark_velocity_sync();
    }

    // Vanilla parity: a firing guardian's body follows its head exactly, which
    // is what keeps the beam pinned to its eye.
    if guardian.active_attack_target_id() != 0 {
        let (_, pitch) = guardian.rotation();
        guardian.set_rotation((guardian.y_head_rot(), pitch));
    }
}

/// Vanilla parity: `Guardian.travelInWater`, which replaces the living swim
/// outright rather than adding to it.
pub(super) fn travel_in_water<G: GuardianLike + ?Sized>(
    guardian: &G,
    input: DVec3,
) -> Option<MoveResult> {
    guardian.move_relative(WATER_TRAVEL_SPEED, input);
    let result = guardian.move_entity(MoverType::SelfMovement, guardian.velocity())?;
    guardian.set_velocity(guardian.velocity() * WATER_TRAVEL_DRAG);

    if !guardian.is_moving() && guardian.target().is_none() {
        guardian.set_velocity(guardian.velocity() + DVec3::new(0.0, -IDLE_SINK_RATE, 0.0));
    }

    Some(result)
}

/// Vanilla parity: `Guardian.getWalkTargetValue`.
///
/// TODO: vanilla falls back to `Mob.getWalkTargetValue` for a block that is not
/// water. Foton's `PathfinderMob::get_walk_target_value` default only
/// implements that darkness formula for animals, so this falls back to `0.0`.
pub(super) fn walk_target_value<G: GuardianLike + ?Sized>(guardian: &G, pos: BlockPos) -> f32 {
    let Some(world) = guardian.level() else {
        return 0.0;
    };
    if !world
        .get_block_state(pos)
        .get_fluid_state()
        .fluid_id
        .has_tag(&FluidTag::WATER)
    {
        return 0.0;
    }

    WATER_WALK_TARGET_VALUE + world.pathfinding_cost_from_light_levels(pos)
}

/// Vanilla parity: the thorns half of `Guardian.hurtServer`, plus the wander
/// re-roll it does on every hit whether the thorns fired or not.
pub(super) fn on_hurt<G: GuardianLike + ?Sized>(
    guardian: &G,
    world: &World,
    source: &DamageSource,
) {
    guardian.guardian_state().lock().trigger_stroll = true;

    if guardian.is_moving()
        || source.is(&DamageTypeTag::AVOIDS_GUARDIAN_THORNS)
        || source.damage_type.key == vanilla_damage_types::THORNS.key
    {
        return;
    }

    let Some(direct_id) = source.direct_entity_id else {
        return;
    };
    let Some(direct) = world.get_entity_by_id(direct_id) else {
        return;
    };
    let Some(living) = direct.as_living_entity() else {
        return;
    };

    let thorns = DamageSource::environment(&vanilla_damage_types::THORNS)
        .with_causing_entity(guardian.id())
        .with_direct_entity(guardian.id());
    living.hurt_server(world, &thorns, THORNS_DAMAGE);
}

/// Vanilla parity: `Guardian` installs a `GuardianMoveControl`.
pub(super) fn tick_move_control<G: GuardianLike>(guardian: &G) {
    let moving = GuardianMoveControl::tick(guardian);
    guardian.set_moving(moving);
}

/// Returns whether a guardian may appear at `pos`.
///
/// Vanilla parity: `Guardian.checkGuardianSpawnRules`.
pub(super) fn check_spawn_rules(
    world: &Arc<World>,
    spawn_reason: EntitySpawnReason,
    pos: BlockPos,
) -> bool {
    if rand::random_range(0..SPAWN_SKY_ROLL) != 0 && can_see_sky_from_below_water(world, pos) {
        return false;
    }
    if world.difficulty() == Difficulty::Peaceful {
        return false;
    }

    let in_water = |at: BlockPos| {
        world
            .get_block_state(at)
            .get_fluid_state()
            .fluid_id
            .has_tag(&FluidTag::WATER)
    };
    (spawn_reason.is_spawner() || in_water(pos)) && in_water(pos.below())
}

/// Returns whether open sky reaches `pos` through nothing but water.
///
/// Vanilla parity: `LevelReader.canSeeSkyFromBelowWater`. Foton has no such
/// default yet; the guardian is its only caller.
fn can_see_sky_from_below_water(world: &Arc<World>, pos: BlockPos) -> bool {
    let sea_level = world.sea_level;
    if pos.y() >= sea_level {
        return world.can_see_sky(pos);
    }

    let scan_point = BlockPos::new(pos.x(), sea_level, pos.z());
    if !world.can_see_sky(scan_point) {
        return false;
    }

    let mut cursor = scan_point.below();
    while cursor.y() > pos.y() {
        let state = world.get_block_state(cursor);
        // Vanilla asks `BlockState.liquid()`, the `LiquidBlock` flag, which in
        // vanilla's block data is water and lava and nothing else.
        let is_liquid_block = state.get_block() == &vanilla_blocks::WATER
            || state.get_block() == &vanilla_blocks::LAVA;
        if state.get_light_dampening() > 0 && !is_liquid_block {
            return false;
        }
        cursor = cursor.below();
    }

    true
}

/// Charges the beam, then lets it go all at once.
///
/// Vanilla parity: `Guardian.GuardianAttackGoal`. The elder keeps firing at
/// anything it can see; an ordinary guardian gives up on a target that has
/// closed to within three blocks.
struct GuardianAttackGoal {
    hooks: GuardianHooks,
    /// Ticks the beam has been charging, starting ten below zero.
    attack_time: i32,
}

impl GuardianAttackGoal {
    const fn new(hooks: GuardianHooks) -> Self {
        Self {
            hooks,
            attack_time: 0,
        }
    }

    /// Looks straight at the target, which is what aims the beam.
    fn look_at_target(mob: &dyn PathfinderMob, target: &SharedEntity) {
        let position = target.position();
        mob.mob_base().controls().lock().look_control.set_look_at(
            DVec3::new(position.x, target.get_eye_y(), position.z),
            ATTACK_LOOK_TURN_RATE,
            ATTACK_LOOK_TURN_RATE,
        );
    }
}

impl Goal for GuardianAttackGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::MOVE | GoalControls::LOOK
    }

    /// Vanilla parity: `GuardianAttackGoal.canUse`.
    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        mob.target().is_some_and(|target| {
            target
                .as_living_entity()
                .is_some_and(LivingEntity::is_alive)
        })
    }

    /// Vanilla parity: `GuardianAttackGoal.canContinueToUse`.
    fn can_continue_to_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        if !self.can_use(mob) {
            return false;
        }
        if (self.hooks.is_elder)(mob) {
            return true;
        }

        mob.target().is_some_and(|target| {
            mob.position().distance_squared(target.position()) > TARGET_MIN_RANGE_SQR
        })
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        self.attack_time = ATTACK_WIND_UP_TICKS;
        mob.mob_base().navigation().lock().stop();
        if let Some(target) = mob.target() {
            Self::look_at_target(mob, &target);
        }
        mob.mark_velocity_sync();
    }

    fn stop(&mut self, mob: &dyn PathfinderMob) {
        (self.hooks.set_active_attack_target)(mob, 0);
        mob.set_target(None);
        (self.hooks.trigger_stroll)(mob);
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    /// Vanilla parity: `GuardianAttackGoal.tick`.
    fn tick(&mut self, mob: &dyn PathfinderMob) {
        let Some(target) = mob.target() else {
            return;
        };

        mob.mob_base().navigation().lock().stop();
        Self::look_at_target(mob, &target);

        if !mob.has_line_of_sight(target.as_ref()) {
            mob.set_target(None);
            return;
        }

        self.attack_time += 1;
        if self.attack_time == 0 {
            (self.hooks.set_active_attack_target)(mob, target.id());
            if !mob.is_silent() {
                mob.broadcast_entity_event(EntityStatus::GuardianAttackSound);
            }
            return;
        }

        if self.attack_time < (self.hooks.attack_duration)(mob) {
            return;
        }

        let Some(world) = mob.level() else {
            return;
        };
        let Some(living_target) = target.as_living_entity() else {
            return;
        };

        let mut magic_damage = BASE_BEAM_DAMAGE;
        if world.difficulty() == Difficulty::Hard {
            magic_damage += HARD_DIFFICULTY_BONUS;
        }
        if (self.hooks.is_elder)(mob) {
            magic_damage += ELDER_BEAM_BONUS;
        }

        let beam = DamageSource::environment(&vanilla_damage_types::INDIRECT_MAGIC)
            .with_causing_entity(mob.id())
            .with_direct_entity(mob.id());
        living_target.hurt_server(world.as_ref(), &beam, magic_damage);
        let _ = mob.do_hurt_target(world.as_ref(), &target);
        mob.set_target(None);
    }
}

/// The wander goal, with the guardian's own re-roll flag and goal controls.
///
/// Vanilla parity: the `RandomStrollGoal` the guardian keeps a reference to,
/// plus the `setFlags(MOVE, LOOK)` it applies to it afterward.
struct GuardianRandomStrollGoal {
    hooks: GuardianHooks,
    stroll: RandomStrollGoal,
}

impl GuardianRandomStrollGoal {
    const fn new(hooks: GuardianHooks, speed_modifier: f64, interval: i32) -> Self {
        Self {
            hooks,
            stroll: RandomStrollGoal::with_interval(speed_modifier, interval),
        }
    }
}

impl Goal for GuardianRandomStrollGoal {
    /// Vanilla parity: `this.randomStrollGoal.setFlags(EnumSet.of(MOVE, LOOK))`.
    fn controls(&self) -> GoalControls {
        GoalControls::MOVE | GoalControls::LOOK
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        if (self.hooks.take_stroll_trigger)(mob) {
            self.stroll.trigger();
        }

        self.stroll.can_use(mob)
    }

    fn can_continue_to_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        self.stroll.can_continue_to_use(mob)
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        self.stroll.start(mob);
    }

    fn stop(&mut self, mob: &dyn PathfinderMob) {
        self.stroll.stop(mob);
    }
}

/// The restriction goal, with the guardian's goal controls.
///
/// Vanilla parity: the `goal.setFlags(EnumSet.of(MOVE, LOOK))` the guardian
/// applies to its `MoveTowardsRestrictionGoal`.
struct GuardianMoveTowardsRestrictionGoal {
    inner: MoveTowardsRestrictionGoal,
}

impl GuardianMoveTowardsRestrictionGoal {
    const fn new(speed_modifier: f64) -> Self {
        Self {
            inner: MoveTowardsRestrictionGoal::new(speed_modifier),
        }
    }
}

impl Goal for GuardianMoveTowardsRestrictionGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::MOVE | GoalControls::LOOK
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        self.inner.can_use(mob)
    }

    fn can_continue_to_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        self.inner.can_continue_to_use(mob)
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        self.inner.start(mob);
    }

    fn stop(&mut self, mob: &dyn PathfinderMob) {
        self.inner.stop(mob);
    }
}

/// Picks the in-water or out-of-water half of a guardian sound pair.
///
/// Vanilla parity: the `isInWater() ? ... : ...` of all four guardian sound
/// getters, which is why a beached guardian sounds like a fish.
pub(super) fn sound_in_or_out_of_water<G: GuardianLike + ?Sized>(
    guardian: &G,
    in_water: SoundEventRef,
    on_land: SoundEventRef,
) -> SoundEventRef {
    if guardian.is_in_water() {
        in_water
    } else {
        on_land
    }
}
