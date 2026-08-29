//! Vanilla `RandomLookAround`.

use foton_utils::value_providers::UniformIntProvider;
use glam::DVec3;

use super::{BrainContext, MemoryModuleId, MemoryStatus, TimedBehavior};
use crate::entity::ai::brain::memory::memory_module_types;
use crate::entity::ai::brain::position_tracker::PositionTracker;

/// Vanilla parity: the `Mth.clamp(..., -90.0F, 90.0F)` of `start`.
const MIN_PITCH: f32 = -90.0;
const MAX_PITCH: f32 = 90.0;

/// Looks somewhere at random, and then not again for a while.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.behavior.RandomLookAround`.
/// The gaze cooldown it sets is what keeps a mob's head still between glances
/// rather than swiveling every tick.
pub struct RandomLookAround {
    interval: UniformIntProvider,
    max_yaw: f32,
    min_pitch: f32,
    pitch_range: f32,
}

/// Vanilla parity: the `entryCondition` map built in the constructor.
const ENTRY_CONDITION: &[(MemoryModuleId, MemoryStatus)] = &[
    (
        memory_module_types::LOOK_TARGET.id(),
        MemoryStatus::ValueAbsent,
    ),
    (
        memory_module_types::GAZE_COOLDOWN_TICKS.id(),
        MemoryStatus::ValueAbsent,
    ),
];

impl RandomLookAround {
    /// Vanilla parity: `new RandomLookAround(IntProvider, float, float, float)`.
    ///
    /// # Panics
    ///
    /// If `min_pitch` is above `max_pitch`, which is the same
    /// `IllegalArgumentException` vanilla's constructor throws.
    #[must_use]
    pub fn new(interval: UniformIntProvider, max_yaw: f32, min_pitch: f32, max_pitch: f32) -> Self {
        assert!(
            min_pitch <= max_pitch,
            "Minimum pitch is larger than maximum pitch! {min_pitch} > {max_pitch}"
        );
        Self {
            interval,
            max_yaw,
            min_pitch,
            pitch_range: max_pitch - min_pitch,
        }
    }
}

impl TimedBehavior for RandomLookAround {
    fn entry_condition(&self) -> &[(MemoryModuleId, MemoryStatus)] {
        ENTRY_CONDITION
    }

    fn start(&mut self, ctx: &BrainContext<'_>) {
        let body = ctx.mob();
        let pitch =
            (rand::random::<f32>() * self.pitch_range + self.min_pitch).clamp(MIN_PITCH, MAX_PITCH);
        let yaw = wrap_degrees(
            body.rotation().0 + 2.0 * rand::random::<f32>() * self.max_yaw - self.max_yaw,
        );
        let look = body.calculate_view_vector(pitch, yaw);

        let position = body.position();
        let eye = DVec3::new(position.x, body.get_eye_y(), position.z);
        ctx.brain().set_memory(
            memory_module_types::LOOK_TARGET,
            PositionTracker::of_position(eye + look),
        );
        ctx.brain().set_memory(
            memory_module_types::GAZE_COOLDOWN_TICKS,
            rand::random_range(self.interval.min_inclusive..=self.interval.max_inclusive),
        );
    }

    fn debug_name(&self) -> &'static str {
        "RandomLookAround"
    }
}

/// Vanilla parity: `Mth.wrapDegrees`.
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
