use glam::DVec3;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::sound_events;
use steel_utils::BlockPos;

use super::reduced_tick_delay;
use super::selector::{Goal, GoalControls};
use crate::entity::PathfinderMob;
use crate::fluid::FluidStateExt as _;

/// How far ahead the dolphin checks before it breaches.
///
/// Vanilla parity: `DolphinJumpGoal.STEPS_TO_CHECK`, which skips two and three
/// so the arc is only sampled where it matters.
const STEPS_TO_CHECK: [i32; 6] = [0, 1, 4, 5, 6, 7];

/// Forward push a breach starts with.
const JUMP_FORWARD: f64 = 0.6;
/// Upward push a breach starts with.
const JUMP_UP: f64 = 0.7;
/// Below this squared vertical speed the arc is treated as flat.
const FLAT_ARC_SPEED_SQR: f64 = 0.03;
/// Degrees of pitch below which a flat arc counts as level.
const LEVEL_PITCH: f32 = 10.0;
/// How fast the pitch falls back to level at the top of the arc.
const PITCH_LEVEL_LERP: f32 = 0.2;

/// Leaps clear of the water and arcs back in.
///
/// Vanilla parity: `DolphinJumpGoal`. The dolphin only starts if the water and
/// the air ahead of it are both clear the whole way, which is why one never
/// jumps into a cliff.
pub(crate) struct DolphinJumpGoal {
    interval: i32,
    breached: bool,
}

impl DolphinJumpGoal {
    #[must_use]
    pub(crate) const fn new(interval: i32) -> Self {
        Self {
            interval: reduced_tick_delay(interval),
            breached: false,
        }
    }

    fn water_is_clear(mob: &dyn PathfinderMob, pos: BlockPos) -> bool {
        let Some(world) = mob.level() else {
            return false;
        };
        let state = world.get_block_state(pos);
        state.get_fluid_state().is_water() && !state.blocks_motion()
    }

    fn surface_is_clear(mob: &dyn PathfinderMob, pos: BlockPos) -> bool {
        let Some(world) = mob.level() else {
            return false;
        };
        world.get_block_state(pos.above()).is_air()
            && world.get_block_state(pos.above().above()).is_air()
    }
}

impl Goal for DolphinJumpGoal {
    fn controls(&self) -> GoalControls {
        // Vanilla parity: `JumpGoal` sets MOVE and JUMP.
        GoalControls::MOVE | GoalControls::JUMP
    }

    fn is_interruptable(&self) -> bool {
        false
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        if rand::random_range(0..self.interval.max(1)) != 0 {
            return false;
        }

        let motion = mob.direction_yaw();
        let (step_x, step_z) = motion.offset_xz();
        let mob_pos = mob.block_position();

        STEPS_TO_CHECK.into_iter().all(|step| {
            let pos = mob_pos.offset(step_x * step, 0, step_z * step);
            Self::water_is_clear(mob, pos) && Self::surface_is_clear(mob, pos)
        })
    }

    fn can_continue_to_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        let vertical_speed = mob.velocity().y;
        let pitch = mob.rotation().1;
        let arc_is_over = vertical_speed * vertical_speed < FLAT_ARC_SPEED_SQR
            && pitch != 0.0
            && pitch.abs() < LEVEL_PITCH
            && mob.is_in_water();

        !arc_is_over && !mob.on_ground()
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        let direction = mob.direction_yaw();
        let (step_x, step_z) = direction.offset_xz();
        mob.set_velocity(
            mob.velocity()
                + DVec3::new(
                    f64::from(step_x) * JUMP_FORWARD,
                    JUMP_UP,
                    f64::from(step_z) * JUMP_FORWARD,
                ),
        );
        mob.mob_base().navigation().lock().stop();
    }

    fn stop(&mut self, mob: &dyn PathfinderMob) {
        let (yaw, _) = mob.rotation();
        mob.set_rotation((yaw, 0.0));
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        // Vanilla never clears `breached`, so the jump sound plays once for the
        // life of the goal rather than once per leap.
        let already_breached = self.breached;
        if !already_breached && let Some(world) = mob.level() {
            self.breached = world
                .get_block_state(mob.block_position())
                .get_fluid_state()
                .is_water();
        }

        if self.breached && !already_breached {
            mob.play_sound(&sound_events::ENTITY_DOLPHIN_JUMP, 1.0, 1.0);
        }

        let movement = mob.velocity();
        let (yaw, pitch) = mob.rotation();
        if movement.y * movement.y < FLAT_ARC_SPEED_SQR && pitch != 0.0 {
            mob.set_rotation((yaw, rot_lerp(PITCH_LEVEL_LERP, pitch, 0.0)));
        } else if movement.length() > 1.0e-5 {
            let horizontal = movement.x.hypot(movement.z);
            let rotation = (-movement.y).atan2(horizontal).to_degrees();
            mob.set_rotation((yaw, rotation as f32));
        }
    }
}

/// Vanilla parity: `Mth.rotLerp`.
fn rot_lerp(delta: f32, from: f32, to: f32) -> f32 {
    delta.mul_add(wrap_degrees(to - from), from)
}

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
