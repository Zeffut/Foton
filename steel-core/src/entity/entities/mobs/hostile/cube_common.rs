//! What a slime and a magma cube share.
//!
//! Vanilla parity: `AbstractCubeMob`. A cube does not walk: it hops, steering
//! by turning in place between hops, and the size decides everything else --
//! health, speed, hitbox, attack damage, whether it hurts on contact, and how
//! many smaller cubes it leaves behind.
//!
//! Steel has no swappable move control, so vanilla's `CubeMobMoveControl` is
//! [`tick_move_control`] here, driven by state the goals set. The four goals
//! reach their cube through [`CubeHooks`], because a goal is handed a
//! `&dyn PathfinderMob` and has to recover the concrete type; everything else
//! is a free function over `C: CubeLike` and needs no downcast at all.

use std::sync::Arc;

use glam::DVec3;
use steel_registry::entity_type::EntityDimensions;
use steel_registry::sound_event::SoundEventRef;
use steel_registry::{sound_events, vanilla_attributes};
use steel_utils::Downcast as _;
use steel_utils::locks::SyncMutex;

use crate::entity::SharedEntity;
use crate::entity::ai::goal::{Goal, GoalControls};
use crate::entity::living_base::LivingTravelInput;
use crate::entity::mob::rotlerp;
use crate::entity::{Entity, Mob, PathfinderMob};
use crate::world::World;

/// Largest size a cube may be set to.
///
/// Vanilla parity: the `Mth.clamp(size, 1, 127)` of `setSize`.
pub(super) const MAX_SIZE: i32 = 127;

/// Size at or below which a cube is the smallest kind.
///
/// Vanilla parity: `AbstractCubeMob.isTiny`.
pub(super) const TINY_SIZE: i32 = 1;

/// Speed a cube of size zero would move at.
///
/// Vanilla parity: the `0.2F` base of `setSize`.
const BASE_SPEED: f64 = 0.2;

/// Extra speed each size step adds.
///
/// Vanilla parity: the `0.1F * size` of `setSize`. A big cube is genuinely
/// faster than a small one, which is why they are dangerous in the open.
const SPEED_PER_SIZE: f64 = 0.1;

/// How far a cube turns toward its target each tick while chasing.
///
/// Vanilla parity: the `10.0F` of `lookAt(target, 10.0F, 10.0F)`.
const LOOK_TURN_RATE: f32 = 10.0;

/// How far a cube turns toward its chosen heading each tick.
///
/// Vanilla parity: the `90.0F` of `CubeMobMoveControl.tick`.
const TURN_RATE: f32 = 90.0;

/// How long a cube keeps chasing before it loses interest.
///
/// Vanilla parity: the `growTiredTimer = 300` of `CubeMobAttackGoal`.
const GROW_TIRED_TICKS: i32 = 300;

/// Chance per tick that a cube in a fluid pushes itself upward.
///
/// Vanilla parity: the `nextFloat() < 0.8F` of `CubeMobFloatGoal`.
const FLOAT_JUMP_CHANCE: f32 = 0.8;

/// Speed a cube asks for while floating out of a fluid.
const FLOAT_SPEED: f64 = 1.2;

/// Shortest wait before a cube picks a new heading.
const REDIRECT_MIN_TICKS: i32 = 40;

/// Extra random wait on top of that.
const REDIRECT_SPREAD_TICKS: i32 = 60;

/// Volume scale per size step.
///
/// Vanilla parity: `AbstractCubeMob.getSoundVolume`.
const VOLUME_PER_SIZE: f32 = 0.4;

/// The hopping state vanilla splits between the mob and its move control.
#[derive(Debug, Default)]
pub(super) struct CubeState {
    /// Whether the cube was on the ground last tick, for the squash sound.
    pub was_on_ground: bool,
    /// Heading the goals have asked for, in degrees.
    pub wanted_y_rot: f32,
    /// Whether the cube is chasing something, which shortens the hop delay.
    pub is_aggressive: bool,
    /// Speed multiplier the goals have asked for, if any.
    pub wanted_movement: Option<f64>,
    /// Ticks until the next hop.
    pub jump_delay: i32,
}

/// What a concrete cube exposes to the shared code.
pub(super) trait CubeLike: Mob {
    /// Returns the hopping state.
    fn cube_state(&self) -> &SyncMutex<CubeState>;

    /// Returns this cube's size.
    fn size(&self) -> i32;

    /// Writes the synced size, and nothing else.
    ///
    /// [`apply_size`] does everything the size decides; this is only the store.
    fn store_size(&self, size: i32);

    /// Sets the size, and with it everything the size decides.
    ///
    /// Vanilla parity: `AbstractCubeMob.setSize`, plus whatever the subclass
    /// adds on top of it.
    fn set_size(&self, size: i32, update_health: bool) {
        apply_size(self, size, update_health);
    }

    /// Returns whether this cube hurts what it touches.
    ///
    /// Vanilla parity: `AbstractCubeMob.isDealsDamage`. A tiny slime is
    /// harmless, which is the whole reason players let them pile up -- and the
    /// magma cube overrides exactly this to say that a tiny one is not.
    fn deals_damage(&self) -> bool {
        !self.is_tiny() && self.is_effective_ai()
    }

    /// Returns ticks until the next hop.
    ///
    /// Vanilla parity: `AbstractCubeMob.getJumpDelay`.
    fn jump_delay(&self) -> i32 {
        rand::random_range(0..20) + 10
    }

    /// Returns the noise this cube makes leaving the ground.
    fn jump_sound(&self) -> SoundEventRef;

    /// Returns the noise this cube makes landing.
    fn squish_sound(&self) -> SoundEventRef;

    /// Creates one child for a split, already sized and turned.
    fn split_child(&self, position: DVec3, world: &Arc<World>) -> SharedEntity;

    /// Returns whether this cube is the smallest kind.
    fn is_tiny(&self) -> bool {
        self.size() <= TINY_SIZE
    }

    /// Returns the health a cube of this size has.
    ///
    /// Vanilla parity: `AbstractCubeMob.setcubeMobHealth`, which squares the
    /// size. The sulfur cube overrides exactly this and grows linearly instead,
    /// which is why it is a hook rather than a line of [`apply_size`].
    fn max_health_for_size(&self, size: i32) -> f64 {
        f64::from(size * size)
    }
}

/// How a shared goal reaches the cube it was handed.
///
/// A goal receives a `&dyn PathfinderMob`, so the concrete type has to be
/// recovered by downcast. Each cube builds these once with [`hooks_for`], and
/// that call is the only place the type is named.
#[derive(Clone, Copy)]
pub(super) struct CubeHooks {
    /// Points the cube at a heading, and says whether it is chasing.
    pub set_heading: fn(&dyn PathfinderMob, f32, bool),
    /// Asks for a speed on the next hop.
    pub set_wanted_movement: fn(&dyn PathfinderMob, f64),
    /// Asks for the default speed, unless a goal already asked for one.
    pub request_default_movement: fn(&dyn PathfinderMob),
    /// Returns whether the cube hurts what it touches.
    pub deals_damage: fn(&dyn PathfinderMob) -> bool,
}

/// Builds the hooks for one concrete cube type.
pub(super) fn hooks_for<C>() -> CubeHooks
where
    C: CubeLike + steel_utils::DowncastType + 'static,
{
    CubeHooks {
        set_heading: |mob, heading, aggressive| {
            if let Some(cube) = mob.downcast_ref::<C>() {
                let mut state = cube.cube_state().lock();
                state.wanted_y_rot = heading;
                state.is_aggressive = aggressive;
            }
        },
        set_wanted_movement: |mob, speed| {
            if let Some(cube) = mob.downcast_ref::<C>() {
                cube.cube_state().lock().wanted_movement = Some(speed);
            }
        },
        request_default_movement: |mob| {
            if let Some(cube) = mob.downcast_ref::<C>() {
                let mut state = cube.cube_state().lock();
                if state.wanted_movement.is_none() {
                    state.wanted_movement = Some(1.0);
                }
            }
        },
        deals_damage: |mob| mob.downcast_ref::<C>().is_some_and(CubeLike::deals_damage),
    }
}

/// Applies everything the size decides.
///
/// Vanilla parity: `AbstractCubeMob.setSize` exactly -- health and speed and
/// nothing else. The attack damage and the experience reward are each
/// subclass's own `setSize` override, because `SulfurCube` overrides `setSize`
/// too and deliberately sets neither.
pub(super) fn apply_size<C: CubeLike + ?Sized>(cube: &C, size: i32, update_health: bool) {
    let size = size.clamp(1, MAX_SIZE);
    cube.store_size(size);
    cube.refresh_dimensions();

    {
        let mut attributes = cube.attributes().lock();
        attributes.set_base_value(
            vanilla_attributes::MAX_HEALTH,
            cube.max_health_for_size(size),
        );
        attributes.set_base_value(
            vanilla_attributes::MOVEMENT_SPEED,
            SPEED_PER_SIZE.mul_add(f64::from(size), BASE_SPEED),
        );
    }

    if update_health {
        cube.set_health(cube.get_max_health());
    }
}

/// Returns this cube's hitbox.
///
/// Vanilla parity: `AbstractCubeMob.getDefaultDimensions`, the type's own
/// scaled by the size.
pub(super) fn dimensions_for_size<C: CubeLike>(cube: &C) -> EntityDimensions {
    #[expect(
        clippy::cast_precision_loss,
        reason = "size is clamped to 127 and scales a hitbox"
    )]
    let scale = cube.size() as f32;
    cube.entity_type().dimensions.scale(scale)
}

/// Vanilla parity: `AbstractCubeMob.getSoundVolume`.
pub(super) fn sound_volume<C: CubeLike>(cube: &C) -> f32 {
    #[expect(
        clippy::cast_precision_loss,
        reason = "size is clamped to 127 and scales a volume"
    )]
    let size = cube.size() as f32;
    VOLUME_PER_SIZE * size
}

/// Vanilla parity: `AbstractCubeMob.getSoundPitch`.
pub(super) fn sound_pitch<C: CubeLike>(cube: &C) -> f32 {
    let adjuster = if cube.is_tiny() { 1.4 } else { 0.8 };
    (rand::random::<f32>() - rand::random::<f32>()).mul_add(0.2, 1.0) * adjuster
}

/// Plays the squash on landing.
///
/// Vanilla parity: the landing half of `AbstractCubeMob.tick`. The sound is the
/// observable part; the squish animation is client-side.
pub(super) fn tick_landing<C: CubeLike>(cube: &C) {
    let on_ground = cube.on_ground();
    let landed = {
        let mut state = cube.cube_state().lock();
        let landed = on_ground && !state.was_on_ground;
        state.was_on_ground = on_ground;
        landed
    };
    if landed {
        cube.play_sound(cube.squish_sound(), sound_volume(cube), sound_pitch(cube));
    }
}

/// Hurts a target the cube is touching.
///
/// Vanilla parity: `AbstractCubeMob.dealDamage`.
pub(super) fn deal_damage<C: CubeLike>(cube: &C, world: &Arc<World>, target: &SharedEntity) {
    if !Entity::is_alive(cube) {
        return;
    }
    let Some(living) = target.as_living_entity() else {
        return;
    };
    if !cube.is_within_melee_attack_range(living) || !cube.has_line_of_sight(target.as_ref()) {
        return;
    }
    if cube.mob_do_hurt_target(world, target) {
        cube.play_sound(&sound_events::ENTITY_SLIME_ATTACK, 1.0, 1.0);
    }
}

/// Hurts the player who walked into the cube.
///
/// Vanilla parity: `AbstractCubeMob.playerTouch`.
pub(super) fn player_touch<C: CubeLike>(cube: &C, player: &SharedEntity) {
    if !cube.deals_damage() {
        return;
    }
    let Some(world) = cube.level() else {
        return;
    };
    deal_damage(cube, &world, player);
}

/// Splits into smaller cubes.
///
/// Vanilla parity: the `remove` override of `AbstractCubeMob`. The children are
/// placed on the corners of the parent's footprint, which is why a big cube
/// bursts outward rather than stacking.
pub(super) fn split_on_death<C: CubeLike>(cube: &C, world: &Arc<World>) {
    if cube.size() <= 1 {
        return;
    }

    // Vanilla parity: `getSplitCount`, two to four children.
    let count = 2 + rand::random_range(0..3);
    let width = f64::from(cube.dimensions_for_pose(cube.pose()).width);
    let offset = width / 2.0;
    let origin = cube.position();

    for index in 0..count {
        let dx = (f64::from(index % 2) - 0.5) * offset;
        let dz = (f64::from(index / 2) - 0.5) * offset;
        let position = DVec3::new(origin.x + dx, origin.y + 0.5, origin.z + dz);

        let child = cube.split_child(position, world);
        if let Err(error) = world.try_add_entity(child) {
            log::debug!("cube split rejected: {error}");
        }
    }
}

/// Rolls the size a naturally spawned cube appears at.
///
/// Vanilla parity: `AbstractCubeMob.setSpawnSize`. Harder difficulties tilt the
/// roll upward, so a hard-mode swamp has more big slimes in it.
pub(super) fn set_spawn_size<C: CubeLike>(cube: &C, world: &Arc<World>) {
    let mut size_scale = rand::random_range(0..3);
    let difficulty = world.get_current_difficulty_at(cube.block_position());
    if size_scale < 2 && rand::random::<f32>() < 0.5 * difficulty.special_multiplier() {
        size_scale += 1;
    }
    cube.set_size(1 << size_scale, true);
}

/// Clears the walking input, which is how a cube waits between hops.
///
/// Vanilla parity: the `xxa = 0; zza = 0` of `CubeMobMoveControl.tick`.
fn stop_traveling<C: CubeLike>(cube: &C) {
    let input = cube.travel_input();
    cube.set_travel_input(LivingTravelInput::new(0.0, input.vertical(), 0.0));
}

/// Turns in place and hops, instead of walking a path.
///
/// Vanilla parity: `AbstractCubeMob.CubeMobMoveControl.tick`. A cube only moves
/// while airborne, so the hop cadence is its speed; chasing shortens the delay
/// to a third, which is what makes a hunting cube close in.
pub(super) fn tick_move_control<C: CubeLike>(cube: &C) {
    let (wanted_y_rot, is_aggressive, wanted_movement) = {
        let mut state = cube.cube_state().lock();
        (
            state.wanted_y_rot,
            state.is_aggressive,
            state.wanted_movement.take(),
        )
    };

    let (yaw, pitch) = cube.rotation();
    let turned = rotlerp(yaw, wanted_y_rot, TURN_RATE);
    cube.set_rotation((turned, pitch));
    cube.set_y_head_rot(turned);
    cube.set_y_body_rot(turned);

    let Some(speed_modifier) = wanted_movement else {
        stop_traveling(cube);
        return;
    };

    let movement_speed = cube
        .attributes()
        .lock()
        .required_value(vanilla_attributes::MOVEMENT_SPEED);
    #[expect(
        clippy::cast_possible_truncation,
        reason = "movement speed is a small attribute value"
    )]
    let speed = (speed_modifier * movement_speed) as f32;

    if !cube.on_ground() {
        cube.set_mob_speed(speed);
        return;
    }

    let jump_now = {
        let mut state = cube.cube_state().lock();
        state.jump_delay -= 1;
        if state.jump_delay <= 0 {
            state.jump_delay = cube.jump_delay();
            if is_aggressive {
                state.jump_delay /= 3;
            }
            true
        } else {
            false
        }
    };

    if jump_now {
        cube.set_mob_speed(speed);
        cube.jump_control_jump();
        cube.play_sound(cube.jump_sound(), sound_volume(cube), sound_pitch(cube));
    } else {
        stop_traveling(cube);
        cube.set_mob_speed(0.0);
    }
}

/// Hops out of water or lava.
///
/// Vanilla parity: `AbstractCubeMob.CubeMobFloatGoal`.
pub(super) struct CubeFloatGoal {
    hooks: CubeHooks,
}

impl CubeFloatGoal {
    #[must_use]
    pub(super) const fn new(hooks: CubeHooks) -> Self {
        Self { hooks }
    }
}

impl Goal for CubeFloatGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::JUMP | GoalControls::MOVE
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        mob.is_in_water() || mob.is_in_lava()
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        if rand::random::<f32>() < FLOAT_JUMP_CHANCE {
            mob.jump_control_jump();
        }
        (self.hooks.set_wanted_movement)(mob, FLOAT_SPEED);
    }
}

/// Faces whatever the cube is chasing and hops at it.
///
/// Vanilla parity: `AbstractCubeMob.CubeMobAttackGoal`.
pub(super) struct CubeAttackGoal {
    hooks: CubeHooks,
    grow_tired_timer: i32,
}

impl CubeAttackGoal {
    #[must_use]
    pub(super) const fn new(hooks: CubeHooks) -> Self {
        Self {
            hooks,
            grow_tired_timer: 0,
        }
    }
}

impl Goal for CubeAttackGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::LOOK
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        mob.target().is_some()
    }

    fn start(&mut self, _mob: &dyn PathfinderMob) {
        self.grow_tired_timer = GROW_TIRED_TICKS;
    }

    fn can_continue_to_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        if mob.target().is_none() {
            return false;
        }
        self.grow_tired_timer -= 1;
        self.grow_tired_timer > 0
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        let Some(target) = mob.target() else {
            return;
        };

        // Vanilla parity: `lookAt(target, 10.0F, 10.0F)`, a rate-limited turn
        // rather than a snap, and then the move control is handed whatever yaw
        // that reached. Snapping here would let a cube pivot instantly.
        let to_target = target.position() - mob.position();
        #[expect(
            clippy::cast_possible_truncation,
            reason = "an angle in degrees, immediately used as a rotation"
        )]
        let wanted = -(to_target.x.atan2(to_target.z).to_degrees() as f32);
        let (yaw, pitch) = mob.rotation();
        let turned = rotlerp(yaw, wanted, LOOK_TURN_RATE);
        mob.set_rotation((turned, pitch));

        (self.hooks.set_heading)(mob, turned, (self.hooks.deals_damage)(mob));
    }
}

/// Picks a new heading every couple of seconds.
///
/// Vanilla parity: `AbstractCubeMob.CubeMobRandomDirectionGoal`.
pub(super) struct CubeRandomDirectionGoal {
    hooks: CubeHooks,
    chosen_degrees: f32,
    next_randomize_time: i32,
}

impl CubeRandomDirectionGoal {
    #[must_use]
    pub(super) const fn new(hooks: CubeHooks) -> Self {
        Self {
            hooks,
            chosen_degrees: 0.0,
            next_randomize_time: 0,
        }
    }
}

impl Goal for CubeRandomDirectionGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::LOOK
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        mob.target().is_none() && (mob.on_ground() || mob.is_in_water() || mob.is_in_lava())
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        self.next_randomize_time -= 1;
        if self.next_randomize_time <= 0 {
            self.next_randomize_time =
                REDIRECT_MIN_TICKS + rand::random_range(0..REDIRECT_SPREAD_TICKS);
            #[expect(
                clippy::cast_precision_loss,
                reason = "a whole-degree heading below 360"
            )]
            let chosen = rand::random_range(0..360) as f32;
            self.chosen_degrees = chosen;
        }

        (self.hooks.set_heading)(mob, self.chosen_degrees, false);
    }
}

/// Keeps the cube hopping when nothing else asks it to.
///
/// Vanilla parity: `AbstractCubeMob.CubeMobKeepOnJumpingGoal`.
pub(super) struct CubeKeepOnJumpingGoal {
    hooks: CubeHooks,
}

impl CubeKeepOnJumpingGoal {
    #[must_use]
    pub(super) const fn new(hooks: CubeHooks) -> Self {
        Self { hooks }
    }
}

impl Goal for CubeKeepOnJumpingGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::JUMP | GoalControls::MOVE
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        !mob.is_passenger()
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        (self.hooks.request_default_movement)(mob);
    }
}
