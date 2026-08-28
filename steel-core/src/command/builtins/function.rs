//! Vanilla datapack function command.

use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicI32, Ordering},
};

use simdnbt::owned::NbtCompound;
use steel_utils::{Identifier, translations};
use text_components::TextComponent;

use super::super::{
    brigadier::{CommandNodeBuilder, CommandSyntaxError},
    execution::{
        ChainModifiers, CommandResultCallback, CommandSource, CustomCommandExecutor,
        ExecutionCommandSource, ExecutionControl, FunctionEntries, FunctionOrTag,
        SteelArgumentType, SteelCommandRuntime, SteelContextChain, argument, literal,
    },
    functions::CommandFunction,
    registration::CommandRegistration,
};

pub(super) fn registration() -> CommandRegistration<CommandSource> {
    CommandRegistration::new(Identifier::vanilla_static("function"), |_| command())
}

fn command() -> CommandNodeBuilder<CommandSource, SteelCommandRuntime> {
    // TODO: `function <name> with block|entity|storage <...>` needs the data
    // accessors `/data` has not been ported yet, so only the literal compound
    // form of macro arguments is available.
    literal("function").then(
        argument("name", SteelArgumentType::function())
            .executes_custom(RunFunction)
            .then(
                argument("arguments", SteelArgumentType::nbt_compound_tag())
                    .executes_custom(RunFunction),
            ),
    )
}

struct RunFunction;

impl CustomCommandExecutor<CommandSource> for RunFunction {
    fn run(
        &self,
        source: Arc<CommandSource>,
        chain: &SteelContextChain<CommandSource>,
        modifiers: ChainModifiers,
        control: &mut ExecutionControl<'_, CommandSource>,
    ) {
        // Vanilla parity: `CustomCommandExecutor.WithErrorHandling`, which
        // reports the error to the sender and fails the command's callback.
        if let Err(error) = run_function(&source, chain, modifiers, control) {
            source.handle_error(&error, modifiers.is_forked());
            source.callback().on_result(false, 0);
        }
    }
}

fn run_function(
    source: &Arc<CommandSource>,
    chain: &SteelContextChain<CommandSource>,
    modifiers: ChainModifiers,
    control: &mut ExecutionControl<'_, CommandSource>,
) -> Result<(), CommandSyntaxError> {
    let context = chain.top_context().copy_for(Arc::clone(source));
    let reference = context.function_or_tag("name")?.clone();
    let functions = resolve_functions(source, &reference)?;
    let arguments = context.nbt_compound("arguments").ok().cloned();

    let mut instantiated = Vec::with_capacity(functions.len());
    for function in &functions {
        let entries = instantiate(source, function, arguments.as_ref()).map_err(|reason| {
            CommandSyntaxError::dynamic(
                translations::COMMANDS_FUNCTION_INSTANTIATION_FAILURE
                    .message([TextComponent::from(function.id().to_string()), *reason])
                    .component(),
            )
        })?;
        instantiated.push(entries);
    }

    source.send_success(&scheduled_message(&functions), true);
    queue_functions(&functions, &instantiated, source, control, modifiers);
    Ok(())
}

/// Compiles one function for this call's arguments.
pub(crate) fn instantiate(
    source: &CommandSource,
    function: &CommandFunction,
    arguments: Option<&NbtCompound>,
) -> Result<FunctionEntries<CommandSource>, Box<TextComponent>> {
    source
        .server()
        .with_command_dispatcher(|dispatcher| function.instantiate(arguments, dispatcher))
}

/// Resolves a parsed name into the functions it stands for.
///
/// Vanilla parity: `FunctionArgument.getFunction` for a plain name and
/// `FunctionCommand.ERROR_NO_FUNCTIONS` for a tag that resolves to nothing. An
/// unknown name never yields an empty list that would silently run nothing.
pub(crate) fn resolve_functions(
    source: &CommandSource,
    reference: &FunctionOrTag,
) -> Result<Vec<Arc<CommandFunction>>, CommandSyntaxError> {
    let library = source.server().functions.library();
    match reference {
        FunctionOrTag::Function(id) => library
            .function(id)
            .map(|function| vec![Arc::clone(function)])
            .ok_or_else(|| {
                CommandSyntaxError::dynamic(
                    translations::ARGUMENTS_FUNCTION_UNKNOWN
                        .message([TextComponent::from(id.to_string())])
                        .component(),
                )
            }),
        FunctionOrTag::Tag(id) => {
            let functions = library.tag(id);
            if functions.is_empty() {
                return Err(CommandSyntaxError::dynamic(
                    translations::COMMANDS_FUNCTION_SCHEDULED_NO_FUNCTIONS
                        .message([TextComponent::from(format!("#{id}"))])
                        .component(),
                ));
            }
            Ok(functions.to_vec())
        }
    }
}

fn scheduled_message(functions: &[Arc<CommandFunction>]) -> TextComponent {
    if let [only] = functions {
        return translations::COMMANDS_FUNCTION_SCHEDULED_SINGLE
            .message([TextComponent::from(only.id().to_string())])
            .component();
    }
    let names = functions
        .iter()
        .map(|function| function.id().to_string())
        .collect::<Vec<_>>()
        .join(", ");
    translations::COMMANDS_FUNCTION_SCHEDULED_MULTIPLE
        .message([TextComponent::from(names)])
        .component()
}

/// Queues every selected function, wiring up the caller's result reporting.
///
/// Vanilla parity: `FunctionCommand.queueFunctions`.
fn queue_functions(
    functions: &[Arc<CommandFunction>],
    instantiated: &[FunctionEntries<CommandSource>],
    original_source: &Arc<CommandSource>,
    control: &mut ExecutionControl<'_, CommandSource>,
    modifiers: ChainModifiers,
) {
    // Vanilla parity: `FunctionCommand.modifySenderForExecution`. Vanilla also
    // caps the permission level at gamemaster; Steel has no level ladder to cap,
    // and a function keeps the caller's permission set, which cannot grant more
    // than the caller already had.
    let function_source = Arc::new(
        original_source
            .with_suppressed_output()
            .with_callback(CommandResultCallback::empty()),
    );

    if modifiers.is_return() {
        let shared = CommandResultCallback::chain(
            original_source.callback(),
            control.current_frame().return_value_consumer(),
        );
        for (function, entries) in functions.iter().zip(instantiated) {
            let callback = decorate_output(original_source, function.id(), shared.clone());
            control.queue_function_call(
                Arc::clone(entries),
                Arc::clone(&function_source),
                callback,
                true,
            );
        }
        control.queue_fallthrough();
        return;
    }

    let original_callback = original_source.callback();
    match (functions, instantiated) {
        ([], _) | (_, []) => {}
        ([only], [entries]) => {
            let callback = decorate_output(original_source, only.id(), original_callback);
            control.queue_function_call(Arc::clone(entries), function_source, callback, false);
        }
        _ => {
            // Vanilla parity: several functions report one summed result to the
            // caller, and only when at least one of them produced a result.
            let any_result = Arc::new(AtomicBool::new(false));
            let sum = Arc::new(AtomicI32::new(0));
            for (function, entries) in functions.iter().zip(instantiated) {
                let any_result = Arc::clone(&any_result);
                let sum = Arc::clone(&sum);
                let partial = CommandResultCallback::new(move |_success, result| {
                    any_result.store(true, Ordering::Relaxed);
                    sum.fetch_add(result, Ordering::Relaxed);
                });
                let callback = decorate_output(original_source, function.id(), partial);
                control.queue_function_call(
                    Arc::clone(entries),
                    Arc::clone(&function_source),
                    callback,
                    false,
                );
            }
            control.queue_callback(move || {
                if any_result.load(Ordering::Relaxed) {
                    original_callback.on_result(true, sum.load(Ordering::Relaxed));
                }
            });
        }
    }
}

/// Adds vanilla's per-function `commands.function.result` feedback.
fn decorate_output(
    original_source: &Arc<CommandSource>,
    id: &Identifier,
    callback: CommandResultCallback,
) -> CommandResultCallback {
    if original_source.is_silent() {
        return callback;
    }
    let source = Arc::clone(original_source);
    let id = id.clone();
    CommandResultCallback::new(move |success, result| {
        let message = translations::COMMANDS_FUNCTION_RESULT
            .message([
                TextComponent::from(id.to_string()),
                TextComponent::from(result.to_string()),
            ])
            .component();
        source.send_success(&message, true);
        callback.on_result(success, result);
    })
}

#[cfg(test)]
mod tests {
    use steel_registry::init_vanilla_registry;

    use super::super::create_dispatcher;
    use crate::command::execution::SteelArgumentType;

    #[test]
    fn function_graph_takes_a_name_and_optional_macro_arguments() {
        init_vanilla_registry();
        let Ok(dispatcher) = create_dispatcher() else {
            panic!("built-in commands should register");
        };
        let Some(root) = dispatcher.children(dispatcher.root()).and_then(|children| {
            children.iter().copied().find(|child| {
                dispatcher
                    .node(*child)
                    .is_some_and(|node| node.name() == "function")
            })
        }) else {
            panic!("function root should exist");
        };
        let Some(children) = dispatcher.children(root) else {
            panic!("function root should have children");
        };
        assert_eq!(children.len(), 1);
        let Some(name) = dispatcher.node(children[0]) else {
            panic!("function name argument should exist");
        };
        assert_eq!(name.name(), "name");
        assert!(name.is_executable());
        assert_eq!(name.argument_type(), Some(&SteelArgumentType::function()));

        let Some(name_children) = dispatcher.children(children[0]) else {
            panic!("function name should accept macro arguments");
        };
        assert_eq!(name_children.len(), 1);
        let Some(arguments) = dispatcher.node(name_children[0]) else {
            panic!("function arguments should exist");
        };
        assert_eq!(arguments.name(), "arguments");
        assert!(arguments.is_executable());
        assert_eq!(
            arguments.argument_type(),
            Some(&SteelArgumentType::nbt_compound_tag())
        );
    }
}
