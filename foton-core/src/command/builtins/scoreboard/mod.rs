//! The `/scoreboard` command.
//!
//! Vanilla parity: `net.minecraft.server.commands.ScoreboardCommand`. It is
//! the only writer of objectives, display slots and most scores, and it works
//! on the same per-domain [`Scoreboard`] that selectors, `execute if score`
//! and `execute store result score` read, so a datapack's counters and the
//! conditions that test them are one state.
//!
//! The objective subcommands are in [`objectives`], the score subcommands in
//! [`players`]; what both share is here.

use foton_protocol::packets::game::NumberFormat;
use foton_utils::{Identifier, translations};
use text_components::{Modifier as _, TextComponent, format::Color};

use super::super::{
    brigadier::{CommandNodeBuilder, CommandSyntaxError},
    execution::{
        CommandSource, CommandTextResolver, FotonArgumentType, FotonCommandContext,
        FotonCommandRuntime, argument, literal,
    },
    registration::CommandRegistration,
};
use crate::scoreboard::{ObjectiveData, Scoreboard, ScoreboardError, ScoreboardObjective};

mod objectives;
mod players;

pub(super) fn registration() -> CommandRegistration<CommandSource> {
    CommandRegistration::new(Identifier::vanilla_static("scoreboard"), |_| command())
}

fn command() -> CommandNodeBuilder<CommandSource, FotonCommandRuntime> {
    literal("scoreboard")
        .then(objectives::subcommands())
        .then(players::subcommands())
}

type Context<'a> = &'a FotonCommandContext<CommandSource>;

/// The scoreboard of the domain the command was run in.
///
/// Vanilla reaches one server-wide scoreboard; Foton keeps one per domain,
/// beside the boss bars and the command storage.
fn source_scoreboard(context: Context<'_>) -> Result<&Scoreboard, CommandSyntaxError> {
    let source = context.source();
    source
        .server()
        .scoreboards
        .get(source.world().domain())
        .ok_or_else(|| {
            CommandSyntaxError::dynamic(format!(
                "Domain '{}' has no command scoreboard",
                source.world().domain()
            ))
        })
}

/// Vanilla parity: `ObjectiveArgument.getObjective`, which fails on an
/// unknown name.
fn objective(context: Context<'_>, name: &str) -> Result<ScoreboardObjective, CommandSyntaxError> {
    let objective_name = context.objective_name(name)?;
    source_scoreboard(context)?
        .objective(objective_name)
        .ok_or_else(|| {
            CommandSyntaxError::dynamic(
                translations::ARGUMENTS_OBJECTIVE_NOT_FOUND
                    .message([objective_name.to_owned()])
                    .component(),
            )
        })
}

/// Vanilla parity: `ObjectiveArgument.getWritableObjective`, which also
/// refuses an objective whose criteria only the game may write.
fn writable_objective(
    context: Context<'_>,
    name: &str,
) -> Result<ScoreboardObjective, CommandSyntaxError> {
    let objective = objective(context, name)?;
    if objective.is_read_only() {
        return Err(CommandSyntaxError::dynamic(
            translations::ARGUMENTS_OBJECTIVE_READONLY
                .message([objective.name().to_owned()])
                .component(),
        ));
    }
    Ok(objective)
}

/// Everything about an objective that [`objective`] just resolved.
fn data(
    context: Context<'_>,
    objective: &ScoreboardObjective,
) -> Result<ObjectiveData, CommandSyntaxError> {
    source_scoreboard(context)?
        .objective_data(objective.name())
        .ok_or_else(|| {
            CommandSyntaxError::dynamic(
                translations::ARGUMENTS_OBJECTIVE_NOT_FOUND
                    .message([objective.name().to_owned()])
                    .component(),
            )
        })
}

/// Vanilla parity: `ComponentArgument.getResolvedComponent`.
fn resolved_component(
    context: Context<'_>,
    name: &str,
) -> Result<TextComponent, CommandSyntaxError> {
    context
        .text_component(name)?
        .try_resolve(&CommandTextResolver::new(context.source()))
}

/// Vanilla parity: `ComponentUtils.formatList` with its default separator.
fn format_list(pieces: Vec<TextComponent>) -> TextComponent {
    let mut joined = Vec::with_capacity(pieces.len().saturating_mul(2));
    for (index, piece) in pieces.into_iter().enumerate() {
        if index > 0 {
            joined.push(TextComponent::plain(", ").color(Color::Gray));
        }
        joined.push(piece);
    }
    TextComponent::new().add_children(joined)
}

fn text(value: impl Into<String>) -> TextComponent {
    TextComponent::plain(value.into())
}

fn count(value: usize) -> i32 {
    i32::try_from(value).unwrap_or(i32::MAX)
}

/// A scoreboard refusal reported to the command's user.
///
/// The objective was resolved moments earlier, so these only occur when
/// another thread removed it in between.
fn scoreboard_error(error: ScoreboardError) -> CommandSyntaxError {
    CommandSyntaxError::dynamic(error.to_string())
}

/// Vanilla parity: `ScoreboardCommand.addNumberFormats` -- `blank`, `fixed
/// <contents>`, `styled <style>`, and the bare node, which clears the format.
fn number_formats(
    top: CommandNodeBuilder<CommandSource, FotonCommandRuntime>,
    callback: fn(Context<'_>, Option<NumberFormat>) -> Result<i32, CommandSyntaxError>,
) -> CommandNodeBuilder<CommandSource, FotonCommandRuntime> {
    top.then(literal("blank").executes(move |context| callback(context, Some(NumberFormat::Blank))))
        .then(literal("fixed").then(
            argument("contents", FotonArgumentType::component()).executes(move |context| {
                let contents = resolved_component(context, "contents")?;
                callback(context, Some(NumberFormat::Fixed(contents)))
            }),
        ))
        .then(
            literal("styled").then(argument("style", FotonArgumentType::style()).executes(
                move |context| {
                    let style = context.style("style")?.clone();
                    callback(context, Some(NumberFormat::Styled(style)))
                },
            )),
        )
        .executes(move |context| callback(context, None))
}
