//! Vanilla slot-replacing command.
//!
//! Vanilla parity: `ItemCommands`. A target is a block's container slot or the
//! numbered slot of every matched entity; the stack put there is a literal
//! item, or the one read out of a source slot.

use std::slice;

use std::sync::Arc;

use foton_registry::item_stack::ItemStack;
use foton_utils::{BlockPos, Identifier, translations};
use text_components::{TextComponent, translation::TranslatedMessage};

use super::super::execution::ItemModifierArgument;
use super::super::{
    brigadier::{ArgumentType, CommandNodeBuilder, CommandSyntaxError},
    execution::{
        CommandSource, FotonArgumentType, FotonCommandContext, FotonCommandRuntime, argument,
        literal,
    },
    registration::CommandRegistration,
};
use super::{execute::condition::loaded_block_position, give::item_display_name};
use crate::{
    entity::SharedEntity,
    inventory::{
        lock::{ContainerLockGuard, ContainerRef},
        slot_ranges::container_slot_item,
    },
    item_modifier::{ItemModifier, ModifierContext},
};

type Context<'a> = &'a FotonCommandContext<CommandSource>;
type Builder = CommandNodeBuilder<CommandSource, FotonCommandRuntime>;

/// Where a stack goes.
#[derive(Clone, Copy)]
enum Target {
    Block,
    Entity,
}

/// Where a stack comes from.
#[derive(Clone, Copy)]
enum Source {
    Block,
    Entity,
}

pub(super) fn registration() -> CommandRegistration<CommandSource> {
    CommandRegistration::new(Identifier::vanilla_static("item"), |_| command())
}

fn command() -> Builder {
    literal("item")
        .then(replace_command())
        .then(modify_command())
}

fn replace_command() -> Builder {
    literal("replace")
        .then(literal("block").then(
            argument("pos", FotonArgumentType::block_pos()).then(replace_branch(Target::Block)),
        ))
        .then(literal("entity").then(
            argument("targets", FotonArgumentType::entities()).then(replace_branch(Target::Entity)),
        ))
}

fn modify_command() -> Builder {
    literal("modify")
        .then(
            literal("block").then(
                argument("pos", FotonArgumentType::block_pos()).then(
                    argument("slot", FotonArgumentType::slot()).then(
                        argument("modifier", FotonArgumentType::item_modifier())
                            .executes(modify_block_item),
                    ),
                ),
            ),
        )
        .then(
            literal("entity").then(
                argument("targets", FotonArgumentType::entities()).then(
                    argument("slot", FotonArgumentType::slot()).then(
                        argument("modifier", FotonArgumentType::item_modifier())
                            .executes(modify_entity_item),
                    ),
                ),
            ),
        )
}

/// The `<slot> with ...` and `<slot> from ...` subtrees both targets share.
fn replace_branch(target: Target) -> Builder {
    argument("slot", FotonArgumentType::slot())
        .then(
            literal("with").then(
                argument("item", FotonArgumentType::item_stack())
                    .executes(move |context| replace_with(context, target, 1))
                    .then(argument("count", ArgumentType::integer(1, 99)).executes(
                        move |context| {
                            let count = context.integer("count")?;
                            replace_with(context, target, count)
                        },
                    )),
            ),
        )
        .then(
            literal("from")
                .then(source_branch(target, Source::Block))
                .then(source_branch(target, Source::Entity)),
        )
}

fn source_branch(target: Target, source: Source) -> Builder {
    let (name, source_argument) = match source {
        Source::Block => ("block", FotonArgumentType::block_pos()),
        Source::Entity => ("entity", FotonArgumentType::entity()),
    };
    literal(name).then(
        argument("source", source_argument).then(
            argument("sourceSlot", FotonArgumentType::slot())
                .executes(move |context| replace_from(context, target, source, false))
                .then(
                    argument("modifier", FotonArgumentType::item_modifier())
                        .executes(move |context| replace_from(context, target, source, true)),
                ),
        ),
    )
}

/// Vanilla parity: `ItemArgument.getItem(...).createItemStack(count)`.
fn replace_with(
    context: Context<'_>,
    target: Target,
    count: i32,
) -> Result<i32, CommandSyntaxError> {
    let prototype = context.item_stack("item")?;
    let max = prototype.max_stack_size();
    if count > max {
        let message = translations::ARGUMENTS_ITEM_OVERSTACKED
            .message([prototype.item().key.to_string(), max.to_string()])
            .component();
        return Err(CommandSyntaxError::dynamic(message));
    }
    set_item(context, target, prototype.copy_with_count(count))
}

fn replace_from(
    context: Context<'_>,
    target: Target,
    source: Source,
    modified: bool,
) -> Result<i32, CommandSyntaxError> {
    let mut stack = read_source(context, source)?;
    if modified {
        stack = resolve_modifier(context)?.apply(stack, &modifier_context(context));
    }
    set_item(context, target, stack)
}

fn set_item(
    context: Context<'_>,
    target: Target,
    stack: ItemStack,
) -> Result<i32, CommandSyntaxError> {
    let slot = context.slot("slot")?;
    match target {
        Target::Block => set_block_item(context, slot, stack),
        Target::Entity => set_entity_item(context, slot, &stack),
    }
}

/// Vanilla parity: `ItemCommands.setBlockItem`.
fn set_block_item(
    context: Context<'_>,
    slot: i32,
    stack: ItemStack,
) -> Result<i32, CommandSyntaxError> {
    let position = loaded_block_position(context, "pos")?;
    let container_ref = block_container(context, position, ContainerSide::Target)?;
    let index = target_slot_index(&container_ref, slot)?;
    set_block_slot(context, &container_ref, index, position, stack)?;
    Ok(1)
}

/// Puts the stack in the slot and says so.
fn set_block_slot(
    context: Context<'_>,
    container_ref: &ContainerRef,
    index: usize,
    position: BlockPos,
    stack: ItemStack,
) -> Result<(), CommandSyntaxError> {
    let name = item_display_name(&stack);
    let mut guard = ContainerLockGuard::lock_all(slice::from_ref(container_ref));
    guard.set_item(container_ref.container_id(), index, stack);
    drop(guard);

    let message = translations::COMMANDS_ITEM_BLOCK_SET_SUCCESS
        .message([
            TextComponent::from(position.x().to_string()),
            TextComponent::from(position.y().to_string()),
            TextComponent::from(position.z().to_string()),
            name,
        ])
        .component();
    context.source().send_success(&message, true);
    Ok(())
}

/// A slot id as an index into the container, or vanilla's
/// `ERROR_TARGET_INAPPLICABLE_SLOT`.
fn target_slot_index(container_ref: &ContainerRef, slot: i32) -> Result<usize, CommandSyntaxError> {
    usize::try_from(slot)
        .ok()
        .filter(|&index| index < container_ref.container_size())
        .ok_or_else(|| no_such_slot(slot))
}

fn no_such_slot(slot: i32) -> CommandSyntaxError {
    CommandSyntaxError::dynamic(
        translations::COMMANDS_ITEM_TARGET_NO_SUCH_SLOT
            .message([slot.to_string()])
            .component(),
    )
}

/// Vanilla parity: `ItemCommands.setEntityItem`.
fn set_entity_item(
    context: Context<'_>,
    slot: i32,
    stack: &ItemStack,
) -> Result<i32, CommandSyntaxError> {
    let targets = context.entities("targets")?;
    let changed = targets
        .iter()
        .filter(|entity| set_entity_slot(entity, slot, stack.clone()))
        .collect::<Vec<_>>();

    let name = item_display_name(stack);
    let message = match changed.as_slice() {
        [] => {
            return Err(CommandSyntaxError::dynamic(
                translations::COMMANDS_ITEM_TARGET_NO_CHANGED_KNOWN_ITEM
                    .message([name, TextComponent::from(slot.to_string())])
                    .component(),
            ));
        }
        [entity] => translations::COMMANDS_ITEM_ENTITY_SET_SUCCESS_SINGLE
            .message([entity.display_name(), name])
            .component(),
        _ => translations::COMMANDS_ITEM_ENTITY_SET_SUCCESS_MULTIPLE
            .message([TextComponent::from(changed.len().to_string()), name])
            .component(),
    };
    context.source().send_success(&message, true);
    i32::try_from(changed.len()).map_err(|_| count_overflow())
}

/// Puts `stack` in one slot of one entity, then tells a player's client.
///
/// Vanilla parity: `SlotAccess.set` followed by
/// `serverPlayer.containerMenu.broadcastChanges()`.
fn set_entity_slot(entity: &SharedEntity, slot: i32, stack: ItemStack) -> bool {
    if !entity.set_slot_item(slot, stack) {
        return false;
    }
    if let Some(player) = entity.as_player() {
        player.broadcast_inventory_changes();
    }
    true
}

/// Reads the stack the source names. Vanilla parity: `ItemCommands.getBlockItem`
/// and `getItemInSlot`, which hand back a copy so a later write cannot reach
/// the source.
fn read_source(context: Context<'_>, source: Source) -> Result<ItemStack, CommandSyntaxError> {
    let slot = context.slot("sourceSlot")?;
    let stack = match source {
        Source::Block => {
            let position = loaded_block_position(context, "source")?;
            let container_ref = block_container(context, position, ContainerSide::Source)?;
            let guard = ContainerLockGuard::lock_all(slice::from_ref(&container_ref));
            guard
                .get(container_ref.container_id())
                .and_then(|container| container_slot_item(container, slot))
        }
        Source::Entity => context.entity("source")?.slot_item(slot),
    };
    stack.ok_or_else(|| {
        CommandSyntaxError::dynamic(
            translations::COMMANDS_ITEM_SOURCE_NO_SUCH_SLOT
                .message([slot.to_string()])
                .component(),
        )
    })
}

#[derive(Clone, Copy)]
enum ContainerSide {
    Target,
    Source,
}

/// Vanilla parity: `ItemCommands.getContainer`, whose `instanceof Container`
/// is a block entity with a container capability. A double chest is two of
/// them, and the position names only its own half.
fn block_container(
    context: Context<'_>,
    position: BlockPos,
    side: ContainerSide,
) -> Result<ContainerRef, CommandSyntaxError> {
    context
        .source()
        .world()
        .get_block_entity(position)
        .and_then(ContainerRef::from_block_entity)
        .ok_or_else(|| {
            let translation = match side {
                ContainerSide::Target => &translations::COMMANDS_ITEM_TARGET_NOT_A_CONTAINER,
                ContainerSide::Source => &translations::COMMANDS_ITEM_SOURCE_NOT_A_CONTAINER,
            };
            CommandSyntaxError::dynamic(
                translation
                    .message([
                        position.x().to_string(),
                        position.y().to_string(),
                        position.z().to_string(),
                    ])
                    .component(),
            )
        })
}

/// Vanilla parity: `ItemCommands.modifyBlockItem`.
fn modify_block_item(context: Context<'_>) -> Result<i32, CommandSyntaxError> {
    let slot = context.slot("slot")?;
    let position = loaded_block_position(context, "pos")?;
    let container_ref = block_container(context, position, ContainerSide::Target)?;
    let index = target_slot_index(&container_ref, slot)?;
    let modifier = resolve_modifier(context)?;

    let current = ContainerLockGuard::lock_all(slice::from_ref(&container_ref))
        .get(container_ref.container_id())
        .and_then(|container| container_slot_item(container, slot))
        .ok_or_else(|| no_such_slot(slot))?;
    let stack = modifier.apply(current, &modifier_context(context));
    set_block_slot(context, &container_ref, index, position, stack)?;
    Ok(1)
}

/// Vanilla parity: `ItemCommands.modifyEntityItem`. Each entity is modified
/// from what it holds, so a count of changed entities is also the count of
/// distinct results.
fn modify_entity_item(context: Context<'_>) -> Result<i32, CommandSyntaxError> {
    let slot = context.slot("slot")?;
    let targets = context.entities("targets")?;
    let modifier = resolve_modifier(context)?;
    let modifier_context = modifier_context(context);

    let mut changed = Vec::new();
    for entity in &targets {
        let Some(current) = entity.slot_item(slot) else {
            continue;
        };
        let stack = modifier.apply(current, &modifier_context);
        if set_entity_slot(entity, slot, stack.clone()) {
            changed.push((entity, stack));
        }
    }

    let message = match changed.as_slice() {
        [] => {
            return Err(CommandSyntaxError::dynamic(
                translations::COMMANDS_ITEM_TARGET_NO_CHANGES
                    .message([slot.to_string()])
                    .component(),
            ));
        }
        [(entity, stack)] => translations::COMMANDS_ITEM_ENTITY_SET_SUCCESS_SINGLE
            .message([entity.display_name(), item_display_name(stack)])
            .component(),
        _ => {
            // Vanilla passes this two-placeholder message only the count, so
            // the item name is left blank in the line a player reads.
            let key = translations::COMMANDS_ITEM_ENTITY_SET_SUCCESS_MULTIPLE.0;
            TranslatedMessage::new(
                key,
                Some(Box::new([TextComponent::from(changed.len().to_string())])),
            )
            .component()
        }
    };
    context.source().send_success(&message, true);
    i32::try_from(changed.len()).map_err(|_| count_overflow())
}

/// Reads the modifier argument, looking a named one up in the datapacks the
/// server has loaded.
fn resolve_modifier(context: Context<'_>) -> Result<Arc<ItemModifier>, CommandSyntaxError> {
    match context.item_modifier("modifier")? {
        ItemModifierArgument::Inline(modifier) => Ok(Arc::clone(modifier)),
        ItemModifierArgument::Reference(id) => context
            .source()
            .server()
            .functions
            .library()
            .item_modifier(id)
            .map(Arc::clone)
            .ok_or_else(|| {
                CommandSyntaxError::dynamic(
                    translations::ARGUMENT_RESOURCE_OR_ID_NO_SUCH_ELEMENT
                        .message([id.to_string(), "minecraft:item_modifier".to_owned()])
                        .component(),
                )
            }),
    }
}

/// Vanilla parity: the `COMMAND` loot param set, an origin and no entity.
fn modifier_context<'a>(context: Context<'a>) -> ModifierContext<'a> {
    let position = context.source().position();
    ModifierContext {
        origin: (position.x, position.y, position.z),
        world: &**context.source().world(),
    }
}

fn count_overflow() -> CommandSyntaxError {
    CommandSyntaxError::dynamic("Changed entity count exceeds the command result range")
}
