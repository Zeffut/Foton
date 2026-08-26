//! Vanilla-shaped goal selector and movement goals.

mod avoid_entity;
mod beg;
mod break_door;
mod breath_air;
mod breed_goal;
mod cat_lie_on_bed;
mod cat_sit_on_block;
mod climb_on_top_of_powder_snow;
mod dolphin_jump;
mod door_interact;
mod eat_block_goal;
mod flee_sun;
mod float_goal;
mod follow_mob;
mod follow_owner;
mod follow_parent;
mod follow_player_ridden_entity;
mod ghast_look;
mod hold_ground_attack;
mod hurt_by_target;
mod interact;
mod land_on_owners_shoulder;
mod leap_at_target;
mod llama_follow_caravan;
mod long_distance_patrol;
mod look_at_player;
mod melee_attack;
mod move_to_block;
mod move_towards_restriction;
mod move_towards_target;
mod nearest_attackable_target;
mod non_tame_random_target;
mod ocelot_attack;
mod offer_flower;
mod open_door;
mod owner_hurt_target;
mod panic_goal;
mod raider_celebration;
mod random_float_around;
mod random_look_around;
mod random_pos;
mod random_stand;
mod random_stroll;
mod random_swimming;
mod ranged_attack;
mod ranged_bow_attack;
mod ranged_crossbow_attack;
mod reset_universal_anger_target;
mod restrict_sun;
mod run_around_like_crazy;
mod selector;
mod sit_when_ordered_to;
mod spellcaster_casting_spell;
mod spellcaster_use_spell;
mod tamable_animal_panic;
mod target_goal;
mod tempt_goal;
mod try_find_water;
mod water_avoiding_random_flying;
mod water_avoiding_random_stroll;

pub(crate) use avoid_entity::{AvoidEntityGoal, no_creative_or_spectator};
pub(crate) use beg::BegGoal;
pub(crate) use break_door::BreakDoorGoal;
pub(crate) use breath_air::BreathAirGoal;
pub(crate) use breed_goal::BreedGoal;
pub(crate) use cat_lie_on_bed::CatLieOnBedGoal;
pub(crate) use cat_sit_on_block::CatSitOnBlockGoal;
pub(crate) use climb_on_top_of_powder_snow::ClimbOnTopOfPowderSnowGoal;
pub(crate) use dolphin_jump::DolphinJumpGoal;
pub(crate) use eat_block_goal::EatBlockGoal;
pub(crate) use flee_sun::FleeSunGoal;
pub(crate) use float_goal::FloatGoal;
pub(crate) use follow_mob::FollowMobGoal;
pub(crate) use follow_owner::FollowOwnerGoal;
pub(crate) use follow_parent::FollowParentGoal;
pub(crate) use follow_player_ridden_entity::FollowPlayerRiddenEntityGoal;
pub(crate) use ghast_look::GhastLookGoal;
pub(crate) use hold_ground_attack::HoldGroundAttackGoal;
pub(crate) use hurt_by_target::HurtByTargetGoal;
pub(crate) use land_on_owners_shoulder::LandOnOwnersShoulderGoal;
pub(crate) use leap_at_target::LeapAtTargetGoal;
pub(crate) use llama_follow_caravan::LlamaFollowCaravanGoal;
pub(crate) use long_distance_patrol::LongDistancePatrolGoal;
pub(crate) use look_at_player::LookAtPlayerGoal;
pub(crate) use melee_attack::MeleeAttackGoal;
pub(crate) use move_to_block::MoveToBlockGoal;
pub(crate) use move_towards_restriction::MoveTowardsRestrictionGoal;
pub(crate) use move_towards_target::MoveTowardsTargetGoal;
pub(crate) use nearest_attackable_target::NearestAttackableTargetGoal;
pub(crate) use non_tame_random_target::NonTameRandomTargetGoal;
pub(crate) use ocelot_attack::OcelotAttackGoal;
pub(crate) use offer_flower::OfferFlowerGoal;
pub(crate) use open_door::OpenDoorGoal;
pub(crate) use owner_hurt_target::OwnerHurtTargetGoal;
pub(crate) use panic_goal::{PanicGoal, block_pos_corner, look_for_water};
pub(crate) use raider_celebration::RaiderCelebrationGoal;
pub(crate) use random_float_around::RandomFloatAroundGoal;
pub(crate) use random_look_around::RandomLookAroundGoal;
/// Vanilla parity: `DefaultRandomPos.getPosTowards`, used by mobs that steer
/// toward a remote point rather than wandering, such as the turtle heading home.
pub(crate) use random_pos::{default_random_pos_towards, land_random_pos, land_random_pos_away};
pub(crate) use random_stand::RandomStandGoal;
pub(crate) use random_stroll::RandomStrollGoal;
pub(crate) use random_swimming::RandomSwimmingGoal;
pub(crate) use ranged_attack::RangedAttackGoal;
pub(crate) use ranged_bow_attack::RangedBowAttackGoal;
pub(crate) use ranged_crossbow_attack::RangedCrossbowAttackGoal;
pub(crate) use reset_universal_anger_target::ResetUniversalAngerTargetGoal;
pub(crate) use restrict_sun::RestrictSunGoal;
pub(crate) use run_around_like_crazy::RunAroundLikeCrazyGoal;
pub(crate) use selector::{Goal, GoalControl, GoalControls, GoalSelector};
pub(crate) use sit_when_ordered_to::SitWhenOrderedToGoal;
pub(crate) use spellcaster_casting_spell::SpellcasterCastingSpellGoal;
pub(crate) use spellcaster_use_spell::{DEFAULT_CAST_WARMUP_TIME, SpellcasterUseSpellBase};
pub(crate) use tamable_animal_panic::TamableAnimalPanicGoal;
pub(crate) use tempt_goal::{TemptGoal, TemptScareRule};
pub(crate) use try_find_water::TryFindWaterGoal;
pub(crate) use water_avoiding_random_flying::{
    WaterAvoidingRandomFlyingGoal, flying_stroll_position,
};
pub(crate) use water_avoiding_random_stroll::{
    WATER_AVOIDING_RANDOM_STROLL_PROBABILITY, WaterAvoidingRandomStrollGoal,
};

/// Halves a tick delay, rounding up.
///
/// Vanilla parity: `Goal.reducedTickDelay`.
pub(crate) const fn reduced_tick_delay(ticks: i32) -> i32 {
    (ticks + 1) / 2
}
