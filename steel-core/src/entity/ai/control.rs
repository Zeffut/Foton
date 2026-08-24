//! Mob control state.

use glam::DVec3;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::vanilla_attributes;

use crate::behavior::{BLOCK_BEHAVIORS, BlockCollisionContext};
use crate::entity::block_effects;
use crate::entity::mob::rotlerp;
use crate::entity::{LivingTravelInput, Mob};

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
    pub fn tick(self, mob: &dyn Mob) -> bool {
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

/// How a ghast steers.
///
/// Vanilla parity: `Ghast.GhastMoveControl`. A ghast does not path and does not
/// accelerate smoothly: every few ticks it checks whether the straight line to
/// its destination is clear and, if it is, gives itself one shove along it.
/// That intermittent shove is the whole of a ghast's drift.
///
/// Vanilla's control also carries a `careful` flag and a `shouldBeStopped`
/// supplier. Both exist for the happy ghast -- a ghast constructs it with
/// `(this, false, () -> false)` -- so neither is modeled here; add them with
/// that mob.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GhastMoveControl {
    /// Ticks left before the next shove.
    ///
    /// Vanilla parity: `GhastMoveControl.floatDuration`. Vanilla keeps this on
    /// the control object; Steel's controls are recreated each tick, so the
    /// ghast holds it and hands it in.
    float_duration: i32,
}

impl GhastMoveControl {
    #[must_use]
    pub const fn new(float_duration: i32) -> Self {
        Self { float_duration }
    }

    /// Ticks the control and returns the new float duration for the mob to keep.
    ///
    /// Vanilla parity: `GhastMoveControl.tick`.
    #[must_use]
    pub fn tick(self, mob: &dyn Mob) -> i32 {
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

        let remaining = self.float_duration - 1;
        if remaining > 0 {
            return remaining;
        }

        let travel = wanted_position - mob.position();
        if !Self::can_reach(mob, travel) {
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
    /// Vanilla parity: `GhastMoveControl.canReach` with `careful` false, which
    /// reduces `blockTraversalPossible` to the collision test alone.
    fn can_reach(mob: &dyn Mob, travel: DVec3) -> bool {
        let Some(world) = mob.level() else {
            return false;
        };
        let aabb = mob.bounding_box();
        let aabb_at_destination = aabb.translate(travel);
        let start = mob.position();
        let end = start + travel;

        block_effects::for_each_block_intersected_between(
            start,
            end,
            aabb_at_destination,
            |pos, _iteration| {
                if aabb.intersects_block(pos) {
                    return true;
                }

                let state = world.get_block_state(pos);
                if state.is_air() {
                    return true;
                }

                let shape = BLOCK_BEHAVIORS
                    .get_behavior(state.get_block())
                    .get_collision_shape(
                        state,
                        world.as_ref(),
                        pos,
                        BlockCollisionContext::empty(),
                    );
                !block_effects::collided_with_shape_moving_from(aabb, start, end, pos, shape)
            },
        )
        .is_some()
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
    pub fn tick(self, mob: &dyn Mob) {
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
