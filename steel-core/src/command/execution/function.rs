//! `/function` name arguments.

use steel_utils::Identifier;

use super::argument::{identifier_matches, parse_identifier};
use super::source::CommandArgumentSource;
use crate::command::brigadier::{CommandSyntaxError, StringReader, SuggestionsBuilder};

/// A parsed function name: one function, or a whole function tag.
///
/// Vanilla parity: `FunctionArgument.Result`. The name is only read here; which
/// functions it stands for is resolved when the command runs, so a function may
/// call one that is loaded after it and a reload is always picked up.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum FunctionOrTag {
    Function(Identifier),
    Tag(Identifier),
}

pub(super) fn parse_function_or_tag(
    reader: &mut StringReader<'_>,
) -> Result<FunctionOrTag, CommandSyntaxError> {
    if reader.peek() != Some('#') {
        return parse_identifier(reader).map(FunctionOrTag::Function);
    }

    let start = reader.checkpoint();
    reader.skip();
    match parse_identifier(reader) {
        Ok(key) => Ok(FunctionOrTag::Tag(key)),
        Err(error) => {
            reader.restore(start);
            Err(error)
        }
    }
}

pub(super) fn suggest_functions<S>(source: &S, builder: &mut SuggestionsBuilder<'_>)
where
    S: CommandArgumentSource + ?Sized,
{
    let remaining = builder.remaining_lowercase().to_owned();
    let suggestions = if let Some(tag_prefix) = remaining.strip_prefix('#') {
        source
            .command_function_tag_names()
            .into_iter()
            .filter_map(|tag| tag.parse::<Identifier>().ok())
            .filter(|tag| identifier_matches(tag_prefix, tag))
            .map(|tag| format!("#{tag}"))
            .collect::<Vec<_>>()
    } else {
        source
            .command_function_names()
            .into_iter()
            .filter_map(|name| name.parse::<Identifier>().ok())
            .filter(|name| identifier_matches(&remaining, name))
            .map(|name| name.to_string())
            .chain(
                source
                    .command_function_tag_names()
                    .into_iter()
                    .filter_map(|tag| tag.parse::<Identifier>().ok())
                    .filter(|tag| identifier_matches(&remaining, tag))
                    .map(|tag| format!("#{tag}")),
            )
            .collect()
    };
    for suggestion in suggestions {
        builder.suggest(suggestion);
    }
}
