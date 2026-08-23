//! Vanilla-shaped goal selector and movement goals.

mod avoid_entity;
mod breath_air;
mod breed_goal;
mod climb_on_top_of_powder_snow;
mod dolphin_jump;
mod door_interact;
mod eat_block_goal;
mod flee_sun;
mod float_goal;
mod follow_mob;
mod follow_parent;
mod follow_player_ridden_entity;
mod hurt_by_target;
mod interact;
mod leap_at_target;
mod look_at_player;
mod melee_attack;
mod move_to_block;
mod move_towards_restriction;
mod move_towards_target;
mod nearest_attackable_target;
mod open_door;
mod panic_goal;
mod random_look_around;
mod random_pos;
mod random_stroll;
mod random_swimming;
mod ranged_attack;
mod ranged_bow_attack;
mod reset_universal_anger_target;
mod restrict_sun;
mod selector;
mod target_goal;
mod tempt_goal;
mod try_find_water;
mod water_avoiding_random_stroll;

pub(crate) use avoid_entity::{AvoidEntityGoal, no_creative_or_spectator};
pub(crate) use breath_air::BreathAirGoal;
pub(crate) use breed_goal::BreedGoal;
pub(crate) use climb_on_top_of_powder_snow::ClimbOnTopOfPowderSnowGoal;
pub(crate) use dolphin_jump::DolphinJumpGoal;
pub(crate) use eat_block_goal::EatBlockGoal;
pub(crate) use flee_sun::FleeSunGoal;
pub(crate) use float_goal::FloatGoal;
pub(crate) use follow_parent::FollowParentGoal;
pub(crate) use follow_player_ridden_entity::FollowPlayerRiddenEntityGoal;
pub(crate) use hurt_by_target::HurtByTargetGoal;
pub(crate) use leap_at_target::LeapAtTargetGoal;
pub(crate) use look_at_player::LookAtPlayerGoal;
pub(crate) use melee_attack::MeleeAttackGoal;
pub(crate) use move_to_block::MoveToBlockGoal;
pub(crate) use nearest_attackable_target::NearestAttackableTargetGoal;
pub(crate) use panic_goal::{PanicGoal, block_pos_corner, look_for_water};
pub(crate) use random_look_around::RandomLookAroundGoal;
/// Vanilla parity: `DefaultRandomPos.getPosTowards`, used by mobs that steer
/// toward a remote point rather than wandering, such as the turtle heading home.
pub(crate) use random_pos::default_random_pos_towards;
pub(crate) use random_stroll::RandomStrollGoal;
pub(crate) use random_swimming::RandomSwimmingGoal;
pub(crate) use ranged_attack::RangedAttackGoal;
pub(crate) use ranged_bow_attack::RangedBowAttackGoal;
pub(crate) use reset_universal_anger_target::ResetUniversalAngerTargetGoal;
pub(crate) use restrict_sun::RestrictSunGoal;
pub(crate) use selector::{Goal, GoalControl, GoalControls, GoalSelector};
pub(crate) use tempt_goal::TemptGoal;
pub(crate) use try_find_water::TryFindWaterGoal;
pub(crate) use water_avoiding_random_stroll::WaterAvoidingRandomStrollGoal;

/// Halves a tick delay, rounding up.
///
/// Vanilla parity: `Goal.reducedTickDelay`.
pub(crate) const fn reduced_tick_delay(ticks: i32) -> i32 {
    (ticks + 1) / 2
}
