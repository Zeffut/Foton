//! The `/team` command.
//!
//! Vanilla parity: `net.minecraft.server.commands.TeamCommand`.
//!
//! Only the subcommands backed by state that something reads are here. Vanilla
//! also carries `displayName`, `color`, `prefix`, `suffix`,
//! `nametagVisibility`, `deathMessageVisibility` and `collisionRule`; every one
//! of those exists to reach a client through `CSetPlayerTeam`, which Foton does
//! not send yet. Storing them would be writing data nothing can observe, so
//! they are left out until the packet exists rather than accepted and dropped.
//!
//! What is here changes behavior: membership, which `@e[team=]` already reads,
//! and the two options `Player.canHarmPlayer` and `Entity.isAlliedTo` consult.

use foton_utils::{Identifier, translations};
use text_components::TextComponent;

use super::super::{
    brigadier::{ArgumentType, CommandNodeBuilder, CommandSyntaxError},
    execution::{
        CommandSource, FotonArgumentType, FotonCommandContext, FotonCommandRuntime,
        ScoreHolderWildcard, argument, literal,
    },
    registration::CommandRegistration,
};
use crate::scoreboard::{ScoreHolder, Scoreboard, ScoreboardTeam};

pub(super) fn registration() -> CommandRegistration<CommandSource> {
    CommandRegistration::new(Identifier::vanilla_static("team"), |_| command())
}

fn command() -> CommandNodeBuilder<CommandSource, FotonCommandRuntime> {
    literal("team")
        .then(
            literal("list")
                .executes(list_teams)
                .then(argument("team", ArgumentType::word()).executes(list_members)),
        )
        .then(literal("add").then(argument("team", ArgumentType::word()).executes(add_team)))
        .then(literal("remove").then(argument("team", ArgumentType::word()).executes(remove_team)))
        .then(literal("empty").then(argument("team", ArgumentType::word()).executes(empty_team)))
        .then(
            literal("join").then(
                argument("team", ArgumentType::word()).then(
                    argument("members", FotonArgumentType::score_holders()).executes(join_team),
                ),
            ),
        )
        .then(
            literal("leave")
                .then(argument("members", FotonArgumentType::score_holders()).executes(leave_team)),
        )
        .then(literal("modify").then(modify_subcommands()))
}

fn modify_subcommands() -> CommandNodeBuilder<CommandSource, FotonCommandRuntime> {
    argument("team", ArgumentType::word())
        .then(
            literal("friendlyFire")
                .then(argument("allowed", ArgumentType::bool()).executes(set_friendly_fire)),
        )
        .then(
            literal("seeFriendlyInvisibles").then(
                argument("allowed", ArgumentType::bool()).executes(set_see_friendly_invisibles),
            ),
        )
}

/// The scoreboard of the domain the command was run in.
///
/// Vanilla reaches one server-wide scoreboard; Foton keeps one per domain,
/// beside the boss bars and the command storage.
fn source_scoreboard(
    context: &FotonCommandContext<CommandSource>,
) -> Result<&Scoreboard, CommandSyntaxError> {
    let source = context.source();
    source
        .server()
        .scoreboards
        .get(source.world().domain())
        .ok_or_else(|| {
            CommandSyntaxError::dynamic(format!(
                "Domain '{}' has no scoreboard",
                source.world().domain()
            ))
        })
}

/// Vanilla parity: `TeamArgument.getTeam`, which fails on an unknown name.
fn team(
    context: &FotonCommandContext<CommandSource>,
) -> Result<ScoreboardTeam, CommandSyntaxError> {
    let name = context.string("team")?.to_owned();
    source_scoreboard(context)?
        .team(&name)
        .ok_or_else(|| CommandSyntaxError::dynamic(format!("Unknown team '{name}'")))
}

/// Vanilla parity: `TeamCommand.listTeams`.
fn list_teams(context: &FotonCommandContext<CommandSource>) -> Result<i32, CommandSyntaxError> {
    let names = source_scoreboard(context)?.team_names();
    let message = if names.is_empty() {
        TextComponent::from(&translations::COMMANDS_TEAM_LIST_TEAMS_EMPTY)
    } else {
        translations::COMMANDS_TEAM_LIST_TEAMS_SUCCESS
            .message([names.len().to_string(), names.join(", ")])
            .component()
    };
    context.source().send_success(&message, false);
    Ok(i32::try_from(names.len()).unwrap_or(i32::MAX))
}

/// Vanilla parity: `TeamCommand.listMembers`.
fn list_members(context: &FotonCommandContext<CommandSource>) -> Result<i32, CommandSyntaxError> {
    let team = team(context)?;
    let entries = source_scoreboard(context)?.team_entries(&team);
    let message = if entries.is_empty() {
        translations::COMMANDS_TEAM_LIST_MEMBERS_EMPTY
            .message([team.name().to_owned()])
            .component()
    } else {
        translations::COMMANDS_TEAM_LIST_MEMBERS_SUCCESS
            .message([
                team.name().to_owned(),
                entries.len().to_string(),
                entries.join(", "),
            ])
            .component()
    };
    context.source().send_success(&message, false);
    Ok(i32::try_from(entries.len()).unwrap_or(i32::MAX))
}

/// Vanilla parity: `TeamCommand.createTeam`.
fn add_team(context: &FotonCommandContext<CommandSource>) -> Result<i32, CommandSyntaxError> {
    let name = context.string("team")?.to_owned();
    let scoreboard = source_scoreboard(context)?;
    let Ok(team) = scoreboard.add_team(name) else {
        return Err(CommandSyntaxError::dynamic(TextComponent::from(
            &translations::COMMANDS_TEAM_ADD_DUPLICATE,
        )));
    };
    let message = translations::COMMANDS_TEAM_ADD_SUCCESS
        .message([team.name().to_owned()])
        .component();
    context.source().send_success(&message, true);
    Ok(1)
}

/// Vanilla parity: `TeamCommand.deleteTeam`.
fn remove_team(context: &FotonCommandContext<CommandSource>) -> Result<i32, CommandSyntaxError> {
    let team = team(context)?;
    source_scoreboard(context)?.remove_team(&team);
    let message = translations::COMMANDS_TEAM_REMOVE_SUCCESS
        .message([team.name().to_owned()])
        .component();
    context.source().send_success(&message, true);
    Ok(1)
}

/// Vanilla parity: `TeamCommand.emptyTeam`, which refuses an empty team.
fn empty_team(context: &FotonCommandContext<CommandSource>) -> Result<i32, CommandSyntaxError> {
    let team = team(context)?;
    let scoreboard = source_scoreboard(context)?;
    let entries = scoreboard.team_entries(&team);
    if entries.is_empty() {
        return Err(CommandSyntaxError::dynamic(TextComponent::from(
            &translations::COMMANDS_TEAM_EMPTY_UNCHANGED,
        )));
    }
    for entry in &entries {
        scoreboard.remove_holder_from_team(&ScoreHolder::new(entry.clone()));
    }
    let message = translations::COMMANDS_TEAM_EMPTY_SUCCESS
        .message([entries.len().to_string(), team.name().to_owned()])
        .component();
    context.source().send_success(&message, true);
    Ok(i32::try_from(entries.len()).unwrap_or(i32::MAX))
}

/// Vanilla parity: `TeamCommand.joinTeam`.
fn join_team(context: &FotonCommandContext<CommandSource>) -> Result<i32, CommandSyntaxError> {
    let team = team(context)?;
    let members = context.score_holders("members", ScoreHolderWildcard::Empty)?;
    let scoreboard = source_scoreboard(context)?;
    for member in &members {
        scoreboard
            .add_holder_to_team(member, &team)
            .map_err(|error| CommandSyntaxError::dynamic(error.to_string()))?;
    }

    let message = if let [only] = members.as_slice() {
        translations::COMMANDS_TEAM_JOIN_SUCCESS_SINGLE
            .message([only.name().to_owned(), team.name().to_owned()])
            .component()
    } else {
        translations::COMMANDS_TEAM_JOIN_SUCCESS_MULTIPLE
            .message([members.len().to_string(), team.name().to_owned()])
            .component()
    };
    context.source().send_success(&message, true);
    Ok(i32::try_from(members.len()).unwrap_or(i32::MAX))
}

/// Vanilla parity: `TeamCommand.leaveTeam`.
fn leave_team(context: &FotonCommandContext<CommandSource>) -> Result<i32, CommandSyntaxError> {
    let members = context.score_holders("members", ScoreHolderWildcard::Empty)?;
    let scoreboard = source_scoreboard(context)?;
    for member in &members {
        scoreboard.remove_holder_from_team(member);
    }

    let message = if let [only] = members.as_slice() {
        translations::COMMANDS_TEAM_LEAVE_SUCCESS_SINGLE
            .message([only.name().to_owned()])
            .component()
    } else {
        translations::COMMANDS_TEAM_LEAVE_SUCCESS_MULTIPLE
            .message([members.len().to_string()])
            .component()
    };
    context.source().send_success(&message, true);
    Ok(i32::try_from(members.len()).unwrap_or(i32::MAX))
}

/// Vanilla parity: `TeamCommand.setFriendlyFire`, which refuses a no-op.
fn set_friendly_fire(
    context: &FotonCommandContext<CommandSource>,
) -> Result<i32, CommandSyntaxError> {
    let team = team(context)?;
    let allowed = context.boolean("allowed")?;
    let scoreboard = source_scoreboard(context)?;
    let mut options = scoreboard.team_options(&team);
    if options.allow_friendly_fire == allowed {
        return Err(CommandSyntaxError::dynamic(TextComponent::from(
            if allowed {
                &translations::COMMANDS_TEAM_OPTION_FRIENDLYFIRE_ALREADY_ENABLED
            } else {
                &translations::COMMANDS_TEAM_OPTION_FRIENDLYFIRE_ALREADY_DISABLED
            },
        )));
    }

    options.allow_friendly_fire = allowed;
    scoreboard.set_team_options(&team, options);
    let message = if allowed {
        translations::COMMANDS_TEAM_OPTION_FRIENDLYFIRE_ENABLED
            .message([team.name().to_owned()])
            .component()
    } else {
        translations::COMMANDS_TEAM_OPTION_FRIENDLYFIRE_DISABLED
            .message([team.name().to_owned()])
            .component()
    };
    context.source().send_success(&message, true);
    Ok(0)
}

/// Vanilla parity: `TeamCommand.setFriendlySight`, which refuses a no-op.
fn set_see_friendly_invisibles(
    context: &FotonCommandContext<CommandSource>,
) -> Result<i32, CommandSyntaxError> {
    let team = team(context)?;
    let allowed = context.boolean("allowed")?;
    let scoreboard = source_scoreboard(context)?;
    let mut options = scoreboard.team_options(&team);
    if options.see_friendly_invisibles == allowed {
        return Err(CommandSyntaxError::dynamic(TextComponent::from(
            if allowed {
                &translations::COMMANDS_TEAM_OPTION_SEE_FRIENDLY_INVISIBLES_ALREADY_ENABLED
            } else {
                &translations::COMMANDS_TEAM_OPTION_SEE_FRIENDLY_INVISIBLES_ALREADY_DISABLED
            },
        )));
    }

    options.see_friendly_invisibles = allowed;
    scoreboard.set_team_options(&team, options);
    let message = if allowed {
        translations::COMMANDS_TEAM_OPTION_SEE_FRIENDLY_INVISIBLES_ENABLED
            .message([team.name().to_owned()])
            .component()
    } else {
        translations::COMMANDS_TEAM_OPTION_SEE_FRIENDLY_INVISIBLES_DISABLED
            .message([team.name().to_owned()])
            .component()
    };
    context.source().send_success(&message, true);
    Ok(0)
}
