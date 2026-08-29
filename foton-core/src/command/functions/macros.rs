//! Function macros.
//!
//! Vanilla parity: `StringTemplate` and `MacroFunction`. A `$` line is not a
//! command until the call supplies its arguments, so it is kept as a template
//! and parsed on instantiation; the eight most recent argument sets are cached
//! so a macro called every tick with the same values is parsed once.

use std::sync::Arc;

use foton_utils::{Identifier, locks::SyncMutex, nbt::to_canonical_snbt, translations};
use simdnbt::owned::{NbtCompound, NbtTag};
use text_components::TextComponent;

use super::super::brigadier::CommandDispatcher;
use super::super::execution::{
    ExecutionCommandSource, FotonCommandRuntime, FunctionEntries, UnboundEntryAction,
};
use super::parser::parse_command_line;

/// How many instantiated argument sets one macro keeps.
const MAX_CACHE_ENTRIES: usize = 8;

/// A macro line split into its literal parts and the variables between them.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct StringTemplate {
    segments: Vec<String>,
    variables: Vec<String>,
}

impl StringTemplate {
    /// Splits a `$` line on its `$(name)` variables.
    ///
    /// A line with no variable at all is an error, matching vanilla: a `$` line
    /// that substitutes nothing is a typo, not a constant command.
    pub(super) fn parse(input: &str) -> Result<Self, String> {
        let mut segments = Vec::new();
        let mut variables = Vec::new();
        let mut start = 0;
        let mut cursor = input.find('$');

        while let Some(index) = cursor {
            if input[index..].starts_with("$(") {
                segments.push(input[start..index].to_owned());
                let Some(offset) = input[index..].find(')') else {
                    return Err("unterminated macro variable".to_owned());
                };
                let end = index + offset;
                let variable = &input[index + 2..end];
                if !Self::is_valid_variable_name(variable) {
                    return Err(format!("invalid macro variable name '{variable}'"));
                }
                variables.push(variable.to_owned());
                start = end + 1;
                cursor = input[start..].find('$').map(|offset| start + offset);
            } else {
                cursor = input[index + 1..]
                    .find('$')
                    .map(|offset| index + 1 + offset);
            }
        }

        if start == 0 {
            return Err("no variables in macro".to_owned());
        }
        if start != input.len() {
            segments.push(input[start..].to_owned());
        }
        Ok(Self {
            segments,
            variables,
        })
    }

    fn is_valid_variable_name(variable: &str) -> bool {
        !variable.is_empty()
            && variable
                .chars()
                .all(|character| character.is_alphanumeric() || character == '_')
    }

    pub(super) fn variables(&self) -> &[String] {
        &self.variables
    }

    /// Fills the template's variables in, in the order they appear.
    pub(super) fn substitute(&self, arguments: &[String]) -> String {
        let mut result = String::new();
        for (index, argument) in arguments.iter().enumerate() {
            if let Some(segment) = self.segments.get(index) {
                result.push_str(segment);
            }
            result.push_str(argument);
        }
        if self.segments.len() > self.variables.len()
            && let Some(last) = self.segments.last()
        {
            result.push_str(last);
        }
        result
    }
}

/// One line of a function that contains at least one macro line.
pub(super) enum MacroEntry<S>
where
    S: ExecutionCommandSource,
{
    /// A line that needs no substitution, already compiled.
    Plain(Arc<dyn UnboundEntryAction<S>>),
    /// A `$` line, kept as a template with the argument slots it reads.
    Template {
        template: StringTemplate,
        parameters: Vec<usize>,
    },
}

/// A function whose body cannot be compiled until a call supplies arguments.
pub(crate) struct MacroFunction<S>
where
    S: ExecutionCommandSource,
{
    parameters: Vec<String>,
    entries: Vec<MacroEntry<S>>,
    compilation_source: S,
    cache: SyncMutex<Vec<(Vec<String>, FunctionEntries<S>)>>,
}

impl<S> MacroFunction<S>
where
    S: ExecutionCommandSource + Clone,
{
    pub(super) fn new(
        parameters: Vec<String>,
        entries: Vec<MacroEntry<S>>,
        compilation_source: S,
    ) -> Self {
        Self {
            parameters,
            entries,
            compilation_source,
            cache: SyncMutex::new(Vec::new()),
        }
    }

    /// Compiles the body for one set of arguments.
    ///
    /// Vanilla parity: `MacroFunction.instantiate`.
    pub(crate) fn instantiate(
        &self,
        id: &Identifier,
        arguments: Option<&NbtCompound>,
        dispatcher: &CommandDispatcher<S, FotonCommandRuntime>,
    ) -> Result<FunctionEntries<S>, Box<TextComponent>> {
        let Some(arguments) = arguments else {
            return Err(Box::new(
                translations::COMMANDS_FUNCTION_ERROR_MISSING_ARGUMENTS
                    .message([TextComponent::from(id.to_string())])
                    .component(),
            ));
        };

        let mut values = Vec::with_capacity(self.parameters.len());
        for parameter in &self.parameters {
            let Some(value) = arguments.get(parameter.as_str()) else {
                return Err(Box::new(
                    translations::COMMANDS_FUNCTION_ERROR_MISSING_ARGUMENT
                        .message([
                            TextComponent::from(id.to_string()),
                            TextComponent::from(parameter.clone()),
                        ])
                        .component(),
                ));
            };
            values.push(stringify(value));
        }

        if let Some(cached) = self.cached(&values) {
            return Ok(cached);
        }

        let mut entries: Vec<Arc<dyn UnboundEntryAction<S>>> =
            Vec::with_capacity(self.entries.len());
        for entry in &self.entries {
            match entry {
                MacroEntry::Plain(action) => entries.push(Arc::clone(action)),
                MacroEntry::Template {
                    template,
                    parameters,
                } => {
                    let substitutions = parameters
                        .iter()
                        .filter_map(|index| values.get(*index).cloned())
                        .collect::<Vec<_>>();
                    let command = template.substitute(&substitutions);
                    match parse_command_line(&command, dispatcher, &self.compilation_source) {
                        Ok(action) => entries.push(action),
                        Err(error) => {
                            return Err(Box::new(
                                translations::COMMANDS_FUNCTION_ERROR_PARSE
                                    .message([
                                        TextComponent::from(id.to_string()),
                                        TextComponent::from(command),
                                        TextComponent::from(error.to_string()),
                                    ])
                                    .component(),
                            ));
                        }
                    }
                }
            }
        }

        let instantiated: FunctionEntries<S> = entries.into();
        self.store(values, &instantiated);
        Ok(instantiated)
    }

    fn cached(&self, values: &[String]) -> Option<FunctionEntries<S>> {
        let mut cache = self.cache.lock();
        let index = cache.iter().position(|(key, _)| key == values)?;
        let entry = cache.remove(index);
        let instantiated = Arc::clone(&entry.1);
        cache.push(entry);
        Some(instantiated)
    }

    fn store(&self, values: Vec<String>, instantiated: &FunctionEntries<S>) {
        let mut cache = self.cache.lock();
        if cache.len() >= MAX_CACHE_ENTRIES {
            cache.remove(0);
        }
        cache.push((values, Arc::clone(instantiated)));
    }
}

/// Renders one macro argument the way vanilla substitutes it.
///
/// Vanilla parity: `MacroFunction.stringify`. A string goes in unquoted and a
/// number without its type suffix, so `$(n)` in `say $(n)` reads as written;
/// anything else falls back to the SNBT `Tag.toString` produces.
fn stringify(tag: &NbtTag) -> String {
    match tag {
        NbtTag::Byte(value) => value.to_string(),
        NbtTag::Short(value) => value.to_string(),
        NbtTag::Int(value) => value.to_string(),
        NbtTag::Long(value) => value.to_string(),
        NbtTag::Float(value) => decimal_string(f64::from(*value)),
        NbtTag::Double(value) => decimal_string(*value),
        NbtTag::String(value) => value.to_str().into_owned(),
        other => to_canonical_snbt(other).unwrap_or_default(),
    }
}

/// Vanilla's `DecimalFormat("#")` with at most fifteen fraction digits.
fn decimal_string(value: f64) -> String {
    if !value.is_finite() {
        return value.to_string();
    }
    let formatted = format!("{value:.15}");
    let trimmed = formatted.trim_end_matches('0').trim_end_matches('.');
    if trimmed.is_empty() || trimmed == "-" {
        return "0".to_owned();
    }
    trimmed.to_owned()
}
