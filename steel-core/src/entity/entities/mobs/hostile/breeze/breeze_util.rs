//! Vanilla `BreezeUtil`.
//!
//! The two geometry questions every breeze behavior asks: where would I like to
//! be relative to my target, and can I see a spot from here.

use glam::DVec3;
use steel_registry::vanilla_attributes;

use crate::entity::{LivingEntity, PathfinderMob};
use crate::world::{ClipBlockShape, ClipFluid};

/// Vanilla parity: `BreezeUtil.MAX_LINE_OF_SIGHT_TEST_RANGE`.
const MAX_LINE_OF_SIGHT_TEST_RANGE: f64 = 50.0;

/// Vanilla parity: the `90` degree spread of `randomPointBehindTarget`.
const SPREAD_DEGREES: f32 = 90.0;

/// Vanilla parity: the `Mth.lerp(nextFloat(), 4.0F, 8.0F)` radius of the same.
const MIN_RADIUS: f32 = 4.0;
const MAX_RADIUS: f32 = 8.0;

/// Returns a point roughly behind `enemy`, four to eight blocks out.
///
/// Vanilla parity: `BreezeUtil.randomPointBehindTarget`. The angle is the
/// target's own head rotation turned half around and then jittered by a
/// gaussian, so a breeze tends to land behind whoever it is fighting and only
/// sometimes off to one side.
#[must_use]
pub(super) fn random_point_behind_target(enemy: &dyn LivingEntity) -> DVec3 {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "a gaussian sample immediately used as an angle in degrees"
    )]
    let jitter = next_gaussian() as f32 * SPREAD_DEGREES / 2.0;
    let view_angle = enemy.y_head_rot() + 180.0 + jitter;
    // Vanilla parity: `Mth.lerp(random.nextFloat(), 4.0F, 8.0F)`.
    let radius = (MAX_RADIUS - MIN_RADIUS).mul_add(rand::random::<f32>(), MIN_RADIUS);
    enemy.position() + direction_from_yaw(view_angle) * f64::from(radius)
}

/// Vanilla parity: `Vec3.directionFromRotation(0.0F, yRot)`, whose pitch of
/// zero leaves a purely horizontal unit vector.
#[must_use]
fn direction_from_yaw(y_rot: f32) -> DVec3 {
    let yaw = -y_rot.to_radians();
    DVec3::new(f64::from(yaw.sin()), 0.0, f64::from(yaw.cos()))
}

/// Returns whether nothing solid stands between the breeze and `target`.
///
/// Vanilla parity: `BreezeUtil.hasLineOfSight`. Anything past the test range
/// counts as out of sight without a ray being cast at all.
#[must_use]
pub(super) fn has_line_of_sight(breeze: &dyn PathfinderMob, target: DVec3) -> bool {
    let from = breeze.position();
    if target.distance(from) > max_line_of_sight_test_range(breeze) {
        return false;
    }
    let Some(world) = breeze.level() else {
        return false;
    };
    world
        .clip(from, target, ClipBlockShape::Collider, ClipFluid::None)
        .is_miss()
}

/// Vanilla parity: the private `BreezeUtil.getMaxLineOfSightTestRange`.
fn max_line_of_sight_test_range(breeze: &dyn PathfinderMob) -> f64 {
    breeze
        .attributes()
        .lock()
        .get_value(vanilla_attributes::FOLLOW_RANGE)
        .unwrap_or(0.0)
        .max(MAX_LINE_OF_SIGHT_TEST_RANGE)
}

/// Draws a standard normal sample.
///
/// Vanilla parity: `RandomSource.nextGaussian`, which is Java's Marsaglia polar
/// method. Steel's seeded `Random` has one, but a breeze picking where to jump
/// is incidental live randomness rather than anything a save has to reproduce,
/// so this runs on the unseeded runtime RNG and only the distribution matters.
fn next_gaussian() -> f64 {
    loop {
        let x = 2.0 * rand::random::<f64>() - 1.0;
        let y = 2.0 * rand::random::<f64>() - 1.0;
        let squared = x.mul_add(x, y * y);
        if squared >= 1.0 || squared == 0.0 {
            continue;
        }
        return x * (-2.0 * squared.ln() / squared).sqrt();
    }
}
