//! The gift a village gives back.
//!
//! Vanilla parity: `net.minecraft.world.entity.ai.behavior.GiveGiftToHero`,
//! which vanilla types to `Villager` even though it sits in the shared
//! `ai/behavior` package.

use foton_registry::loot_table::LootTableRef;
use foton_registry::{vanilla_loot_tables, vanilla_mob_effects};

use super::villager;
use crate::entity::ai::brain::behavior::{
    BrainContext, MemoryModuleId, MemoryStatus, TimedBehavior, utils,
};
use crate::entity::ai::brain::memory::{EntityMemory, memory_module_types};
use crate::entity::ai::brain::position_tracker::PositionTracker;
use crate::entity::living_entity::gift_loot_items_with_rng;
use crate::entity::{AgeableMob, SharedEntity};

/// Vanilla parity: `GiveGiftToHero.THROW_GIFT_AT_DISTANCE`.
const THROW_GIFT_AT_DISTANCE: i32 = 5;
/// Vanilla parity: `GiveGiftToHero.MIN_TIME_BETWEEN_GIFTS`.
const MIN_TIME_BETWEEN_GIFTS: i32 = 600;
/// Vanilla parity: the `nextInt(6001)` spread on top of that minimum, which is
/// `MAX_TIME_BETWEEN_GIFTS - MIN_TIME_BETWEEN_GIFTS + 1`.
const TIME_BETWEEN_GIFTS_SPREAD: i32 = 6_001;
/// Vanilla parity: `GiveGiftToHero.TIME_TO_DELAY_FOR_HEAD_TO_FINISH_TURNING`.
const TIME_TO_DELAY_FOR_HEAD_TO_FINISH_TURNING: i64 = 20;
/// Vanilla parity: `GiveGiftToHero.SPEED_MODIFIER`.
const SPEED_MODIFIER: f64 = 0.5;
/// Vanilla parity: the `new GiveGiftToHero(100)` every package builds.
pub const GIFT_TIMEOUT: i32 = 100;

/// Vanilla parity: the `ImmutableMap` handed to `GiveGiftToHero`'s `super(...)`.
const GIFT_ENTRY_CONDITION: &[(MemoryModuleId, MemoryStatus)] = &[
    (
        memory_module_types::WALK_TARGET.id(),
        MemoryStatus::Registered,
    ),
    (
        memory_module_types::LOOK_TARGET.id(),
        MemoryStatus::Registered,
    ),
    (
        memory_module_types::INTERACTION_TARGET.id(),
        MemoryStatus::Registered,
    ),
    (
        memory_module_types::NEAREST_VISIBLE_PLAYER.id(),
        MemoryStatus::ValuePresent,
    ),
];

/// Throws a present at the player who saved the village.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.behavior.GiveGiftToHero`. A
/// villager gives one gift per run and then waits between half a minute and
/// five and a half before it will offer another.
pub struct GiveGiftToHero {
    timeout: i32,
    time_until_next_gift: i32,
    gift_given_during_this_run: bool,
    time_since_start: i64,
}

impl GiveGiftToHero {
    /// Vanilla parity: `new GiveGiftToHero(timeout)`.
    #[must_use]
    pub const fn new(timeout: i32) -> Self {
        Self {
            timeout,
            time_until_next_gift: MIN_TIME_BETWEEN_GIFTS,
            gift_given_during_this_run: false,
            time_since_start: 0,
        }
    }

    /// Vanilla parity: `GiveGiftToHero.getNearestTargetableHero`, the memory
    /// filtered down to a player who actually carries the effect.
    fn nearest_targetable_hero(ctx: &BrainContext<'_>) -> Option<SharedEntity> {
        let player = ctx
            .brain()
            .get_memory(memory_module_types::NEAREST_VISIBLE_PLAYER)?
            .get()?;
        let is_hero = player
            .as_living_entity()
            .is_some_and(|living| living.has_mob_effect(vanilla_mob_effects::HERO_OF_THE_VILLAGE));
        is_hero.then_some(player)
    }
}

impl TimedBehavior for GiveGiftToHero {
    fn entry_condition(&self) -> &[(MemoryModuleId, MemoryStatus)] {
        GIFT_ENTRY_CONDITION
    }

    fn duration(&self) -> (i32, i32) {
        (self.timeout, self.timeout)
    }

    /// Vanilla parity: `GiveGiftToHero.checkExtraStartConditions`, whose
    /// countdown only runs while a hero is actually in sight.
    fn check_extra_start_conditions(&mut self, ctx: &BrainContext<'_>) -> bool {
        if Self::nearest_targetable_hero(ctx).is_none() {
            return false;
        }
        if self.time_until_next_gift > 0 {
            self.time_until_next_gift -= 1;
            return false;
        }
        true
    }

    /// Vanilla parity: `GiveGiftToHero.start`.
    fn start(&mut self, ctx: &BrainContext<'_>) {
        self.gift_given_during_this_run = false;
        self.time_since_start = ctx.game_time();
        let Some(hero) = Self::nearest_targetable_hero(ctx) else {
            return;
        };
        let brain = ctx.brain();
        brain.set_memory(
            memory_module_types::INTERACTION_TARGET,
            EntityMemory::new(&hero),
        );
        brain.set_memory(
            memory_module_types::LOOK_TARGET,
            PositionTracker::of_entity(&hero, true),
        );
    }

    fn can_still_use(&mut self, ctx: &BrainContext<'_>) -> bool {
        Self::nearest_targetable_hero(ctx).is_some() && !self.gift_given_during_this_run
    }

    /// Vanilla parity: `GiveGiftToHero.tick`.
    fn tick(&mut self, ctx: &BrainContext<'_>) {
        let Some(hero) = Self::nearest_targetable_hero(ctx) else {
            return;
        };
        let brain = ctx.brain();
        brain.set_memory(
            memory_module_types::LOOK_TARGET,
            PositionTracker::of_entity(&hero, true),
        );

        let within_throwing_distance = utils::block_closer_than(
            ctx.mob().block_position(),
            hero.block_position(),
            f64::from(THROW_GIFT_AT_DISTANCE),
        );
        if !within_throwing_distance {
            utils::set_walk_and_look_target_memories(
                brain,
                PositionTracker::of_entity(&hero, true),
                SPEED_MODIFIER,
                THROW_GIFT_AT_DISTANCE,
            );
            return;
        }
        if ctx.game_time() - self.time_since_start <= TIME_TO_DELAY_FOR_HEAD_TO_FINISH_TURNING {
            return;
        }

        let Some(body) = villager(ctx) else {
            return;
        };
        let mut rng = rand::rng();
        for gift in gift_loot_items_with_rng(body, loot_table_to_throw(ctx), &mut rng) {
            utils::throw_item(body, gift, hero.position());
        }
        self.gift_given_during_this_run = true;
    }

    /// Vanilla parity: `GiveGiftToHero.stop`.
    fn stop(&mut self, ctx: &BrainContext<'_>) {
        self.time_until_next_gift =
            MIN_TIME_BETWEEN_GIFTS + rand::random_range(0..TIME_BETWEEN_GIFTS_SPREAD);
        let brain = ctx.brain();
        brain.erase_memory(memory_module_types::INTERACTION_TARGET.id());
        brain.erase_memory(memory_module_types::WALK_TARGET.id());
        brain.erase_memory(memory_module_types::LOOK_TARGET.id());
    }

    fn debug_name(&self) -> &'static str {
        "GiveGiftToHero"
    }
}

/// The table a villager's gift is rolled from.
///
/// Vanilla parity: `GiveGiftToHero.getLootTableToThrow` and the `GIFTS` map it
/// reads. That map is a literal `ImmutableMap` in the behavior itself -- no
/// datapack reaches it and `FotonExtractor` emits nothing for it -- so it is
/// mirrored here entry for entry. `none` and `nitwit` are the two professions
/// vanilla leaves out of the map, and `getOrDefault` sends them to the
/// unemployed table.
fn loot_table_to_throw(ctx: &BrainContext<'_>) -> LootTableRef {
    use vanilla_loot_tables as gifts;

    let Some(body) = villager(ctx) else {
        return &gifts::GAMEPLAY_HERO_OF_THE_VILLAGE_UNEMPLOYED_GIFT;
    };
    if AgeableMob::is_baby(body) {
        return &gifts::GAMEPLAY_HERO_OF_THE_VILLAGE_BABY_GIFT;
    }
    let profession = body.profession();
    if profession.key.namespace != "minecraft" {
        return &gifts::GAMEPLAY_HERO_OF_THE_VILLAGE_UNEMPLOYED_GIFT;
    }
    match profession.key.path.as_ref() {
        "armorer" => &gifts::GAMEPLAY_HERO_OF_THE_VILLAGE_ARMORER_GIFT,
        "butcher" => &gifts::GAMEPLAY_HERO_OF_THE_VILLAGE_BUTCHER_GIFT,
        "cartographer" => &gifts::GAMEPLAY_HERO_OF_THE_VILLAGE_CARTOGRAPHER_GIFT,
        "cleric" => &gifts::GAMEPLAY_HERO_OF_THE_VILLAGE_CLERIC_GIFT,
        "farmer" => &gifts::GAMEPLAY_HERO_OF_THE_VILLAGE_FARMER_GIFT,
        "fisherman" => &gifts::GAMEPLAY_HERO_OF_THE_VILLAGE_FISHERMAN_GIFT,
        "fletcher" => &gifts::GAMEPLAY_HERO_OF_THE_VILLAGE_FLETCHER_GIFT,
        "leatherworker" => &gifts::GAMEPLAY_HERO_OF_THE_VILLAGE_LEATHERWORKER_GIFT,
        "librarian" => &gifts::GAMEPLAY_HERO_OF_THE_VILLAGE_LIBRARIAN_GIFT,
        "mason" => &gifts::GAMEPLAY_HERO_OF_THE_VILLAGE_MASON_GIFT,
        "shepherd" => &gifts::GAMEPLAY_HERO_OF_THE_VILLAGE_SHEPHERD_GIFT,
        "toolsmith" => &gifts::GAMEPLAY_HERO_OF_THE_VILLAGE_TOOLSMITH_GIFT,
        "weaponsmith" => &gifts::GAMEPLAY_HERO_OF_THE_VILLAGE_WEAPONSMITH_GIFT,
        _ => &gifts::GAMEPLAY_HERO_OF_THE_VILLAGE_UNEMPLOYED_GIFT,
    }
}
