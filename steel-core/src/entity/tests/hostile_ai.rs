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
//!
//! That door was still one short. The tick the server runs is
//! `Entity::tick` -> `tick_living_entity` -> `ai_step` -> `default_ai_step` ->
//! `server_ai_step`, and entering at `server_ai_step` steps over the two links
//! above it. A creaking proved it: its `tick` called `Entity::default_tick`,
//! which is only vanilla's `Entity.baseTick`, so nothing below it ran -- and
//! `a_creaking_runs_its_brain` stayed green the whole time, because it came in
//! under the break. `assert_the_tick_reaches_the_goals` closes that: it starts
//! at `Entity::tick`, the one call the server makes, and every mob is in it.

use super::*;
use crate::entity::entities::{
    AllayEntity, ArmadilloEntity, AxolotlEntity, BatEntity, BeeEntity, BlazeEntity, BoggedEntity,
    BreezeEntity, CamelEntity, CamelHuskEntity, CatEntity, CaveSpiderEntity, ChickenEntity,
    CodEntity, CopperGolemEntity, CowEntity, CreakingEntity, CreeperEntity, DolphinEntity,
    DonkeyEntity, DrownedEntity, ElderGuardianEntity, EndermanEntity, EndermiteEntity,
    EvokerEntity, FoxEntity, FrogEntity, GhastEntity, GiantEntity, GlowSquidEntity, GoatEntity,
    GuardianEntity, HappyGhastEntity, HoglinEntity, HorseEntity, HuskEntity, IllusionerEntity,
    IronGolemEntity, LlamaEntity, MagmaCubeEntity, MuleEntity, MushroomCowEntity, NautilusEntity,
    OcelotEntity, PandaEntity, ParchedEntity, ParrotEntity, PhantomEntity, PigEntity,
    PiglinBruteEntity, PiglinEntity, PillagerEntity, PolarBearEntity, PufferfishEntity,
    RabbitEntity, RavagerEntity, SalmonEntity, SheepEntity, ShulkerEntity, SilverfishEntity,
    SkeletonEntity, SkeletonHorseEntity, SlimeEntity, SnifferEntity, SnowGolemEntity, SpiderEntity,
    SquidEntity, StrayEntity, StriderEntity, SulfurCubeEntity, TadpoleEntity, TraderLlamaEntity,
    TropicalFishEntity, TurtleEntity, VexEntity, VillagerEntity, VindicatorEntity,
    WanderingTraderEntity, WardenEntity, WitchEntity, WitherBoss, WitherSkeletonEntity, WolfEntity,
    ZoglinEntity, ZombieEntity, ZombieHorseEntity, ZombieNautilusEntity, ZombieVillagerEntity,
    ZombifiedPiglinEntity,
};
use std::sync::atomic::{AtomicBool, Ordering};

use crate::entity::ai::goal::{Goal, GoalControls};
use crate::entity::{Entity, LivingEntity, Mob, MobEffectInstance, PathfinderMob, next_entity_id};
use steel_registry::{vanilla_entities, vanilla_mob_effects};

/// A living entity's own tick is the door every one of the above comes through.
///
/// `Entity::tick` defaults to `LivingEntity::tick_living_entity`, and a mob that overrides
/// it has to call that itself -- `Entity::default_tick` is only `baseTick`, so a mob that
/// reached for it instead lost its item use, its mob effects, its death handling and its
/// whole `ai_step`. Nine mobs did, and then a tenth. The witness is a mob effect's
/// duration, because `tick_mob_effects` is near the top of the living tick and needs no
/// world.
fn living_tick_runs(mob: &impl LivingEntity) -> bool {
    mob.add_mob_effect(MobEffectInstance::with_duration(
        vanilla_mob_effects::GLOWING,
        100,
        0,
    ));
    Entity::tick(mob);
    mob.mob_effect(vanilla_mob_effects::GLOWING)
        .is_some_and(|effect| effect.duration() < 100)
}

macro_rules! assert_living_tick_runs {
    ($($name:ident: $ty:ty, $entity_type:expr;)*) => {
        $(
            #[test]
            fn $name() {
                init_vanilla_registry();
                let mob = <$ty>::new($entity_type, next_entity_id(), DVec3::ZERO, Weak::new());
                assert!(
                    living_tick_runs(&mob),
                    "this mob's own tick never reaches `LivingEntity::tick_living_entity`, \
                     so nothing below `Entity.baseTick` runs for it"
                );
            }
        )*
    };
}

// Only the mobs that override `Entity::tick` need this: the rest inherit the default,
// which is `tick_living_entity` itself. Every override belongs here, because the whole
// point is that the override is where the link gets dropped.
assert_living_tick_runs! {
    a_warden_runs_its_living_tick: WardenEntity, &vanilla_entities::WARDEN;
    an_allay_runs_its_living_tick: AllayEntity, &vanilla_entities::ALLAY;
    a_polar_bear_runs_its_living_tick: PolarBearEntity, &vanilla_entities::POLAR_BEAR;
    an_armadillo_runs_its_living_tick: ArmadilloEntity, &vanilla_entities::ARMADILLO;
    a_panda_runs_its_living_tick: PandaEntity, &vanilla_entities::PANDA;
    a_sniffer_runs_its_living_tick: SnifferEntity, &vanilla_entities::SNIFFER;
    a_camel_runs_its_living_tick: CamelEntity, &vanilla_entities::CAMEL;
    a_pufferfish_runs_its_living_tick: PufferfishEntity, &vanilla_entities::PUFFERFISH;
    a_dolphin_runs_its_living_tick: DolphinEntity, &vanilla_entities::DOLPHIN;
    a_nautilus_runs_its_living_tick: NautilusEntity, &vanilla_entities::NAUTILUS;
    a_zombie_nautilus_runs_its_living_tick: ZombieNautilusEntity, &vanilla_entities::ZOMBIE_NAUTILUS;
    a_vex_runs_its_living_tick: VexEntity, &vanilla_entities::VEX;
    a_shulker_runs_its_living_tick: ShulkerEntity, &vanilla_entities::SHULKER;
    // The rest of the `Entity::tick` overrides, none of which were covered.
    a_creaking_runs_its_living_tick: CreakingEntity, &vanilla_entities::CREAKING;
    an_endermite_runs_its_living_tick: EndermiteEntity, &vanilla_entities::ENDERMITE;
    a_wolf_runs_its_living_tick: WolfEntity, &vanilla_entities::WOLF;
    a_copper_golem_runs_its_living_tick: CopperGolemEntity, &vanilla_entities::COPPER_GOLEM;
    a_strider_runs_its_living_tick: StriderEntity, &vanilla_entities::STRIDER;
    a_fox_runs_its_living_tick: FoxEntity, &vanilla_entities::FOX;
    a_happy_ghast_runs_its_living_tick: HappyGhastEntity, &vanilla_entities::HAPPY_GHAST;
    a_cat_runs_its_living_tick: CatEntity, &vanilla_entities::CAT;
    a_parrot_runs_its_living_tick: ParrotEntity, &vanilla_entities::PARROT;
    a_skeleton_horse_runs_its_living_tick: SkeletonHorseEntity, &vanilla_entities::SKELETON_HORSE;
    a_llama_runs_its_living_tick: LlamaEntity, &vanilla_entities::LLAMA;
    a_mule_runs_its_living_tick: MuleEntity, &vanilla_entities::MULE;
    a_horse_runs_its_living_tick: HorseEntity, &vanilla_entities::HORSE;
    a_trader_llama_runs_its_living_tick: TraderLlamaEntity, &vanilla_entities::TRADER_LLAMA;
    a_zombie_horse_runs_its_living_tick: ZombieHorseEntity, &vanilla_entities::ZOMBIE_HORSE;
    a_camel_husk_runs_its_living_tick: CamelHuskEntity, &vanilla_entities::CAMEL_HUSK;
    a_donkey_runs_its_living_tick: DonkeyEntity, &vanilla_entities::DONKEY;
    a_zombie_villager_runs_its_living_tick: ZombieVillagerEntity, &vanilla_entities::ZOMBIE_VILLAGER;
}

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
    // The warden is brain-driven too, and it is the one mob whose brain is the
    // whole animal: without `Brain::tick` it never emerges, never sniffs and
    // never digs away.
    a_warden_runs_its_brain: WardenEntity, &vanilla_entities::WARDEN;
    // Both nautilus mobs are brain-driven and override `Entity::tick` for their
    // dash clock, so they have two ways to fall out of the tick: this is the
    // one that proves `server_ai_step` still reaches `Brain::tick`.
    a_nautilus_runs_its_brain: NautilusEntity, &vanilla_entities::NAUTILUS;
    a_zombie_nautilus_runs_its_brain: ZombieNautilusEntity, &vanilla_entities::ZOMBIE_NAUTILUS;
}

/// The one call the server makes, run end to end.
///
/// `World::tick_entities` calls `Entity::tick` and nothing else, so that is the
/// only door worth defending. Behind it: `tick_living_entity` -> `ai_step` ->
/// `default_ai_step` -> `server_ai_step` -> `mob_server_ai_step`. Four links,
/// four ways to be a mob that stands still and compiles, and the two tests
/// above only cover the first and the last. `no_action_time` is the same
/// witness `ai_step_runs` uses; what differs is where the knock comes from.
///
/// Health is set first because `default_ai_step` runs the AI only for a mob
/// that is not `isImmobile`, and vanilla's `isImmobile` is `isDeadOrDying` --
/// a bare constructor leaves the health at zero, which no mob the server ticks
/// ever has.
fn the_tick_reaches_the_goals(mob: &impl Mob) -> bool {
    mob.set_health(1.0);
    assert!(
        !LivingEntity::is_dead_or_dying(mob),
        "test setup failed: this mob is still dead after `set_health`, so the \
         assertion below would be vacuous"
    );
    mob.set_no_action_time(0);
    Entity::tick(mob);
    mob.no_action_time() > 0
}

macro_rules! assert_the_tick_reaches_the_goals {
    ($($name:ident: $ty:ty, $entity_type:expr;)*) => {
        $(
            #[test]
            fn $name() {
                init_vanilla_registry();
                let mob = <$ty>::new($entity_type, next_entity_id(), DVec3::ZERO, Weak::new());
                assert!(
                    the_tick_reaches_the_goals(&mob),
                    "the server's own `Entity::tick` never reaches this mob's goals. \
                     One of four links is cut: `tick` -> `tick_living_entity`, \
                     `tick_living_entity` -> `ai_step`, `ai_step` -> `default_ai_step`, \
                     or `server_ai_step` -> `mob_server_ai_step`"
                );
            }
        )*
    };
}

// Every mob, without exception, save the ender dragon: vanilla's
// `EnderDragon.aiStep` does not call `super.aiStep`, registers no goals and
// drives itself from its phase manager instead, so it has no goal step to
// reach. `ender_dragon/tests.rs` covers the phases.
assert_the_tick_reaches_the_goals! {
    a_zombie_ticks_its_goals: ZombieEntity, &vanilla_entities::ZOMBIE;
    a_husk_ticks_its_goals: HuskEntity, &vanilla_entities::HUSK;
    a_drowned_ticks_its_goals: DrownedEntity, &vanilla_entities::DROWNED;
    a_zombified_piglin_ticks_its_goals: ZombifiedPiglinEntity, &vanilla_entities::ZOMBIFIED_PIGLIN;
    a_skeleton_ticks_its_goals: SkeletonEntity, &vanilla_entities::SKELETON;
    a_stray_ticks_its_goals: StrayEntity, &vanilla_entities::STRAY;
    a_bogged_ticks_its_goals: BoggedEntity, &vanilla_entities::BOGGED;
    a_parched_ticks_its_goals: ParchedEntity, &vanilla_entities::PARCHED;
    a_giant_ticks_its_goals: GiantEntity, &vanilla_entities::GIANT;
    a_wither_skeleton_ticks_its_goals: WitherSkeletonEntity, &vanilla_entities::WITHER_SKELETON;
    a_creeper_ticks_its_goals: CreeperEntity, &vanilla_entities::CREEPER;
    a_spider_ticks_its_goals: SpiderEntity, &vanilla_entities::SPIDER;
    a_cave_spider_ticks_its_goals: CaveSpiderEntity, &vanilla_entities::CAVE_SPIDER;
    an_enderman_ticks_its_goals: EndermanEntity, &vanilla_entities::ENDERMAN;
    a_silverfish_ticks_its_goals: SilverfishEntity, &vanilla_entities::SILVERFISH;
    a_witch_ticks_its_goals: WitchEntity, &vanilla_entities::WITCH;
    a_pillager_ticks_its_goals: PillagerEntity, &vanilla_entities::PILLAGER;
    a_vindicator_ticks_its_goals: VindicatorEntity, &vanilla_entities::VINDICATOR;
    an_evoker_ticks_its_goals: EvokerEntity, &vanilla_entities::EVOKER;
    an_illusioner_ticks_its_goals: IllusionerEntity, &vanilla_entities::ILLUSIONER;
    a_ravager_ticks_its_goals: RavagerEntity, &vanilla_entities::RAVAGER;
    a_slime_ticks_its_goals: SlimeEntity, &vanilla_entities::SLIME;
    a_magma_cube_ticks_its_goals: MagmaCubeEntity, &vanilla_entities::MAGMA_CUBE;
    a_sulfur_cube_ticks_its_goals: SulfurCubeEntity, &vanilla_entities::SULFUR_CUBE;
    an_iron_golem_ticks_its_goals: IronGolemEntity, &vanilla_entities::IRON_GOLEM;
    a_snow_golem_ticks_its_goals: SnowGolemEntity, &vanilla_entities::SNOW_GOLEM;
    a_copper_golem_ticks_its_goals: CopperGolemEntity, &vanilla_entities::COPPER_GOLEM;
    a_blaze_ticks_its_goals: BlazeEntity, &vanilla_entities::BLAZE;
    a_ghast_ticks_its_goals: GhastEntity, &vanilla_entities::GHAST;
    a_guardian_ticks_its_goals: GuardianEntity, &vanilla_entities::GUARDIAN;
    an_elder_guardian_ticks_its_goals: ElderGuardianEntity, &vanilla_entities::ELDER_GUARDIAN;
    an_endermite_ticks_its_goals: EndermiteEntity, &vanilla_entities::ENDERMITE;
    a_vex_ticks_its_goals: VexEntity, &vanilla_entities::VEX;
    a_phantom_ticks_its_goals: PhantomEntity, &vanilla_entities::PHANTOM;
    a_shulker_ticks_its_goals: ShulkerEntity, &vanilla_entities::SHULKER;
    a_wither_ticks_its_goals: WitherBoss, &vanilla_entities::WITHER;
    a_piglin_ticks_its_brain: PiglinEntity, &vanilla_entities::PIGLIN;
    a_piglin_brute_ticks_its_brain: PiglinBruteEntity, &vanilla_entities::PIGLIN_BRUTE;
    a_hoglin_ticks_its_brain: HoglinEntity, &vanilla_entities::HOGLIN;
    a_zoglin_ticks_its_brain: ZoglinEntity, &vanilla_entities::ZOGLIN;
    a_breeze_ticks_its_brain: BreezeEntity, &vanilla_entities::BREEZE;
    a_creaking_ticks_its_brain: CreakingEntity, &vanilla_entities::CREAKING;
    a_warden_ticks_its_brain: WardenEntity, &vanilla_entities::WARDEN;
    a_nautilus_ticks_its_brain: NautilusEntity, &vanilla_entities::NAUTILUS;
    a_zombie_nautilus_ticks_its_brain: ZombieNautilusEntity, &vanilla_entities::ZOMBIE_NAUTILUS;
    // Ambient, neutral and passive mobs. None of these were in either list
    // above, and a player meets most of them before ever meeting a hostile.
    a_bat_ticks_its_goals: BatEntity, &vanilla_entities::BAT;
    a_wolf_ticks_its_goals: WolfEntity, &vanilla_entities::WOLF;
    a_pig_ticks_its_goals: PigEntity, &vanilla_entities::PIG;
    a_cow_ticks_its_goals: CowEntity, &vanilla_entities::COW;
    a_mooshroom_ticks_its_goals: MushroomCowEntity, &vanilla_entities::MOOSHROOM;
    a_sheep_ticks_its_goals: SheepEntity, &vanilla_entities::SHEEP;
    a_chicken_ticks_its_goals: ChickenEntity, &vanilla_entities::CHICKEN;
    a_rabbit_ticks_its_goals: RabbitEntity, &vanilla_entities::RABBIT;
    a_goat_ticks_its_goals: GoatEntity, &vanilla_entities::GOAT;
    a_polar_bear_ticks_its_goals: PolarBearEntity, &vanilla_entities::POLAR_BEAR;
    a_panda_ticks_its_goals: PandaEntity, &vanilla_entities::PANDA;
    a_fox_ticks_its_goals: FoxEntity, &vanilla_entities::FOX;
    a_cat_ticks_its_goals: CatEntity, &vanilla_entities::CAT;
    an_ocelot_ticks_its_goals: OcelotEntity, &vanilla_entities::OCELOT;
    a_parrot_ticks_its_goals: ParrotEntity, &vanilla_entities::PARROT;
    a_bee_ticks_its_goals: BeeEntity, &vanilla_entities::BEE;
    a_turtle_ticks_its_goals: TurtleEntity, &vanilla_entities::TURTLE;
    a_strider_ticks_its_goals: StriderEntity, &vanilla_entities::STRIDER;
    a_happy_ghast_ticks_its_goals: HappyGhastEntity, &vanilla_entities::HAPPY_GHAST;
    an_armadillo_ticks_its_goals: ArmadilloEntity, &vanilla_entities::ARMADILLO;
    an_allay_ticks_its_brain: AllayEntity, &vanilla_entities::ALLAY;
    a_frog_ticks_its_brain: FrogEntity, &vanilla_entities::FROG;
    a_tadpole_ticks_its_brain: TadpoleEntity, &vanilla_entities::TADPOLE;
    an_axolotl_ticks_its_brain: AxolotlEntity, &vanilla_entities::AXOLOTL;
    a_sniffer_ticks_its_brain: SnifferEntity, &vanilla_entities::SNIFFER;
    // The equines, which all override `Entity::tick` for their own reasons.
    a_horse_ticks_its_goals: HorseEntity, &vanilla_entities::HORSE;
    a_donkey_ticks_its_goals: DonkeyEntity, &vanilla_entities::DONKEY;
    a_mule_ticks_its_goals: MuleEntity, &vanilla_entities::MULE;
    a_llama_ticks_its_goals: LlamaEntity, &vanilla_entities::LLAMA;
    a_trader_llama_ticks_its_goals: TraderLlamaEntity, &vanilla_entities::TRADER_LLAMA;
    a_skeleton_horse_ticks_its_goals: SkeletonHorseEntity, &vanilla_entities::SKELETON_HORSE;
    a_zombie_horse_ticks_its_goals: ZombieHorseEntity, &vanilla_entities::ZOMBIE_HORSE;
    a_camel_ticks_its_goals: CamelEntity, &vanilla_entities::CAMEL;
    a_camel_husk_ticks_its_goals: CamelHuskEntity, &vanilla_entities::CAMEL_HUSK;
    // The villagers and the trader, all three brain-driven.
    a_villager_ticks_its_brain: VillagerEntity, &vanilla_entities::VILLAGER;
    a_wandering_trader_ticks_its_goals: WanderingTraderEntity, &vanilla_entities::WANDERING_TRADER;
    a_zombie_villager_ticks_its_goals: ZombieVillagerEntity, &vanilla_entities::ZOMBIE_VILLAGER;
    // The water mobs.
    a_squid_ticks_its_goals: SquidEntity, &vanilla_entities::SQUID;
    a_glow_squid_ticks_its_goals: GlowSquidEntity, &vanilla_entities::GLOW_SQUID;
    a_cod_ticks_its_goals: CodEntity, &vanilla_entities::COD;
    a_salmon_ticks_its_goals: SalmonEntity, &vanilla_entities::SALMON;
    a_tropical_fish_ticks_its_goals: TropicalFishEntity, &vanilla_entities::TROPICAL_FISH;
    a_pufferfish_ticks_its_goals: PufferfishEntity, &vanilla_entities::PUFFERFISH;
    a_dolphin_ticks_its_goals: DolphinEntity, &vanilla_entities::DOLPHIN;
}

/// Ticking an empty goal list is still not an AI.
///
/// `assert_the_tick_reaches_the_goals` proves the tick arrives; this proves
/// there is something waiting for it. A mob that registered nothing would pass
/// the first and stand still in game, which is the same symptom by another
/// road.
fn has_something_to_run(mob: &impl Mob) -> bool {
    let goal_count = mob.mob_base().goal_selector().lock().goal_count()
        + mob.mob_base().target_selector().lock().goal_count();
    let brain_is_alive = Mob::brain(mob).is_some_and(|brain| !brain.is_brain_dead());
    goal_count > 0 || brain_is_alive
}

macro_rules! assert_it_has_something_to_run {
    ($($name:ident: $ty:ty, $entity_type:expr;)*) => {
        $(
            #[test]
            fn $name() {
                init_vanilla_registry();
                let mob = <$ty>::new($entity_type, next_entity_id(), DVec3::ZERO, Weak::new());
                assert!(
                    has_something_to_run(&mob),
                    "this mob registers neither a goal nor a brain, so the tick \
                     that reaches it has nothing to run"
                );
            }
        )*
    };
}

// Four mobs are deliberately absent, and the reasons are not the same.
//
// The bat and the giant are right. Neither overrides `registerGoals` in
// vanilla: a giant genuinely stands where it spawned, and everything a bat does
// is in `Bat.customServerAiStep`, which `mob_server_ai_step` reaches without a
// goal list. There is no generic way to see a `custom_server_ai_step` override
// from here, so they are named rather than asserted.
//
// The goat and the wandering trader used to be the ledger of what was wrong
// here. Both are in the list above now.
//
// The goat is the one vanilla animal with no `registerGoals` at all: everything
// it does lives in `GoatAi`, and Steel now builds that brain -- the core set,
// the idle set, the long jump and the ram, with `RamTarget` and
// `PrepareRamNearestTarget` ported beside it.
//
// The wandering trader registers the thirteen of vanilla's eighteen goals that
// Steel has goal types for. The five it still lacks -- two `UseItemGoal`s,
// `TradeWithPlayerGoal`, `LookAtTradingPlayerGoal` and `WanderToPositionGoal`
// -- are named where they would be registered.
assert_it_has_something_to_run! {

    a_zombie_registers_its_goals: ZombieEntity, &vanilla_entities::ZOMBIE;
    a_husk_registers_its_goals: HuskEntity, &vanilla_entities::HUSK;
    a_drowned_registers_its_goals: DrownedEntity, &vanilla_entities::DROWNED;
    a_zombified_piglin_registers_its_goals: ZombifiedPiglinEntity, &vanilla_entities::ZOMBIFIED_PIGLIN;
    a_skeleton_registers_its_goals: SkeletonEntity, &vanilla_entities::SKELETON;
    a_stray_registers_its_goals: StrayEntity, &vanilla_entities::STRAY;
    a_bogged_registers_its_goals: BoggedEntity, &vanilla_entities::BOGGED;
    a_parched_registers_its_goals: ParchedEntity, &vanilla_entities::PARCHED;
    a_wither_skeleton_registers_its_goals: WitherSkeletonEntity, &vanilla_entities::WITHER_SKELETON;
    a_creeper_registers_its_goals: CreeperEntity, &vanilla_entities::CREEPER;
    a_spider_registers_its_goals: SpiderEntity, &vanilla_entities::SPIDER;
    a_cave_spider_registers_its_goals: CaveSpiderEntity, &vanilla_entities::CAVE_SPIDER;
    an_enderman_registers_its_goals: EndermanEntity, &vanilla_entities::ENDERMAN;
    a_wandering_trader_registers_its_goals: WanderingTraderEntity, &vanilla_entities::WANDERING_TRADER;
    a_goat_registers_its_brain: GoatEntity, &vanilla_entities::GOAT;
    a_silverfish_registers_its_goals: SilverfishEntity, &vanilla_entities::SILVERFISH;
    a_witch_registers_its_goals: WitchEntity, &vanilla_entities::WITCH;
    a_pillager_registers_its_goals: PillagerEntity, &vanilla_entities::PILLAGER;
    a_vindicator_registers_its_goals: VindicatorEntity, &vanilla_entities::VINDICATOR;
    an_evoker_registers_its_goals: EvokerEntity, &vanilla_entities::EVOKER;
    an_illusioner_registers_its_goals: IllusionerEntity, &vanilla_entities::ILLUSIONER;
    a_ravager_registers_its_goals: RavagerEntity, &vanilla_entities::RAVAGER;
    a_slime_registers_its_goals: SlimeEntity, &vanilla_entities::SLIME;
    a_magma_cube_registers_its_goals: MagmaCubeEntity, &vanilla_entities::MAGMA_CUBE;
    a_sulfur_cube_registers_its_goals: SulfurCubeEntity, &vanilla_entities::SULFUR_CUBE;
    an_iron_golem_registers_its_goals: IronGolemEntity, &vanilla_entities::IRON_GOLEM;
    a_snow_golem_registers_its_goals: SnowGolemEntity, &vanilla_entities::SNOW_GOLEM;
    a_copper_golem_registers_its_goals: CopperGolemEntity, &vanilla_entities::COPPER_GOLEM;
    a_blaze_registers_its_goals: BlazeEntity, &vanilla_entities::BLAZE;
    a_ghast_registers_its_goals: GhastEntity, &vanilla_entities::GHAST;
    a_guardian_registers_its_goals: GuardianEntity, &vanilla_entities::GUARDIAN;
    an_elder_guardian_registers_its_goals: ElderGuardianEntity, &vanilla_entities::ELDER_GUARDIAN;
    an_endermite_registers_its_goals: EndermiteEntity, &vanilla_entities::ENDERMITE;
    a_vex_registers_its_goals: VexEntity, &vanilla_entities::VEX;
    a_phantom_registers_its_goals: PhantomEntity, &vanilla_entities::PHANTOM;
    a_shulker_registers_its_goals: ShulkerEntity, &vanilla_entities::SHULKER;
    a_wither_registers_its_goals: WitherBoss, &vanilla_entities::WITHER;
    a_piglin_registers_its_brain: PiglinEntity, &vanilla_entities::PIGLIN;
    a_piglin_brute_registers_its_brain: PiglinBruteEntity, &vanilla_entities::PIGLIN_BRUTE;
    a_hoglin_registers_its_brain: HoglinEntity, &vanilla_entities::HOGLIN;
    a_zoglin_registers_its_brain: ZoglinEntity, &vanilla_entities::ZOGLIN;
    a_breeze_registers_its_brain: BreezeEntity, &vanilla_entities::BREEZE;
    a_creaking_registers_its_brain: CreakingEntity, &vanilla_entities::CREAKING;
    a_warden_registers_its_brain: WardenEntity, &vanilla_entities::WARDEN;
    a_nautilus_registers_its_brain: NautilusEntity, &vanilla_entities::NAUTILUS;
    a_zombie_nautilus_registers_its_brain: ZombieNautilusEntity, &vanilla_entities::ZOMBIE_NAUTILUS;
    // Ambient, neutral and passive mobs. None of these were in either list
    // above, and a player meets most of them before ever meeting a hostile.
    a_wolf_registers_its_goals: WolfEntity, &vanilla_entities::WOLF;
    a_pig_registers_its_goals: PigEntity, &vanilla_entities::PIG;
    a_cow_registers_its_goals: CowEntity, &vanilla_entities::COW;
    a_mooshroom_registers_its_goals: MushroomCowEntity, &vanilla_entities::MOOSHROOM;
    a_sheep_registers_its_goals: SheepEntity, &vanilla_entities::SHEEP;
    a_chicken_registers_its_goals: ChickenEntity, &vanilla_entities::CHICKEN;
    a_rabbit_registers_its_goals: RabbitEntity, &vanilla_entities::RABBIT;
    a_polar_bear_registers_its_goals: PolarBearEntity, &vanilla_entities::POLAR_BEAR;
    a_panda_registers_its_goals: PandaEntity, &vanilla_entities::PANDA;
    a_fox_registers_its_goals: FoxEntity, &vanilla_entities::FOX;
    a_cat_registers_its_goals: CatEntity, &vanilla_entities::CAT;
    an_ocelot_registers_its_goals: OcelotEntity, &vanilla_entities::OCELOT;
    a_parrot_registers_its_goals: ParrotEntity, &vanilla_entities::PARROT;
    a_bee_registers_its_goals: BeeEntity, &vanilla_entities::BEE;
    a_turtle_registers_its_goals: TurtleEntity, &vanilla_entities::TURTLE;
    a_strider_registers_its_goals: StriderEntity, &vanilla_entities::STRIDER;
    a_happy_ghast_registers_its_goals: HappyGhastEntity, &vanilla_entities::HAPPY_GHAST;
    an_armadillo_registers_its_goals: ArmadilloEntity, &vanilla_entities::ARMADILLO;
    an_allay_registers_its_brain: AllayEntity, &vanilla_entities::ALLAY;
    a_frog_registers_its_brain: FrogEntity, &vanilla_entities::FROG;
    a_tadpole_registers_its_brain: TadpoleEntity, &vanilla_entities::TADPOLE;
    an_axolotl_registers_its_brain: AxolotlEntity, &vanilla_entities::AXOLOTL;
    a_sniffer_registers_its_brain: SnifferEntity, &vanilla_entities::SNIFFER;
    // The equines, which all override `Entity::tick` for their own reasons.
    a_horse_registers_its_goals: HorseEntity, &vanilla_entities::HORSE;
    a_donkey_registers_its_goals: DonkeyEntity, &vanilla_entities::DONKEY;
    a_mule_registers_its_goals: MuleEntity, &vanilla_entities::MULE;
    a_llama_registers_its_goals: LlamaEntity, &vanilla_entities::LLAMA;
    a_trader_llama_registers_its_goals: TraderLlamaEntity, &vanilla_entities::TRADER_LLAMA;
    a_skeleton_horse_registers_its_goals: SkeletonHorseEntity, &vanilla_entities::SKELETON_HORSE;
    a_zombie_horse_registers_its_goals: ZombieHorseEntity, &vanilla_entities::ZOMBIE_HORSE;
    a_camel_registers_its_goals: CamelEntity, &vanilla_entities::CAMEL;
    a_camel_husk_registers_its_goals: CamelHuskEntity, &vanilla_entities::CAMEL_HUSK;
    // The villagers and the trader, all three brain-driven.
    a_villager_registers_its_brain: VillagerEntity, &vanilla_entities::VILLAGER;
    a_zombie_villager_registers_its_goals: ZombieVillagerEntity, &vanilla_entities::ZOMBIE_VILLAGER;
    // The water mobs.
    a_squid_registers_its_goals: SquidEntity, &vanilla_entities::SQUID;
    a_glow_squid_registers_its_goals: GlowSquidEntity, &vanilla_entities::GLOW_SQUID;
    a_cod_registers_its_goals: CodEntity, &vanilla_entities::COD;
    a_salmon_registers_its_goals: SalmonEntity, &vanilla_entities::SALMON;
    a_tropical_fish_registers_its_goals: TropicalFishEntity, &vanilla_entities::TROPICAL_FISH;
    a_pufferfish_registers_its_goals: PufferfishEntity, &vanilla_entities::PUFFERFISH;
    a_dolphin_registers_its_goals: DolphinEntity, &vanilla_entities::DOLPHIN;
}

/// A goal that answers "no" and remembers being asked.
///
/// Empty controls, so it can never lose a priority conflict: whether it was
/// polled is a fact about the selector, not about this goal. It answers `false`
/// so it never starts, because a running goal stops being offered `can_use`.
struct ProbeGoal(Arc<AtomicBool>);

impl Goal for ProbeGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::EMPTY
    }

    fn can_use(&mut self, _mob: &dyn PathfinderMob) -> bool {
        self.0.store(true, Ordering::Relaxed);
        false
    }
}

/// Reaching `mob_server_ai_step` is not the same as running the goals.
///
/// `Mob::tick_goal_selectors` is the sixth call in that body and its trait
/// default is `{}`, so a mob that forgets the override registers a full goal
/// set, ticks past it every tick and stands still -- and `no_action_time`, the
/// witness the tests above use, is bumped one line earlier and never notices.
/// Vanilla `Mob.serverAiStep` ticks the goal selector for every mob it runs,
/// brain-driven or not, so every mob is asked here.
///
/// The probe is polled only on the ticks where `tick_count + id` is even, which
/// is why this runs the tick four times rather than once.
fn the_tick_polls_the_goals(mob: &impl Mob) -> bool {
    let polled = Arc::new(AtomicBool::new(false));
    mob.mob_base()
        .goal_selector()
        .lock()
        .add_goal(0, ProbeGoal(Arc::clone(&polled)));

    mob.set_health(1.0);
    assert!(
        !LivingEntity::is_dead_or_dying(mob),
        "test setup failed: a dead mob skips `ai_step`, so the assertion would \
         be vacuous"
    );
    for _ in 0..4 {
        Entity::tick(mob);
    }
    polled.load(Ordering::Relaxed)
}

macro_rules! assert_the_tick_polls_the_goals {
    ($($name:ident: $ty:ty, $entity_type:expr;)*) => {
        $(
            #[test]
            fn $name() {
                init_vanilla_registry();
                let mob = <$ty>::new($entity_type, next_entity_id(), DVec3::ZERO, Weak::new());
                assert!(
                    the_tick_polls_the_goals(&mob),
                    "this mob's goal selector is never ticked: `mob_server_ai_step` \
                     reaches `tick_goal_selectors`, whose trait default is empty, \
                     and this mob does not override it"
                );
            }
        )*
    };
}

assert_the_tick_polls_the_goals! {


    a_zombie_polls_its_goals: ZombieEntity, &vanilla_entities::ZOMBIE;
    a_husk_polls_its_goals: HuskEntity, &vanilla_entities::HUSK;
    a_drowned_polls_its_goals: DrownedEntity, &vanilla_entities::DROWNED;
    a_zombified_piglin_polls_its_goals: ZombifiedPiglinEntity, &vanilla_entities::ZOMBIFIED_PIGLIN;
    a_skeleton_polls_its_goals: SkeletonEntity, &vanilla_entities::SKELETON;
    a_stray_polls_its_goals: StrayEntity, &vanilla_entities::STRAY;
    a_bogged_polls_its_goals: BoggedEntity, &vanilla_entities::BOGGED;
    a_parched_polls_its_goals: ParchedEntity, &vanilla_entities::PARCHED;
    a_giant_polls_its_goals: GiantEntity, &vanilla_entities::GIANT;
    a_wither_skeleton_polls_its_goals: WitherSkeletonEntity, &vanilla_entities::WITHER_SKELETON;
    a_creeper_polls_its_goals: CreeperEntity, &vanilla_entities::CREEPER;
    a_spider_polls_its_goals: SpiderEntity, &vanilla_entities::SPIDER;
    a_cave_spider_polls_its_goals: CaveSpiderEntity, &vanilla_entities::CAVE_SPIDER;
    an_enderman_polls_its_goals: EndermanEntity, &vanilla_entities::ENDERMAN;
    a_silverfish_polls_its_goals: SilverfishEntity, &vanilla_entities::SILVERFISH;
    a_witch_polls_its_goals: WitchEntity, &vanilla_entities::WITCH;
    a_pillager_polls_its_goals: PillagerEntity, &vanilla_entities::PILLAGER;
    a_vindicator_polls_its_goals: VindicatorEntity, &vanilla_entities::VINDICATOR;
    an_evoker_polls_its_goals: EvokerEntity, &vanilla_entities::EVOKER;
    an_illusioner_polls_its_goals: IllusionerEntity, &vanilla_entities::ILLUSIONER;
    a_ravager_polls_its_goals: RavagerEntity, &vanilla_entities::RAVAGER;
    a_slime_polls_its_goals: SlimeEntity, &vanilla_entities::SLIME;
    a_magma_cube_polls_its_goals: MagmaCubeEntity, &vanilla_entities::MAGMA_CUBE;
    a_sulfur_cube_polls_its_goals: SulfurCubeEntity, &vanilla_entities::SULFUR_CUBE;
    an_iron_golem_polls_its_goals: IronGolemEntity, &vanilla_entities::IRON_GOLEM;
    a_snow_golem_polls_its_goals: SnowGolemEntity, &vanilla_entities::SNOW_GOLEM;
    a_copper_golem_polls_its_goals: CopperGolemEntity, &vanilla_entities::COPPER_GOLEM;
    a_blaze_polls_its_goals: BlazeEntity, &vanilla_entities::BLAZE;
    a_ghast_polls_its_goals: GhastEntity, &vanilla_entities::GHAST;
    a_guardian_polls_its_goals: GuardianEntity, &vanilla_entities::GUARDIAN;
    an_elder_guardian_polls_its_goals: ElderGuardianEntity, &vanilla_entities::ELDER_GUARDIAN;
    an_endermite_polls_its_goals: EndermiteEntity, &vanilla_entities::ENDERMITE;
    a_vex_polls_its_goals: VexEntity, &vanilla_entities::VEX;
    a_phantom_polls_its_goals: PhantomEntity, &vanilla_entities::PHANTOM;
    a_shulker_polls_its_goals: ShulkerEntity, &vanilla_entities::SHULKER;
    a_wither_polls_its_goals: WitherBoss, &vanilla_entities::WITHER;
    a_piglin_polls_its_goals: PiglinEntity, &vanilla_entities::PIGLIN;
    a_piglin_brute_polls_its_goals: PiglinBruteEntity, &vanilla_entities::PIGLIN_BRUTE;
    a_hoglin_polls_its_goals: HoglinEntity, &vanilla_entities::HOGLIN;
    a_zoglin_polls_its_goals: ZoglinEntity, &vanilla_entities::ZOGLIN;
    a_breeze_polls_its_goals: BreezeEntity, &vanilla_entities::BREEZE;
    a_creaking_polls_its_goals: CreakingEntity, &vanilla_entities::CREAKING;
    a_warden_polls_its_goals: WardenEntity, &vanilla_entities::WARDEN;
    a_nautilus_polls_its_goals: NautilusEntity, &vanilla_entities::NAUTILUS;
    a_zombie_nautilus_polls_its_goals: ZombieNautilusEntity, &vanilla_entities::ZOMBIE_NAUTILUS;
    // Ambient, neutral and passive mobs. None of these were in either list
    // above, and a player meets most of them before ever meeting a hostile.
    a_bat_polls_its_goals: BatEntity, &vanilla_entities::BAT;
    a_wolf_polls_its_goals: WolfEntity, &vanilla_entities::WOLF;
    a_pig_polls_its_goals: PigEntity, &vanilla_entities::PIG;
    a_cow_polls_its_goals: CowEntity, &vanilla_entities::COW;
    a_mooshroom_polls_its_goals: MushroomCowEntity, &vanilla_entities::MOOSHROOM;
    a_sheep_polls_its_goals: SheepEntity, &vanilla_entities::SHEEP;
    a_chicken_polls_its_goals: ChickenEntity, &vanilla_entities::CHICKEN;
    a_rabbit_polls_its_goals: RabbitEntity, &vanilla_entities::RABBIT;
    a_goat_polls_its_goals: GoatEntity, &vanilla_entities::GOAT;
    a_polar_bear_polls_its_goals: PolarBearEntity, &vanilla_entities::POLAR_BEAR;
    a_panda_polls_its_goals: PandaEntity, &vanilla_entities::PANDA;
    a_fox_polls_its_goals: FoxEntity, &vanilla_entities::FOX;
    a_cat_polls_its_goals: CatEntity, &vanilla_entities::CAT;
    an_ocelot_polls_its_goals: OcelotEntity, &vanilla_entities::OCELOT;
    a_parrot_polls_its_goals: ParrotEntity, &vanilla_entities::PARROT;
    a_bee_polls_its_goals: BeeEntity, &vanilla_entities::BEE;
    a_turtle_polls_its_goals: TurtleEntity, &vanilla_entities::TURTLE;
    a_strider_polls_its_goals: StriderEntity, &vanilla_entities::STRIDER;
    a_happy_ghast_polls_its_goals: HappyGhastEntity, &vanilla_entities::HAPPY_GHAST;
    an_armadillo_polls_its_goals: ArmadilloEntity, &vanilla_entities::ARMADILLO;
    an_allay_polls_its_goals: AllayEntity, &vanilla_entities::ALLAY;
    a_frog_polls_its_goals: FrogEntity, &vanilla_entities::FROG;
    a_tadpole_polls_its_goals: TadpoleEntity, &vanilla_entities::TADPOLE;
    an_axolotl_polls_its_goals: AxolotlEntity, &vanilla_entities::AXOLOTL;
    a_sniffer_polls_its_goals: SnifferEntity, &vanilla_entities::SNIFFER;
    // The equines, which all override `Entity::tick` for their own reasons.
    a_horse_polls_its_goals: HorseEntity, &vanilla_entities::HORSE;
    a_donkey_polls_its_goals: DonkeyEntity, &vanilla_entities::DONKEY;
    a_mule_polls_its_goals: MuleEntity, &vanilla_entities::MULE;
    a_llama_polls_its_goals: LlamaEntity, &vanilla_entities::LLAMA;
    a_trader_llama_polls_its_goals: TraderLlamaEntity, &vanilla_entities::TRADER_LLAMA;
    a_skeleton_horse_polls_its_goals: SkeletonHorseEntity, &vanilla_entities::SKELETON_HORSE;
    a_zombie_horse_polls_its_goals: ZombieHorseEntity, &vanilla_entities::ZOMBIE_HORSE;
    a_camel_polls_its_goals: CamelEntity, &vanilla_entities::CAMEL;
    a_camel_husk_polls_its_goals: CamelHuskEntity, &vanilla_entities::CAMEL_HUSK;
    // The villagers and the trader, all three brain-driven.
    a_villager_polls_its_goals: VillagerEntity, &vanilla_entities::VILLAGER;
    a_wandering_trader_polls_its_goals: WanderingTraderEntity, &vanilla_entities::WANDERING_TRADER;
    a_zombie_villager_polls_its_goals: ZombieVillagerEntity, &vanilla_entities::ZOMBIE_VILLAGER;
    // The water mobs.
    a_squid_polls_its_goals: SquidEntity, &vanilla_entities::SQUID;
    a_glow_squid_polls_its_goals: GlowSquidEntity, &vanilla_entities::GLOW_SQUID;
    a_cod_polls_its_goals: CodEntity, &vanilla_entities::COD;
    a_salmon_polls_its_goals: SalmonEntity, &vanilla_entities::SALMON;
    a_tropical_fish_polls_its_goals: TropicalFishEntity, &vanilla_entities::TROPICAL_FISH;
    a_pufferfish_polls_its_goals: PufferfishEntity, &vanilla_entities::PUFFERFISH;
    a_dolphin_polls_its_goals: DolphinEntity, &vanilla_entities::DOLPHIN;
}
