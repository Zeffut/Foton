//! The `/bossbar` command.
//!
//! Vanilla parity: `net.minecraft.server.commands.BossBarCommands`. It is the
//! only way to put a bar on a player's screen that no boss is behind, and the
//! only reader and writer of the named bars a domain persists.
//!
//! Every `set` refuses a change that would change nothing. That is not
//! decoration: `/bossbar set <id> value 5` returning its value makes it a
//! useful `execute store` source, and a no-op that quietly reported success
//! would make a datapack loop think it had made progress.

use std::sync::Arc;

use steel_protocol::packets::game::{BossBarColor, BossBarOverlay};
use steel_utils::{Identifier, translations};
use text_components::{Modifier as _, TextComponent, translation::Translation};

use super::super::{
    brigadier::{ArgumentType, CommandNodeBuilder, CommandSyntaxError},
    execution::{
        CommandSource, SteelArgumentType, SteelCommandContext, SteelCommandRuntime, argument,
        literal,
    },
    registration::CommandRegistration,
};
use crate::boss_event::custom::{CustomBossEvent, CustomBossEvents};
use crate::entity::Entity as _;
use crate::player::Player;

pub(super) fn registration() -> CommandRegistration<CommandSource> {
    CommandRegistration::new(Identifier::vanilla_static("bossbar"), |_| command())
}

fn command() -> CommandNodeBuilder<CommandSource, SteelCommandRuntime> {
    literal("bossbar")
        .then(
            literal("add").then(
                argument("id", SteelArgumentType::boss_bar_id())
                    .then(argument("name", SteelArgumentType::component()).executes(create_bar)),
            ),
        )
        .then(
            literal("remove")
                .then(argument("id", SteelArgumentType::boss_bar_id()).executes(remove_bar)),
        )
        .then(literal("list").executes(list_bars))
        .then(literal("set").then(set_subcommands()))
        .then(
            literal("get").then(
                argument("id", SteelArgumentType::boss_bar_id())
                    .then(literal("value").executes(get_value))
                    .then(literal("max").executes(get_max))
                    .then(literal("visible").executes(get_visible))
                    .then(literal("players").executes(get_players)),
            ),
        )
}

fn set_subcommands() -> CommandNodeBuilder<CommandSource, SteelCommandRuntime> {
    let mut colors = literal("color");
    for color in BossBarColor::VALUES {
        colors = colors.then(
            literal(color.serialized_name()).executes(move |context| set_color(context, color)),
        );
    }
    let mut styles = literal("style");
    for overlay in BossBarOverlay::VALUES {
        styles = styles.then(
            literal(overlay.serialized_name()).executes(move |context| set_style(context, overlay)),
        );
    }

    argument("id", SteelArgumentType::boss_bar_id())
        .then(
            literal("name")
                .then(argument("name", SteelArgumentType::component()).executes(set_name)),
        )
        .then(colors)
        .then(styles)
        .then(
            literal("value")
                .then(argument("value", ArgumentType::integer(0, i32::MAX)).executes(set_value)),
        )
        .then(
            literal("max")
                .then(argument("max", ArgumentType::integer(1, i32::MAX)).executes(set_max)),
        )
        .then(
            literal("visible")
                .then(argument("visible", ArgumentType::bool()).executes(set_visible)),
        )
        .then(
            literal("players")
                .executes(|context| set_players(context, &[]))
                .then(
                    argument("targets", SteelArgumentType::players()).executes(|context| {
                        let targets = context.optional_players("targets")?;
                        set_players(context, &targets)
                    }),
                ),
        )
}

/// The boss bars of the domain the command was run in.
///
/// Vanilla reaches one server-wide collection. Steel keeps them per domain,
/// beside the scoreboards and the command storage `execute store` addresses
/// the same way.
pub(crate) fn source_boss_bars(
    context: &SteelCommandContext<CommandSource>,
) -> Result<&CustomBossEvents, CommandSyntaxError> {
    let source = context.source();
    source
        .server()
        .boss_bars
        .get(source.world().domain())
        .ok_or_else(|| {
            CommandSyntaxError::dynamic(format!(
                "Domain '{}' has no boss bars",
                source.world().domain()
            ))
        })
}

/// Vanilla parity: `BossBarCommands.getBossBar`.
pub(crate) fn boss_bar(
    context: &SteelCommandContext<CommandSource>,
) -> Result<Arc<CustomBossEvent>, CommandSyntaxError> {
    let id = context.boss_bar_id("id")?.clone();
    source_boss_bars(context)?
        .get(&id)
        .ok_or_else(|| unknown_bar(&id))
}

fn unknown_bar(id: &Identifier) -> CommandSyntaxError {
    CommandSyntaxError::dynamic(
        translations::COMMANDS_BOSSBAR_UNKNOWN
            .message([id.to_string()])
            .component(),
    )
}

fn unchanged(translation: &'static Translation<0>) -> CommandSyntaxError {
    CommandSyntaxError::dynamic(TextComponent::from(translation))
}

/// Vanilla parity: `BossBarCommands.createBar`.
fn create_bar(context: &SteelCommandContext<CommandSource>) -> Result<i32, CommandSyntaxError> {
    let id = context.boss_bar_id("id")?.clone();
    let name = context.text_component("name")?.clone();
    let events = source_boss_bars(context)?;
    if events.get(&id).is_some() {
        return Err(CommandSyntaxError::dynamic(
            translations::COMMANDS_BOSSBAR_CREATE_FAILED
                .message([id.to_string()])
                .component(),
        ));
    }

    let bar = events.create(id, name);
    let message = translations::COMMANDS_BOSSBAR_CREATE_SUCCESS
        .message([bar.display_name()])
        .component();
    context.source().send_success(&message, true);
    Ok(i32::try_from(events.ids().len()).unwrap_or(i32::MAX))
}

/// Vanilla parity: `BossBarCommands.removeBar`.
fn remove_bar(context: &SteelCommandContext<CommandSource>) -> Result<i32, CommandSyntaxError> {
    let bar = boss_bar(context)?;
    let events = source_boss_bars(context)?;
    bar.remove_all_players();
    events.remove(bar.custom_id());
    let message = translations::COMMANDS_BOSSBAR_REMOVE_SUCCESS
        .message([bar.display_name()])
        .component();
    context.source().send_success(&message, true);
    Ok(i32::try_from(events.ids().len()).unwrap_or(i32::MAX))
}

/// Vanilla parity: `BossBarCommands.listBars`.
fn list_bars(context: &SteelCommandContext<CommandSource>) -> Result<i32, CommandSyntaxError> {
    let bars = source_boss_bars(context)?.events();
    let message = if bars.is_empty() {
        TextComponent::from(&translations::COMMANDS_BOSSBAR_LIST_BARS_NONE)
    } else {
        translations::COMMANDS_BOSSBAR_LIST_BARS_SOME
            .message([
                TextComponent::plain(bars.len().to_string()),
                format_list(bars.iter().map(|bar| bar.display_name()).collect()),
            ])
            .component()
    };
    context.source().send_success(&message, false);
    Ok(i32::try_from(bars.len()).unwrap_or(i32::MAX))
}

/// Vanilla parity: `BossBarCommands.getValue`.
fn get_value(context: &SteelCommandContext<CommandSource>) -> Result<i32, CommandSyntaxError> {
    let bar = boss_bar(context)?;
    let message = translations::COMMANDS_BOSSBAR_GET_VALUE
        .message([
            bar.display_name(),
            TextComponent::plain(bar.value().to_string()),
        ])
        .component();
    context.source().send_success(&message, true);
    Ok(bar.value())
}

/// Vanilla parity: `BossBarCommands.getMax`.
fn get_max(context: &SteelCommandContext<CommandSource>) -> Result<i32, CommandSyntaxError> {
    let bar = boss_bar(context)?;
    let message = translations::COMMANDS_BOSSBAR_GET_MAX
        .message([
            bar.display_name(),
            TextComponent::plain(bar.max().to_string()),
        ])
        .component();
    context.source().send_success(&message, true);
    Ok(bar.max())
}

/// Vanilla parity: `BossBarCommands.getVisible`.
fn get_visible(context: &SteelCommandContext<CommandSource>) -> Result<i32, CommandSyntaxError> {
    let bar = boss_bar(context)?;
    let visible = bar.event().is_visible();
    let translation = if visible {
        &translations::COMMANDS_BOSSBAR_GET_VISIBLE_VISIBLE
    } else {
        &translations::COMMANDS_BOSSBAR_GET_VISIBLE_HIDDEN
    };
    let message = translation.message([bar.display_name()]).component();
    context.source().send_success(&message, true);
    Ok(i32::from(visible))
}

/// Vanilla parity: `BossBarCommands.getPlayers`.
fn get_players(context: &SteelCommandContext<CommandSource>) -> Result<i32, CommandSyntaxError> {
    let bar = boss_bar(context)?;
    let viewers = bar.event().players();
    let message = if viewers.is_empty() {
        translations::COMMANDS_BOSSBAR_GET_PLAYERS_NONE
            .message([bar.display_name()])
            .component()
    } else {
        translations::COMMANDS_BOSSBAR_GET_PLAYERS_SOME
            .message([
                bar.display_name(),
                TextComponent::plain(viewers.len().to_string()),
                player_list(&viewers),
            ])
            .component()
    };
    context.source().send_success(&message, true);
    Ok(i32::try_from(viewers.len()).unwrap_or(i32::MAX))
}

/// Vanilla parity: `BossBarCommands.setName`.
fn set_name(context: &SteelCommandContext<CommandSource>) -> Result<i32, CommandSyntaxError> {
    let bar = boss_bar(context)?;
    let name = context.text_component("name")?.clone();
    if bar.event().name() == name {
        return Err(unchanged(
            &translations::COMMANDS_BOSSBAR_SET_NAME_UNCHANGED,
        ));
    }

    bar.event().set_name(name);
    let message = translations::COMMANDS_BOSSBAR_SET_NAME_SUCCESS
        .message([bar.display_name()])
        .component();
    context.source().send_success(&message, true);
    Ok(0)
}

/// Vanilla parity: `BossBarCommands.setColor`.
fn set_color(
    context: &SteelCommandContext<CommandSource>,
    color: BossBarColor,
) -> Result<i32, CommandSyntaxError> {
    let bar = boss_bar(context)?;
    if bar.event().color() == color {
        return Err(unchanged(
            &translations::COMMANDS_BOSSBAR_SET_COLOR_UNCHANGED,
        ));
    }

    bar.event().set_color(color);
    let message = translations::COMMANDS_BOSSBAR_SET_COLOR_SUCCESS
        .message([bar.display_name()])
        .component();
    context.source().send_success(&message, true);
    Ok(0)
}

/// Vanilla parity: `BossBarCommands.setStyle`.
fn set_style(
    context: &SteelCommandContext<CommandSource>,
    overlay: BossBarOverlay,
) -> Result<i32, CommandSyntaxError> {
    let bar = boss_bar(context)?;
    if bar.event().overlay() == overlay {
        return Err(unchanged(
            &translations::COMMANDS_BOSSBAR_SET_STYLE_UNCHANGED,
        ));
    }

    bar.event().set_overlay(overlay);
    let message = translations::COMMANDS_BOSSBAR_SET_STYLE_SUCCESS
        .message([bar.display_name()])
        .component();
    context.source().send_success(&message, true);
    Ok(0)
}

/// Vanilla parity: `BossBarCommands.setValue`.
fn set_value(context: &SteelCommandContext<CommandSource>) -> Result<i32, CommandSyntaxError> {
    let bar = boss_bar(context)?;
    let value = context.integer("value")?;
    if bar.value() == value {
        return Err(unchanged(
            &translations::COMMANDS_BOSSBAR_SET_VALUE_UNCHANGED,
        ));
    }

    bar.set_value(value);
    let message = translations::COMMANDS_BOSSBAR_SET_VALUE_SUCCESS
        .message([bar.display_name(), TextComponent::plain(value.to_string())])
        .component();
    context.source().send_success(&message, true);
    Ok(value)
}

/// Vanilla parity: `BossBarCommands.setMax`.
fn set_max(context: &SteelCommandContext<CommandSource>) -> Result<i32, CommandSyntaxError> {
    let bar = boss_bar(context)?;
    let max = context.integer("max")?;
    if bar.max() == max {
        return Err(unchanged(&translations::COMMANDS_BOSSBAR_SET_MAX_UNCHANGED));
    }

    bar.set_max(max);
    let message = translations::COMMANDS_BOSSBAR_SET_MAX_SUCCESS
        .message([bar.display_name(), TextComponent::plain(max.to_string())])
        .component();
    context.source().send_success(&message, true);
    Ok(max)
}

/// Vanilla parity: `BossBarCommands.setVisible`.
fn set_visible(context: &SteelCommandContext<CommandSource>) -> Result<i32, CommandSyntaxError> {
    let bar = boss_bar(context)?;
    let visible = context.boolean("visible")?;
    if bar.event().is_visible() == visible {
        return Err(unchanged(if visible {
            &translations::COMMANDS_BOSSBAR_SET_VISIBILITY_UNCHANGED_VISIBLE
        } else {
            &translations::COMMANDS_BOSSBAR_SET_VISIBILITY_UNCHANGED_HIDDEN
        }));
    }

    bar.event().set_visible(visible);
    let translation = if visible {
        &translations::COMMANDS_BOSSBAR_SET_VISIBLE_SUCCESS_VISIBLE
    } else {
        &translations::COMMANDS_BOSSBAR_SET_VISIBLE_SUCCESS_HIDDEN
    };
    let message = translation.message([bar.display_name()]).component();
    context.source().send_success(&message, true);
    // Vanilla returns zero either way; the bar's visibility is not a count.
    Ok(0)
}

/// Vanilla parity: `BossBarCommands.setPlayers`.
fn set_players(
    context: &SteelCommandContext<CommandSource>,
    targets: &[Arc<Player>],
) -> Result<i32, CommandSyntaxError> {
    let bar = boss_bar(context)?;
    if !bar.set_players(targets) {
        return Err(unchanged(
            &translations::COMMANDS_BOSSBAR_SET_PLAYERS_UNCHANGED,
        ));
    }

    let message = if targets.is_empty() {
        translations::COMMANDS_BOSSBAR_SET_PLAYERS_SUCCESS_NONE
            .message([bar.display_name()])
            .component()
    } else {
        translations::COMMANDS_BOSSBAR_SET_PLAYERS_SUCCESS_SOME
            .message([
                bar.display_name(),
                TextComponent::plain(targets.len().to_string()),
                player_list(targets),
            ])
            .component()
    };
    context.source().send_success(&message, true);
    Ok(i32::try_from(targets.len()).unwrap_or(i32::MAX))
}

/// Vanilla parity: `ComponentUtils.formatList` with its default separator.
///
/// The pieces are components rather than strings because a bar's display name
/// carries a hover and a shift-click insertion, and flattening them to text
/// would throw both away.
fn format_list(pieces: Vec<TextComponent>) -> TextComponent {
    let mut joined = Vec::with_capacity(pieces.len().saturating_mul(2));
    for (index, piece) in pieces.into_iter().enumerate() {
        if index > 0 {
            joined.push(TextComponent::plain(", "));
        }
        joined.push(piece);
    }
    TextComponent::new().add_children(joined)
}

fn player_list(players: &[Arc<Player>]) -> TextComponent {
    format_list(players.iter().map(|player| player.display_name()).collect())
}
