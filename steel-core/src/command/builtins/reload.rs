//! Vanilla datapack reload command.
//!
//! Vanilla parity: `ReloadCommand`, which rebuilds every datapack-driven
//! resource. Functions are the only one Steel loads from a datapack today --
//! loot tables, recipes and advancements are compiled in by the build scripts --
//! so reloading them is the whole of a reload here.
//!
//! Vanilla blocks its server thread on the reload. A Steel tick may not wait,
//! so the read and the reparse happen off the tick and the command suspends
//! until they land. The suspension is global: a reload replaces the functions
//! every later command could call, so nothing may run in between.

use std::sync::Arc;

use steel_utils::{Identifier, translations};
use text_components::TextComponent;
use tokio::sync::oneshot::{self, Receiver, error::TryRecvError};
use tokio::task::spawn_blocking;

use super::super::registration::CommandRegistration;
use super::super::{
    brigadier::{CommandNodeBuilder, CommandSyntaxError},
    execution::{
        CommandResultSuspension, CommandResultSuspensionPoll, CommandSource,
        CommandSuspensionOrder, SteelCommandContext, SteelCommandRuntime, literal,
    },
    functions::FunctionReloadReport,
};

pub(super) fn registration() -> CommandRegistration<CommandSource> {
    CommandRegistration::new(Identifier::vanilla_static("reload"), |_| command())
}

fn command() -> CommandNodeBuilder<CommandSource, SteelCommandRuntime> {
    literal("reload").executes_suspended(start_reload)
}

#[expect(
    clippy::unnecessary_wraps,
    reason = "suspended command executors share one fallible signature"
)]
fn start_reload(
    context: &SteelCommandContext<CommandSource>,
) -> Result<ReloadDatapacks, CommandSyntaxError> {
    let source = context.source().clone();
    let server = Arc::clone(source.server());
    let (sender, receiver) = oneshot::channel();
    spawn_blocking(move || {
        let report = server.reload_functions();
        // The receiver is gone only when the command was cancelled, which
        // leaves the freshly loaded library in place -- the same state a
        // reload that nobody was watching would have produced.
        drop(sender.send(report));
    });
    Ok(ReloadDatapacks { receiver, source })
}

struct ReloadDatapacks {
    receiver: Receiver<FunctionReloadReport>,
    source: CommandSource,
}

impl CommandResultSuspension for ReloadDatapacks {
    fn order(&self) -> CommandSuspensionOrder {
        CommandSuspensionOrder::Global
    }

    fn poll(&mut self) -> CommandResultSuspensionPoll {
        match self.receiver.try_recv() {
            Err(TryRecvError::Empty) => CommandResultSuspensionPoll::Pending,
            Err(TryRecvError::Closed) => {
                CommandResultSuspensionPoll::Ready(Err(CommandSyntaxError::dynamic(
                    TextComponent::from(&translations::COMMANDS_RELOAD_FAILURE),
                )))
            }
            Ok(report) => {
                for error in &report.errors {
                    log::error!("Datapack reload: {error}");
                }
                log::info!(
                    "Reloaded {} function(s) and {} function tag(s)",
                    report.functions,
                    report.tags
                );
                self.source.send_success(
                    &TextComponent::from(&translations::COMMANDS_RELOAD_SUCCESS),
                    true,
                );
                // Vanilla parity: `ReloadCommand` returns 0 on success.
                CommandResultSuspensionPoll::Ready(Ok(0))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use steel_registry::init_vanilla_registry;

    use super::super::create_dispatcher;

    #[test]
    fn reload_is_a_bare_executable_literal() {
        init_vanilla_registry();
        let Ok(dispatcher) = create_dispatcher() else {
            panic!("built-in commands should register");
        };
        let Some(root) = dispatcher.children(dispatcher.root()).and_then(|children| {
            children.iter().copied().find(|child| {
                dispatcher
                    .node(*child)
                    .is_some_and(|node| node.name() == "reload")
            })
        }) else {
            panic!("reload root should exist");
        };
        let Some(node) = dispatcher.node(root) else {
            panic!("reload root should exist");
        };
        assert!(node.is_executable());
        assert!(dispatcher.children(root).is_none_or(<[_]>::is_empty));
    }
}
