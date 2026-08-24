//! Every mob's goals reach the goal selector through one call and one only.
//!
//! `LivingEntity::server_ai_step` does nothing unless a mob overrides it to
//! call `Mob::mob_server_ai_step`, and that call is the only path to
//! `tick_goal_selectors`, `tick_path_navigation` and `tick_move_control`. Every
//! passive and water mob overrode it. None of the fifteen hostiles did, nor
//! the iron or the snow golem, so each of them registered a full goal set --
//! the zombie's includes a melee attack and a target selector -- and then
//! never ticked any of it.
//!
//! Nothing caught it, because the mob-local unit tests call
//! `mob_server_ai_step` directly and so step straight over the missing
//! override. These tests come in through `LivingEntity::server_ai_step`
//! instead, which is the door the tick actually uses.

use super::*;
use crate::entity::entities::{
    CaveSpiderEntity, CreeperEntity, DrownedEntity, EndermanEntity, HuskEntity, IronGolemEntity,
    MagmaCubeEntity, SilverfishEntity, SkeletonEntity, SlimeEntity, SnowGolemEntity, SpiderEntity,
    StrayEntity, WitchEntity, WitherSkeletonEntity, ZombieEntity, ZombifiedPiglinEntity,
};
use crate::entity::{LivingEntity, Mob, next_entity_id};
use steel_registry::vanilla_entities;

/// `mob_server_ai_step` bumps `no_action_time` before it does anything else,
/// which makes it the cheapest possible witness that the whole body ran.
fn ai_step_runs(mob: &impl Mob) -> bool {
    mob.set_no_action_time(0);
    LivingEntity::server_ai_step(mob);
    mob.no_action_time() == 1
}

macro_rules! assert_ai_runs {
    ($($name:ident: $ty:ty, $entity_type:expr;)*) => {
        $(
            #[test]
            fn $name() {
                init_vanilla_registry();
                let mob = <$ty>::new($entity_type, next_entity_id(), DVec3::ZERO, Weak::new());
                assert!(
                    ai_step_runs(&mob),
                    "this mob's goals never tick: `server_ai_step` does not reach \
                     `Mob::mob_server_ai_step`"
                );
            }
        )*
    };
}

assert_ai_runs! {
    a_zombie_runs_its_goals: ZombieEntity, &vanilla_entities::ZOMBIE;
    a_husk_runs_its_goals: HuskEntity, &vanilla_entities::HUSK;
    a_drowned_runs_its_goals: DrownedEntity, &vanilla_entities::DROWNED;
    a_zombified_piglin_runs_its_goals: ZombifiedPiglinEntity, &vanilla_entities::ZOMBIFIED_PIGLIN;
    a_skeleton_runs_its_goals: SkeletonEntity, &vanilla_entities::SKELETON;
    a_stray_runs_its_goals: StrayEntity, &vanilla_entities::STRAY;
    a_wither_skeleton_runs_its_goals: WitherSkeletonEntity, &vanilla_entities::WITHER_SKELETON;
    a_creeper_runs_its_goals: CreeperEntity, &vanilla_entities::CREEPER;
    a_spider_runs_its_goals: SpiderEntity, &vanilla_entities::SPIDER;
    a_cave_spider_runs_its_goals: CaveSpiderEntity, &vanilla_entities::CAVE_SPIDER;
    an_enderman_runs_its_goals: EndermanEntity, &vanilla_entities::ENDERMAN;
    a_silverfish_runs_its_goals: SilverfishEntity, &vanilla_entities::SILVERFISH;
    a_witch_runs_its_goals: WitchEntity, &vanilla_entities::WITCH;
    a_slime_runs_its_goals: SlimeEntity, &vanilla_entities::SLIME;
    a_magma_cube_runs_its_goals: MagmaCubeEntity, &vanilla_entities::MAGMA_CUBE;
    an_iron_golem_runs_its_goals: IronGolemEntity, &vanilla_entities::IRON_GOLEM;
    a_snow_golem_runs_its_goals: SnowGolemEntity, &vanilla_entities::SNOW_GOLEM;
}
