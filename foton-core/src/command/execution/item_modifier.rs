//! `/item` modifier arguments.
//!
//! Vanilla parity: `ResourceOrIdArgument.lootModifier`, which reads either an
//! id or an inline value written as SNBT.

use std::sync::Arc;

use foton_utils::{Identifier, nbt::parse_snbt_argument, translations};
use text_components::TextComponent;

use super::argument::{identifier_matches, parse_identifier};
use super::source::CommandArgumentSource;
use crate::command::brigadier::{
    CommandSyntaxError, CommandSyntaxErrorKind, StringReader, SuggestionsBuilder,
};
use crate::item_modifier::{self, ItemModifier};

/// A parsed modifier argument.
///
/// A named modifier is resolved when the command runs rather than when it
/// parses: a function is compiled before the datapack's modifiers are
/// installed, so a parse-time lookup would reject every function that names one.
#[derive(Clone, Debug)]
pub(crate) enum ItemModifierArgument {
    Reference(Identifier),
    Inline(Arc<ItemModifier>),
}

pub(super) fn parse_item_modifier(
    reader: &mut StringReader<'_>,
) -> Result<ItemModifierArgument, CommandSyntaxError> {
    if !matches!(reader.peek(), Some('{' | '[')) {
        return parse_identifier(reader).map(ItemModifierArgument::Reference);
    }
    let (tag, consumed) = match parse_snbt_argument(reader.remaining()) {
        Ok(parsed) => parsed,
        Err(error) => {
            let _ = reader.advance_bytes(error.cursor());
            return Err(reader.error(CommandSyntaxErrorKind::Dynamic(Box::new(error.component()))));
        }
    };
    let modifier = item_modifier::from_nbt(&tag).map_err(|error| {
        reader.error(CommandSyntaxErrorKind::Dynamic(Box::new(
            translations::ARGUMENT_RESOURCE_OR_ID_FAILED_TO_PARSE
                .message([TextComponent::plain(error)])
                .component(),
        )))
    })?;
    let _ = reader.advance_bytes(consumed);
    Ok(ItemModifierArgument::Inline(Arc::new(modifier)))
}

pub(super) fn suggest_item_modifiers<S>(source: &S, builder: &mut SuggestionsBuilder<'_>)
where
    S: CommandArgumentSource + ?Sized,
{
    let remaining = builder.remaining_lowercase().to_owned();
    let names = source
        .item_modifier_names()
        .into_iter()
        .filter_map(|name| name.parse::<Identifier>().ok())
        .filter(|name| identifier_matches(&remaining, name))
        .map(|name| name.to_string())
        .collect::<Vec<_>>();
    for name in names {
        builder.suggest(name);
    }
}
