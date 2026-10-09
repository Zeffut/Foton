//! Vanilla fill command.

use std::io::Cursor;
use std::sync::Arc;

use foton_registry::{
    REGISTRY, blocks::block_state_ext::BlockStateExt as _, vanilla_blocks,
    vanilla_game_rules::MAX_BLOCK_MODIFICATIONS,
};
use foton_utils::{
    BlockPos, BlockStateId, Identifier,
    nbt::{compare_nbt_compounds, nbt_compounds_equal},
    translations,
    types::UpdateFlags,
};
use simdnbt::{borrow::read_compound, owned::NbtCompound};
use text_components::TextComponent;

use super::super::{
    brigadier::{CommandNodeBuilder, CommandSyntaxError},
    execution::{
        BlockPredicate, CommandSource, FotonArgumentType, FotonCommandContext, FotonCommandRuntime,
        argument, literal,
    },
    registration::CommandRegistration,
};
use crate::{
    behavior::update_from_neighbour_shapes, world::World, world::tick_scheduler::TickPriority,
};

/// Vanilla `FillCommand.Mode`: what happens to the positions of the region.
#[derive(Clone, Copy, PartialEq, Eq)]
enum FillMode {
    Replace,
    Outline,
    Hollow,
    Destroy,
}

/// Which positions of the region are touched.
#[derive(Clone, Copy)]
enum FillFilter<'a> {
    Everything,
    /// `fill ... keep`: vanilla's `level.isEmptyBlock(pos)`.
    Air,
    Predicate(&'a BlockPredicate),
}

/// Vanilla `BlockInput`: a state, the properties the command spelled out, and
/// optional block-entity data.
struct BlockInput {
    state: BlockStateId,
    properties: Vec<(Box<str>, Box<str>)>,
    nbt: Option<NbtCompound>,
}

pub(super) fn registration() -> CommandRegistration<CommandSource> {
    CommandRegistration::new(Identifier::vanilla_static("fill"), |_| command())
}

fn command() -> CommandNodeBuilder<CommandSource, FotonCommandRuntime> {
    literal("fill").then(
        argument("from", FotonArgumentType::block_pos()).then(
            argument("to", FotonArgumentType::block_pos()).then(
                wrap_with_mode(
                    argument("block", FotonArgumentType::block_state()),
                    FillFilterSource::Everything,
                )
                .then(
                    literal("replace")
                        .executes(|c| {
                            fill(c, FillMode::Replace, FillFilterSource::Everything, false)
                        })
                        .then(wrap_with_mode(
                            argument("filter", FotonArgumentType::block_predicate()),
                            FillFilterSource::Argument,
                        )),
                )
                .then(
                    literal("keep")
                        .executes(|c| fill(c, FillMode::Replace, FillFilterSource::Air, false)),
                ),
            ),
        ),
    )
}

/// Where the filter of a fill comes from, resolved once the context exists.
#[derive(Clone, Copy)]
enum FillFilterSource {
    Everything,
    Air,
    Argument,
}

/// Vanilla `FillCommand.wrapWithMode`: the plain executor plus the four mode
/// literals and `strict`.
fn wrap_with_mode(
    builder: CommandNodeBuilder<CommandSource, FotonCommandRuntime>,
    filter: FillFilterSource,
) -> CommandNodeBuilder<CommandSource, FotonCommandRuntime> {
    builder
        .executes(move |c| fill(c, FillMode::Replace, filter, false))
        .then(literal("outline").executes(move |c| fill(c, FillMode::Outline, filter, false)))
        .then(literal("hollow").executes(move |c| fill(c, FillMode::Hollow, filter, false)))
        .then(literal("destroy").executes(move |c| fill(c, FillMode::Destroy, filter, false)))
        .then(literal("strict").executes(move |c| fill(c, FillMode::Replace, filter, true)))
}

/// Vanilla parity: `FillCommand.fillBlocks`.
fn fill(
    context: &FotonCommandContext<CommandSource>,
    mode: FillMode,
    filter: FillFilterSource,
    strict: bool,
) -> Result<i32, CommandSyntaxError> {
    let from = loaded_block_pos(context, "from")?;
    let to = loaded_block_pos(context, "to")?;
    let target = block_input(context)?;
    let filter = match filter {
        FillFilterSource::Everything => FillFilter::Everything,
        FillFilterSource::Air => FillFilter::Air,
        FillFilterSource::Argument => FillFilter::Predicate(context.block_predicate("filter")?),
    };
    let world = context.source().world();

    let (min, max) = (
        BlockPos::new(
            from.x().min(to.x()),
            from.y().min(to.y()),
            from.z().min(to.z()),
        ),
        BlockPos::new(
            from.x().max(to.x()),
            from.y().max(to.y()),
            from.z().max(to.z()),
        ),
    );
    let area = i64::from(max.x() - min.x() + 1)
        * i64::from(max.y() - min.y() + 1)
        * i64::from(max.z() - min.z() + 1);
    let limit = world.get_game_rule(&MAX_BLOCK_MODIFICATIONS);
    if area > i64::from(limit) {
        return Err(CommandSyntaxError::dynamic(
            translations::COMMANDS_FILL_TOOBIG
                .message([limit.to_string(), area.to_string()])
                .component(),
        ));
    }

    let hollow_core = BlockInput {
        state: vanilla_blocks::AIR.default_state(),
        properties: Vec::new(),
        nbt: None,
    };
    // Vanilla: `2 | (strict ? 816 : 256)`.
    let flags = if strict {
        UpdateFlags::UPDATE_CLIENTS
            | UpdateFlags::UPDATE_SKIP_BLOCK_ENTITY_SIDEEFFECTS
            | UpdateFlags::UPDATE_SKIP_ON_PLACE
            | UpdateFlags::UPDATE_SUPPRESS_DROPS
            | UpdateFlags::UPDATE_KNOWN_SHAPE
    } else {
        UpdateFlags::UPDATE_CLIENTS | UpdateFlags::UPDATE_SKIP_BLOCK_ENTITY_SIDEEFFECTS
    };

    let mut count = 0_i32;
    let mut updates = Vec::new();
    for pos in BlockPos::between_closed(min, max) {
        if !filter_matches(world, filter, pos) {
            continue;
        }
        let old_state = world.get_block_state(pos);
        let affected = mode == FillMode::Destroy && world.destroy_block(pos, true);

        let on_shell = pos.x() == min.x()
            || pos.x() == max.x()
            || pos.y() == min.y()
            || pos.y() == max.y()
            || pos.z() == min.z()
            || pos.z() == max.z();
        let block = match mode {
            FillMode::Replace | FillMode::Destroy => Some(&target),
            FillMode::Outline => on_shell.then_some(&target),
            FillMode::Hollow => Some(if on_shell { &target } else { &hollow_core }),
        };

        let placed = block.is_some_and(|block| block.place(world, pos, flags));
        if placed && !strict {
            updates.push((pos, old_state));
        }
        if placed || affected {
            count += 1;
        }
    }

    for (pos, old_state) in updates {
        world.update_neighbour_on_block_set(pos, old_state);
        let state = world.get_block_state(pos);
        if state.has_fluid() {
            world.schedule_fluid_tick(
                pos,
                state.get_fluid_state().fluid_id,
                0,
                TickPriority::ExtremelyHigh,
            );
        }
    }

    if count == 0 {
        return Err(CommandSyntaxError::dynamic(TextComponent::from(
            &translations::COMMANDS_FILL_FAILED,
        )));
    }
    context.source().send_success(
        &translations::COMMANDS_FILL_SUCCESS
            .message([count.to_string()])
            .component(),
        true,
    );
    Ok(count)
}

/// Vanilla parity: `BlockPosArgument.getLoadedBlockPos`.
pub(super) fn loaded_block_pos(
    context: &FotonCommandContext<CommandSource>,
    name: &str,
) -> Result<BlockPos, CommandSyntaxError> {
    let pos = context.coordinates(name)?.block_pos(context.source());
    let world = context.source().world();
    if !world.is_full_chunk_loaded_at(pos) {
        return Err(CommandSyntaxError::dynamic(TextComponent::from(
            &translations::ARGUMENT_POS_UNLOADED,
        )));
    }
    if !world.is_in_valid_bounds(pos) {
        return Err(CommandSyntaxError::dynamic(TextComponent::from(
            &translations::ARGUMENT_POS_OUTOFWORLD,
        )));
    }
    Ok(pos)
}

fn block_input(
    context: &FotonCommandContext<CommandSource>,
) -> Result<BlockInput, CommandSyntaxError> {
    let BlockPredicate::Block {
        block,
        properties,
        nbt,
    } = context.block_predicate("block")?
    else {
        // The block-state parser refuses tags, so this arm is unreachable in
        // practice; an error is cheaper than a dead server if that changes.
        return Err(CommandSyntaxError::dynamic(
            "A block tag cannot be placed; name a single block.",
        ));
    };
    let state = REGISTRY
        .blocks
        .state_id_from_block_properties(
            block,
            properties
                .iter()
                .map(|(name, value)| (name.as_ref(), value.as_ref())),
        )
        .ok_or_else(|| {
            CommandSyntaxError::dynamic(
                "This Block is not registered or a property name/value is invalid.",
            )
        })?;
    Ok(BlockInput {
        state,
        properties: properties.clone(),
        nbt: nbt.clone(),
    })
}

/// Vanilla's `Predicate<BlockInWorld>` filter: state match, then block-entity
/// data when the predicate carries any.
fn filter_matches(world: &Arc<World>, filter: FillFilter<'_>, pos: BlockPos) -> bool {
    let predicate = match filter {
        FillFilter::Everything => return true,
        FillFilter::Air => return world.get_block_state(pos).is_air(),
        FillFilter::Predicate(predicate) => predicate,
    };
    if !predicate.matches_state(world.get_block_state(pos)) {
        return false;
    }
    let Some(expected) = predicate.nbt() else {
        return true;
    };
    world.get_block_entity(pos).is_some_and(|entity| {
        compare_nbt_compounds(expected, &entity.save_with_full_metadata(), true)
    })
}

impl BlockInput {
    /// Vanilla parity: `BlockInput.place`. Returns whether anything changed.
    fn place(&self, world: &Arc<World>, pos: BlockPos, flags: UpdateFlags) -> bool {
        let mut state = if flags.contains(UpdateFlags::UPDATE_KNOWN_SHAPE) {
            self.state
        } else {
            update_from_neighbour_shapes(world, self.state, pos)
        };
        if state.is_air() {
            state = self.state;
        }
        state = self.overwrite_with_defined_properties(state);

        let mut affected = world.set_block(pos, state, flags);
        let Some(nbt) = &self.nbt else {
            return affected;
        };
        let Some(entity) = world.get_block_entity(pos) else {
            return affected;
        };

        let before = entity.save_without_metadata();
        let mut bytes = Vec::new();
        nbt.write(&mut bytes);
        let Ok(borrowed) = read_compound(&mut Cursor::new(bytes.as_slice())) else {
            return affected;
        };
        entity.load_with_components(&borrowed);
        if !nbt_compounds_equal(&before, &entity.save_without_metadata()) {
            affected = true;
            entity.set_changed();
            world.send_block_updated(pos);
        }
        affected
    }

    /// Vanilla parity: `BlockInput.overwriteWithDefinedProperties`. Neighbour
    /// shape updates may have changed properties the command named; the command
    /// wins, but only for properties the resulting block actually has.
    fn overwrite_with_defined_properties(&self, state: BlockStateId) -> BlockStateId {
        if state == self.state || self.properties.is_empty() {
            return state;
        }
        let block = state.get_block();
        let mut merged: Vec<(String, String)> = REGISTRY
            .blocks
            .get_properties(state)
            .into_iter()
            .map(|(name, value)| (name.to_owned(), value.to_owned()))
            .collect();
        for (name, value) in &self.properties {
            if let Some(slot) = merged
                .iter_mut()
                .find(|(existing, _)| existing == name.as_ref())
            {
                slot.1 = value.to_string();
            }
        }
        REGISTRY
            .blocks
            .state_id_from_block_properties(
                block,
                merged
                    .iter()
                    .map(|(name, value)| (name.as_str(), value.as_str())),
            )
            .unwrap_or(state)
    }
}
