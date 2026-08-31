//! The `/bug` command: a tester files a report without leaving the game.
//!
//! Not vanilla. A test session's value is what the testers noticed, and most of
//! it is lost in the gap between noticing and writing it down somewhere else.
//! So the report is filed where the player is standing, and everything that can
//! be captured without asking -- who, where, which world, which build -- is
//! captured rather than typed.
//!
//! The category is a set of literals rather than a free string so that the
//! client offers them as completions and the file stays sortable.

use std::env::current_dir;
use std::path::PathBuf;

use foton_utils::Identifier;
use text_components::{Modifier as _, TextComponent, format::Color};

use super::super::{
    brigadier::{ArgumentType, CommandNodeBuilder, CommandSyntaxError},
    execution::{CommandSource, FotonCommandContext, FotonCommandRuntime, argument, literal},
    registration::CommandRegistration,
};
use crate::bug_dialog::show_bug_dialog;
use crate::bug_report::{BugCategory, BugReport, MAX_DESCRIPTION, forward};

/// How many reports `/bug list` shows.
const LIST_LIMIT: usize = 10;

pub(super) fn registration() -> CommandRegistration<CommandSource> {
    // A tester types the word that comes to mind. One that lands on "unknown
    // command" rarely gets retried with a synonym -- the report is simply lost,
    // at the moment it was easiest to write.
    CommandRegistration::new(Identifier::from_foton("bug"), |_| command())
        .alias("report")
        .alias("bugreport")
}

fn command() -> CommandNodeBuilder<CommandSource, FotonCommandRuntime> {
    let mut node = literal("bug")
        // Bare `/bug` opens the form. The typed form below stays: it is the
        // only way to file from a console or a command block, and the only one
        // that works if a client ever refuses the dialog.
        .executes(open_form)
        .then(literal("list").executes(list_reports));
    for category in BugCategory::ALL {
        node = node.then(
            literal(category.name()).then(
                argument("description", ArgumentType::greedy_string())
                    .executes(move |context| file_report(context, category)),
            ),
        );
    }
    node
}

/// Puts the report form on the caller's screen.
#[expect(
    clippy::unnecessary_wraps,
    reason = "Command executors share a fallible callback signature."
)]
fn open_form(context: &FotonCommandContext<CommandSource>) -> Result<i32, CommandSyntaxError> {
    let source = context.source();
    let Some(player) = source.player() else {
        source.send_failure(
            TextComponent::from("Only a player can open the form; use /bug <category> <text>.")
                .color(Color::Red),
        );
        return Ok(0);
    };
    player.send_packet(show_bug_dialog());
    Ok(1)
}

fn file_report(
    context: &FotonCommandContext<CommandSource>,
    category: BugCategory,
) -> Result<i32, CommandSyntaxError> {
    let description = context.string("description")?.trim().to_owned();
    if description.is_empty() {
        context
            .source()
            .send_failure(TextComponent::from("Describe the bug first.").color(Color::Red));
        return Ok(0);
    }
    if description.len() > MAX_DESCRIPTION {
        context.source().send_failure(
            TextComponent::from(format!(
                "That is longer than {MAX_DESCRIPTION} characters. Trim it and file again."
            ))
            .color(Color::Red),
        );
        return Ok(0);
    }

    let source = context.source();
    let Some(player) = source.player() else {
        source.send_failure(
            TextComponent::from(
                "Only a player can file a bug: a report needs a place to point at.",
            )
            .color(Color::Red),
        );
        return Ok(0);
    };

    let position = source.position();
    let report = BugReport::now(
        player.gameprofile.name.clone(),
        player.gameprofile.id.to_string(),
        source.world().key.to_string(),
        [position.x, position.y, position.z],
        category,
        description,
    );

    let run_dir = current_dir().unwrap_or_else(|_| PathBuf::from("."));
    match report.append_in(&run_dir) {
        Ok(number) => {
            if let Some(webhook) = player.config.bug_report_webhook.as_ref() {
                forward(webhook, &report, number);
            }
            source.send_success(
                &TextComponent::from(format!("Filed report #{number}. Thanks."))
                    .color(Color::Green),
                // Every operator online should see a report land: on a test
                // session that is the whole point of running one.
                true,
            );
            Ok(number.try_into().unwrap_or(i32::MAX))
        }
        Err(error) => {
            log::error!("failed to write a bug report: {error}");
            source.send_failure(
                TextComponent::from("The report could not be saved. Tell an operator.")
                    .color(Color::Red),
            );
            Ok(0)
        }
    }
}

#[expect(
    clippy::unnecessary_wraps,
    reason = "Command executors share a fallible callback signature."
)]
fn list_reports(context: &FotonCommandContext<CommandSource>) -> Result<i32, CommandSyntaxError> {
    let run_dir = current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let reports = BugReport::read_all(&run_dir).unwrap_or_default();
    let source = context.source();

    if reports.is_empty() {
        source.send_success(
            &TextComponent::from("No reports filed yet.").color(Color::Gray),
            false,
        );
        return Ok(0);
    }

    let total = reports.len();
    let shown = reports.len().saturating_sub(LIST_LIMIT);
    source.send_success(
        &TextComponent::from(format!("{total} report(s), most recent last:")).color(Color::Gray),
        false,
    );
    for (index, report) in reports.iter().enumerate().skip(shown) {
        source.send_success(
            &TextComponent::from(format!(
                "#{} [{}] {} - {}",
                index + 1,
                report.category.name(),
                report.player,
                first_line(&report.description),
            )),
            false,
        );
    }
    Ok(total.try_into().unwrap_or(i32::MAX))
}

/// The first line of a description, shortened for a chat listing.
fn first_line(description: &str) -> String {
    let line = description.lines().next().unwrap_or_default();
    if line.chars().count() <= 60 {
        return line.to_owned();
    }
    let mut short: String = line.chars().take(57).collect();
    short.push_str("...");
    short
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_long_first_line_is_shortened_rather_than_cut_mid_character() {
        let long = "e".repeat(200);
        let shortened = first_line(&long);
        assert_eq!(shortened.chars().count(), 60);
        assert!(shortened.ends_with("..."));
    }

    #[test]
    fn only_the_first_line_reaches_a_chat_listing() {
        assert_eq!(first_line("first\nsecond\nthird"), "first");
    }

    /// Multi-byte text must not be sliced through a character.
    #[test]
    fn shortening_counts_characters_not_bytes() {
        let accented = "é".repeat(200);
        let shortened = first_line(&accented);
        assert_eq!(shortened.chars().count(), 60);
    }
}
