//! `/execute if` and `/execute unless` conditions.

use std::slice;
use std::sync::Arc;

use foton_registry::{blocks::block_state_ext::BlockStateExt as _, vanilla_blocks};
use foton_utils::{
    BlockPos, BoundingBox, ChunkPos, SectionPos,
    locks::SyncMutex,
    nbt::{NbtPath, compare_nbt_compounds},
    translations,
};
use simdnbt::owned::NbtTag;
use text_components::TextComponent;

use super::super::super::{
    brigadier::{CommandNodeBuilder, CommandRedirectTarget, CommandSyntaxError},
    execution::{
        ChainModifiers, CommandResultCallback, CommandSource, CustomModifierExecutor,
        ExecutionCommandSource, ExecutionControl, FotonArgumentType, FotonCommandContext,
        FotonCommandRuntime, FotonContextChain, argument, literal,
    },
};
use super::super::function::{instantiate, resolve_functions};
use super::{objective, source_command_storage, source_scoreboard};
use crate::inventory::lock::{ContainerLockGuard, ContainerRef};
use crate::inventory::slot_ranges::container_slot_item;
use crate::{block_entity::SharedBlockEntity, world::World};

type Builder = CommandNodeBuilder<CommandSource, FotonCommandRuntime>;

const EXECUTE_ROOT: CommandRedirectTarget = CommandRedirectTarget::CommandRoot;
const MAX_BLOCKS_REGION: i64 = 32_768;

pub(super) fn conditionals(name: &'static str, expected: bool) -> Builder {
    // TODO: Add predicate after its runtime registry is ported.
    // TODO: Restore Foton stopwatch conditions with the stopwatch command system.
    literal(name)
        .then(biome_condition(expected))
        .then(block_condition(expected))
        .then(blocks_condition(expected))
        .then(data_condition(expected))
        .then(dimension_condition(expected))
        .then(entity_condition(expected))
        .then(function_condition(expected))
        .then(items_condition(expected))
        .then(loaded_condition(expected))
        .then(score_condition(expected))
}

/// `execute if|unless function <name>`.
///
/// Vanilla parity: `ExecuteCommand.ExecuteIfFunctionCustomModifier`. The
/// condition cannot be a plain fork modifier, because whether a source passes
/// depends on the result of a function that only runs later in the queue.
fn function_condition(expected: bool) -> Builder {
    literal("function").then(
        argument("name", FotonArgumentType::function()).redirects_custom(
            EXECUTE_ROOT,
            ExecuteIfFunction { expected },
            true,
        ),
    )
}

struct ExecuteIfFunction {
    expected: bool,
}

impl CustomModifierExecutor<CommandSource> for ExecuteIfFunction {
    fn apply(
        &self,
        original_source: Arc<CommandSource>,
        sources: Vec<Arc<CommandSource>>,
        chain: &FotonContextChain<CommandSource>,
        modifiers: ChainModifiers,
        control: &mut ExecutionControl<'_, CommandSource>,
    ) {
        let context = chain.top_context().copy_for(Arc::clone(&original_source));
        let functions = match context
            .function_or_tag("name")
            .and_then(|reference| resolve_functions(&original_source, reference))
        {
            Ok(functions) => functions,
            Err(error) => {
                original_source.handle_error(&error, modifiers.is_forked());
                return;
            }
        };
        // Vanilla parity: with no functions to run, nothing at all is queued --
        // not even the continuation -- so the rest of the chain never runs.
        if functions.is_empty() {
            return;
        }

        // Vanilla parity: `ExecuteCommand.scheduleFunctionConditionsAndTest`
        // instantiates with no arguments, so a macro function used as a
        // condition fails here rather than running with empty substitutions.
        let mut entries = Vec::with_capacity(functions.len());
        for function in &functions {
            match instantiate(&original_source, function, None) {
                Ok(instantiated) => entries.push(instantiated),
                Err(reason) => {
                    let error = CommandSyntaxError::dynamic(
                        translations::COMMANDS_EXECUTE_FUNCTION_INSTANTIATION_FAILURE
                            .message([TextComponent::from(function.id().to_string()), *reason])
                            .component(),
                    );
                    original_source.handle_error(&error, modifiers.is_forked());
                    return;
                }
            }
        }
        let passing = Arc::new(SyncMutex::new(Vec::new()));
        let expected = self.expected;
        for source in sources {
            let function_source = Arc::new(
                source
                    .with_suppressed_output()
                    .with_callback(CommandResultCallback::empty()),
            );
            let passing = Arc::clone(&passing);
            let candidate = Arc::clone(&source);
            let result_callback = CommandResultCallback::new(move |_success, result| {
                if (result != 0) == expected {
                    passing.lock().push(Arc::clone(&candidate));
                }
            });
            let entries = entries.clone();
            control.queue_isolated(result_callback, move |isolated| {
                let consumer = isolated.current_frame().return_value_consumer();
                for entries in entries {
                    isolated.queue_function_call(
                        entries,
                        Arc::clone(&function_source),
                        consumer.clone(),
                        true,
                    );
                }
                isolated.queue_fallthrough();
            });
        }

        let Some(next_stage) = chain.next_stage() else {
            unreachable!("an execute condition redirects to a following command stage")
        };
        control.queue_deferred_contexts(next_stage, original_source, passing, modifiers);
    }
}

fn data_condition(expected: bool) -> Builder {
    literal("data")
        .then(
            literal("block").then(
                argument("sourcePos", FotonArgumentType::block_pos())
                    .then(data_path(DataSource::Block, expected)),
            ),
        )
        .then(
            literal("entity").then(
                argument("source", FotonArgumentType::entity())
                    .then(data_path(DataSource::Entity, expected)),
            ),
        )
        .then(
            literal("storage").then(
                argument("source", FotonArgumentType::storage_key())
                    .then(data_path(DataSource::Storage, expected)),
            ),
        )
}

fn data_path(source: DataSource, expected: bool) -> Builder {
    argument("path", FotonArgumentType::nbt_path())
        .forks(EXECUTE_ROOT, move |context| {
            let matches = data_match_count(context, source)? > 0;
            Ok(conditional_sources(context.source(), expected, matches))
        })
        .executes(move |context| {
            let count = data_match_count(context, source)?;
            execute_numeric_condition(context, expected, count)
        })
}

#[derive(Clone, Copy)]
enum DataSource {
    Block,
    Entity,
    Storage,
}

fn data_match_count(
    context: &FotonCommandContext<CommandSource>,
    source: DataSource,
) -> Result<i32, CommandSyntaxError> {
    let tag = match source {
        DataSource::Block => {
            let position = loaded_block_position(context, "sourcePos")?;
            let block_entity = context
                .source()
                .world()
                .get_block_entity(position)
                .ok_or_else(invalid_block_data_source)?;
            let data = block_entity.save_with_full_metadata();
            NbtTag::Compound(data)
        }
        DataSource::Entity => {
            let entity = context.entity("source")?;
            NbtTag::Compound(entity.nbt_for_data_compare())
        }
        DataSource::Storage => {
            let key = context.identifier("source")?;
            NbtTag::Compound(source_command_storage(context)?.get(key))
        }
    };
    let path = context.nbt_path("path")?;
    matching_data_count(path, &tag)
}

fn matching_data_count(path: &NbtPath, tag: &NbtTag) -> Result<i32, CommandSyntaxError> {
    i32::try_from(path.count_matching(tag))
        .map_err(|_| CommandSyntaxError::dynamic("NBT match count exceeds the command range"))
}

pub(super) fn invalid_block_data_source() -> CommandSyntaxError {
    CommandSyntaxError::dynamic(TextComponent::from(
        &translations::COMMANDS_DATA_BLOCK_INVALID,
    ))
}

fn dimension_condition(expected: bool) -> Builder {
    literal("dimension").then(
        argument("dimension", FotonArgumentType::world())
            .forks(EXECUTE_ROOT, move |context| {
                let matches = dimension_matches(context)?;
                Ok(conditional_sources(context.source(), expected, matches))
            })
            .executes(move |context| {
                execute_boolean_condition(context, expected, dimension_matches(context)?)
            }),
    )
}

fn dimension_matches(
    context: &FotonCommandContext<CommandSource>,
) -> Result<bool, CommandSyntaxError> {
    let world = context
        .world_argument("dimension")?
        .resolve(context.source())?;
    Ok(Arc::ptr_eq(context.source().world(), &world))
}

fn blocks_condition(expected: bool) -> Builder {
    literal("blocks").then(
        argument("start", FotonArgumentType::block_pos()).then(
            argument("end", FotonArgumentType::block_pos()).then(
                argument("destination", FotonArgumentType::block_pos())
                    .then(blocks_mode("all", expected, false))
                    .then(blocks_mode("masked", expected, true)),
            ),
        ),
    )
}

fn blocks_mode(name: &'static str, expected: bool, skip_air: bool) -> Builder {
    literal(name)
        .forks(EXECUTE_ROOT, move |context| {
            let matches = matching_block_region_count(context, skip_air)?.is_some();
            Ok(conditional_sources(context.source(), expected, matches))
        })
        .executes(move |context| {
            let count = matching_block_region_count(context, skip_air)?;
            execute_blocks_condition(context, expected, count)
        })
}

fn matching_block_region_count(
    context: &FotonCommandContext<CommandSource>,
    skip_air: bool,
) -> Result<Option<i32>, CommandSyntaxError> {
    let source_start = loaded_block_position(context, "start")?;
    let source_end = loaded_block_position(context, "end")?;
    let destination_start = loaded_block_position(context, "destination")?;
    let source_region = BoundingBox::from_corners(source_start, source_end);
    let destination_end = destination_start.offset(
        source_region.max_x() - source_region.min_x(),
        source_region.max_y() - source_region.min_y(),
        source_region.max_z() - source_region.min_z(),
    );
    let destination_region = BoundingBox::from_corners(destination_start, destination_end);
    let area = block_region_volume(&source_region);
    if area > MAX_BLOCKS_REGION {
        return Err(blocks_too_big(area));
    }

    let world = context.source().world();
    ensure_region_chunks_loaded(world, &source_region)?;
    ensure_region_chunks_loaded(world, &destination_region)?;

    let offset_x = destination_region.min_x() - source_region.min_x();
    let offset_y = destination_region.min_y() - source_region.min_y();
    let offset_z = destination_region.min_z() - source_region.min_z();
    let mut count = 0;
    for z in source_region.min_z()..=source_region.max_z() {
        for y in source_region.min_y()..=source_region.max_y() {
            for x in source_region.min_x()..=source_region.max_x() {
                let source_pos = BlockPos::new(x, y, z);
                let source_state = world.get_block_state(source_pos);
                if !should_compare_block(source_state, skip_air) {
                    continue;
                }
                let destination_pos = source_pos.offset(offset_x, offset_y, offset_z);
                if source_state != world.get_block_state(destination_pos)
                    || !block_entities_match(world, source_pos, destination_pos)
                {
                    return Ok(None);
                }
                count += 1;
            }
        }
    }
    Ok(Some(count))
}

fn block_region_volume(region: &BoundingBox) -> i64 {
    let x_span = i64::from(region.max_x()) - i64::from(region.min_x()) + 1;
    let y_span = i64::from(region.max_y()) - i64::from(region.min_y()) + 1;
    let z_span = i64::from(region.max_z()) - i64::from(region.min_z()) + 1;
    x_span.saturating_mul(y_span).saturating_mul(z_span)
}

// Foton's synchronous command runner rejects unloaded region chunks instead of loading them.
fn ensure_region_chunks_loaded(
    world: &World,
    region: &BoundingBox,
) -> Result<(), CommandSyntaxError> {
    if region.max_y() < world.get_min_y() || region.min_y() > world.get_max_y() {
        return Ok(());
    }
    let min_chunk_x = SectionPos::block_to_section_coord(region.min_x());
    let max_chunk_x = SectionPos::block_to_section_coord(region.max_x());
    let min_chunk_z = SectionPos::block_to_section_coord(region.min_z());
    let max_chunk_z = SectionPos::block_to_section_coord(region.max_z());
    for chunk_z in min_chunk_z..=max_chunk_z {
        for chunk_x in min_chunk_x..=max_chunk_x {
            if !ChunkPos::is_valid(chunk_x, chunk_z) {
                continue;
            }
            let pos = BlockPos::new(chunk_x * 16, world.get_min_y(), chunk_z * 16);
            if !world.is_full_chunk_loaded_at(pos) {
                return Err(unloaded_position());
            }
        }
    }
    Ok(())
}

fn should_compare_block(state: foton_utils::BlockStateId, skip_air: bool) -> bool {
    !skip_air || state.get_block() != &vanilla_blocks::AIR
}

fn block_entities_match(world: &World, source: BlockPos, destination: BlockPos) -> bool {
    let source_entity = world.get_block_entity(source);
    let destination_entity = world.get_block_entity(destination);
    block_entity_data_matches(source_entity.as_ref(), destination_entity.as_ref())
}

fn block_entity_data_matches(
    source: Option<&SharedBlockEntity>,
    destination: Option<&SharedBlockEntity>,
) -> bool {
    let Some(source) = source else {
        return true;
    };
    let Some(destination) = destination else {
        return false;
    };
    if Arc::ptr_eq(source, destination) {
        return true;
    }
    if source.get_type() != destination.get_type() {
        return false;
    }
    let source_data = source.save_custom_only();
    let destination_data = destination.save_custom_only();
    source_data.len() == destination_data.len()
        && compare_nbt_compounds(&source_data, &destination_data, false)
}

fn execute_blocks_condition(
    context: &FotonCommandContext<CommandSource>,
    expected: bool,
    count: Option<i32>,
) -> Result<i32, CommandSyntaxError> {
    match (expected, count) {
        (true, Some(count)) => {
            let message = translations::COMMANDS_EXECUTE_CONDITIONAL_PASS_COUNT
                .message([TextComponent::from(count.to_string())])
                .component();
            context.source().send_success(&message, false);
            Ok(count)
        }
        (true, None) => Err(conditional_failed()),
        (false, Some(count)) => Err(conditional_failed_count(count)),
        (false, None) => {
            context.source().send_success(
                &TextComponent::from(&translations::COMMANDS_EXECUTE_CONDITIONAL_PASS),
                false,
            );
            Ok(1)
        }
    }
}

fn blocks_too_big(area: i64) -> CommandSyntaxError {
    let message = translations::COMMANDS_EXECUTE_BLOCKS_TOOBIG
        .message([
            TextComponent::from(MAX_BLOCKS_REGION.to_string()),
            TextComponent::from(area.to_string()),
        ])
        .component();
    CommandSyntaxError::dynamic(message)
}

fn block_condition(expected: bool) -> Builder {
    literal("block").then(
        argument("pos", FotonArgumentType::block_pos()).then(
            argument("block", FotonArgumentType::block_predicate())
                .forks(EXECUTE_ROOT, move |context| {
                    let matches = block_matches(context)?;
                    Ok(conditional_sources(context.source(), expected, matches))
                })
                .executes(move |context| {
                    execute_boolean_condition(context, expected, block_matches(context)?)
                }),
        ),
    )
}

fn block_matches(context: &FotonCommandContext<CommandSource>) -> Result<bool, CommandSyntaxError> {
    let position = loaded_block_position(context, "pos")?;
    let predicate = context.block_predicate("block")?;
    let world = context.source().world();
    if !predicate.matches_state(world.get_block_state(position)) {
        return Ok(false);
    }
    let Some(expected_nbt) = predicate.nbt() else {
        return Ok(true);
    };
    let Some(block_entity) = world.get_block_entity(position) else {
        return Ok(false);
    };
    let actual_nbt = block_entity.save_with_full_metadata();
    Ok(compare_nbt_compounds(expected_nbt, &actual_nbt, true))
}

fn biome_condition(expected: bool) -> Builder {
    literal("biome").then(
        argument("pos", FotonArgumentType::block_pos()).then(
            argument("biome", FotonArgumentType::biome_or_tag())
                .forks(EXECUTE_ROOT, move |context| {
                    let matches = biome_matches(context)?;
                    Ok(conditional_sources(context.source(), expected, matches))
                })
                .executes(move |context| {
                    execute_boolean_condition(context, expected, biome_matches(context)?)
                }),
        ),
    )
}

fn biome_matches(context: &FotonCommandContext<CommandSource>) -> Result<bool, CommandSyntaxError> {
    let position = loaded_block_position(context, "pos")?;
    let world = context.source().world();
    let biome = world.biome_at(position).ok_or_else(|| {
        CommandSyntaxError::dynamic(TextComponent::from(&translations::ARGUMENT_POS_UNLOADED))
    })?;
    let expected = context.biome_or_tag("biome")?;
    Ok(expected.matches(biome))
}

pub(super) fn loaded_block_position(
    context: &FotonCommandContext<CommandSource>,
    name: &str,
) -> Result<foton_utils::BlockPos, CommandSyntaxError> {
    let position = context.coordinates(name)?.block_pos(context.source());
    let world = context.source().world();
    if !world.is_full_chunk_loaded_at(position) {
        return Err(unloaded_position());
    }
    if !world.is_in_valid_bounds(position) {
        return Err(CommandSyntaxError::dynamic(TextComponent::from(
            &translations::ARGUMENT_POS_OUTOFWORLD,
        )));
    }
    Ok(position)
}

fn unloaded_position() -> CommandSyntaxError {
    CommandSyntaxError::dynamic(TextComponent::from(&translations::ARGUMENT_POS_UNLOADED))
}

fn entity_condition(expected: bool) -> Builder {
    literal("entity").then(
        argument("entities", FotonArgumentType::entities())
            .forks(EXECUTE_ROOT, move |context| {
                let matches = !context.optional_entities("entities")?.is_empty();
                Ok(conditional_sources(context.source(), expected, matches))
            })
            .executes(move |context| {
                let count =
                    i32::try_from(context.optional_entities("entities")?.len()).map_err(|_| {
                        CommandSyntaxError::dynamic("Entity count exceeds the command result range")
                    })?;
                execute_numeric_condition(context, expected, count)
            }),
    )
}

fn items_condition(expected: bool) -> Builder {
    literal("items")
        .then(
            literal("entity").then(
                argument("entities", FotonArgumentType::entities()).then(
                    argument("slots", FotonArgumentType::item_slots()).then(
                        argument("item_predicate", FotonArgumentType::item_predicate())
                            .forks(EXECUTE_ROOT, move |context| {
                                let matches = entity_item_count(context)? > 0;
                                Ok(conditional_sources(context.source(), expected, matches))
                            })
                            .executes(move |context| {
                                let count = entity_item_count(context)?;
                                execute_numeric_condition(context, expected, count)
                            }),
                    ),
                ),
            ),
        )
        .then(
            literal("block").then(
                argument("pos", FotonArgumentType::block_pos()).then(
                    argument("slots", FotonArgumentType::item_slots()).then(
                        argument("item_predicate", FotonArgumentType::item_predicate())
                            .forks(EXECUTE_ROOT, move |context| {
                                let matches = block_item_count(context)? > 0;
                                Ok(conditional_sources(context.source(), expected, matches))
                            })
                            .executes(move |context| {
                                let count = block_item_count(context)?;
                                execute_numeric_condition(context, expected, count)
                            }),
                    ),
                ),
            ),
        )
}

/// Counts matching items across every named slot of every matched entity.
///
/// Vanilla parity: the `SlotProvider` overload of `ExecuteCommand.countItems`.
/// The answer is the number of *items*, not of slots: a stack of sixty-four
/// counts sixty-four. A slot the entity does not have is skipped; one it has
/// and has left empty is tested and contributes its count, which is zero.
fn entity_item_count(
    context: &FotonCommandContext<CommandSource>,
) -> Result<i32, CommandSyntaxError> {
    // Vanilla uses `EntityArgument.getEntities`, which refuses to match
    // nothing: a selector that finds no entity fails the command rather than
    // quietly counting zero.
    let entities = context.entities("entities")?;
    let slots = context.item_slots("slots")?;
    let predicate = context.item_predicate("item_predicate")?;

    let mut count = 0i64;
    for entity in &entities {
        for &slot in slots.slots() {
            let Some(stack) = entity.slot_item(slot) else {
                continue;
            };
            if predicate.matches(&stack) {
                count += i64::from(stack.count());
            }
        }
    }
    item_count_result(count)
}

/// Counts matching items across every named slot of one container block.
///
/// Vanilla parity: the `BlockPos` overload of `ExecuteCommand.countItems`,
/// which bounds every slot id against the container's own size rather than
/// against the range that named it -- `container.*` over a hopper reads five
/// slots and stops.
fn block_item_count(
    context: &FotonCommandContext<CommandSource>,
) -> Result<i32, CommandSyntaxError> {
    let position = loaded_block_position(context, "pos")?;
    let slots = context.item_slots("slots")?;
    let predicate = context.item_predicate("item_predicate")?;

    let block_entity = context
        .source()
        .world()
        .get_block_entity(position)
        .ok_or_else(|| not_a_container(position))?;
    let container_ref =
        ContainerRef::from_block_entity(block_entity).ok_or_else(|| not_a_container(position))?;
    // Locking is also what rolls a still-packed loot table, so an untouched
    // dungeon chest answers with what a player opening it would find.
    let guard = ContainerLockGuard::lock_all(slice::from_ref(&container_ref));
    let Some(container) = guard.get(container_ref.container_id()) else {
        return Err(not_a_container(position));
    };

    let mut count = 0i64;
    for &slot in slots.slots() {
        let Some(stack) = container_slot_item(container, slot) else {
            continue;
        };
        if predicate.matches(&stack) {
            count += i64::from(stack.count());
        }
    }
    item_count_result(count)
}

fn item_count_result(count: i64) -> Result<i32, CommandSyntaxError> {
    i32::try_from(count)
        .map_err(|_| CommandSyntaxError::dynamic("Item count exceeds the command result range"))
}

fn not_a_container(position: BlockPos) -> CommandSyntaxError {
    CommandSyntaxError::dynamic(
        translations::COMMANDS_ITEM_SOURCE_NOT_A_CONTAINER
            .message([
                position.x().to_string(),
                position.y().to_string(),
                position.z().to_string(),
            ])
            .component(),
    )
}

fn loaded_condition(expected: bool) -> Builder {
    literal("loaded").then(
        argument("pos", FotonArgumentType::block_pos())
            .forks(EXECUTE_ROOT, move |context| {
                let matches = loaded_matches(context)?;
                Ok(conditional_sources(context.source(), expected, matches))
            })
            .executes(move |context| {
                execute_boolean_condition(context, expected, loaded_matches(context)?)
            }),
    )
}

fn loaded_matches(
    context: &FotonCommandContext<CommandSource>,
) -> Result<bool, CommandSyntaxError> {
    let position = context.coordinates("pos")?.block_pos(context.source());
    Ok(context
        .source()
        .world()
        .is_entity_ticking_chunk_loaded(position))
}

fn score_condition(expected: bool) -> Builder {
    literal("score").then(
        argument("target", FotonArgumentType::score_holder()).then(
            argument("targetObjective", FotonArgumentType::objective())
                .then(score_comparison("=", ScoreComparison::Equal, expected))
                .then(score_comparison("<", ScoreComparison::Less, expected))
                .then(score_comparison(
                    "<=",
                    ScoreComparison::LessOrEqual,
                    expected,
                ))
                .then(score_comparison(">", ScoreComparison::Greater, expected))
                .then(score_comparison(
                    ">=",
                    ScoreComparison::GreaterOrEqual,
                    expected,
                ))
                .then(
                    literal("matches").then(
                        argument("range", FotonArgumentType::int_range())
                            .forks(EXECUTE_ROOT, move |context| {
                                let matches = score_range_matches(context)?;
                                Ok(conditional_sources(context.source(), expected, matches))
                            })
                            .executes(move |context| {
                                execute_boolean_condition(
                                    context,
                                    expected,
                                    score_range_matches(context)?,
                                )
                            }),
                    ),
                ),
        ),
    )
}

fn score_comparison(name: &'static str, comparison: ScoreComparison, expected: bool) -> Builder {
    literal(name).then(
        argument("source", FotonArgumentType::score_holder()).then(
            argument("sourceObjective", FotonArgumentType::objective())
                .forks(EXECUTE_ROOT, move |context| {
                    let matches = scores_match(context, comparison)?;
                    Ok(conditional_sources(context.source(), expected, matches))
                })
                .executes(move |context| {
                    execute_boolean_condition(context, expected, scores_match(context, comparison)?)
                }),
        ),
    )
}

fn scores_match(
    context: &FotonCommandContext<CommandSource>,
    comparison: ScoreComparison,
) -> Result<bool, CommandSyntaxError> {
    let scoreboard = source_scoreboard(context)?;
    let target = context.score_holder("target")?;
    let target_objective = objective(context, scoreboard, "targetObjective")?;
    let source = context.score_holder("source")?;
    let source_objective = objective(context, scoreboard, "sourceObjective")?;
    let Some(target_score) = scoreboard.score(&target, &target_objective) else {
        return Ok(false);
    };
    let Some(source_score) = scoreboard.score(&source, &source_objective) else {
        return Ok(false);
    };
    Ok(comparison.matches(target_score, source_score))
}

fn score_range_matches(
    context: &FotonCommandContext<CommandSource>,
) -> Result<bool, CommandSyntaxError> {
    let scoreboard = source_scoreboard(context)?;
    let target = context.score_holder("target")?;
    let target_objective = objective(context, scoreboard, "targetObjective")?;
    let range = context.int_range("range")?;
    Ok(scoreboard
        .score(&target, &target_objective)
        .is_some_and(|score| range.matches(score)))
}

#[derive(Clone, Copy)]
enum ScoreComparison {
    Equal,
    Less,
    LessOrEqual,
    Greater,
    GreaterOrEqual,
}

impl ScoreComparison {
    const fn matches(self, target: i32, source: i32) -> bool {
        match self {
            Self::Equal => target == source,
            Self::Less => target < source,
            Self::LessOrEqual => target <= source,
            Self::Greater => target > source,
            Self::GreaterOrEqual => target >= source,
        }
    }
}

fn conditional_sources(
    source: &CommandSource,
    expected: bool,
    matches: bool,
) -> Vec<CommandSource> {
    if matches == expected {
        vec![source.clone()]
    } else {
        Vec::new()
    }
}

fn execute_boolean_condition(
    context: &FotonCommandContext<CommandSource>,
    expected: bool,
    matches: bool,
) -> Result<i32, CommandSyntaxError> {
    if matches != expected {
        return Err(conditional_failed());
    }
    context.source().send_success(
        &TextComponent::from(&translations::COMMANDS_EXECUTE_CONDITIONAL_PASS),
        false,
    );
    Ok(1)
}

fn execute_numeric_condition(
    context: &FotonCommandContext<CommandSource>,
    expected: bool,
    count: i32,
) -> Result<i32, CommandSyntaxError> {
    if expected {
        if count == 0 {
            return Err(conditional_failed());
        }
        let message = translations::COMMANDS_EXECUTE_CONDITIONAL_PASS_COUNT
            .message([TextComponent::from(count.to_string())])
            .component();
        context.source().send_success(&message, false);
        return Ok(count);
    }

    if count != 0 {
        return Err(conditional_failed_count(count));
    }
    context.source().send_success(
        &TextComponent::from(&translations::COMMANDS_EXECUTE_CONDITIONAL_PASS),
        false,
    );
    Ok(1)
}

fn conditional_failed() -> CommandSyntaxError {
    CommandSyntaxError::dynamic(TextComponent::from(
        &translations::COMMANDS_EXECUTE_CONDITIONAL_FAIL,
    ))
}

fn conditional_failed_count(count: i32) -> CommandSyntaxError {
    let message = translations::COMMANDS_EXECUTE_CONDITIONAL_FAIL_COUNT
        .message([TextComponent::from(count.to_string())])
        .component();
    CommandSyntaxError::dynamic(message)
}

#[cfg(test)]
mod tests {
    use std::sync::Weak;

    use foton_registry::{init_vanilla_registry, vanilla_block_entity_types, vanilla_blocks};
    use foton_utils::nbt::parse_nbt_path;
    use simdnbt::owned::{NbtCompound, NbtList, NbtTag};

    use super::*;
    use crate::block_entity::entities::RawBlockEntity;

    fn raw_block_entity(value: i32, pos: BlockPos, reverse_order: bool) -> SharedBlockEntity {
        let mut data = NbtCompound::new();
        if reverse_order {
            data.insert("other", 11_i32);
            data.insert("value", value);
        } else {
            data.insert("value", value);
            data.insert("other", 11_i32);
        }
        data.insert("x", pos.x());
        Arc::new(RawBlockEntity::with_data(
            &vanilla_block_entity_types::BARREL,
            Weak::new(),
            pos,
            vanilla_blocks::BARREL.default_state(),
            data,
        ))
    }

    #[test]
    fn block_region_volume_uses_inclusive_normalized_corners() {
        let region = BoundingBox::from_corners(BlockPos::new(2, 5, -1), BlockPos::new(-1, 3, 2));

        assert_eq!(block_region_volume(&region), 48);
    }

    #[test]
    fn data_match_count_returns_selected_tag_count() {
        let path = parse_nbt_path("items[].value").expect("path should parse");
        let mut first = NbtCompound::new();
        first.insert("value", 1);
        let mut second = NbtCompound::new();
        second.insert("value", 2);
        let mut root = NbtCompound::new();
        root.insert("items", NbtList::Compound(vec![first, second]));
        let tag = NbtTag::Compound(root);

        assert_eq!(
            matching_data_count(&path, &tag).expect("count should fit"),
            2
        );
    }

    #[test]
    fn masked_regions_skip_only_vanilla_air() {
        init_vanilla_registry();

        assert!(!should_compare_block(
            vanilla_blocks::AIR.default_state(),
            true
        ));
        assert!(should_compare_block(
            vanilla_blocks::CAVE_AIR.default_state(),
            true
        ));
        assert!(should_compare_block(
            vanilla_blocks::VOID_AIR.default_state(),
            true
        ));
        assert!(should_compare_block(
            vanilla_blocks::AIR.default_state(),
            false
        ));
    }

    #[test]
    fn region_block_entities_compare_type_and_custom_data_only() {
        init_vanilla_registry();
        let source = raw_block_entity(7, BlockPos::new(1, 64, 1), false);
        let matching = raw_block_entity(7, BlockPos::new(4, 70, 4), true);
        let different = raw_block_entity(8, BlockPos::new(4, 70, 4), false);

        assert!(block_entity_data_matches(Some(&source), Some(&source)));
        assert!(block_entity_data_matches(Some(&source), Some(&matching)));
        assert!(!block_entity_data_matches(Some(&source), Some(&different)));
        assert!(!block_entity_data_matches(Some(&source), None));
        assert!(block_entity_data_matches(None, Some(&matching)));
    }
}
