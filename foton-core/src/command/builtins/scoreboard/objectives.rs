//! `/scoreboard objectives`.

use foton_protocol::packets::game::ObjectiveRenderType;
use foton_utils::translations;
use text_components::TextComponent;

use super::{
    Context, count, data, format_list, number_formats, objective, resolved_component,
    source_scoreboard, text,
};
use crate::command::{
    brigadier::{ArgumentType, CommandNodeBuilder, CommandSyntaxError},
    execution::{CommandSource, FotonArgumentType, FotonCommandRuntime, argument, literal},
};
use crate::scoreboard::ObjectiveData;
use foton_protocol::packets::game::NumberFormat;

type Node = CommandNodeBuilder<CommandSource, FotonCommandRuntime>;

pub(super) fn subcommands() -> Node {
    literal("objectives")
        .then(literal("list").executes(list_objectives))
        .then(
            literal("add").then(
                argument("objective", ArgumentType::word()).then(
                    argument("criteria", FotonArgumentType::objective_criteria())
                        .executes(|context| add_objective(context, false))
                        .then(
                            argument("displayName", FotonArgumentType::component())
                                .executes(|context| add_objective(context, true)),
                        ),
                ),
            ),
        )
        .then(literal("modify").then(modify_subcommands()))
        .then(
            literal("remove").then(
                argument("objective", FotonArgumentType::objective()).executes(remove_objective),
            ),
        )
        .then(
            literal("setdisplay").then(
                argument("slot", FotonArgumentType::display_slot())
                    .executes(clear_display_slot)
                    .then(
                        argument("objective", FotonArgumentType::objective())
                            .executes(set_display_slot),
                    ),
            ),
        )
}

fn modify_subcommands() -> Node {
    let mut render_types = literal("rendertype");
    for render_type in ObjectiveRenderType::VALUES {
        render_types = render_types.then(
            literal(render_type.id())
                .executes(move |context| set_render_type(context, render_type)),
        );
    }
    argument("objective", FotonArgumentType::objective())
        .then(literal("displayname").then(
            argument("displayName", FotonArgumentType::component()).executes(set_display_name),
        ))
        .then(render_types)
        .then(
            literal("displayautoupdate")
                .then(argument("value", ArgumentType::bool()).executes(set_display_auto_update)),
        )
        .then(number_formats(
            literal("numberformat"),
            set_objective_format,
        ))
}

/// Vanilla parity: `ScoreboardCommand.listObjectives`.
fn list_objectives(context: Context<'_>) -> Result<i32, CommandSyntaxError> {
    let objectives = source_scoreboard(context)?.objectives();
    let message = if objectives.is_empty() {
        TextComponent::from(&translations::COMMANDS_SCOREBOARD_OBJECTIVES_LIST_EMPTY)
    } else {
        translations::COMMANDS_SCOREBOARD_OBJECTIVES_LIST_SUCCESS
            .message([
                text(objectives.len().to_string()),
                format_list(
                    objectives
                        .iter()
                        .map(ObjectiveData::formatted_display_name)
                        .collect(),
                ),
            ])
            .component()
    };
    context.source().send_success(&message, false);
    Ok(count(objectives.len()))
}

/// Vanilla parity: `ScoreboardCommand.addObjective`.
fn add_objective(context: Context<'_>, with_display_name: bool) -> Result<i32, CommandSyntaxError> {
    let name = context.string("objective")?.to_owned();
    let criteria = context.objective_criteria("criteria")?;
    let display_name = if with_display_name {
        resolved_component(context, "displayName")?
    } else {
        text(name.clone())
    };
    let scoreboard = source_scoreboard(context)?;
    if scoreboard
        .create_objective(name.clone(), criteria, display_name)
        .is_err()
    {
        return Err(CommandSyntaxError::dynamic(TextComponent::from(
            &translations::COMMANDS_SCOREBOARD_OBJECTIVES_ADD_DUPLICATE,
        )));
    }
    let Some(created) = scoreboard.objective_data(&name) else {
        return Ok(count(scoreboard.objective_names().len()));
    };
    let message = translations::COMMANDS_SCOREBOARD_OBJECTIVES_ADD_SUCCESS
        .message([created.formatted_display_name()])
        .component();
    context.source().send_success(&message, true);
    Ok(count(scoreboard.objective_names().len()))
}

/// Vanilla parity: `ScoreboardCommand.setDisplayName`, silent when nothing changes.
fn set_display_name(context: Context<'_>) -> Result<i32, CommandSyntaxError> {
    let objective = objective(context, "objective")?;
    let display_name = resolved_component(context, "displayName")?;
    if data(context, &objective)?.display_name == display_name {
        return Ok(0);
    }
    source_scoreboard(context)?.set_objective_display_name(&objective, display_name);
    let message = translations::COMMANDS_SCOREBOARD_OBJECTIVES_MODIFY_DISPLAYNAME
        .message([
            text(objective.name()),
            data(context, &objective)?.formatted_display_name(),
        ])
        .component();
    context.source().send_success(&message, true);
    Ok(0)
}

/// Vanilla parity: `ScoreboardCommand.setRenderType`, silent when nothing changes.
fn set_render_type(
    context: Context<'_>,
    render_type: ObjectiveRenderType,
) -> Result<i32, CommandSyntaxError> {
    let objective = objective(context, "objective")?;
    if data(context, &objective)?.render_type == render_type {
        return Ok(0);
    }
    source_scoreboard(context)?.set_objective_render_type(&objective, render_type);
    let message = translations::COMMANDS_SCOREBOARD_OBJECTIVES_MODIFY_RENDERTYPE
        .message([data(context, &objective)?.formatted_display_name()])
        .component();
    context.source().send_success(&message, true);
    Ok(0)
}

/// Vanilla parity: `ScoreboardCommand.setDisplayAutoUpdate`, silent when
/// nothing changes.
fn set_display_auto_update(context: Context<'_>) -> Result<i32, CommandSyntaxError> {
    let objective = objective(context, "objective")?;
    let enabled = context.boolean("value")?;
    if data(context, &objective)?.display_auto_update == enabled {
        return Ok(0);
    }
    source_scoreboard(context)?.set_objective_display_auto_update(&objective, enabled);
    let translation = if enabled {
        &translations::COMMANDS_SCOREBOARD_OBJECTIVES_MODIFY_DISPLAY_AUTO_UPDATE_ENABLE
    } else {
        &translations::COMMANDS_SCOREBOARD_OBJECTIVES_MODIFY_DISPLAY_AUTO_UPDATE_DISABLE
    };
    // Vanilla also passes the formatted display name, which the sentence has
    // no placeholder for.
    let message = translation.message([text(objective.name())]).component();
    context.source().send_success(&message, true);
    Ok(0)
}

/// Vanilla parity: `ScoreboardCommand.setObjectiveFormat`.
fn set_objective_format(
    context: Context<'_>,
    format: Option<NumberFormat>,
) -> Result<i32, CommandSyntaxError> {
    let objective = objective(context, "objective")?;
    let set = format.is_some();
    source_scoreboard(context)?.set_objective_number_format(&objective, format);
    let translation = if set {
        &translations::COMMANDS_SCOREBOARD_OBJECTIVES_MODIFY_OBJECTIVE_FORMAT_SET
    } else {
        &translations::COMMANDS_SCOREBOARD_OBJECTIVES_MODIFY_OBJECTIVE_FORMAT_CLEAR
    };
    let message = translation.message([text(objective.name())]).component();
    context.source().send_success(&message, true);
    Ok(0)
}

/// Vanilla parity: `ScoreboardCommand.removeObjective`.
fn remove_objective(context: Context<'_>) -> Result<i32, CommandSyntaxError> {
    let objective = objective(context, "objective")?;
    let removed = data(context, &objective)?;
    let scoreboard = source_scoreboard(context)?;
    scoreboard.remove_objective(&objective);
    let message = translations::COMMANDS_SCOREBOARD_OBJECTIVES_REMOVE_SUCCESS
        .message([removed.formatted_display_name()])
        .component();
    context.source().send_success(&message, true);
    Ok(count(scoreboard.objective_names().len()))
}

/// Vanilla parity: `ScoreboardCommand.clearDisplaySlot`.
fn clear_display_slot(context: Context<'_>) -> Result<i32, CommandSyntaxError> {
    let slot = context.display_slot("slot")?;
    let scoreboard = source_scoreboard(context)?;
    if scoreboard.display_objective(slot).is_none() {
        return Err(CommandSyntaxError::dynamic(TextComponent::from(
            &translations::COMMANDS_SCOREBOARD_OBJECTIVES_DISPLAY_ALREADY_EMPTY,
        )));
    }
    scoreboard.set_display_objective(slot, None);
    let message = translations::COMMANDS_SCOREBOARD_OBJECTIVES_DISPLAY_CLEARED
        .message([text(slot.serialized_name())])
        .component();
    context.source().send_success(&message, true);
    Ok(0)
}

/// Vanilla parity: `ScoreboardCommand.setDisplaySlot`, which refuses a slot
/// that already shows the objective.
fn set_display_slot(context: Context<'_>) -> Result<i32, CommandSyntaxError> {
    let slot = context.display_slot("slot")?;
    let objective = objective(context, "objective")?;
    let scoreboard = source_scoreboard(context)?;
    if scoreboard.display_objective(slot).as_deref() == Some(objective.name()) {
        return Err(CommandSyntaxError::dynamic(TextComponent::from(
            &translations::COMMANDS_SCOREBOARD_OBJECTIVES_DISPLAY_ALREADY_SET,
        )));
    }
    scoreboard.set_display_objective(slot, Some(&objective));
    let message = translations::COMMANDS_SCOREBOARD_OBJECTIVES_DISPLAY_SET
        .message([
            text(slot.serialized_name()),
            data(context, &objective)?.display_name,
        ])
        .component();
    context.source().send_success(&message, true);
    Ok(0)
}
