//! `.mcfunction` source parsing.
//!
//! Vanilla parity: `CommandFunction.fromLines` and `FunctionBuilder`. An
//! ordinary line is parsed once at load time and kept as an unbound action, so
//! a call only has to bind its source and a syntax error is reported by the
//! load rather than by the call. A `$` line cannot be parsed that early: it
//! becomes a template, and the whole file becomes a macro function.

use std::{fmt, sync::Arc};

use foton_utils::Identifier;

use super::super::brigadier::{CommandDispatcher, CommandSyntaxError};
use super::super::execution::{
    CommandSource, ExecutionCommandSource, FotonCommandRuntime, FunctionEntries, UnboundCommand,
    UnboundEntryAction,
};
use super::library::CommandFunction;
use super::macros::{MacroEntry, MacroFunction, StringTemplate};

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

/// A compiled function body, which is only ready to run if it has no macros.
pub(crate) enum FunctionBody<S>
where
    S: ExecutionCommandSource,
{
    Plain(FunctionEntries<S>),
    Macro(MacroFunction<S>),
}

/// Compiles one `.mcfunction` file into a callable function.
pub(crate) fn parse_function(
    id: Identifier,
    source: &str,
    dispatcher: &CommandDispatcher<CommandSource, FotonCommandRuntime>,
    compilation_source: &CommandSource,
) -> Result<CommandFunction, FunctionParseError> {
    let body = parse_body(source, dispatcher, compilation_source)?;
    Ok(CommandFunction::new(id, body))
}

/// Compiles `.mcfunction` text into one entry per command line.
pub(crate) fn parse_body<S>(
    source: &str,
    dispatcher: &CommandDispatcher<S, FotonCommandRuntime>,
    compilation_source: &S,
) -> Result<FunctionBody<S>, FunctionParseError>
where
    S: ExecutionCommandSource + Clone,
{
    let lines: Vec<&str> = source.lines().collect();
    let mut builder = BodyBuilder::new();
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
            builder.add_macro(&line[1..], line_number)?;
            continue;
        }

        match parse_command_line(line, dispatcher, compilation_source) {
            Ok(action) => builder.add_command(action),
            Err(error) => {
                return Err(FunctionParseError::new(
                    line_number,
                    format!("whilst parsing command: {error}"),
                ));
            }
        }
    }

    Ok(builder.build(compilation_source))
}

/// Parses one command into an action any source can be bound to.
///
/// Vanilla parity: `CommandFunction.parseCommand`.
pub(super) fn parse_command_line<S>(
    line: &str,
    dispatcher: &CommandDispatcher<S, FotonCommandRuntime>,
    compilation_source: &S,
) -> Result<Arc<dyn UnboundEntryAction<S>>, CommandSyntaxError>
where
    S: ExecutionCommandSource + Clone,
{
    let parse = dispatcher.parse(line, compilation_source.clone());
    let chain = dispatcher.context_chain(parse)?;
    Ok(Arc::new(UnboundCommand::new(chain)))
}

/// Collects compiled lines, switching to macro entries at the first `$` line.
///
/// Vanilla parity: `FunctionBuilder`.
struct BodyBuilder<S>
where
    S: ExecutionCommandSource,
{
    plain: Option<Vec<Arc<dyn UnboundEntryAction<S>>>>,
    macro_entries: Vec<MacroEntry<S>>,
    macro_arguments: Vec<String>,
}

impl<S> BodyBuilder<S>
where
    S: ExecutionCommandSource + Clone,
{
    fn new() -> Self {
        Self {
            plain: Some(Vec::new()),
            macro_entries: Vec::new(),
            macro_arguments: Vec::new(),
        }
    }

    fn add_command(&mut self, action: Arc<dyn UnboundEntryAction<S>>) {
        if let Some(plain) = &mut self.plain {
            plain.push(action);
        } else {
            self.macro_entries.push(MacroEntry::Plain(action));
        }
    }

    fn add_macro(&mut self, line: &str, line_number: usize) -> Result<(), FunctionParseError> {
        let template = StringTemplate::parse(line)
            .map_err(|error| FunctionParseError::new(line_number, error))?;
        if let Some(plain) = self.plain.take() {
            self.macro_entries = plain.into_iter().map(MacroEntry::Plain).collect();
        }
        let parameters = template
            .variables()
            .iter()
            .map(|variable| self.argument_index(variable))
            .collect::<Vec<_>>();
        self.macro_entries.push(MacroEntry::Template {
            template,
            parameters,
        });
        Ok(())
    }

    fn argument_index(&mut self, name: &str) -> usize {
        if let Some(index) = self
            .macro_arguments
            .iter()
            .position(|existing| existing == name)
        {
            return index;
        }
        self.macro_arguments.push(name.to_owned());
        self.macro_arguments.len() - 1
    }

    fn build(self, compilation_source: &S) -> FunctionBody<S> {
        if let Some(plain) = self.plain {
            return FunctionBody::Plain(plain.into());
        }
        FunctionBody::Macro(MacroFunction::new(
            self.macro_arguments,
            self.macro_entries,
            compilation_source.clone(),
        ))
    }
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
