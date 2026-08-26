use std::str::FromStr as _;

use steel_registry::dimension_type::DimensionTypeRef;
use steel_registry::timeline::{Ease, KeyframeValue, TimelineRef, Track};
use steel_registry::{REGISTRY, RegistryExt as _, TaggedRegistryExt as _};
use steel_utils::Identifier;

use super::clock::WorldClockManager;
use crate::entity::ai::brain::{Activity, ScheduleAttribute};

const SKY_LIGHT_LEVEL_ATTRIBUTE: &str = "minecraft:gameplay/sky_light_level";
const SUN_ANGLE_ATTRIBUTE: &str = "minecraft:visual/sun_angle";
const TURTLE_EGG_HATCH_CHANCE_ATTRIBUTE: &str = "minecraft:gameplay/turtle_egg_hatch_chance";
const CAT_WAKING_UP_GIFT_CHANCE_ATTRIBUTE: &str = "minecraft:gameplay/cat_waking_up_gift_chance";
const CREAKING_ACTIVE_ATTRIBUTE: &str = "minecraft:gameplay/creaking_active";
const BEES_STAY_IN_HIVE_ATTRIBUTE: &str = "minecraft:gameplay/bees_stay_in_hive";
/// The declared default of `EnvironmentAttributes.CREAKING_ACTIVE`. No dimension type
/// overrides it, so only the overworld `day` timeline ever turns it on.
const DEFAULT_CREAKING_ACTIVE: bool = false;
/// The declared default of `EnvironmentAttributes.BEES_STAY_IN_HIVE`. No dimension
/// type overrides it, so a bee only shelters where the overworld `day` timeline or
/// the weather layer says so.
const DEFAULT_BEES_STAY_IN_HIVE: bool = false;
/// The declared default of `EnvironmentAttributes.CLOUD_HEIGHT`. Only the
/// overworld sets it, and it sets it to this.
const DEFAULT_CLOUD_HEIGHT: f32 = 192.33;
const DEFAULT_SKY_LIGHT_LEVEL: f32 = 15.0;
const DEFAULT_SUN_ANGLE: f32 = 0.0;
/// Vanilla `DimensionDefaults.TURTLE_EGG_HATCH_CHANCE`, which is also the
/// declared default of `EnvironmentAttributes.TURTLE_EGG_HATCH_CHANCE`. No
/// vanilla dimension type overrides it, so every dimension starts here and only
/// the overworld `day` timeline raises it.
const DEFAULT_TURTLE_EGG_HATCH_CHANCE: f32 = 0.002;
/// The declared default of `EnvironmentAttributes.CAT_WAKING_UP_GIFT_CHANCE`.
/// No dimension type overrides it, so a cat only ever brings a gift where the
/// overworld `day` timeline raises it -- at dawn, which is the whole point.
const DEFAULT_CAT_WAKING_UP_GIFT_CHANCE: f32 = 0.0;
/// The bounds of `AttributeRange.UNIT_FLOAT`.
const MIN_UNIT_FLOAT: f32 = 0.0;
const MAX_UNIT_FLOAT: f32 = 1.0;
const MIN_SKY_LIGHT_LEVEL: f32 = 0.0;
const MAX_SKY_LIGHT_LEVEL: f32 = 15.0;
const RAIN_SKY_LIGHT_TARGET: f32 = 4.0;
const RAIN_SKY_LIGHT_ALPHA: f32 = 0.3125;
const THUNDER_SKY_LIGHT_TARGET: f32 = 4.0;
const THUNDER_SKY_LIGHT_ALPHA: f32 = 0.527_343_75;

#[must_use]
pub(super) fn sky_light_level(
    dimension_type: DimensionTypeRef,
    clock_manager: &WorldClockManager,
    rain_level: f32,
    thunder_level: f32,
    can_have_weather: bool,
) -> f32 {
    let mut value = dimension_type
        .sky_light_level
        .unwrap_or(DEFAULT_SKY_LIGHT_LEVEL);
    value = apply_timeline_float_attribute(
        value,
        dimension_type,
        clock_manager,
        SKY_LIGHT_LEVEL_ATTRIBUTE,
    );
    if can_have_weather {
        value = apply_weather_sky_light_level(value, rain_level, thunder_level);
    }
    value.clamp(MIN_SKY_LIGHT_LEVEL, MAX_SKY_LIGHT_LEVEL)
}

#[must_use]
pub(super) fn sun_angle_degrees(
    dimension_type: DimensionTypeRef,
    clock_manager: &WorldClockManager,
) -> f32 {
    apply_timeline_float_attribute(
        DEFAULT_SUN_ANGLE,
        dimension_type,
        clock_manager,
        SUN_ANGLE_ATTRIBUTE,
    )
}

/// Returns the `gameplay/turtle_egg_hatch_chance` environment attribute.
///
/// The overworld `day` timeline pushes this to 1.0 for the stretch of night
/// that ends just before sunrise and leaves it at the default the rest of the
/// time, which is what makes turtle eggs hatch at dawn.
#[must_use]
pub(super) fn turtle_egg_hatch_chance(
    dimension_type: DimensionTypeRef,
    clock_manager: &WorldClockManager,
) -> f32 {
    apply_timeline_float_attribute(
        DEFAULT_TURTLE_EGG_HATCH_CHANCE,
        dimension_type,
        clock_manager,
        TURTLE_EGG_HATCH_CHANCE_ATTRIBUTE,
    )
    .clamp(MIN_UNIT_FLOAT, MAX_UNIT_FLOAT)
}

/// Returns the `gameplay/cat_waking_up_gift_chance` environment attribute.
///
/// The overworld `day` timeline raises this to 0.7 across the morning, which is
/// what decides whether a cat that slept on its owner's bed leaves a present.
#[must_use]
pub(super) fn cat_waking_up_gift_chance(
    dimension_type: DimensionTypeRef,
    clock_manager: &WorldClockManager,
) -> f32 {
    apply_timeline_float_attribute(
        DEFAULT_CAT_WAKING_UP_GIFT_CHANCE,
        dimension_type,
        clock_manager,
        CAT_WAKING_UP_GIFT_CHANCE_ATTRIBUTE,
    )
    .clamp(MIN_UNIT_FLOAT, MAX_UNIT_FLOAT)
}

/// Returns the `gameplay/creaking_active` environment attribute.
///
/// The overworld `day` timeline turns this on for the stretch of night between 12600 and
/// 23401 ticks, which is what wakes a creaking heart and, in vanilla, lets it hold a
/// creaking. It defaults to off and no dimension type overrides it, so a dimension without
/// the `day` timeline never wakes one.
#[must_use]
pub(super) fn creaking_active(
    dimension_type: DimensionTypeRef,
    clock_manager: &WorldClockManager,
) -> bool {
    apply_timeline_bool_attribute(
        DEFAULT_CREAKING_ACTIVE,
        dimension_type,
        clock_manager,
        CREAKING_ACTIVE_ATTRIBUTE,
    )
}

/// Returns the `gameplay/bees_stay_in_hive` environment attribute.
///
/// Two layers stack. The overworld `day` timeline turns it on for the night --
/// vanilla's `addModifierTrack(BEES_STAY_IN_HIVE, BooleanModifier.OR, 12542 -> true,
/// 23460 -> false)` -- and `WeatherAttributes` sets it outright while it is raining
/// or thundering.
///
/// The weather layer is a plain `rain_level > 0.0` test rather than
/// [`World::is_raining`](crate::world::World::is_raining)'s `> 0.2`, because the
/// attribute's boolean state-change lerp is `LerpFunction.ofStep(0.0)`: it takes the
/// weather value for any non-zero alpha, so the bees head home the moment the sky
/// starts to darken rather than a few seconds later.
#[must_use]
pub(super) fn bees_stay_in_hive(
    dimension_type: DimensionTypeRef,
    clock_manager: &WorldClockManager,
    rain_level: f32,
    thunder_level: f32,
) -> bool {
    if rain_level > 0.0 || thunder_level > 0.0 {
        return true;
    }

    apply_timeline_bool_attribute(
        DEFAULT_BEES_STAY_IN_HIVE,
        dimension_type,
        clock_manager,
        BEES_STAY_IN_HIVE_ATTRIBUTE,
    )
}

/// Returns the height the cloud layer starts at, or `None` where a dimension
/// has none.
///
/// Vanilla parity: the `visual/cloud_height` environment attribute, gated on
/// `visual/cloud_color`'s alpha the way `Entity.isInClouds` gates on it. Steel
/// keeps the color as the ARGB string the registry hands the client, so the
/// gate reads its alpha byte. The timeline layer that tints the clouds through
/// the day is not consulted: no vanilla timeline makes them transparent, and it
/// is the dimension that decides whether there are any.
#[must_use]
pub(super) fn cloud_bottom(dimension_type: DimensionTypeRef) -> Option<f32> {
    let color = dimension_type.cloud_color?;
    let alpha = u8::from_str_radix(color.strip_prefix('#')?.get(..2)?, 16).ok()?;
    (alpha != 0).then(|| dimension_type.cloud_height.unwrap_or(DEFAULT_CLOUD_HEIGHT))
}

/// Returns the activity-valued environment attribute `schedule` names.
///
/// This is the whole of a villager's day: the `villager_schedule` timeline is in
/// `#minecraft:universal`, so every dimension carries it, and its two tracks
/// step a villager -- or a baby -- between IDLE, WORK, MEET, PLAY and REST as
/// the overworld clock turns.
#[must_use]
pub(super) fn scheduled_activity(
    dimension_type: DimensionTypeRef,
    clock_manager: &WorldClockManager,
    schedule: ScheduleAttribute,
) -> Activity {
    apply_timeline_activity_attribute(
        ScheduleAttribute::DEFAULT_ACTIVITY,
        dimension_type,
        clock_manager,
        schedule.attribute_name(),
    )
}

#[must_use]
pub(super) fn sky_darkening(sky_light_level: f32) -> u8 {
    (MAX_SKY_LIGHT_LEVEL - sky_light_level.clamp(MIN_SKY_LIGHT_LEVEL, MAX_SKY_LIGHT_LEVEL)) as u8
}

/// Layers every timeline the dimension declares over `value`.
///
/// Vanilla parity: the way `EnvironmentAttributeSystem` stacks one
/// `AttributeTrackSampler` per timeline in `DimensionType.timelines()` over the
/// attribute's base value, in declaration order.
fn apply_timeline_attribute<T>(
    mut value: T,
    dimension_type: DimensionTypeRef,
    clock_manager: &WorldClockManager,
    attribute: &str,
    apply_track: fn(T, TimelineRef, &WorldClockManager, &str) -> T,
) -> T {
    let Some(timelines) = dimension_type.timelines else {
        return value;
    };
    if let Some(tag) = timelines.strip_prefix('#') {
        let Ok(tag) = Identifier::from_str(tag) else {
            return value;
        };
        for timeline in REGISTRY.timelines.iter_tag(&tag) {
            value = apply_track(value, timeline, clock_manager, attribute);
        }
        return value;
    }

    let Ok(key) = Identifier::from_str(timelines) else {
        return value;
    };
    let Some(timeline) = REGISTRY.timelines.by_key(&key) else {
        return value;
    };
    apply_track(value, timeline, clock_manager, attribute)
}

fn apply_timeline_float_attribute(
    value: f32,
    dimension_type: DimensionTypeRef,
    clock_manager: &WorldClockManager,
    attribute: &str,
) -> f32 {
    apply_timeline_attribute(
        value,
        dimension_type,
        clock_manager,
        attribute,
        apply_timeline_float_track,
    )
}

fn apply_timeline_bool_attribute(
    value: bool,
    dimension_type: DimensionTypeRef,
    clock_manager: &WorldClockManager,
    attribute: &str,
) -> bool {
    apply_timeline_attribute(
        value,
        dimension_type,
        clock_manager,
        attribute,
        apply_timeline_bool_track,
    )
}

fn apply_timeline_activity_attribute(
    value: Activity,
    dimension_type: DimensionTypeRef,
    clock_manager: &WorldClockManager,
    attribute: &str,
) -> Activity {
    apply_timeline_attribute(
        value,
        dimension_type,
        clock_manager,
        attribute,
        apply_timeline_activity_track,
    )
}

/// Reads the activity a step track names at the clock's current tick.
///
/// `AttributeTypes.ACTIVITY` is `ofNotInterpolated`, so no modifier library
/// applies: a track that names one simply replaces the value, which is what
/// vanilla's only two activity tracks -- the villager schedule's -- do.
fn apply_timeline_activity_track(
    value: Activity,
    timeline: TimelineRef,
    clock_manager: &WorldClockManager,
    attribute: &str,
) -> Activity {
    let Some(track) = timeline.tracks.iter().find(|track| track.name == attribute) else {
        return value;
    };
    let Some(total_ticks) = clock_manager.total_ticks(timeline.clock) else {
        return value;
    };
    let Some(sample) = sample_step_track(track, timeline.period_ticks.map(i64::from), total_ticks)
    else {
        return value;
    };
    keyframe_activity_value(sample).unwrap_or(value)
}

fn apply_timeline_bool_track(
    value: bool,
    timeline: TimelineRef,
    clock_manager: &WorldClockManager,
    attribute: &str,
) -> bool {
    let Some(track) = timeline.tracks.iter().find(|track| track.name == attribute) else {
        return value;
    };
    let Some(total_ticks) = clock_manager.total_ticks(timeline.clock) else {
        return value;
    };
    let Some(sample) = sample_step_track(track, timeline.period_ticks.map(i64::from), total_ticks)
        .and_then(keyframe_bool_value)
    else {
        return value;
    };
    match track.modifier {
        // Vanilla `BooleanModifier.OR`, the only boolean modifier any timeline uses.
        Some("or") => value || sample,
        Some("and") => value && sample,
        None => sample,
        _ => value,
    }
}

/// Samples a keyframe track that steps rather than interpolates.
///
/// Vanilla bakes a non-numeric attribute's track -- `AttributeType.ofNotInterpolated`,
/// which every boolean and every activity attribute is -- with `LerpFunction.ofStep(1.0F)`:
/// a segment holds its `from` value for every tick strictly before the next keyframe. So
/// such a track is a step function over its keyframes, wrapped by the timeline period.
fn sample_step_track(
    track: &Track,
    period_ticks: Option<i64>,
    ticks: i64,
) -> Option<&KeyframeValue> {
    let keyframes = track.keyframes;
    match keyframes.len() {
        0 => return None,
        1 => return Some(&keyframes[0].value),
        _ => {}
    }

    let sample_ticks = period_ticks.map_or(ticks, |period| ticks.rem_euclid(period));
    let first = &keyframes[0];
    let last = &keyframes[keyframes.len() - 1];

    if period_ticks.is_some() && sample_ticks < first.ticks {
        return Some(&last.value);
    }

    for segment in keyframes.windows(2) {
        if sample_ticks < segment[1].ticks {
            return Some(&segment[0].value);
        }
    }

    Some(&last.value)
}

const fn keyframe_bool_value(value: &KeyframeValue) -> Option<bool> {
    match value {
        KeyframeValue::Bool(value) => Some(*value),
        _ => None,
    }
}

/// Reads an activity out of a keyframe, which stores it as its registry key.
fn keyframe_activity_value(value: &KeyframeValue) -> Option<Activity> {
    match value {
        KeyframeValue::String(key) => Activity::by_key(key),
        _ => None,
    }
}

fn apply_timeline_float_track(
    value: f32,
    timeline: TimelineRef,
    clock_manager: &WorldClockManager,
    attribute: &str,
) -> f32 {
    let Some(track) = timeline.tracks.iter().find(|track| track.name == attribute) else {
        return value;
    };
    let Some(total_ticks) = clock_manager.total_ticks(timeline.clock) else {
        return value;
    };
    let Some(sample) = sample_float_track(track, timeline.period_ticks.map(i64::from), total_ticks)
    else {
        return value;
    };
    match track.modifier {
        Some("multiply") => value * sample,
        // Vanilla `FloatModifier.MAXIMUM`, used by the turtle-egg hatch chance.
        Some("maximum") => value.max(sample),
        None => sample,
        _ => value,
    }
}

fn sample_float_track(track: &Track, period_ticks: Option<i64>, ticks: i64) -> Option<f32> {
    let keyframes = track.keyframes;
    match keyframes.len() {
        0 => return None,
        1 => return keyframe_float_value(&keyframes[0].value),
        _ => {}
    }

    let sample_ticks = period_ticks.map_or(ticks, |period| ticks.rem_euclid(period));
    let first = &keyframes[0];
    let last = &keyframes[keyframes.len() - 1];

    if let Some(period) = period_ticks
        && sample_ticks < first.ticks
    {
        return interpolate_float_segment(
            track,
            last.ticks - period,
            &last.value,
            first.ticks,
            &first.value,
            sample_ticks,
        );
    }

    for segment in keyframes.windows(2) {
        let from = &segment[0];
        let to = &segment[1];
        if sample_ticks < to.ticks {
            return interpolate_float_segment(
                track,
                from.ticks,
                &from.value,
                to.ticks,
                &to.value,
                sample_ticks,
            );
        }
    }

    if let Some(period) = period_ticks {
        return interpolate_float_segment(
            track,
            last.ticks,
            &last.value,
            first.ticks + period,
            &first.value,
            sample_ticks,
        );
    }

    keyframe_float_value(&last.value)
}

fn interpolate_float_segment(
    track: &Track,
    from_ticks: i64,
    from_value: &KeyframeValue,
    to_ticks: i64,
    to_value: &KeyframeValue,
    sample_ticks: i64,
) -> Option<f32> {
    let from = keyframe_float_value(from_value)?;
    let to = keyframe_float_value(to_value)?;
    if sample_ticks <= from_ticks {
        return Some(from);
    }
    if sample_ticks >= to_ticks {
        return Some(to);
    }

    let alpha = (sample_ticks - from_ticks) as f32 / (to_ticks - from_ticks) as f32;
    let eased_alpha = apply_easing(track.ease.as_ref(), alpha)?;
    Some(from + eased_alpha * (to - from))
}

fn apply_easing(ease: Option<&Ease>, alpha: f32) -> Option<f32> {
    match ease {
        None | Some(Ease::Named("linear")) => Some(alpha),
        Some(Ease::Named("constant")) => Some(0.0),
        Some(Ease::CubicBezier([x1, y1, x2, y2])) => Some(cubic_bezier(alpha, *x1, *y1, *x2, *y2)),
        Some(Ease::Named(_)) => None,
    }
}

fn cubic_bezier(x: f32, x1: f32, y1: f32, x2: f32, y2: f32) -> f32 {
    let x_curve = CubicCurve::from_controls(x1, x2);
    let y_curve = CubicCurve::from_controls(y1, y2);
    y_curve.sample(x_curve.solve_t(x))
}

#[derive(Clone, Copy)]
struct CubicCurve {
    a: f32,
    b: f32,
    c: f32,
}

impl CubicCurve {
    const ERROR_EPSILON: f32 = 1.0E-5;

    fn from_controls(first: f32, second: f32) -> Self {
        Self {
            a: (3.0 * first - 3.0 * second) + 1.0,
            b: -6.0 * first + 3.0 * second,
            c: 3.0 * first,
        }
    }

    fn sample(self, t: f32) -> f32 {
        ((self.a * t + self.b) * t + self.c) * t
    }

    fn sample_gradient(self, t: f32) -> f32 {
        (3.0 * self.a * t + 2.0 * self.b) * t + self.c
    }

    fn solve_t(self, x: f32) -> f32 {
        let mut t = x;
        for _ in 0..4 {
            let error = self.sample(t) - x;
            if error.abs() < Self::ERROR_EPSILON {
                return t;
            }
            let gradient = self.sample_gradient(t);
            if gradient < Self::ERROR_EPSILON {
                break;
            }
            t -= (error / gradient).clamp(-0.25, 0.25);
        }
        self.solve_t_bisect(x, t)
    }

    #[expect(
        clippy::manual_midpoint,
        reason = "evaluation order mirrors vanilla CubicBezier.solveTBisect float arithmetic"
    )]
    fn solve_t_bisect(self, x: f32, initial_t: f32) -> f32 {
        let mut lower = 0.0;
        let mut upper = 1.0;
        let mut t = initial_t;
        while lower < upper {
            let error = self.sample(t) - x;
            if error.abs() < Self::ERROR_EPSILON {
                return t;
            }
            if error < 0.0 {
                lower = t;
            } else {
                upper = t;
            }
            t = (upper + lower) / 2.0;
        }
        t
    }
}

const fn keyframe_float_value(value: &KeyframeValue) -> Option<f32> {
    match value {
        KeyframeValue::Float(value) => Some(*value),
        _ => None,
    }
}

fn apply_weather_sky_light_level(mut value: f32, rain_level: f32, thunder_level: f32) -> f32 {
    let thunder_level = thunder_level.clamp(0.0, 1.0);
    let rain_level = (rain_level - thunder_level).clamp(0.0, 1.0);
    if rain_level > 0.0 {
        let rain_value = lerp(RAIN_SKY_LIGHT_ALPHA, value, RAIN_SKY_LIGHT_TARGET);
        value = lerp(rain_level, value, rain_value);
    }
    if thunder_level > 0.0 {
        let thunder_value = lerp(THUNDER_SKY_LIGHT_ALPHA, value, THUNDER_SKY_LIGHT_TARGET);
        value = lerp(thunder_level, value, thunder_value);
    }
    value
}

fn lerp(alpha: f32, from: f32, to: f32) -> f32 {
    from + alpha * (to - from)
}

#[cfg(test)]
mod tests {
    use steel_registry::init_vanilla_registry;
    use steel_registry::vanilla_dimension_types::{OVERWORLD, THE_NETHER};
    use steel_registry::vanilla_world_clocks;

    use super::*;

    const F32_CLOSE_EPSILON: f32 = 0.000_001;
    const OVERWORLD_WAKE_UP_TICKS: i64 = 0;
    const OVERWORLD_DAY_TICKS: i64 = 1_000;
    const OVERWORLD_NOON_TICKS: i64 = 6_000;
    const OVERWORLD_SUNSET_TICKS: i64 = 12_000;
    const OVERWORLD_SUNSET_INTERPOLATION_TICKS: i64 = 12_768;
    const OVERWORLD_MIDNIGHT_TICKS: i64 = 18_000;
    /// Inside the `day` timeline's 21062..21905 turtle-egg hatch window.
    const OVERWORLD_PRE_SUNRISE_TICKS: i64 = 21_500;

    fn assert_f32_close(left: f32, right: f32) {
        assert!(
            (left - right).abs() < F32_CLOSE_EPSILON,
            "left={left}, right={right}"
        );
    }

    fn clock_manager_at(total_ticks: i64) -> WorldClockManager {
        let mut manager = WorldClockManager::new();
        assert_eq!(
            manager.set_total_ticks(&vanilla_world_clocks::OVERWORLD, total_ticks),
            Some(())
        );
        manager
    }

    #[test]
    fn overworld_sky_light_uses_generated_day_timeline() {
        init_vanilla_registry();

        assert_f32_close(
            sky_light_level(
                &OVERWORLD,
                &clock_manager_at(OVERWORLD_DAY_TICKS),
                0.0,
                0.0,
                true,
            ),
            15.0,
        );
        assert_f32_close(
            sky_light_level(
                &OVERWORLD,
                &clock_manager_at(OVERWORLD_NOON_TICKS),
                0.0,
                0.0,
                true,
            ),
            15.0,
        );
        assert_f32_close(
            sky_light_level(
                &OVERWORLD,
                &clock_manager_at(OVERWORLD_MIDNIGHT_TICKS),
                0.0,
                0.0,
                true,
            ),
            4.0,
        );
    }

    #[test]
    fn overworld_sky_light_interpolates_sunset_from_generated_keyframes() {
        init_vanilla_registry();

        assert_f32_close(
            sky_light_level(
                &OVERWORLD,
                &clock_manager_at(OVERWORLD_SUNSET_INTERPOLATION_TICKS),
                0.0,
                0.0,
                true,
            ),
            9.503_051,
        );
    }

    #[test]
    fn overworld_sun_angle_uses_vanilla_cubic_bezier_easing() {
        init_vanilla_registry();

        assert_f32_close(
            sun_angle_degrees(&OVERWORLD, &clock_manager_at(OVERWORLD_WAKE_UP_TICKS)),
            282.374_33,
        );
        assert_f32_close(
            sun_angle_degrees(&OVERWORLD, &clock_manager_at(OVERWORLD_SUNSET_TICKS)),
            77.625_66,
        );
        assert_f32_close(
            sun_angle_degrees(&OVERWORLD, &clock_manager_at(OVERWORLD_MIDNIGHT_TICKS)),
            180.0,
        );
    }

    #[test]
    fn sky_light_level_applies_vanilla_weather_alpha_layers() {
        init_vanilla_registry();

        assert_f32_close(
            sky_light_level(
                &OVERWORLD,
                &clock_manager_at(OVERWORLD_NOON_TICKS),
                1.0,
                0.0,
                true,
            ),
            11.5625,
        );
        assert_f32_close(
            sky_light_level(
                &OVERWORLD,
                &clock_manager_at(OVERWORLD_NOON_TICKS),
                1.0,
                1.0,
                true,
            ),
            9.199_219,
        );
    }

    #[test]
    fn fixed_nether_sky_light_uses_dimension_attribute() {
        init_vanilla_registry();

        assert_f32_close(
            sky_light_level(
                &THE_NETHER,
                &clock_manager_at(OVERWORLD_NOON_TICKS),
                0.0,
                0.0,
                false,
            ),
            4.0,
        );
    }

    #[test]
    fn turtle_eggs_are_sure_to_advance_only_in_the_stretch_before_sunrise() {
        init_vanilla_registry();

        assert_f32_close(
            turtle_egg_hatch_chance(&OVERWORLD, &clock_manager_at(OVERWORLD_PRE_SUNRISE_TICKS)),
            1.0,
        );
        assert_f32_close(
            turtle_egg_hatch_chance(&OVERWORLD, &clock_manager_at(OVERWORLD_MIDNIGHT_TICKS)),
            DEFAULT_TURTLE_EGG_HATCH_CHANCE,
        );
        assert_f32_close(
            turtle_egg_hatch_chance(&OVERWORLD, &clock_manager_at(OVERWORLD_NOON_TICKS)),
            DEFAULT_TURTLE_EGG_HATCH_CHANCE,
        );
    }

    #[test]
    fn a_dimension_without_the_day_timeline_never_speeds_turtle_eggs_up() {
        init_vanilla_registry();

        assert_f32_close(
            turtle_egg_hatch_chance(&THE_NETHER, &clock_manager_at(OVERWORLD_PRE_SUNRISE_TICKS)),
            DEFAULT_TURTLE_EGG_HATCH_CHANCE,
        );
    }

    /// A boolean track is a step function, not an interpolated one -- vanilla bakes it with
    /// `LerpFunction.ofConstant()`. The creaking is awake for exactly the stretch between
    /// the two keyframes, and reading the edges wrong would wake every creaking heart a
    /// whole day out of phase.
    #[test]
    fn the_creaking_is_active_for_exactly_the_night_the_day_timeline_names() {
        init_vanilla_registry();

        // The `day` timeline switches this on at 12600 and off at 23401.
        assert!(!creaking_active(&OVERWORLD, &clock_manager_at(12_599)));
        assert!(creaking_active(&OVERWORLD, &clock_manager_at(12_600)));
        assert!(creaking_active(
            &OVERWORLD,
            &clock_manager_at(OVERWORLD_MIDNIGHT_TICKS)
        ));
        assert!(creaking_active(&OVERWORLD, &clock_manager_at(23_400)));
        assert!(!creaking_active(&OVERWORLD, &clock_manager_at(23_401)));
        assert!(!creaking_active(
            &OVERWORLD,
            &clock_manager_at(OVERWORLD_NOON_TICKS)
        ));
        // Before the first keyframe the period wraps back to the last one.
        assert!(!creaking_active(
            &OVERWORLD,
            &clock_manager_at(OVERWORLD_WAKE_UP_TICKS)
        ));
    }

    /// The attribute defaults to off and no dimension type overrides it, so a dimension
    /// without the overworld `day` timeline never wakes a creaking heart.
    #[test]
    fn a_dimension_without_the_day_timeline_never_wakes_the_creaking() {
        init_vanilla_registry();

        assert!(!creaking_active(
            &THE_NETHER,
            &clock_manager_at(OVERWORLD_MIDNIGHT_TICKS)
        ));
    }

    /// The whole of a villager's day comes out of this one string-valued track,
    /// and every boundary below is a keyframe of `Timelines.VILLAGER_SCHEDULE`.
    /// Reading a step track as if it interpolated, or forgetting that the period
    /// wraps the stretch before the first keyframe onto the last one, would put
    /// a village to bed at the wrong hour.
    #[test]
    fn the_villager_schedule_steps_an_adult_through_its_working_day() {
        init_vanilla_registry();

        let at = |ticks| {
            scheduled_activity(
                &OVERWORLD,
                &clock_manager_at(ticks),
                ScheduleAttribute::VillagerActivity,
            )
        };

        // Before the first keyframe the period wraps back onto the last one.
        assert_eq!(at(0), Activity::Rest);
        assert_eq!(at(9), Activity::Rest);
        assert_eq!(at(10), Activity::Idle);
        assert_eq!(at(1_999), Activity::Idle);
        assert_eq!(at(2_000), Activity::Work);
        assert_eq!(at(8_999), Activity::Work);
        assert_eq!(at(9_000), Activity::Meet);
        assert_eq!(at(10_999), Activity::Meet);
        assert_eq!(at(11_000), Activity::Idle);
        assert_eq!(at(12_000), Activity::Rest);
        assert_eq!(at(23_999), Activity::Rest);
        // A second day samples the same as the first.
        assert_eq!(at(24_000 + 2_000), Activity::Work);
    }

    #[test]
    fn a_baby_plays_where_an_adult_works() {
        init_vanilla_registry();

        let at = |ticks| {
            scheduled_activity(
                &OVERWORLD,
                &clock_manager_at(ticks),
                ScheduleAttribute::BabyVillagerActivity,
            )
        };

        assert_eq!(at(10), Activity::Idle);
        assert_eq!(at(3_000), Activity::Play);
        assert_eq!(at(6_000), Activity::Idle);
        assert_eq!(at(10_000), Activity::Play);
        assert_eq!(at(12_000), Activity::Rest);
    }

    /// `villager_schedule` is tagged `#minecraft:universal`, which every
    /// dimension's timeline tag nests, so a villager taken to the nether keeps
    /// the same day. This also proves the tag loader expands a nested tag.
    #[test]
    fn the_villager_schedule_reaches_every_dimension() {
        init_vanilla_registry();

        assert_eq!(
            scheduled_activity(
                &THE_NETHER,
                &clock_manager_at(OVERWORLD_NOON_TICKS),
                ScheduleAttribute::VillagerActivity,
            ),
            Activity::Work
        );
    }

    #[test]
    fn sky_darkening_matches_vanilla_integer_cast() {
        assert_eq!(sky_darkening(15.0), 0);
        assert_eq!(sky_darkening(11.5625), 3);
        assert_eq!(sky_darkening(4.0), 11);
    }
}
