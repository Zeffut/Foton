//! Code generation for the parity ledger.
//!
//! Every class in `classes.json` that no behavior struct claims is a piece of
//! vanilla the server does not implement. The generator already skips those
//! silently, which is how all three furnaces went unregistered: the behavior
//! was written, a macro hid the struct from the scanner, and nothing said so.
//!
//! This writes the skipped classes down so a test can compare them against a
//! committed ledger. A gap that opens shows up in a diff, and one that closes
//! has to be crossed off deliberately.

use std::collections::BTreeSet;
use std::fmt::Write as _;

use crate::common::scan_object_behaviors;
use crate::{blocks::BlockClass, entities::EntityClass, items::ItemClass};

fn unclaimed<'a>(
    classes: impl Iterator<Item = &'a str>,
    folder: &str,
    attribute: &str,
) -> Vec<String> {
    let discovered = scan_object_behaviors(folder, attribute);
    let mut missing: BTreeSet<String> = BTreeSet::new();
    for class in classes {
        if !discovered.contains_key(class) {
            missing.insert(class.to_owned());
        }
    }
    missing.into_iter().collect()
}

fn list(name: &str, doc: &str, values: &[String]) -> String {
    let mut entries = String::new();
    for value in values {
        writeln!(entries, "    \"{value}\",").expect("writing to a String cannot fail");
    }
    format!("/// {doc}\npub const {name}: &[&str] = &[\n{entries}];\n\n")
}

pub fn build(blocks: &[BlockClass], items: &[ItemClass], entities: &[EntityClass]) -> String {
    let block_gaps = unclaimed(
        blocks.iter().map(|block| block.class.as_str()),
        "blocks",
        "block_behavior",
    );
    let item_gaps = unclaimed(
        items.iter().map(|item| item.class.as_str()),
        "items",
        "item_behavior",
    );
    let entity_gaps = unclaimed(
        entities.iter().map(|entity| entity.class.as_str()),
        "entities",
        "entity_behavior",
    );

    let mut out = String::from(
        "//! Generated parity ledger: vanilla classes with no behavior.\n\
         //!\n\
         //! Compared against `dev/parity-gaps.txt` by a test. Do not edit.\n\n",
    );
    out.push_str(&list(
        "UNCLAIMED_BLOCK_CLASSES",
        "Vanilla block classes no behavior struct claims.",
        &block_gaps,
    ));
    out.push_str(&list(
        "UNCLAIMED_ITEM_CLASSES",
        "Vanilla item classes no behavior struct claims.",
        &item_gaps,
    ));
    out.push_str(&list(
        "UNCLAIMED_ENTITY_CLASSES",
        "Vanilla entity classes no behavior struct claims.",
        &entity_gaps,
    ));
    out
}
