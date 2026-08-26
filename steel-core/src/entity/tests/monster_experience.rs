//! Every hostile mob is worth killing.
//!
//! Vanilla's `Monster` constructor is `this.xpReward = 5`, and Steel has no
//! `Monster` layer, so each mob sets its own. Thirteen of them never did, so a
//! zombie, a skeleton and a creeper all died worth nothing -- and a sculk
//! catalyst next to one stayed inert, because there was no experience to eat.
//!
//! These come in through `Mob::xp_reward`, which is what the death path reads.
//! The slime and the magma cube are worth their size rather than a flat five,
//! and their tests live beside them because `CubeLike` is `pub(super)`.

use super::*;
use crate::entity::entities::{
    BoggedEntity, BreezeEntity, CaveSpiderEntity, CreeperEntity, DrownedEntity, EndermanEntity,
    GiantEntity, HoglinEntity, HuskEntity, ParchedEntity, PiglinBruteEntity, PiglinEntity,
    SilverfishEntity, SkeletonEntity, SpiderEntity, StrayEntity, WitchEntity, WitherSkeletonEntity,
    ZoglinEntity, ZombieEntity, ZombifiedPiglinEntity,
};
use crate::entity::{LivingEntity, Mob, next_entity_id};
use steel_registry::vanilla_entities;

macro_rules! assert_monster_reward {
    ($($name:ident: $ty:ty, $entity_type:expr;)*) => {
        $(
            #[test]
            fn $name() {
                init_vanilla_registry();
                let mob = <$ty>::new($entity_type, next_entity_id(), DVec3::ZERO, Weak::new());
                assert_eq!(
                    mob.xp_reward(),
                    5,
                    "this monster drops no experience: vanilla's `Monster` \
                     constructor sets `xpReward = 5` and nothing here does"
                );
            }
        )*
    };
}

assert_monster_reward! {
    a_zombie_is_worth_five: ZombieEntity, &vanilla_entities::ZOMBIE;
    a_husk_is_worth_five: HuskEntity, &vanilla_entities::HUSK;
    a_drowned_is_worth_five: DrownedEntity, &vanilla_entities::DROWNED;
    a_zombified_piglin_is_worth_five: ZombifiedPiglinEntity, &vanilla_entities::ZOMBIFIED_PIGLIN;
    a_skeleton_is_worth_five: SkeletonEntity, &vanilla_entities::SKELETON;
    a_stray_is_worth_five: StrayEntity, &vanilla_entities::STRAY;
    a_bogged_is_worth_five: BoggedEntity, &vanilla_entities::BOGGED;
    a_parched_is_worth_five: ParchedEntity, &vanilla_entities::PARCHED;
    a_giant_is_worth_five: GiantEntity, &vanilla_entities::GIANT;
    a_wither_skeleton_is_worth_five: WitherSkeletonEntity, &vanilla_entities::WITHER_SKELETON;
    a_creeper_is_worth_five: CreeperEntity, &vanilla_entities::CREEPER;
    a_spider_is_worth_five: SpiderEntity, &vanilla_entities::SPIDER;
    a_cave_spider_is_worth_five: CaveSpiderEntity, &vanilla_entities::CAVE_SPIDER;
    an_enderman_is_worth_five: EndermanEntity, &vanilla_entities::ENDERMAN;
    a_silverfish_is_worth_five: SilverfishEntity, &vanilla_entities::SILVERFISH;
    a_witch_is_worth_five: WitchEntity, &vanilla_entities::WITCH;
    a_piglin_is_worth_five: PiglinEntity, &vanilla_entities::PIGLIN;
    a_hoglin_is_worth_five: HoglinEntity, &vanilla_entities::HOGLIN;
    a_zoglin_is_worth_five: ZoglinEntity, &vanilla_entities::ZOGLIN;
}

/// Vanilla parity: the `this.xpReward = 10` of the `Breeze` constructor, twice
/// what the `Monster` constructor would have given it.
#[test]
fn a_breeze_is_worth_twice_an_ordinary_monster() {
    init_vanilla_registry();
    let breeze = BreezeEntity::new(
        &vanilla_entities::BREEZE,
        next_entity_id(),
        DVec3::ZERO,
        Weak::new(),
    );
    assert_eq!(breeze.xp_reward(), 10);
}

/// Vanilla parity: the `this.xpReward = 20` of the `PiglinBrute` constructor,
/// which is the one monster worth more than five without being a boss.
#[test]
fn a_piglin_brute_is_worth_four_times_a_monster() {
    init_vanilla_registry();
    let brute = PiglinBruteEntity::new(
        &vanilla_entities::PIGLIN_BRUTE,
        next_entity_id(),
        DVec3::ZERO,
        Weak::new(),
    );
    assert_eq!(brute.xp_reward(), 20);
}

/// Vanilla parity: `Zombie.getBaseExperienceReward`, which multiplies a baby's
/// reward by two and a half. A grown one is untouched.
#[test]
fn a_baby_zombie_is_worth_more_than_a_grown_one() {
    init_vanilla_registry();
    let grown = ZombieEntity::new(
        &vanilla_entities::ZOMBIE,
        next_entity_id(),
        DVec3::ZERO,
        Weak::new(),
    );
    let grown_reward = grown.base_experience_reward();

    let baby = ZombieEntity::new(
        &vanilla_entities::ZOMBIE,
        next_entity_id(),
        DVec3::ZERO,
        Weak::new(),
    );
    baby.set_baby(true);
    let baby_reward = baby.base_experience_reward();

    assert_eq!(grown_reward, 5);
    assert_eq!(
        baby_reward, 12,
        "5 * 2.5 truncated, as vanilla's `(int)` cast"
    );
    assert!(baby_reward > grown_reward);
}
