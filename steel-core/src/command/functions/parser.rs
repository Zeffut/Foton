//! `.mcfunction` source parsing.
//!
//! Vanilla parity: `CommandFunction.fromLines`. Every line is parsed once at
//! load time and kept as an unbound action, so a call only has to bind its
//! source; a syntax error is reported by the load rather than by the call.

use std::{fmt, sync::Arc};

use steel_utils::Identifier;

use super::super::brigadier::CommandDispatcher;
use super::super::execution::{
    CommandSource, ExecutionCommandSource, FunctionEntries, SteelCommandRuntime, UnboundCommand,
    UnboundEntryAction,
};
use super::library::CommandFunction;

/// Vanilla's per-line length ceiling, applied to joined continuation lines too.
const MAX_COMMAND_LINE_LENGTH: usize = 2_000_000;

/// A `.mcfunction` line that could not be compiled.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct FunctionParseError {
    line: usize,
    message: String,
}

impl FunctionParseError {
    fn new(line: usize, message: impl Into<String>) -> Self {
        Self {
            line,
            message: message.into(),
        }
    }

    #[cfg(test)]
    pub(super) const fn line(&self) -> usize {
        self.line
    }
}

impl fmt::Display for FunctionParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "line {}: {}", self.line, self.message)
    }
}

/// Compiles one `.mcfunction` file into a callable function.
pub(crate) fn parse_function(
    id: Identifier,
    source: &str,
    dispatcher: &CommandDispatcher<CommandSource, SteelCommandRuntime>,
    compilation_source: &CommandSource,
) -> Result<CommandFunction, FunctionParseError> {
    let entries = parse_entries(source, dispatcher, compilation_source)?;
    Ok(CommandFunction::new(id, entries))
}

/// Compiles `.mcfunction` text into one unbound action per command line.
pub(crate) fn parse_entries<S>(
    source: &str,
    dispatcher: &CommandDispatcher<S, SteelCommandRuntime>,
    compilation_source: &S,
) -> Result<FunctionEntries<S>, FunctionParseError>
where
    S: ExecutionCommandSource + Clone,
{
    let lines: Vec<&str> = source.lines().collect();
    let mut entries: Vec<Arc<dyn UnboundEntryAction<S>>> = Vec::new();
    let mut index = 0;

    while index < lines.len() {
        let line_number = index + 1;
        let joined = join_continuations(&lines, &mut index, line_number)?;
        let line = joined.trim();
        index += 1;

        let Some(first) = line.chars().next() else {
            continue;
        };
        if first == '#' {
            continue;
        }
        if first == '/' {
            return Err(slash_prefix_error(line_number, line));
        }
        if first == '$' {
            return Err(FunctionParseError::new(
                line_number,
                "macro lines are not supported yet; \
                 remove the leading '$' or move the line into a plain function",
            ));
        }

        let chain = {
            let parse = dispatcher.parse(line, compilation_source.clone());
            dispatcher.context_chain(parse)
        };
        match chain {
            Ok(chain) => entries.push(Arc::new(UnboundCommand::new(chain))),
            Err(error) => {
                return Err(FunctionParseError::new(
                    line_number,
                    format!("whilst parsing command: {error}"),
                ));
            }
        }
    }

    Ok(entries.into())
}

/// Joins a `\`-terminated line with the lines it continues onto.
///
/// `index` is left on the last line consumed so the caller can advance past it.
fn join_continuations(
    lines: &[&str],
    index: &mut usize,
    line_number: usize,
) -> Result<String, FunctionParseError> {
    let mut joined = lines[*index].trim().to_owned();
    if !ends_with_continuation(&joined) {
        check_line_length(&joined, line_number)?;
        return Ok(joined);
    }

    while ends_with_continuation(&joined) {
        *index += 1;
        let Some(next) = lines.get(*index) else {
            return Err(FunctionParseError::new(
                line_number,
                "line continuation at end of file",
            ));
        };
        joined.pop();
        joined.push_str(next.trim());
        check_line_length(&joined, line_number)?;
    }
    Ok(joined)
}

fn ends_with_continuation(line: &str) -> bool {
    line.ends_with('\\')
}

fn check_line_length(line: &str, line_number: usize) -> Result<(), FunctionParseError> {
    if line.len() > MAX_COMMAND_LINE_LENGTH {
        return Err(FunctionParseError::new(
            line_number,
            format!("command too long: {} characters", line.len()),
        ));
    }
    Ok(())
}

fn slash_prefix_error(line_number: usize, line: &str) -> FunctionParseError {
    let rest = &line[1..];
    if rest.starts_with('/') {
        return FunctionParseError::new(
            line_number,
            format!(
                "unknown or invalid command '{line}' \
                 (if you intended to make a comment, use '#' not '//')"
            ),
        );
    }
    let name: String = rest
        .chars()
        .take_while(|character| !character.is_whitespace())
        .collect();
    FunctionParseError::new(
        line_number,
        format!(
            "unknown or invalid command '{line}' \
             (did you mean '{name}'? Do not use a preceding forwards slash.)"
        ),
    )
}
