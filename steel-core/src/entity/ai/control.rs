//! Mob control state.

use std::f32::consts::{FRAC_PI_2, PI};
use std::sync::Arc;

use glam::{DVec3, Quat, Vec3};
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::vanilla_block_tags::BlockTag;
use steel_registry::vanilla_fluid_tags::FluidTag;
use steel_registry::{REGISTRY, TaggedRegistryExt as _, vanilla_attributes};
use steel_utils::{BlockPos, Direction};

use crate::behavior::{BLOCK_BEHAVIORS, BlockCollisionContext};
use crate::entity::block_effects;
use crate::entity::mob::rotlerp;
use crate::entity::{LivingTravelInput, Mob, collided_with_fluid};
use crate::world::World;

pub(crate) const DEFAULT_LOOK_Y_MAX_ROT_SPEED: f32 = 10.0;
pub(crate) const DEFAULT_LOOK_X_MAX_ROT_ANGLE: f32 = 40.0;
const HEAD_STABLE_ANGLE: f32 = 15.0;
const DELAY_UNTIL_STARTING_TO_FACE_FORWARD: i32 = 10;
const HOW_LONG_IT_TAKES_TO_FACE_FORWARD: f32 = 10.0;

fn wrap_degrees(mut degrees: f32) -> f32 {
    degrees %= 360.0;
    if degrees >= 180.0 {
        degrees -= 360.0;
    }
    if degrees < -180.0 {
        degrees += 360.0;
    }
    degrees
}

pub(crate) fn rotate_towards(from_angle: f32, to_angle: f32, max_rot: f32) -> f32 {
    let diff = wrap_degrees(to_angle - from_angle);
    let diff_clamped = diff.clamp(-max_rot, max_rot);
    from_angle + diff_clamped
}

pub(crate) fn rotate_if_necessary(base_angle: f32, target_angle: f32, max_angle_diff: f32) -> f32 {
    let delta_angle = wrap_degrees(target_angle - base_angle);
    let delta_angle_clamped = delta_angle.clamp(-max_angle_diff, max_angle_diff);
    target_angle - delta_angle_clamped
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MoveControlOperation {
    Wait,
    MoveTo,
    Strafe,
    Jumping,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MoveControl {
    wanted_position: DVec3,
    speed_modifier: f64,
    strafe_forward: f32,
    strafe_right: f32,
    operation: MoveControlOperation,
}

impl MoveControl {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            wanted_position: DVec3::ZERO,
            speed_modifier: 0.0,
            strafe_forward: 0.0,
            strafe_right: 0.0,
            operation: MoveControlOperation::Wait,
        }
    }

    #[must_use]
    pub const fn operation(&self) -> MoveControlOperation {
        self.operation
    }

    /// Returns whether the mob still has somewhere to be.
    ///
    /// Vanilla parity: `MoveControl.hasWanted`, which several goals read to
    /// decide whether the mob is already busy going somewhere.
    #[must_use]
    pub const fn has_wanted(&self) -> bool {
        matches!(self.operation, MoveControlOperation::MoveTo)
    }

    #[must_use]
    pub const fn wanted_position(&self) -> DVec3 {
        self.wanted_position
    }

    #[must_use]
    pub const fn speed_modifier(&self) -> f64 {
        self.speed_modifier
    }

    #[must_use]
    pub const fn strafe_forward(&self) -> f32 {
        self.strafe_forward
    }

    #[must_use]
    pub const fn strafe_right(&self) -> f32 {
        self.strafe_right
    }

    pub fn set_wanted_position(&mut self, position: DVec3, speed_modifier: f64) {
        self.wanted_position = position;
        self.speed_modifier = speed_modifier;
        if self.operation != MoveControlOperation::Jumping {
            self.operation = MoveControlOperation::MoveTo;
        }
    }

    pub const fn strafe(&mut self, forward: f32, right: f32) {
        self.operation = MoveControlOperation::Strafe;
        self.strafe_forward = forward;
        self.strafe_right = right;
        self.speed_modifier = 0.25;
    }

    pub const fn set_wait(&mut self) {
        self.operation = MoveControlOperation::Wait;
    }

    pub const fn set_jumping(&mut self) {
        self.operation = MoveControlOperation::Jumping;
    }
}

impl Default for MoveControl {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JumpControl {
    jump: bool,
}

impl JumpControl {
    #[must_use]
    pub const fn new() -> Self {
        Self { jump: false }
    }

    pub const fn jump(&mut self) {
        self.jump = true;
    }

    /// Returns whether a jump is pending without consuming it.
    ///
    /// Vanilla parity: `Rabbit.RabbitJumpControl.wantJump`, which reads the
    /// protected `jump` field of its superclass.
    #[must_use]
    pub const fn want_jump(self) -> bool {
        self.jump
    }

    /// Clears a pending jump without touching `LivingEntity.jumping`.
    ///
    /// Vanilla parity: the `this.jump = false` of
    /// `Rabbit.RabbitJumpControl.tick`, which replaces the base `tick` outright
    /// so a rabbit that is not jumping is never told to stop jumping.
    pub const fn clear_jump(&mut self) {
        self.jump = false;
    }

    pub const fn tick(&mut self) -> bool {
        let jump = self.jump;
        self.jump = false;
        jump
    }
}

impl Default for JumpControl {
    fn default() -> Self {
        Self::new()
    }
}

/// The extra state a rabbit's jump control keeps on top of [`JumpControl`].
///
/// Vanilla parity: `Rabbit.RabbitJumpControl`. Vanilla swaps the whole control
/// object out on the mob; Steel keeps one [`JumpControl`] in [`MobControls`] and
/// lets the rabbit hold this beside it, so the shared control set stays the same
/// for every other mob. The `tick` half lives on the rabbit because it calls
/// back into `Rabbit.startJumping`.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct RabbitJumpControl {
    can_jump: bool,
}

impl RabbitJumpControl {
    #[must_use]
    pub const fn new() -> Self {
        Self { can_jump: false }
    }

    /// Vanilla parity: `Rabbit.RabbitJumpControl.canJump`.
    #[must_use]
    pub const fn can_jump(self) -> bool {
        self.can_jump
    }

    /// Vanilla parity: `Rabbit.RabbitJumpControl.setCanJump`.
    pub const fn set_can_jump(&mut self, can_jump: bool) {
        self.can_jump = can_jump;
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LookControl {
    wanted_position: DVec3,
    y_max_rot_speed: f32,
    x_max_rot_angle: f32,
    look_at_cooldown: i32,
}

impl LookControl {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            wanted_position: DVec3::ZERO,
            y_max_rot_speed: DEFAULT_LOOK_Y_MAX_ROT_SPEED,
            x_max_rot_angle: DEFAULT_LOOK_X_MAX_ROT_ANGLE,
            look_at_cooldown: 0,
        }
    }

    #[must_use]
    pub const fn wanted_position(&self) -> DVec3 {
        self.wanted_position
    }

    #[must_use]
    pub const fn y_max_rot_speed(&self) -> f32 {
        self.y_max_rot_speed
    }

    #[must_use]
    pub const fn x_max_rot_angle(&self) -> f32 {
        self.x_max_rot_angle
    }

    #[must_use]
    pub const fn is_looking_at_target(&self) -> bool {
        self.look_at_cooldown > 0
    }

    pub const fn set_look_at(
        &mut self,
        position: DVec3,
        y_max_rot_speed: f32,
        x_max_rot_angle: f32,
    ) {
        self.wanted_position = position;
        self.y_max_rot_speed = y_max_rot_speed;
        self.x_max_rot_angle = x_max_rot_angle;
        self.look_at_cooldown = 2;
    }

    pub const fn tick_cooldown(&mut self) -> bool {
        if self.look_at_cooldown <= 0 {
            return false;
        }

        self.look_at_cooldown -= 1;
        true
    }
}

impl Default for LookControl {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BodyRotationInput {
    moving: bool,
    carrying_mob_passenger: bool,
    y_rot: f32,
    y_body_rot: f32,
    y_head_rot: f32,
    max_head_y_rot: f32,
}

impl BodyRotationInput {
    #[must_use]
    pub const fn new(
        moving: bool,
        carrying_mob_passenger: bool,
        y_rot: f32,
        y_body_rot: f32,
        y_head_rot: f32,
        max_head_y_rot: f32,
    ) -> Self {
        Self {
            moving,
            carrying_mob_passenger,
            y_rot,
            y_body_rot,
            y_head_rot,
            max_head_y_rot,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BodyRotationUpdate {
    y_body_rot: f32,
    y_head_rot: f32,
}

impl BodyRotationUpdate {
    #[must_use]
    pub const fn y_body_rot(self) -> f32 {
        self.y_body_rot
    }

    #[must_use]
    pub const fn y_head_rot(self) -> f32 {
        self.y_head_rot
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BodyRotationControl {
    head_stable_time: i32,
    last_stable_y_head_rot: f32,
}

impl BodyRotationControl {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            head_stable_time: 0,
            last_stable_y_head_rot: 0.0,
        }
    }

    #[must_use]
    pub const fn head_stable_time(self) -> i32 {
        self.head_stable_time
    }

    #[must_use]
    pub const fn last_stable_y_head_rot(self) -> f32 {
        self.last_stable_y_head_rot
    }

    pub fn tick(&mut self, input: BodyRotationInput) -> BodyRotationUpdate {
        let mut y_body_rot = input.y_body_rot;
        let mut y_head_rot = input.y_head_rot;

        if input.moving {
            y_body_rot = input.y_rot;
            y_head_rot = rotate_if_necessary(y_head_rot, y_body_rot, input.max_head_y_rot);
            self.last_stable_y_head_rot = y_head_rot;
            self.head_stable_time = 0;
        } else if !input.carrying_mob_passenger {
            if (y_head_rot - self.last_stable_y_head_rot).abs() > HEAD_STABLE_ANGLE {
                self.head_stable_time = 0;
                self.last_stable_y_head_rot = y_head_rot;
                y_body_rot = rotate_if_necessary(y_body_rot, y_head_rot, input.max_head_y_rot);
            } else {
                self.head_stable_time += 1;
                if self.head_stable_time > DELAY_UNTIL_STARTING_TO_FACE_FORWARD {
                    let time_since_starting_to_face_forward =
                        self.head_stable_time - DELAY_UNTIL_STARTING_TO_FACE_FORWARD;
                    let face_forward_fraction = (time_since_starting_to_face_forward as f32
                        / HOW_LONG_IT_TAKES_TO_FACE_FORWARD)
                        .clamp(0.0, 1.0);
                    let angle_remaining_until_facing_forward =
                        input.max_head_y_rot * (1.0 - face_forward_fraction);
                    y_body_rot = rotate_if_necessary(
                        y_body_rot,
                        y_head_rot,
                        angle_remaining_until_facing_forward,
                    );
                }
            }
        }

        BodyRotationUpdate {
            y_body_rot,
            y_head_rot,
        }
    }
}

impl Default for BodyRotationControl {
    fn default() -> Self {
        Self::new()
    }
}

/// Below this much left to turn a swimmer loses no speed at all.
///
/// Vanilla parity: `SmoothSwimmingMoveControl.FULL_SPEED_TURN_THRESHOLD`.
const FULL_SPEED_TURN_THRESHOLD: f32 = 10.0;
/// The span over which the turning penalty ramps to a full stop.
///
/// Vanilla parity: the `50.0F` divisor of `getTurningSpeedFactor`, which is
/// `STOP_TURN_THRESHOLD - FULL_SPEED_TURN_THRESHOLD`.
const TURN_SPEED_FALLOFF: f32 = 50.0;
/// Degrees per tick a swimmer may pitch toward its heading.
///
/// Vanilla parity: the `5.0F` of `SmoothSwimmingMoveControl.tick`.
const SWIMMING_PITCH_RATE: f32 = 5.0;
/// Upward nudge a swimmer with gravity applied gets under water.
const SWIMMING_LIFT: f64 = 0.005;
/// Below this squared distance a swimmer stops rather than steers.
const SWIMMING_MIN_DISTANCE_SQR: f64 = 2.500_000_3e-7;

/// How a mob that swims well steers.
///
/// Vanilla parity: `SmoothSwimmingMoveControl`. A dolphin banks into its turns
/// and loses speed the harder it has to turn, which is what makes it read as a
/// swimmer rather than as a fish being dragged along a path.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SmoothSwimmingMoveControl {
    max_turn_x: f32,
    max_turn_y: f32,
    in_water_speed_modifier: f32,
    outside_water_speed_modifier: f32,
    apply_gravity: bool,
}

impl SmoothSwimmingMoveControl {
    #[must_use]
    pub const fn new(
        max_turn_x: i32,
        max_turn_y: i32,
        in_water_speed_modifier: f32,
        outside_water_speed_modifier: f32,
        apply_gravity: bool,
    ) -> Self {
        Self {
            max_turn_x: max_turn_x as f32,
            max_turn_y: max_turn_y as f32,
            in_water_speed_modifier,
            outside_water_speed_modifier,
            apply_gravity,
        }
    }

    /// Vanilla parity: `SmoothSwimmingMoveControl.getTurningSpeedFactor`.
    #[must_use]
    fn turning_speed_factor(left_to_turn: f32) -> f32 {
        1.0 - ((left_to_turn - FULL_SPEED_TURN_THRESHOLD) / TURN_SPEED_FALLOFF).clamp(0.0, 1.0)
    }

    /// Vanilla parity: `SmoothSwimmingMoveControl.tick`.
    pub fn tick(self, mob: &dyn Mob) {
        if self.apply_gravity && mob.is_in_water() {
            mob.set_velocity(mob.velocity() + DVec3::new(0.0, SWIMMING_LIFT, 0.0));
        }

        let (operation, wanted_position, speed_modifier) = {
            let controls = mob.mob_base().controls().lock();
            let move_control = controls.move_control;
            (
                move_control.operation(),
                move_control.wanted_position(),
                move_control.speed_modifier(),
            )
        };

        let navigating = matches!(operation, MoveControlOperation::MoveTo)
            && !mob.mob_base().navigation().lock().is_done();
        if !navigating {
            mob.set_mob_speed(0.0);
            mob.set_travel_input(LivingTravelInput::new(0.0, 0.0, 0.0));
            return;
        }

        let delta = wanted_position - mob.position();
        if delta.length_squared() < SWIMMING_MIN_DISTANCE_SQR {
            let input = mob.travel_input();
            mob.set_travel_input(LivingTravelInput::new(
                input.sideways(),
                input.vertical(),
                0.0,
            ));
            return;
        }

        let wanted_yaw = (delta.z.atan2(delta.x).to_degrees() as f32) - 90.0;
        let (yaw, pitch) = mob.rotation();
        let turned_yaw = rotlerp(yaw, wanted_yaw, self.max_turn_y);
        mob.set_rotation((turned_yaw, pitch));
        mob.set_y_body_rot(turned_yaw);
        mob.set_y_head_rot(turned_yaw);

        let movement_speed = mob
            .attributes()
            .lock()
            .required_value(vanilla_attributes::MOVEMENT_SPEED);
        let speed = (speed_modifier * movement_speed) as f32;
        if !mob.is_in_water() {
            let left_to_turn = wrap_degrees(mob.rotation().0 - wanted_yaw).abs();
            let turning_speed_factor = Self::turning_speed_factor(left_to_turn);
            mob.set_mob_speed(speed * self.outside_water_speed_modifier * turning_speed_factor);
            return;
        }

        mob.set_mob_speed(speed * self.in_water_speed_modifier);
        let horizontal = delta.x.hypot(delta.z);
        if delta.y.abs() > 1.0e-5 || horizontal.abs() > 1.0e-5 {
            let wanted_pitch = -(delta.y.atan2(horizontal).to_degrees() as f32);
            let wanted_pitch = wrap_degrees(wanted_pitch).clamp(-self.max_turn_x, self.max_turn_x);
            let (yaw, pitch) = mob.rotation();
            mob.set_rotation((
                yaw,
                rotate_towards(pitch, wanted_pitch, SWIMMING_PITCH_RATE),
            ));
        }

        let pitch_radians = mob.rotation().1.to_radians();
        mob.set_travel_input(LivingTravelInput::new(
            0.0,
            -pitch_radians.sin() * speed,
            pitch_radians.cos() * speed,
        ));
    }
}

/// Degrees a swimmer's head leads its look target by.
///
/// Vanilla parity: `SmoothSwimmingLookControl.HEAD_TILT_Y`.
const SMOOTH_SWIMMING_HEAD_TILT_Y: f32 = 20.0;
/// Degrees a swimmer's pitch leads its look target by.
const SMOOTH_SWIMMING_HEAD_TILT_X: f32 = 10.0;
/// Degrees per tick a swimmer levels out over when it has nowhere to look.
const SMOOTH_SWIMMING_LEVEL_RATE: f32 = 5.0;
/// Degrees per tick a swimmer's body follows its head by.
const SMOOTH_SWIMMING_BODY_TURN_RATE: f32 = 4.0;

/// How a mob that swims well looks around.
///
/// Vanilla parity: `SmoothSwimmingLookControl`, which lets the head lead the
/// body and then drags the body after it a few degrees at a time.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SmoothSwimmingLookControl {
    max_y_rot_from_center: f32,
}

impl SmoothSwimmingLookControl {
    #[must_use]
    pub const fn new(max_y_rot_from_center: i32) -> Self {
        Self {
            max_y_rot_from_center: max_y_rot_from_center as f32,
        }
    }

    /// Vanilla parity: `SmoothSwimmingLookControl.tick`.
    pub fn tick(self, mob: &dyn Mob) {
        let look_control = {
            let mut controls = mob.mob_base().controls().lock();
            let look_control = controls.look_control;
            controls.look_control.tick_cooldown();
            look_control
        };

        if look_control.is_looking_at_target() {
            let position = mob.position();
            let wanted_position = look_control.wanted_position();
            let xd = wanted_position.x - position.x;
            let yd = wanted_position.y - mob.get_eye_y();
            let zd = wanted_position.z - position.z;
            let horizontal = xd.hypot(zd);

            if zd.abs() > 1.0e-5 || xd.abs() > 1.0e-5 {
                let wanted_yaw = (zd.atan2(xd).to_degrees() as f32) - 90.0;
                mob.set_y_head_rot(rotate_towards(
                    mob.y_head_rot(),
                    wanted_yaw + SMOOTH_SWIMMING_HEAD_TILT_Y,
                    look_control.y_max_rot_speed(),
                ));
            }

            if yd.abs() > 1.0e-5 || horizontal.abs() > 1.0e-5 {
                let wanted_pitch = -(yd.atan2(horizontal).to_degrees() as f32);
                let (yaw, pitch) = mob.rotation();
                mob.set_rotation((
                    yaw,
                    rotate_towards(
                        pitch,
                        wanted_pitch + SMOOTH_SWIMMING_HEAD_TILT_X,
                        look_control.x_max_rot_angle(),
                    ),
                ));
            }
        } else {
            if mob.mob_base().navigation().lock().is_done() {
                let (yaw, pitch) = mob.rotation();
                mob.set_rotation((yaw, rotate_towards(pitch, 0.0, SMOOTH_SWIMMING_LEVEL_RATE)));
            }

            mob.set_y_head_rot(rotate_towards(
                mob.y_head_rot(),
                mob.y_body_rot(),
                look_control.y_max_rot_speed(),
            ));
        }

        let head_diff_body = wrap_degrees(mob.y_head_rot() - mob.y_body_rot());
        if head_diff_body < -self.max_y_rot_from_center {
            mob.set_y_body_rot(mob.y_body_rot() - SMOOTH_SWIMMING_BODY_TURN_RATE);
        } else if head_diff_body > self.max_y_rot_from_center {
            mob.set_y_body_rot(mob.y_body_rot() + SMOOTH_SWIMMING_BODY_TURN_RATE);
        }
    }
}

/// Degrees per tick a shulker's head drifts back toward its body when it has
/// nothing to look at.
///
/// Vanilla parity: the `10.0F` of the else branch of `LookControl.tick`.
const IDLE_HEAD_RETURN_RATE: f32 = 10.0;

/// How a shulker looks around.
///
/// Vanilla parity: `Shulker.ShulkerLookControl`. A shulker's head turns in the
/// plane of the face it is stuck to rather than around the world's Y axis, it
/// never pitches, and it is never clamped back toward its body -- the body has
/// no meaningful rotation to clamp to.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShulkerLookControl {
    /// The face the shulker is attached to.
    attach_face: Direction,
}

impl ShulkerLookControl {
    #[must_use]
    pub const fn new(attach_face: Direction) -> Self {
        Self { attach_face }
    }

    /// Returns the basis the shulker's head turns in.
    ///
    /// Vanilla parity: the `forward`/`right` pair of
    /// `ShulkerLookControl.getYRotD`, built from `Direction.getRotation`.
    fn face_basis(attach_face: Direction) -> (Vec3, Vec3) {
        let rotation = match attach_face {
            Direction::Down => Quat::from_rotation_x(PI),
            Direction::Up => Quat::IDENTITY,
            Direction::North => Quat::from_rotation_z(PI) * Quat::from_rotation_x(FRAC_PI_2),
            Direction::South => Quat::from_rotation_x(FRAC_PI_2),
            Direction::West => Quat::from_rotation_z(FRAC_PI_2) * Quat::from_rotation_x(FRAC_PI_2),
            Direction::East => Quat::from_rotation_z(-FRAC_PI_2) * Quat::from_rotation_x(FRAC_PI_2),
        };

        // Vanilla parity: `FORWARD` is the south unit vector.
        let forward = rotation * Vec3::new(0.0, 0.0, 1.0);
        let up_normal = attach_face.offset_vec();
        let right =
            Vec3::new(up_normal.x as f32, up_normal.y as f32, up_normal.z as f32).cross(forward);

        (forward, right)
    }

    /// Vanilla parity: `LookControl.tick` with the shulker's three overrides.
    pub fn tick(self, mob: &dyn Mob) {
        let look_control = {
            let mut controls = mob.mob_base().controls().lock();
            let look_control = controls.look_control;
            controls.look_control.tick_cooldown();
            look_control
        };

        let (yaw, _) = mob.rotation();
        // Vanilla parity: `LookControl.resetXRotOnTick` is true for a shulker,
        // so its pitch is zeroed every tick before anything else.
        mob.set_rotation((yaw, 0.0));

        if !look_control.is_looking_at_target() {
            mob.set_y_head_rot(rotate_towards(
                mob.y_head_rot(),
                mob.y_body_rot(),
                IDLE_HEAD_RETURN_RATE,
            ));
            return;
        }

        let (forward, right) = Self::face_basis(self.attach_face.opposite());
        let wanted = look_control.wanted_position();
        let position = mob.position();
        let out = Vec3::new(
            (wanted.x - position.x) as f32,
            (wanted.y - mob.get_eye_y()) as f32,
            (wanted.z - position.z) as f32,
        );
        let delta_right = right.dot(out);
        let delta_forward = forward.dot(out);

        if delta_right.abs() > 1.0e-5 || delta_forward.abs() > 1.0e-5 {
            let wanted_yaw = (-delta_right).atan2(delta_forward).to_degrees();
            mob.set_y_head_rot(rotate_towards(
                mob.y_head_rot(),
                wanted_yaw,
                look_control.y_max_rot_speed(),
            ));
        }

        // Vanilla parity: `ShulkerLookControl.getXRotD` always answers zero.
        let (yaw, pitch) = mob.rotation();
        mob.set_rotation((
            yaw,
            rotate_towards(pitch, 0.0, look_control.x_max_rot_angle()),
        ));
    }
}

/// Moves `current` toward `target` by at most `increment`.
///
/// Vanilla parity: `Mth.approach`.
fn approach(current: f32, target: f32, increment: f32) -> f32 {
    let increment = increment.abs();
    if current < target {
        (current + increment).clamp(current, target)
    } else {
        (current - increment).clamp(target, current)
    }
}

/// Moves an angle toward another the short way round.
///
/// Vanilla parity: `Mth.approachDegrees`.
fn approach_degrees(current: f32, target: f32, increment: f32) -> f32 {
    let difference = wrap_degrees(target - current);
    approach(current, current + difference, increment)
}

/// Fastest a phantom will ever fly.
///
/// Vanilla parity: the `Mth.approach(this.speed, 1.8F, ...)` of
/// `Phantom.PhantomMoveControl.tick`.
const PHANTOM_MAX_SPEED: f32 = 1.8;

/// Speed a phantom drops back to while it is still turning.
///
/// Vanilla parity: the `Mth.approach(this.speed, 0.2F, 0.025F)` of the same
/// method, and the `0.1F` the control starts and resets at.
const PHANTOM_TURNING_SPEED: f32 = 0.2;

/// Speed a phantom resets to when it flies into a wall.
pub const PHANTOM_INITIAL_SPEED: f32 = 0.1;

/// How fast a phantom picks up speed while it is flying straight.
///
/// Vanilla parity: the `0.005F * (1.8F / this.speed)` increment, which is why a
/// slow phantom accelerates hard and a fast one barely gains.
const PHANTOM_ACCELERATION: f32 = 0.005;

/// How fast a phantom sheds speed while it is turning.
const PHANTOM_DECELERATION: f32 = 0.025;

/// Degrees per tick a phantom may turn.
const PHANTOM_MAX_TURN: f32 = 4.0;

/// Below this much turn left a phantom counts as flying straight.
const PHANTOM_STRAIGHT_THRESHOLD: f32 = 3.0;

/// How much a phantom flattens its dive when it is far above or below its
/// target.
///
/// Vanilla parity: the `1.0 - Math.abs(tdy * 0.7F) / sd` of the same method.
const PHANTOM_DIVE_FLATTENING: f64 = 0.7;

/// How much of the gap between its current and wanted velocity a phantom closes
/// each tick.
const PHANTOM_VELOCITY_LERP: f64 = 0.2;

/// How a phantom steers.
///
/// Vanilla parity: `Phantom.PhantomMoveControl`. A phantom ignores the move
/// control's wanted position entirely and flies at the point its goals put in
/// `moveTargetPoint`, banking toward it a few degrees at a time and speeding up
/// only once it is pointed the right way. That is what makes a phantom circle
/// wide and then commit to a dive.
///
/// Returns the phantom's new speed, which vanilla keeps on the control object.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PhantomMoveControl {
    move_target_point: DVec3,
    speed: f32,
}

impl PhantomMoveControl {
    #[must_use]
    pub const fn new(move_target_point: DVec3, speed: f32) -> Self {
        Self {
            move_target_point,
            speed,
        }
    }

    /// Vanilla parity: `PhantomMoveControl.tick`.
    #[must_use]
    pub fn tick(self, mob: &dyn Mob) -> f32 {
        let mut speed = self.speed;
        if mob.horizontal_collision() {
            let (yaw, pitch) = mob.rotation();
            mob.set_rotation((yaw + 180.0, pitch));
            speed = PHANTOM_INITIAL_SPEED;
        }

        let position = mob.position();
        let mut tdx = self.move_target_point.x - position.x;
        let tdy = self.move_target_point.y - position.y;
        let mut tdz = self.move_target_point.z - position.z;
        let mut horizontal = tdx.hypot(tdz);
        if horizontal.abs() <= f64::from(1.0e-5_f32) {
            return speed;
        }

        // Vanilla shortens the horizontal reach when the target is well above
        // or below, which is what turns a shallow approach into a dive.
        let y_relative_scale = 1.0 - (tdy * PHANTOM_DIVE_FLATTENING).abs() / horizontal;
        tdx *= y_relative_scale;
        tdz *= y_relative_scale;
        horizontal = tdx.hypot(tdz);
        let distance = DVec3::new(tdx, tdy, tdz).length();

        let (yaw, _) = mob.rotation();
        let previous_yaw = yaw;
        #[expect(
            clippy::cast_possible_truncation,
            reason = "vanilla stores entity rotation as a float"
        )]
        let wanted_yaw = tdz.atan2(tdx).to_degrees() as f32;
        let turned_yaw = approach_degrees(
            wrap_degrees(yaw + 90.0),
            wrap_degrees(wanted_yaw),
            PHANTOM_MAX_TURN,
        ) - 90.0;
        let (_, pitch) = mob.rotation();
        mob.set_rotation((turned_yaw, pitch));
        mob.set_y_body_rot(turned_yaw);

        if wrap_degrees(turned_yaw - previous_yaw).abs() < PHANTOM_STRAIGHT_THRESHOLD {
            speed = approach(
                speed,
                PHANTOM_MAX_SPEED,
                PHANTOM_ACCELERATION * (PHANTOM_MAX_SPEED / speed),
            );
        } else {
            speed = approach(speed, PHANTOM_TURNING_SPEED, PHANTOM_DECELERATION);
        }

        #[expect(
            clippy::cast_possible_truncation,
            reason = "vanilla stores entity rotation as a float"
        )]
        let wanted_pitch = -(tdy.atan2(horizontal).to_degrees() as f32);
        mob.set_rotation((mob.rotation().0, wanted_pitch));

        let move_angle = f64::from((mob.rotation().0 + 90.0).to_radians());
        let pitch_radians = f64::from(wanted_pitch.to_radians());
        let flight_speed = f64::from(speed);
        let wanted = DVec3::new(
            flight_speed * move_angle.cos() * (tdx / distance).abs(),
            flight_speed * pitch_radians.sin() * (tdy / distance).abs(),
            flight_speed * move_angle.sin() * (tdz / distance).abs(),
        );
        let movement = mob.velocity();
        mob.set_velocity(movement + (wanted - movement) * PHANTOM_VELOCITY_LERP);

        speed
    }
}

/// Degrees per tick a guardian may turn toward its heading.
///
/// Vanilla parity: the `rotlerp(getYRot(), yRotD, 90.0F)` of
/// `Guardian.GuardianMoveControl.tick`, the same cap the base move control uses.
const GUARDIAN_MAX_TURN: f32 = 90.0;

/// How much of the gap to its target speed a guardian closes each tick.
///
/// Vanilla parity: the `Mth.lerp(0.125F, getSpeed(), targetSpeed)` of
/// `Guardian.GuardianMoveControl.tick`.
const GUARDIAN_SPEED_LERP: f32 = 0.125;

/// Amplitude of the sideways sculling a guardian adds to its swimming.
///
/// Vanilla parity: the `* 0.05` of both `push` terms.
const GUARDIAN_SCULL_AMPLITUDE: f64 = 0.05;

/// How fast the horizontal scull oscillates.
const GUARDIAN_SCULL_RATE: f64 = 0.5;

/// How fast the vertical scull oscillates.
const GUARDIAN_BOB_RATE: f64 = 0.75;

/// How much of the vertical scull actually reaches the guardian.
const GUARDIAN_BOB_SCALE: f64 = 0.25;

/// How much of its forward speed a guardian turns into climb or dive.
const GUARDIAN_CLIMB_SCALE: f64 = 0.1;

/// How far ahead a guardian looks while swimming, in blocks.
const GUARDIAN_LOOK_AHEAD: f64 = 2.0;

/// How much of the gap to its new look target a guardian closes each tick.
const GUARDIAN_LOOK_LERP: f64 = 0.125;

/// How fast a swimming guardian's head turns, in degrees per tick.
const GUARDIAN_LOOK_Y_MAX_ROT_SPEED: f32 = 10.0;

/// How far a swimming guardian's head may pitch, in degrees per tick.
const GUARDIAN_LOOK_X_MAX_ROT_ANGLE: f32 = 40.0;

/// How a guardian steers.
///
/// Vanilla parity: `Guardian.GuardianMoveControl`. A guardian does not swim in
/// a straight line: it eases into its speed, sculls from side to side on a sine
/// wave keyed to its own entity id, and drags its own look target along behind
/// its heading. That wobble is what the whole mob reads as.
///
/// Returns whether the guardian is moving, which vanilla writes straight to
/// `Guardian.setMoving`; Steel returns it because the flag lives in the
/// guardian's synchronized data rather than on the control.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GuardianMoveControl;

impl GuardianMoveControl {
    /// Vanilla parity: `GuardianMoveControl.tick`.
    #[must_use]
    pub fn tick(mob: &dyn Mob) -> bool {
        let (operation, wanted_position, speed_modifier) = {
            let controls = mob.mob_base().controls().lock();
            let move_control = controls.move_control;
            (
                move_control.operation(),
                move_control.wanted_position(),
                move_control.speed_modifier(),
            )
        };
        let navigating = matches!(operation, MoveControlOperation::MoveTo)
            && !mob.mob_base().navigation().lock().is_done();
        if !navigating {
            mob.set_mob_speed(0.0);
            return false;
        }

        let position = mob.position();
        let delta = wanted_position - position;
        let length = delta.length();
        let unit = delta / length;

        #[expect(
            clippy::cast_possible_truncation,
            reason = "vanilla stores entity rotation as a float"
        )]
        let wanted_yaw = (delta.z.atan2(delta.x).to_degrees() as f32) - 90.0;
        let (yaw, pitch) = mob.rotation();
        let turned_yaw = rotlerp(yaw, wanted_yaw, GUARDIAN_MAX_TURN);
        mob.set_rotation((turned_yaw, pitch));
        mob.set_y_body_rot(turned_yaw);

        #[expect(
            clippy::cast_possible_truncation,
            reason = "vanilla stores movement speed as a float"
        )]
        let target_speed = (speed_modifier
            * mob
                .attributes()
                .lock()
                .required_value(vanilla_attributes::MOVEMENT_SPEED))
            as f32;
        let new_speed =
            GUARDIAN_SPEED_LERP.mul_add(target_speed - mob.get_speed(), mob.get_speed());
        mob.set_mob_speed(new_speed);

        let phase = f64::from(mob.tick_count() + mob.id());
        let push = (phase * GUARDIAN_SCULL_RATE).sin() * GUARDIAN_SCULL_AMPLITUDE;
        let bob = (phase * GUARDIAN_BOB_RATE).sin() * GUARDIAN_SCULL_AMPLITUDE;
        let yaw_radians = f64::from(mob.rotation().0.to_radians());
        let cos = yaw_radians.cos();
        let sin = yaw_radians.sin();
        mob.set_velocity(
            mob.velocity()
                + DVec3::new(
                    push * cos,
                    bob * (sin + cos) * GUARDIAN_BOB_SCALE
                        + f64::from(new_speed) * unit.y * GUARDIAN_CLIMB_SCALE,
                    push * sin,
                ),
        );

        // Vanilla parity: the guardian eases its own look target along its
        // heading, which is what keeps its eye tracking ahead of it.
        let new_look = DVec3::new(
            position.x + unit.x * GUARDIAN_LOOK_AHEAD,
            mob.get_eye_y() + unit.y / length,
            position.z + unit.z * GUARDIAN_LOOK_AHEAD,
        );
        let mut controls = mob.mob_base().controls().lock();
        let old_look = if controls.look_control.is_looking_at_target() {
            controls.look_control.wanted_position()
        } else {
            new_look
        };
        controls.look_control.set_look_at(
            old_look + (new_look - old_look) * GUARDIAN_LOOK_LERP,
            GUARDIAN_LOOK_Y_MAX_ROT_SPEED,
            GUARDIAN_LOOK_X_MAX_ROT_ANGLE,
        );

        true
    }
}

/// Shortest pause between two nudges of a drifting ghast, in ticks.
///
/// Vanilla parity: the `random.nextInt(5) + 2` of `GhastMoveControl.tick`.
const GHAST_FLOAT_MIN_PAUSE_TICKS: i32 = 2;

/// Span of the random part of that pause.
const GHAST_FLOAT_PAUSE_SPAN: i32 = 5;

/// How hard one nudge pushes, as a multiple of the flying speed attribute.
///
/// Vanilla parity: the `getAttributeValue(FLYING_SPEED) * 5.0 / 3.0` scale.
const GHAST_THRUST_SCALE: f64 = 5.0 / 3.0;

/// How much clearance a careful ghast keeps around where it is heading.
///
/// Vanilla parity: the `aabbAtDestination.inflate(1.0)` of the careful
/// branch of `GhastMoveControl.canReach`.
const CAREFUL_CLEARANCE: f64 = 1.0;

/// How a ghast steers.
///
/// Vanilla parity: `Ghast.GhastMoveControl`. A ghast does not path and does not
/// accelerate smoothly: every few ticks it checks whether the straight line to
/// its destination is clear and, if it is, gives itself one shove along it.
/// That intermittent shove is the whole of a ghast's drift.
///
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GhastMoveControl {
    /// Ticks left before the next shove.
    ///
    /// Vanilla parity: `GhastMoveControl.floatDuration`. Vanilla keeps this on
    /// the control object; Steel's controls are recreated each tick, so the
    /// ghast holds it and hands it in.
    float_duration: i32,
    /// Vanilla parity: `GhastMoveControl.careful`, which a happy ghast sets and
    /// a ghast does not. It is what keeps a mob carrying four players out of
    /// lava, out of walls, and a block clear of both.
    careful: bool,
    /// Vanilla parity: one read of the `shouldBeStopped` supplier. Vanilla asks
    /// a `BooleanSupplier` once at the top of the tick, so the answer is what
    /// the control needs rather than the closure.
    stopped: bool,
}

/// Which fluids a drifting ghast will path through.
///
/// Vanilla parity: the `canPathThroughWater` / `canPathThroughLava` pair of
/// `GhastMoveControl.blockTraversalPossible`, which is only ever the mob's own
/// answer to "am I already in this".
#[derive(Debug, Clone, Copy)]
struct GhastFluidTolerance {
    water: bool,
    lava: bool,
}

impl GhastMoveControl {
    /// Vanilla parity: the `new GhastMoveControl<>(this, false, () -> false)` a
    /// ghast builds for itself.
    #[must_use]
    pub const fn new(float_duration: i32) -> Self {
        Self {
            float_duration,
            careful: false,
            stopped: false,
        }
    }

    /// Vanilla parity: the `careful` constructor argument.
    #[must_use]
    pub const fn careful(mut self) -> Self {
        self.careful = true;
        self
    }

    /// Vanilla parity: what the `shouldBeStopped` supplier answered this tick.
    #[must_use]
    pub const fn stopped(mut self, stopped: bool) -> Self {
        self.stopped = stopped;
        self
    }

    /// Ticks the control and returns the new float duration for the mob to keep.
    ///
    /// Vanilla parity: `GhastMoveControl.tick`.
    #[must_use]
    pub fn tick(self, mob: &dyn Mob) -> i32 {
        if self.stopped {
            mob.mob_base().controls().lock().move_control.set_wait();
            mob.stop_in_place();
        }

        let (operation, wanted_position) = {
            let controls = mob.mob_base().controls().lock();
            (
                controls.move_control.operation(),
                controls.move_control.wanted_position(),
            )
        };
        if operation != MoveControlOperation::MoveTo {
            return self.float_duration;
        }

        // Vanilla parity: `this.floatDuration-- <= 0` tests the old value and
        // keeps the decremented one either way, so a control that arrives on
        // one still waits a tick.
        let remaining = self.float_duration - 1;
        if self.float_duration > 0 {
            return remaining;
        }

        let travel = wanted_position - mob.position();
        if !self.can_reach(mob, travel) {
            mob.mob_base().controls().lock().move_control.set_wait();
            return remaining;
        }

        let flying_speed = mob
            .attributes()
            .lock()
            .required_value(vanilla_attributes::FLYING_SPEED);
        mob.set_velocity(
            mob.velocity() + travel.normalize_or_zero() * (flying_speed * GHAST_THRUST_SCALE),
        );

        remaining + rand::random_range(0..GHAST_FLOAT_PAUSE_SPAN) + GHAST_FLOAT_MIN_PAUSE_TICKS
    }

    /// Returns whether the ghast could slide along `travel` without hitting
    /// anything.
    ///
    /// Vanilla parity: `GhastMoveControl.canReach`.
    fn can_reach(self, mob: &dyn Mob, travel: DVec3) -> bool {
        let Some(world) = mob.level() else {
            return false;
        };
        let aabb = mob.bounding_box();
        let aabb_at_destination = aabb.translate(travel);
        let start = mob.position();
        let end = start + travel;

        if self.careful {
            // Vanilla parity: the `BlockPos.betweenClosed(aabbAtDestination.inflate(1.0))`
            // pre-scan. It runs with no segment and no fluid tolerance, so an
            // occupied block anywhere in the block of clearance around the
            // destination is enough to refuse the whole move.
            let refused = !block_effects::for_each_block_in_aabb(
                aabb_at_destination.inflate(CAREFUL_CLEARANCE),
                |pos| {
                    self.block_traversal_possible(
                        mob,
                        &world,
                        None,
                        pos,
                        GhastFluidTolerance {
                            water: false,
                            lava: false,
                        },
                    )
                },
            );
            if refused {
                return false;
            }
        }

        let tolerance = GhastFluidTolerance {
            water: mob.is_in_water(),
            lava: mob.is_in_lava(),
        };
        block_effects::for_each_block_intersected_between(
            start,
            end,
            aabb_at_destination,
            |pos, _iteration| {
                if aabb.intersects_block(pos) {
                    return true;
                }

                self.block_traversal_possible(mob, &world, Some((start, end)), pos, tolerance)
            },
        )
        .is_some()
    }

    /// Returns whether one block on the way is something the ghast may cross.
    ///
    /// Vanilla parity: `GhastMoveControl.blockTraversalPossible`. Without
    /// `careful` it is the collision test alone; with it, the block tag and the
    /// fluid it holds decide before the collision does.
    fn block_traversal_possible(
        self,
        mob: &dyn Mob,
        world: &Arc<World>,
        segment: Option<(DVec3, DVec3)>,
        pos: BlockPos,
        fluids: GhastFluidTolerance,
    ) -> bool {
        let state = world.get_block_state(pos);
        if state.is_air() {
            return true;
        }

        let shape = BLOCK_BEHAVIORS
            .get_behavior(state.get_block())
            .get_collision_shape(state, world.as_ref(), pos, BlockCollisionContext::empty());
        let path_no_collisions = match segment {
            Some((start, end)) => !block_effects::collided_with_shape_moving_from(
                mob.bounding_box(),
                start,
                end,
                pos,
                shape,
            ),
            None => shape.is_empty(),
        };
        if !self.careful {
            return path_no_collisions;
        }

        if REGISTRY
            .blocks
            .is_in_tag(state.get_block(), &BlockTag::HAPPY_GHAST_AVOIDS)
        {
            return false;
        }

        let fluid_state = state.get_fluid_state();
        if !fluid_state.is_empty()
            && segment.is_none_or(|(start, end)| {
                collided_with_fluid(
                    world,
                    fluid_state,
                    pos,
                    start,
                    end,
                    mob.as_entity_event_source(),
                )
            })
        {
            if fluid_state.fluid_id.has_tag(&FluidTag::WATER) {
                return fluids.water;
            }
            if fluid_state.fluid_id.has_tag(&FluidTag::LAVA) {
                return fluids.lava;
            }
        }

        path_no_collisions
    }
}

/// Fraction of its speed a vex keeps when it arrives.
///
/// Vanilla parity: the `scale(0.5)` of `Vex.VexMoveControl.tick`.
const VEX_ARRIVAL_DAMPING: f64 = 0.5;

/// How much of the remaining gap a vex closes each tick, per speed unit.
///
/// Vanilla parity: the `speedModifier * 0.05 / deltaLength` scale.
const VEX_ACCELERATION: f64 = 0.05;

/// How a vex steers.
///
/// Vanilla parity: `Vex.VexMoveControl`. A vex has no navigation and no
/// gravity: it accelerates straight at whatever point it was given and coasts
/// there, which is what makes it drift through walls in a straight line.
///
/// Unlike the base control this one does not clear the wanted position at the
/// top of the tick -- it clears it only on arrival -- and both of the vex's
/// movement goals read that back through [`MoveControl::operation`], so the
/// difference is the whole of their pacing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VexMoveControl;

impl VexMoveControl {
    /// Vanilla parity: `Vex.VexMoveControl.tick`.
    pub fn tick(mob: &dyn Mob) {
        let (operation, wanted_position, speed_modifier) = {
            let controls = mob.mob_base().controls().lock();
            let move_control = controls.move_control;
            (
                move_control.operation(),
                move_control.wanted_position(),
                move_control.speed_modifier(),
            )
        };
        if operation != MoveControlOperation::MoveTo {
            return;
        }

        let delta = wanted_position - mob.position();
        let delta_length = delta.length();
        if delta_length < mob.bounding_box().size() {
            mob.mob_base().controls().lock().move_control.set_wait();
            mob.set_velocity(mob.velocity() * VEX_ARRIVAL_DAMPING);
            return;
        }

        mob.set_velocity(
            mob.velocity() + delta * (speed_modifier * VEX_ACCELERATION / delta_length),
        );

        // Vanilla faces the target if it has one and its own heading if not.
        let heading = mob.target().map_or_else(
            || {
                let movement = mob.velocity();
                (movement.x, movement.z)
            },
            |target| {
                let position = mob.position();
                let target_position = target.position();
                (
                    target_position.x - position.x,
                    target_position.z - position.z,
                )
            },
        );
        let y_rot = -(heading.0.atan2(heading.1).to_degrees() as f32);
        let (_, pitch) = mob.rotation();
        mob.set_rotation((y_rot, pitch));
        mob.set_y_body_rot(y_rot);
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct MobControls {
    pub move_control: MoveControl,
    pub jump_control: JumpControl,
    pub look_control: LookControl,
    pub body_rotation_control: BodyRotationControl,
}

impl MobControls {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            move_control: MoveControl::new(),
            jump_control: JumpControl::new(),
            look_control: LookControl::new(),
            body_rotation_control: BodyRotationControl::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{BodyRotationControl, BodyRotationInput};

    fn assert_f32_close(left: f32, right: f32) {
        assert!(
            (left - right).abs() < 1.0e-6,
            "expected {left:?} to equal {right:?}"
        );
    }

    #[test]
    fn body_rotation_control_faces_body_forward_while_moving() {
        let mut control = BodyRotationControl::new();

        let update = control.tick(BodyRotationInput::new(true, false, 90.0, 0.0, 200.0, 75.0));

        assert_f32_close(update.y_body_rot(), 90.0);
        assert_f32_close(update.y_head_rot(), 165.0);
        assert_eq!(control.head_stable_time(), 0);
        assert_f32_close(control.last_stable_y_head_rot(), 165.0);
    }

    #[test]
    fn body_rotation_control_turns_body_when_idle_head_moves() {
        let mut control = BodyRotationControl::new();

        let update = control.tick(BodyRotationInput::new(false, false, 0.0, 0.0, 90.0, 75.0));

        assert_f32_close(update.y_body_rot(), 15.0);
        assert_f32_close(update.y_head_rot(), 90.0);
        assert_eq!(control.head_stable_time(), 0);
        assert_f32_close(control.last_stable_y_head_rot(), 90.0);
    }

    #[test]
    fn body_rotation_control_waits_then_turns_body_toward_stable_head() {
        let mut control = BodyRotationControl::new();
        let first = control.tick(BodyRotationInput::new(false, false, 0.0, 0.0, 90.0, 75.0));
        let mut y_body_rot = first.y_body_rot();

        for _ in 0..10 {
            y_body_rot = control
                .tick(BodyRotationInput::new(
                    false, false, 0.0, y_body_rot, 90.0, 75.0,
                ))
                .y_body_rot();
        }

        assert_eq!(control.head_stable_time(), 10);
        assert_f32_close(y_body_rot, 15.0);

        let update = control.tick(BodyRotationInput::new(
            false, false, 0.0, y_body_rot, 90.0, 75.0,
        ));

        assert_eq!(control.head_stable_time(), 11);
        assert_f32_close(update.y_body_rot(), 22.5);
    }

    #[test]
    fn body_rotation_control_does_not_turn_idle_body_when_carrying_mob_passenger() {
        let mut control = BodyRotationControl::new();

        let update = control.tick(BodyRotationInput::new(false, true, 0.0, 0.0, 90.0, 75.0));

        assert_f32_close(update.y_body_rot(), 0.0);
        assert_f32_close(update.y_head_rot(), 90.0);
        assert_eq!(control.head_stable_time(), 0);
        assert_f32_close(control.last_stable_y_head_rot(), 0.0);
    }
}
