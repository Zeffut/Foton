//! The `/bug` form, as a server-built dialog.
//!
//! Minecraft has carried a server-driven dialog system since 1.21.6, so a bug
//! form needs no client mod: the server sends the whole screen and the client
//! sends back what was typed. That matters for a public test session, where
//! asking testers to install anything loses most of them.
//!
//! The dialog is built here rather than registered as a datapack entry because
//! it belongs to one command at one moment. `ClientboundShowDialogPacket`
//! carries a `Holder<Dialog>`, and the inline half of that holder is exactly
//! this case.

use foton_protocol::packets::common::CShowDialog;
use foton_utils::Identifier;
use simdnbt::owned::{NbtCompound, NbtList, NbtTag};

use crate::bug_report::{BugCategory, MAX_DESCRIPTION};

/// The action id the submit button carries back.
///
/// Vanilla parity: the `id` of a `minecraft:dynamic/custom` action, which the
/// client returns in `ServerboundCustomClickActionPacket`.
pub const BUG_REPORT_ACTION: &str = "bug_report";

/// The dialog input holding what the reporter typed.
pub const DESCRIPTION_KEY: &str = "description";

/// The dialog input holding the chosen category.
pub const CATEGORY_KEY: &str = "category";

/// A plain text component, in the shape the dialog codec reads.
fn text(value: &str) -> NbtTag {
    let mut compound = NbtCompound::new();
    compound.insert("text", value);
    NbtTag::Compound(compound)
}

/// The category dropdown, one entry per [`BugCategory`].
///
/// The list is closed on purpose: a free-text category is unsortable within a
/// day of a real test session.
fn category_input() -> NbtCompound {
    let mut options = Vec::new();
    for (index, category) in BugCategory::ALL.into_iter().enumerate() {
        let mut option = NbtCompound::new();
        option.insert("id", category.name());
        option.insert("display", text(category.label()));
        // Vanilla rejects a dialog with more than one initial option, so only
        // the first may claim it.
        option.insert("initial", i8::from(index == 0));
        options.push(option);
    }

    let mut input = NbtCompound::new();
    input.insert("key", CATEGORY_KEY);
    input.insert("type", "minecraft:single_option");
    input.insert("label", text("What does it affect?"));
    input.insert("width", 300);
    input.insert("options", NbtList::Compound(options));
    input
}

/// The multi-line box the reproduction steps go in.
fn description_input() -> NbtCompound {
    let mut multiline = NbtCompound::new();
    multiline.insert("max_lines", 12);
    multiline.insert("height", 128);

    let mut input = NbtCompound::new();
    input.insert("key", DESCRIPTION_KEY);
    input.insert("type", "minecraft:text");
    input.insert("label", text("What happened, and how do we see it again?"));
    input.insert("width", 300);
    input.insert(
        "max_length",
        i32::try_from(MAX_DESCRIPTION).unwrap_or(i32::MAX),
    );
    input.insert("multiline", NbtTag::Compound(multiline));
    input
}

/// The submit button, carrying the action that returns the form.
fn submit_button() -> NbtCompound {
    let mut action = NbtCompound::new();
    action.insert("type", "minecraft:dynamic/custom");
    action.insert("id", Identifier::from_foton(BUG_REPORT_ACTION).to_string());

    let mut button = NbtCompound::new();
    button.insert("label", text("Send report"));
    button.insert("width", 150);
    button.insert("action", NbtTag::Compound(action));
    button
}

/// Builds the `/bug` dialog.
#[must_use]
pub fn bug_dialog() -> NbtCompound {
    let mut body = NbtCompound::new();
    body.insert("type", "minecraft:plain_message");
    body.insert(
        "contents",
        text("Where you are, which world, and which build are recorded for you."),
    );

    let mut dialog = NbtCompound::new();
    dialog.insert("type", "minecraft:multi_action");
    dialog.insert("title", text("Report a bug"));
    dialog.insert("can_close_with_escape", 1_i8);
    // A paused dialog freezes the client's own screen but not the server, and
    // on a shared test server that only makes the reporter miss what happens
    // while they type.
    dialog.insert("pause", 0_i8);
    dialog.insert("body", NbtList::Compound(vec![body]));
    dialog.insert(
        "inputs",
        NbtList::Compound(vec![category_input(), description_input()]),
    );
    dialog.insert("actions", NbtList::Compound(vec![submit_button()]));
    dialog.insert("columns", 1);
    dialog
}

/// The packet that puts the form on a player's screen.
#[must_use]
pub fn show_bug_dialog() -> CShowDialog {
    CShowDialog::inline(bug_dialog())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The dialog names every field the handler later reads.
    ///
    /// A dialog whose input keys drift from the handler's still opens, still
    /// accepts typing, and silently files empty reports -- the failure has no
    /// symptom at the point it happens.
    #[test]
    fn the_form_carries_the_keys_the_handler_reads() {
        let dialog = bug_dialog();
        let inputs = dialog.list("inputs").expect("the form has inputs");
        let compounds = inputs.compounds().expect("inputs are compounds");
        let keys: Vec<_> = compounds
            .iter()
            .filter_map(|input| input.string("key").map(ToString::to_string))
            .collect();
        assert!(keys.contains(&CATEGORY_KEY.to_owned()));
        assert!(keys.contains(&DESCRIPTION_KEY.to_owned()));
    }

    /// Exactly one option may be initial; vanilla rejects the dialog otherwise.
    #[test]
    fn exactly_one_category_starts_selected() {
        let input = category_input();
        let options = input.list("options").expect("options");
        let initial = options
            .compounds()
            .expect("compounds")
            .iter()
            .filter(|option| option.byte("initial").is_some_and(|value| value != 0))
            .count();
        assert_eq!(initial, 1, "vanilla refuses a dialog with several initials");
    }

    /// Every category the storage knows is offered by the form.
    #[test]
    fn the_form_offers_every_category() {
        let input = category_input();
        let options = input.list("options").expect("options");
        assert_eq!(
            options.compounds().expect("compounds").len(),
            BugCategory::ALL.len(),
            "a category the form cannot reach may as well not exist"
        );
    }
}
