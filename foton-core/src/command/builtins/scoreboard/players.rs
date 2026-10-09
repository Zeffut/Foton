//! `/scoreboard players`.

use std::mem;

use foton_protocol::packets::game::NumberFormat;
use foton_utils::translations;
use text_components::TextComponent;

use super::{
    Context, count, data, format_list, number_formats, objective, resolved_component,
    scoreboard_error, source_scoreboard, text, writable_objective,
};
use crate::command::{
    brigadier::{ArgumentType, CommandNodeBuilder, CommandSyntaxError},
    execution::{
        CommandSource, FotonArgumentType, FotonCommandRuntime, ScoreHolderWildcard, ScoreOperation,
        argument, literal,
    },
};
use crate::scoreboard::{ScoreHolder, Scoreboard, ScoreboardObjective};

type Node = CommandNodeBuilder<CommandSource, FotonCommandRuntime>;

pub(super) fn subcommands() -> Node {
    literal("players")
        .then(
            literal("list").executes(list_tracked_players).then(
                argument("target", FotonArgumentType::score_holder())
                    .executes(list_tracked_player_scores),
            ),
        )
        .then(literal("set").then(targets_objective_score(
            ArgumentType::integer(i32::MIN, i32::MAX),
            set_score,
        )))
        .then(
            literal("get").then(
                argument("target", FotonArgumentType::score_holder()).then(
                    argument("objective", FotonArgumentType::objective()).executes(get_score),
                ),
            ),
        )
        .then(literal("add").then(targets_objective_score(
            ArgumentType::integer(0, i32::MAX),
            add_score,
        )))
        .then(literal("remove").then(targets_objective_score(
            ArgumentType::integer(0, i32::MAX),
            remove_score,
        )))
        .then(
            literal("reset").then(
                argument("targets", FotonArgumentType::score_holders())
                    .executes(reset_scores)
                    .then(
                        argument("objective", FotonArgumentType::objective()).executes(reset_score),
                    ),
            ),
        )
        .then(literal("enable").then(
            argument("targets", FotonArgumentType::score_holders()).then(
                argument("objective", FotonArgumentType::objective()).executes(enable_trigger),
            ),
        ))
        .then(
            literal("display")
                .then(
                    literal("name").then(
                        argument("targets", FotonArgumentType::score_holders()).then(
                            argument("objective", FotonArgumentType::objective())
                                .then(
                                    argument("name", FotonArgumentType::component())
                                        .executes(set_score_display),
                                )
                                .executes(clear_score_display),
                        ),
                    ),
                )
                .then(literal("numberformat").then(
                    argument("targets", FotonArgumentType::score_holders()).then(number_formats(
                        argument("objective", FotonArgumentType::objective()),
                        set_score_number_format,
                    )),
                )),
        )
        .then(
            literal("operation").then(
                argument("targets", FotonArgumentType::score_holders()).then(
                    argument("targetObjective", FotonArgumentType::objective()).then(
                        argument("operation", FotonArgumentType::score_operation()).then(
                            argument("source", FotonArgumentType::score_holders()).then(
                                argument("sourceObjective", FotonArgumentType::objective())
                                    .executes(perform_operation),
                            ),
                        ),
                    ),
                ),
            ),
        )
}

/// `<targets> <objective> <score>`, the shape of `set`, `add` and `remove`.
fn targets_objective_score(
    score: ArgumentType,
    executes: fn(Context<'_>) -> Result<i32, CommandSyntaxError>,
) -> Node {
    argument("targets", FotonArgumentType::score_holders()).then(
        argument("objective", FotonArgumentType::objective())
            .then(argument("score", score).executes(executes)),
    )
}

/// `ScoreHolderArgument.getNamesWithDefaultWildcard`.
fn targets(context: Context<'_>, name: &str) -> Result<Vec<ScoreHolder>, CommandSyntaxError> {
    context.score_holders(name, ScoreHolderWildcard::Tracked)
}

/// `ScoreboardCommand.getFirstTargetName`.
fn first_name(holders: &[ScoreHolder]) -> TextComponent {
    holders
        .first()
        .map_or_else(TextComponent::new, ScoreHolder::feedback_display_name)
}

/// Vanilla parity: `ScoreboardCommand.listTrackedPlayers`.
fn list_tracked_players(context: Context<'_>) -> Result<i32, CommandSyntaxError> {
    let holders = source_scoreboard(context)?.tracked_holders();
    let message = if holders.is_empty() {
        TextComponent::from(&translations::COMMANDS_SCOREBOARD_PLAYERS_LIST_EMPTY)
    } else {
        translations::COMMANDS_SCOREBOARD_PLAYERS_LIST_SUCCESS
            .message([
                text(holders.len().to_string()),
                format_list(
                    holders
                        .iter()
                        .map(ScoreHolder::feedback_display_name)
                        .collect(),
                ),
            ])
            .component()
    };
    context.source().send_success(&message, false);
    Ok(count(holders.len()))
}

/// Vanilla parity: `ScoreboardCommand.listTrackedPlayerScores`.
fn list_tracked_player_scores(context: Context<'_>) -> Result<i32, CommandSyntaxError> {
    let holder = context.score_holder("target")?;
    let scoreboard = source_scoreboard(context)?;
    let scores = scoreboard.holder_scores(&holder);
    if scores.is_empty() {
        let message = translations::COMMANDS_SCOREBOARD_PLAYERS_LIST_ENTITY_EMPTY
            .message([holder.feedback_display_name()])
            .component();
        context.source().send_success(&message, false);
        return Ok(0);
    }
    let message = translations::COMMANDS_SCOREBOARD_PLAYERS_LIST_ENTITY_SUCCESS
        .message([
            holder.feedback_display_name(),
            text(scores.len().to_string()),
        ])
        .component();
    context.source().send_success(&message, false);
    for (objective, value) in &scores {
        let name = scoreboard.objective_data(objective).map_or_else(
            || text(objective.clone()),
            |data| data.formatted_display_name(),
        );
        let entry = translations::COMMANDS_SCOREBOARD_PLAYERS_LIST_ENTITY_ENTRY
            .message([name, text(value.to_string())])
            .component();
        context.source().send_success(&entry, false);
    }
    Ok(count(scores.len()))
}

/// Vanilla parity: `ScoreboardCommand.setScore`.
fn set_score(context: Context<'_>) -> Result<i32, CommandSyntaxError> {
    let holders = targets(context, "targets")?;
    let objective = writable_objective(context, "objective")?;
    let value = context.integer("score")?;
    let scoreboard = source_scoreboard(context)?;
    for holder in &holders {
        scoreboard
            .set_score(holder, &objective, value)
            .map_err(scoreboard_error)?;
    }

    let name = data(context, &objective)?.formatted_display_name();
    let message = if let [only] = holders.as_slice() {
        translations::COMMANDS_SCOREBOARD_PLAYERS_SET_SUCCESS_SINGLE
            .message([name, only.feedback_display_name(), text(value.to_string())])
            .component()
    } else {
        translations::COMMANDS_SCOREBOARD_PLAYERS_SET_SUCCESS_MULTIPLE
            .message([
                name,
                text(holders.len().to_string()),
                text(value.to_string()),
            ])
            .component()
    };
    context.source().send_success(&message, true);
    Ok(value.wrapping_mul(count(holders.len())))
}

/// Vanilla parity: `ScoreboardCommand.getScore`.
fn get_score(context: Context<'_>) -> Result<i32, CommandSyntaxError> {
    let holder = context.score_holder("target")?;
    let objective = objective(context, "objective")?;
    let Some(value) = source_scoreboard(context)?.score(&holder, &objective) else {
        return Err(CommandSyntaxError::dynamic(
            translations::COMMANDS_SCOREBOARD_PLAYERS_GET_NULL
                .message([text(objective.name()), holder.feedback_display_name()])
                .component(),
        ));
    };
    let message = translations::COMMANDS_SCOREBOARD_PLAYERS_GET_SUCCESS
        .message([
            holder.feedback_display_name(),
            text(value.to_string()),
            data(context, &objective)?.formatted_display_name(),
        ])
        .component();
    context.source().send_success(&message, false);
    Ok(value)
}

/// `add` and `remove` differ only in the direction of the change and the
/// message; both report the sum of the scores they leave behind.
fn change_scores(context: Context<'_>, subtract: bool) -> Result<i32, CommandSyntaxError> {
    let holders = targets(context, "targets")?;
    let objective = writable_objective(context, "objective")?;
    let delta = context.integer("score")?;
    let scoreboard = source_scoreboard(context)?;
    let mut result = 0_i32;
    for holder in &holders {
        let (current, created) = scoreboard
            .score_or_create(holder, &objective)
            .map_err(scoreboard_error)?;
        let updated = if subtract {
            current.wrapping_sub(delta)
        } else {
            current.wrapping_add(delta)
        };
        if created {
            scoreboard.set_created_score(holder, &objective, updated)
        } else {
            scoreboard.set_score(holder, &objective, updated)
        }
        .map_err(scoreboard_error)?;
        result = result.wrapping_add(updated);
    }

    let name = data(context, &objective)?.formatted_display_name();
    let message = match (subtract, holders.as_slice()) {
        (false, [only]) => translations::COMMANDS_SCOREBOARD_PLAYERS_ADD_SUCCESS_SINGLE
            .message([
                text(delta.to_string()),
                name,
                only.feedback_display_name(),
                text(result.to_string()),
            ])
            .component(),
        (false, _) => translations::COMMANDS_SCOREBOARD_PLAYERS_ADD_SUCCESS_MULTIPLE
            .message([
                text(delta.to_string()),
                name,
                text(holders.len().to_string()),
            ])
            .component(),
        (true, [only]) => translations::COMMANDS_SCOREBOARD_PLAYERS_REMOVE_SUCCESS_SINGLE
            .message([
                text(delta.to_string()),
                name,
                only.feedback_display_name(),
                text(result.to_string()),
            ])
            .component(),
        (true, _) => translations::COMMANDS_SCOREBOARD_PLAYERS_REMOVE_SUCCESS_MULTIPLE
            .message([
                text(delta.to_string()),
                name,
                text(holders.len().to_string()),
            ])
            .component(),
    };
    context.source().send_success(&message, true);
    Ok(result)
}

/// Vanilla parity: `ScoreboardCommand.addScore`.
fn add_score(context: Context<'_>) -> Result<i32, CommandSyntaxError> {
    change_scores(context, false)
}

/// Vanilla parity: `ScoreboardCommand.removeScore`.
fn remove_score(context: Context<'_>) -> Result<i32, CommandSyntaxError> {
    change_scores(context, true)
}

/// Vanilla parity: `ScoreboardCommand.resetScores`.
fn reset_scores(context: Context<'_>) -> Result<i32, CommandSyntaxError> {
    let holders = targets(context, "targets")?;
    let scoreboard = source_scoreboard(context)?;
    for holder in &holders {
        scoreboard.reset_holder(holder);
    }
    let message = if let [only] = holders.as_slice() {
        translations::COMMANDS_SCOREBOARD_PLAYERS_RESET_ALL_SINGLE
            .message([only.feedback_display_name()])
            .component()
    } else {
        translations::COMMANDS_SCOREBOARD_PLAYERS_RESET_ALL_MULTIPLE
            .message([text(holders.len().to_string())])
            .component()
    };
    context.source().send_success(&message, true);
    Ok(count(holders.len()))
}

/// Vanilla parity: `ScoreboardCommand.resetScore`.
fn reset_score(context: Context<'_>) -> Result<i32, CommandSyntaxError> {
    let holders = targets(context, "targets")?;
    let objective = objective(context, "objective")?;
    let scoreboard = source_scoreboard(context)?;
    for holder in &holders {
        scoreboard.reset_score(holder, &objective);
    }
    let name = data(context, &objective)?.formatted_display_name();
    let message = if let [only] = holders.as_slice() {
        translations::COMMANDS_SCOREBOARD_PLAYERS_RESET_SPECIFIC_SINGLE
            .message([name, only.feedback_display_name()])
            .component()
    } else {
        translations::COMMANDS_SCOREBOARD_PLAYERS_RESET_SPECIFIC_MULTIPLE
            .message([name, text(holders.len().to_string())])
            .component()
    };
    context.source().send_success(&message, true);
    Ok(count(holders.len()))
}

/// Vanilla parity: `ScoreboardCommand.enableTrigger`, which counts the scores
/// it actually unlocked and refuses to report success for none.
fn enable_trigger(context: Context<'_>) -> Result<i32, CommandSyntaxError> {
    let holders = targets(context, "targets")?;
    let objective = objective(context, "objective")?;
    let objective_data = data(context, &objective)?;
    if !objective_data.criteria.is_trigger() {
        return Err(CommandSyntaxError::dynamic(TextComponent::from(
            &translations::COMMANDS_SCOREBOARD_PLAYERS_ENABLE_INVALID,
        )));
    }
    let scoreboard = source_scoreboard(context)?;
    let mut unlocked = 0;
    for holder in &holders {
        let locked = scoreboard
            .score_entry(holder, &objective)
            .is_none_or(|score| score.is_locked());
        if locked {
            scoreboard
                .set_score_locked(holder, &objective, false)
                .map_err(scoreboard_error)?;
            unlocked += 1;
        }
    }
    if unlocked == 0 {
        return Err(CommandSyntaxError::dynamic(TextComponent::from(
            &translations::COMMANDS_SCOREBOARD_PLAYERS_ENABLE_FAILED,
        )));
    }

    let name = objective_data.formatted_display_name();
    let message = if let [only] = holders.as_slice() {
        translations::COMMANDS_SCOREBOARD_PLAYERS_ENABLE_SUCCESS_SINGLE
            .message([name, only.feedback_display_name()])
            .component()
    } else {
        translations::COMMANDS_SCOREBOARD_PLAYERS_ENABLE_SUCCESS_MULTIPLE
            .message([name, text(holders.len().to_string())])
            .component()
    };
    context.source().send_success(&message, true);
    Ok(unlocked)
}

/// Vanilla parity: `ScoreboardCommand.setScoreDisplay` with a name.
fn set_score_display(context: Context<'_>) -> Result<i32, CommandSyntaxError> {
    let display = resolved_component(context, "name")?;
    change_score_display(context, Some(display))
}

/// Vanilla parity: `ScoreboardCommand.setScoreDisplay` clearing the name.
fn clear_score_display(context: Context<'_>) -> Result<i32, CommandSyntaxError> {
    change_score_display(context, None)
}

fn change_score_display(
    context: Context<'_>,
    display: Option<TextComponent>,
) -> Result<i32, CommandSyntaxError> {
    let holders = targets(context, "targets")?;
    let objective = objective(context, "objective")?;
    let scoreboard = source_scoreboard(context)?;
    for holder in &holders {
        scoreboard
            .set_score_display(holder, &objective, display.clone())
            .map_err(scoreboard_error)?;
    }

    let objective_name = data(context, &objective)?.formatted_display_name();
    let single = holders.len() == 1;
    let message = match display {
        None if single => {
            translations::COMMANDS_SCOREBOARD_PLAYERS_DISPLAY_NAME_CLEAR_SUCCESS_SINGLE
                .message([first_name(&holders), objective_name])
                .component()
        }
        None => translations::COMMANDS_SCOREBOARD_PLAYERS_DISPLAY_NAME_CLEAR_SUCCESS_MULTIPLE
            .message([text(holders.len().to_string()), objective_name])
            .component(),
        Some(display) if single => {
            translations::COMMANDS_SCOREBOARD_PLAYERS_DISPLAY_NAME_SET_SUCCESS_SINGLE
                .message([display, first_name(&holders), objective_name])
                .component()
        }
        Some(display) => {
            translations::COMMANDS_SCOREBOARD_PLAYERS_DISPLAY_NAME_SET_SUCCESS_MULTIPLE
                .message([display, text(holders.len().to_string()), objective_name])
                .component()
        }
    };
    context.source().send_success(&message, true);
    Ok(count(holders.len()))
}

/// Vanilla parity: `ScoreboardCommand.setScoreNumberFormat`.
fn set_score_number_format(
    context: Context<'_>,
    format: Option<NumberFormat>,
) -> Result<i32, CommandSyntaxError> {
    let holders = targets(context, "targets")?;
    let objective = objective(context, "objective")?;
    let scoreboard = source_scoreboard(context)?;
    let set = format.is_some();
    for holder in &holders {
        scoreboard
            .set_score_number_format(holder, &objective, format.clone())
            .map_err(scoreboard_error)?;
    }

    let objective_name = data(context, &objective)?.formatted_display_name();
    let single = holders.len() == 1;
    let message = match (set, single) {
        (false, true) => {
            translations::COMMANDS_SCOREBOARD_PLAYERS_DISPLAY_NUMBER_FORMAT_CLEAR_SUCCESS_SINGLE
                .message([first_name(&holders), objective_name])
                .component()
        }
        (false, false) => {
            translations::COMMANDS_SCOREBOARD_PLAYERS_DISPLAY_NUMBER_FORMAT_CLEAR_SUCCESS_MULTIPLE
                .message([text(holders.len().to_string()), objective_name])
                .component()
        }
        (true, true) => {
            translations::COMMANDS_SCOREBOARD_PLAYERS_DISPLAY_NUMBER_FORMAT_SET_SUCCESS_SINGLE
                .message([first_name(&holders), objective_name])
                .component()
        }
        (true, false) => {
            translations::COMMANDS_SCOREBOARD_PLAYERS_DISPLAY_NUMBER_FORMAT_SET_SUCCESS_MULTIPLE
                .message([text(holders.len().to_string()), objective_name])
                .component()
        }
    };
    context.source().send_success(&message, true);
    Ok(count(holders.len()))
}

/// Writes the source's score back only for the operation that changes it.
fn store_source(
    scoreboard: &Scoreboard,
    operation: ScoreOperation,
    source: &ScoreHolder,
    source_objective: &ScoreboardObjective,
    value: i32,
) -> Result<(), CommandSyntaxError> {
    if operation == ScoreOperation::Swap {
        scoreboard
            .set_score(source, source_objective, value)
            .map_err(scoreboard_error)?;
    }
    Ok(())
}

/// Vanilla parity: `ScoreboardCommand.performOperation`. Each target takes
/// every source in turn, a division by zero stops the command where it is,
/// and the result is the sum of the targets' final scores.
fn perform_operation(context: Context<'_>) -> Result<i32, CommandSyntaxError> {
    let holders = targets(context, "targets")?;
    let target_objective = writable_objective(context, "targetObjective")?;
    let operation = context.score_operation("operation")?;
    let sources = targets(context, "source")?;
    let source_objective = objective(context, "sourceObjective")?;
    let scoreboard = source_scoreboard(context)?;

    let mut result = 0_i32;
    for target in &holders {
        let (mut current, mut unsent) = scoreboard
            .score_or_create(target, &target_objective)
            .map_err(scoreboard_error)?;
        for source in &sources {
            let (source_value, _) = scoreboard
                .score_or_create(source, &source_objective)
                .map_err(scoreboard_error)?;
            let (target_value, new_source_value) =
                operation.apply(current, source_value).map_err(|_| {
                    CommandSyntaxError::dynamic(TextComponent::from(
                        &translations::ARGUMENTS_OPERATION_DIV0,
                    ))
                })?;
            // A score this command created is sent by its first write.
            if mem::take(&mut unsent) {
                scoreboard.set_created_score(target, &target_objective, target_value)
            } else {
                scoreboard.set_score(target, &target_objective, target_value)
            }
            .map_err(scoreboard_error)?;
            store_source(
                scoreboard,
                operation,
                source,
                &source_objective,
                new_source_value,
            )?;
            // A source that is the target itself changes what the next one sees.
            current = scoreboard
                .score_or_create(target, &target_objective)
                .map_err(scoreboard_error)?
                .0;
        }
        result = result.wrapping_add(current);
    }

    let name = data(context, &target_objective)?.formatted_display_name();
    let message = if let [only] = holders.as_slice() {
        translations::COMMANDS_SCOREBOARD_PLAYERS_OPERATION_SUCCESS_SINGLE
            .message([name, only.feedback_display_name(), text(result.to_string())])
            .component()
    } else {
        translations::COMMANDS_SCOREBOARD_PLAYERS_OPERATION_SUCCESS_MULTIPLE
            .message([name, text(holders.len().to_string())])
            .component()
    };
    context.source().send_success(&message, true);
    Ok(result)
}
