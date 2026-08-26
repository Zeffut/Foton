//! Vanilla `PiglinSpecificSensor`, `PiglinBruteSpecificSensor` and
//! `HoglinSpecificSensor`.
//!
//! The three share a package in vanilla and a file here: all of them walk the
//! same `NEAREST_VISIBLE_LIVING_ENTITIES` snapshot once and split it into the
//! piglin/hoglin bookkeeping the brains read back.

use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::{REGISTRY, TaggedRegistryExt as _, vanilla_blocks, vanilla_entities};
use steel_utils::{BlockPos, BlockStateId, Downcast as _, Identifier};

use super::Sensor;
use crate::entity::ai::brain::behavior::utils;
use crate::entity::ai::brain::context::BrainContext;
use crate::entity::ai::brain::memory::{
    EntityMemory, MemoryModuleId, NearestVisibleLivingEntities, memory_module_types,
};
use crate::entity::entities::HoglinEntity;
use crate::entity::entities::mobs::hostile::piglin_predicates::{
    is_player_holding_loved_item, is_wearing_safe_armor,
};
use crate::entity::{LivingEntity, Mob, SharedEntity};
use crate::world::World;
use steel_registry::vanilla_block_tags::BlockTag;

/// Vanilla parity: `PiglinAi.REPELLENT_DETECTION_RANGE_HORIZONTAL`, shared with
/// `HoglinAi`.
const REPELLENT_DETECTION_RANGE_HORIZONTAL: i32 = 8;
/// Vanilla parity: `PiglinAi.REPELLENT_DETECTION_RANGE_VERTICAL`.
const REPELLENT_DETECTION_RANGE_VERTICAL: i32 = 4;

/// Returns whether `entity` is one of the two zombified piglin-family mobs.
///
/// Vanilla parity: `PiglinAi.isZombified`.
#[must_use]
pub fn is_zombified(entity: &dyn LivingEntity) -> bool {
    let entity = entity.as_entity_event_source();
    utils::is_of_type(entity, &vanilla_entities::ZOMBIFIED_PIGLIN)
        || utils::is_of_type(entity, &vanilla_entities::ZOGLIN)
}

/// Returns whether `entity` is a piglin's nemesis.
///
/// Vanilla parity: the `entity instanceof WitherSkeleton || entity instanceof
/// WitherBoss` of both piglin sensors.
#[must_use]
fn is_nemesis(entity: &dyn LivingEntity) -> bool {
    let entity = entity.as_entity_event_source();
    utils::is_of_type(entity, &vanilla_entities::WITHER_SKELETON)
        || utils::is_of_type(entity, &vanilla_entities::WITHER)
}

/// Returns whether `entity` is an adult piglin or a piglin brute.
///
/// Vanilla parity: the `entity instanceof AbstractPiglin piglin &&
/// piglin.isAdult()` of `PiglinAi.findNearbyAdultPiglins`. A brute has no baby
/// form, so it always counts.
fn is_adult_piglin(entity: &dyn LivingEntity) -> bool {
    let raw = entity.as_entity_event_source();
    if utils::is_of_type(raw, &vanilla_entities::PIGLIN_BRUTE) {
        return true;
    }
    utils::is_of_type(raw, &vanilla_entities::PIGLIN) && !entity.is_baby()
}

/// Vanilla parity: `PiglinAi.findNearbyAdultPiglins`, which reads the
/// unfiltered `NEAREST_LIVING_ENTITIES` rather than the visible subset -- a
/// piglin hears its neighbors through a wall.
fn find_nearby_adult_piglins(ctx: &BrainContext<'_>) -> Vec<EntityMemory> {
    let Some(nearby) = ctx
        .brain()
        .get_memory(memory_module_types::NEAREST_LIVING_ENTITIES)
    else {
        return Vec::new();
    };
    nearby
        .into_iter()
        .filter(|remembered| {
            remembered
                .get()
                .and_then(|entity| entity.as_living_entity().map(is_adult_piglin))
                .unwrap_or(false)
        })
        .collect()
}

/// Vanilla parity: `PiglinSpecificSensor.findNearestRepellent`, plus the
/// `HoglinSpecificSensor` variant that takes a different tag and does not check
/// whether a campfire is lit.
fn find_nearest_repellent(
    world: &World,
    origin: BlockPos,
    tag: &Identifier,
    soul_campfire_must_be_lit: bool,
) -> Option<BlockPos> {
    origin.find_closest_match(
        REPELLENT_DETECTION_RANGE_HORIZONTAL,
        REPELLENT_DETECTION_RANGE_VERTICAL,
        |pos| {
            let state = world.get_block_state(pos);
            if !REGISTRY.blocks.is_in_tag(state.get_block(), tag) {
                return false;
            }
            if soul_campfire_must_be_lit && state.get_block() == &vanilla_blocks::SOUL_CAMPFIRE {
                return is_lit_campfire(state);
            }
            true
        },
    )
}

/// Vanilla parity: `CampfireBlock.isLitCampfire`, which is why an unlit soul
/// campfire repels nothing.
fn is_lit_campfire(state: BlockStateId) -> bool {
    use BlockTag;
    use steel_registry::blocks::properties::BlockStateProperties;

    REGISTRY
        .blocks
        .is_in_tag(state.get_block(), &BlockTag::CAMPFIRES)
        && state.get_value(&BlockStateProperties::LIT)
}

fn visible(ctx: &BrainContext<'_>) -> NearestVisibleLivingEntities {
    ctx.brain()
        .get_memory(memory_module_types::NEAREST_VISIBLE_LIVING_ENTITIES)
        .unwrap_or_default()
}

/// Everything a piglin notices about its own kind and its enemies.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.sensing.PiglinSpecificSensor`.
pub struct PiglinSpecificSensor;

impl Sensor for PiglinSpecificSensor {
    fn required_memories(&self) -> Vec<MemoryModuleId> {
        vec![
            memory_module_types::NEAREST_VISIBLE_LIVING_ENTITIES.id(),
            memory_module_types::NEAREST_LIVING_ENTITIES.id(),
            memory_module_types::NEAREST_VISIBLE_NEMESIS.id(),
            memory_module_types::NEAREST_TARGETABLE_PLAYER_NOT_WEARING_GOLD.id(),
            memory_module_types::NEAREST_PLAYER_HOLDING_WANTED_ITEM.id(),
            memory_module_types::NEAREST_VISIBLE_HUNTABLE_HOGLIN.id(),
            memory_module_types::NEAREST_VISIBLE_BABY_HOGLIN.id(),
            memory_module_types::NEAREST_VISIBLE_ZOMBIFIED.id(),
            memory_module_types::NEAREST_VISIBLE_ADULT_PIGLINS.id(),
            memory_module_types::NEARBY_ADULT_PIGLINS.id(),
            memory_module_types::VISIBLE_ADULT_PIGLIN_COUNT.id(),
            memory_module_types::VISIBLE_ADULT_HOGLIN_COUNT.id(),
            memory_module_types::NEAREST_REPELLENT.id(),
        ]
    }

    #[expect(
        clippy::too_many_lines,
        reason = "one pass over the visible entities, split into the same twelve \
                  memories vanilla's doTick writes in one body"
    )]
    fn do_tick(&mut self, ctx: &BrainContext<'_>) {
        let brain = ctx.brain();
        brain.set_memory_or_erase(
            memory_module_types::NEAREST_REPELLENT,
            find_nearest_repellent(
                ctx.world(),
                ctx.mob().block_position(),
                &BlockTag::PIGLIN_REPELLENTS,
                true,
            ),
        );

        let mut nemesis: Option<SharedEntity> = None;
        let mut huntable_hoglin: Option<SharedEntity> = None;
        let mut baby_hoglin: Option<SharedEntity> = None;
        let mut zombified: Option<SharedEntity> = None;
        let mut player_not_wearing_gold: Option<SharedEntity> = None;
        let mut player_holding_wanted_item: Option<SharedEntity> = None;
        let mut visible_adult_hoglin_count = 0_i32;
        let mut visible_adult_piglins: Vec<EntityMemory> = Vec::new();

        for entity in visible(ctx).find_all(|_| true) {
            let Some(living) = entity.as_living_entity() else {
                continue;
            };
            let raw = entity.as_ref();

            if utils::is_of_type(raw, &vanilla_entities::HOGLIN) {
                if living.is_baby() {
                    if baby_hoglin.is_none() {
                        baby_hoglin = Some(entity);
                    }
                } else {
                    visible_adult_hoglin_count += 1;
                    if huntable_hoglin.is_none()
                        && entity
                            .downcast_ref::<HoglinEntity>()
                            .is_some_and(HoglinEntity::can_be_hunted)
                    {
                        huntable_hoglin = Some(entity);
                    }
                }
            } else if utils::is_of_type(raw, &vanilla_entities::PIGLIN_BRUTE) {
                visible_adult_piglins.push(EntityMemory::new(&entity));
            } else if utils::is_of_type(raw, &vanilla_entities::PIGLIN) {
                if !living.is_baby() {
                    visible_adult_piglins.push(EntityMemory::new(&entity));
                }
            } else if utils::is_of_type(raw, &vanilla_entities::PLAYER) {
                if player_not_wearing_gold.is_none()
                    && !is_wearing_safe_armor(living)
                    && Mob::can_attack(ctx.mob(), living)
                {
                    player_not_wearing_gold = Some(entity.clone());
                }
                if player_holding_wanted_item.is_none()
                    && !raw.is_spectator()
                    && is_player_holding_loved_item(living)
                {
                    player_holding_wanted_item = Some(entity);
                }
            } else if nemesis.is_none() && is_nemesis(living) {
                nemesis = Some(entity);
            } else if zombified.is_none() && is_zombified(living) {
                zombified = Some(entity);
            }
        }

        brain.set_memory_or_erase(
            memory_module_types::NEAREST_VISIBLE_NEMESIS,
            nemesis.as_ref().map(EntityMemory::new),
        );
        brain.set_memory_or_erase(
            memory_module_types::NEAREST_VISIBLE_HUNTABLE_HOGLIN,
            huntable_hoglin.as_ref().map(EntityMemory::new),
        );
        brain.set_memory_or_erase(
            memory_module_types::NEAREST_VISIBLE_BABY_HOGLIN,
            baby_hoglin.as_ref().map(EntityMemory::new),
        );
        brain.set_memory_or_erase(
            memory_module_types::NEAREST_VISIBLE_ZOMBIFIED,
            zombified.as_ref().map(EntityMemory::new),
        );
        brain.set_memory_or_erase(
            memory_module_types::NEAREST_TARGETABLE_PLAYER_NOT_WEARING_GOLD,
            player_not_wearing_gold.as_ref().map(EntityMemory::new),
        );
        brain.set_memory_or_erase(
            memory_module_types::NEAREST_PLAYER_HOLDING_WANTED_ITEM,
            player_holding_wanted_item.as_ref().map(EntityMemory::new),
        );
        brain.set_memory(
            memory_module_types::NEARBY_ADULT_PIGLINS,
            find_nearby_adult_piglins(ctx),
        );
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_possible_wrap,
            reason = "a follow-range scan never holds more piglins than an i32 can count"
        )]
        let visible_adult_piglin_count = visible_adult_piglins.len() as i32;
        brain.set_memory(
            memory_module_types::NEAREST_VISIBLE_ADULT_PIGLINS,
            visible_adult_piglins,
        );
        brain.set_memory(
            memory_module_types::VISIBLE_ADULT_PIGLIN_COUNT,
            visible_adult_piglin_count,
        );
        brain.set_memory(
            memory_module_types::VISIBLE_ADULT_HOGLIN_COUNT,
            visible_adult_hoglin_count,
        );
    }
}

/// The little a piglin brute needs to notice.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.sensing.PiglinBruteSpecificSensor`.
pub struct PiglinBruteSpecificSensor;

impl Sensor for PiglinBruteSpecificSensor {
    fn required_memories(&self) -> Vec<MemoryModuleId> {
        vec![
            memory_module_types::NEAREST_VISIBLE_LIVING_ENTITIES.id(),
            memory_module_types::NEAREST_VISIBLE_NEMESIS.id(),
            memory_module_types::NEARBY_ADULT_PIGLINS.id(),
        ]
    }

    fn do_tick(&mut self, ctx: &BrainContext<'_>) {
        let nemesis = visible(ctx).find_closest(is_nemesis);
        ctx.brain().set_memory_or_erase(
            memory_module_types::NEAREST_VISIBLE_NEMESIS,
            nemesis.as_ref().map(EntityMemory::new),
        );
        ctx.brain().set_memory(
            memory_module_types::NEARBY_ADULT_PIGLINS,
            find_nearby_adult_piglins(ctx),
        );
    }
}

/// What a hoglin watches: warped fungus, and how many piglins are around.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.sensing.HoglinSpecificSensor`.
pub struct HoglinSpecificSensor;

impl Sensor for HoglinSpecificSensor {
    fn required_memories(&self) -> Vec<MemoryModuleId> {
        vec![
            memory_module_types::NEAREST_VISIBLE_LIVING_ENTITIES.id(),
            memory_module_types::NEAREST_REPELLENT.id(),
            memory_module_types::NEAREST_VISIBLE_ADULT_PIGLIN.id(),
            memory_module_types::NEAREST_VISIBLE_ADULT_HOGLINS.id(),
            memory_module_types::VISIBLE_ADULT_PIGLIN_COUNT.id(),
            memory_module_types::VISIBLE_ADULT_HOGLIN_COUNT.id(),
        ]
    }

    fn do_tick(&mut self, ctx: &BrainContext<'_>) {
        let brain = ctx.brain();
        brain.set_memory_or_erase(
            memory_module_types::NEAREST_REPELLENT,
            find_nearest_repellent(
                ctx.world(),
                ctx.mob().block_position(),
                &BlockTag::HOGLIN_REPELLENTS,
                false,
            ),
        );

        let mut adult_piglin: Option<SharedEntity> = None;
        let mut adult_piglin_count = 0_i32;
        let mut adult_hoglins: Vec<EntityMemory> = Vec::new();

        let grown_piglins_and_hoglins = visible(ctx).find_all(|candidate| {
            let raw = candidate.as_entity_event_source();
            !candidate.is_baby()
                && (utils::is_of_type(raw, &vanilla_entities::PIGLIN)
                    || utils::is_of_type(raw, &vanilla_entities::HOGLIN))
        });
        for entity in grown_piglins_and_hoglins {
            if utils::is_of_type(entity.as_ref(), &vanilla_entities::PIGLIN) {
                adult_piglin_count += 1;
                if adult_piglin.is_none() {
                    adult_piglin = Some(entity);
                }
                continue;
            }
            adult_hoglins.push(EntityMemory::new(&entity));
        }

        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_possible_wrap,
            reason = "a follow-range scan never holds more hoglins than an i32 can count"
        )]
        let adult_hoglin_count = adult_hoglins.len() as i32;
        brain.set_memory_or_erase(
            memory_module_types::NEAREST_VISIBLE_ADULT_PIGLIN,
            adult_piglin.as_ref().map(EntityMemory::new),
        );
        brain.set_memory(
            memory_module_types::NEAREST_VISIBLE_ADULT_HOGLINS,
            adult_hoglins,
        );
        brain.set_memory(
            memory_module_types::VISIBLE_ADULT_PIGLIN_COUNT,
            adult_piglin_count,
        );
        brain.set_memory(
            memory_module_types::VISIBLE_ADULT_HOGLIN_COUNT,
            adult_hoglin_count,
        );
    }
}
