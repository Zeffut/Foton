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
    BlazeEntity, BoggedEntity, BreezeEntity, CaveSpiderEntity, CreakingEntity, CreeperEntity,
    DrownedEntity, ElderGuardianEntity, EndermanEntity, EndermiteEntity, EvokerEntity, GhastEntity,
    GiantEntity, GuardianEntity, HoglinEntity, HuskEntity, IllusionerEntity, IronGolemEntity,
    MagmaCubeEntity, ParchedEntity, PhantomEntity, PiglinBruteEntity, PiglinEntity, PillagerEntity,
    RavagerEntity, ShulkerEntity, SilverfishEntity, SkeletonEntity, SlimeEntity, SnowGolemEntity,
    SpiderEntity, StrayEntity, SulfurCubeEntity, VexEntity, VindicatorEntity, WitchEntity,
    WitherBoss, WitherSkeletonEntity, ZoglinEntity, ZombieEntity, ZombifiedPiglinEntity,
};
use crate::entity::{LivingEntity, Mob, next_entity_id};
use steel_registry::vanilla_entities;

/// `mob_server_ai_step` bumps `no_action_time` before it does anything else,
/// which makes it the cheapest possible witness that the whole body ran.
///
/// The bump is one tick for most mobs and two for a raider, whose
/// `update_no_action_time` counts double; either answer proves the body ran.
fn ai_step_runs(mob: &impl Mob) -> bool {
    mob.set_no_action_time(0);
    LivingEntity::server_ai_step(mob);
    mob.no_action_time() > 0
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
    a_bogged_runs_its_goals: BoggedEntity, &vanilla_entities::BOGGED;
    a_parched_runs_its_goals: ParchedEntity, &vanilla_entities::PARCHED;
    a_giant_runs_its_goals: GiantEntity, &vanilla_entities::GIANT;
    a_wither_skeleton_runs_its_goals: WitherSkeletonEntity, &vanilla_entities::WITHER_SKELETON;
    a_creeper_runs_its_goals: CreeperEntity, &vanilla_entities::CREEPER;
    a_spider_runs_its_goals: SpiderEntity, &vanilla_entities::SPIDER;
    a_cave_spider_runs_its_goals: CaveSpiderEntity, &vanilla_entities::CAVE_SPIDER;
    an_enderman_runs_its_goals: EndermanEntity, &vanilla_entities::ENDERMAN;
    a_silverfish_runs_its_goals: SilverfishEntity, &vanilla_entities::SILVERFISH;
    a_witch_runs_its_goals: WitchEntity, &vanilla_entities::WITCH;
    a_pillager_runs_its_goals: PillagerEntity, &vanilla_entities::PILLAGER;
    a_vindicator_runs_its_goals: VindicatorEntity, &vanilla_entities::VINDICATOR;
    an_evoker_runs_its_goals: EvokerEntity, &vanilla_entities::EVOKER;
    an_illusioner_runs_its_goals: IllusionerEntity, &vanilla_entities::ILLUSIONER;
    a_ravager_runs_its_goals: RavagerEntity, &vanilla_entities::RAVAGER;
    a_slime_runs_its_goals: SlimeEntity, &vanilla_entities::SLIME;
    a_magma_cube_runs_its_goals: MagmaCubeEntity, &vanilla_entities::MAGMA_CUBE;
    a_sulfur_cube_runs_its_goals: SulfurCubeEntity, &vanilla_entities::SULFUR_CUBE;
    an_iron_golem_runs_its_goals: IronGolemEntity, &vanilla_entities::IRON_GOLEM;
    a_snow_golem_runs_its_goals: SnowGolemEntity, &vanilla_entities::SNOW_GOLEM;
    a_blaze_runs_its_goals: BlazeEntity, &vanilla_entities::BLAZE;
    a_ghast_runs_its_goals: GhastEntity, &vanilla_entities::GHAST;
    a_guardian_runs_its_goals: GuardianEntity, &vanilla_entities::GUARDIAN;
    an_elder_guardian_runs_its_goals: ElderGuardianEntity, &vanilla_entities::ELDER_GUARDIAN;
    an_endermite_runs_its_goals: EndermiteEntity, &vanilla_entities::ENDERMITE;
    a_vex_runs_its_goals: VexEntity, &vanilla_entities::VEX;
    a_phantom_runs_its_goals: PhantomEntity, &vanilla_entities::PHANTOM;
    a_shulker_runs_its_goals: ShulkerEntity, &vanilla_entities::SHULKER;
    a_wither_runs_its_goals: WitherBoss, &vanilla_entities::WITHER;
    // The four piglin-family mobs are brain-driven rather than goal-driven, so
    // for them this is the check that `server_ai_step` reaches
    // `custom_server_ai_step`, and so `Brain::tick`, at all.
    a_piglin_runs_its_brain: PiglinEntity, &vanilla_entities::PIGLIN;
    a_piglin_brute_runs_its_brain: PiglinBruteEntity, &vanilla_entities::PIGLIN_BRUTE;
    a_hoglin_runs_its_brain: HoglinEntity, &vanilla_entities::HOGLIN;
    a_zoglin_runs_its_brain: ZoglinEntity, &vanilla_entities::ZOGLIN;
    // The breeze is brain-driven too, and registers no goals at all -- without
    // this path nothing it does ever runs.
    a_breeze_runs_its_brain: BreezeEntity, &vanilla_entities::BREEZE;
    // A creaking gates its move, look, jump and navigation ticks on `canMove`,
    // so the one thing that must not be gated is the step that reaches them.
    a_creaking_runs_its_brain: CreakingEntity, &vanilla_entities::CREAKING;
}
