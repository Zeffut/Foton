//! Vanilla operator commands backed by Foton's built-in `op` permission group.

use std::{convert::Infallible, sync::Arc};

use foton_utils::{Identifier, translations};
use text_components::TextComponent;
use tokio::{sync::oneshot, task::JoinHandle};

use super::super::{
    brigadier::{CommandNodeBuilder, CommandSyntaxError},
    execution::{
        CommandResultSuspension, CommandResultSuspensionPoll, CommandSource,
        CommandSuspensionOrder, FotonArgumentType, FotonCommandContext, FotonCommandRuntime,
        GameProfileArgument, argument, literal,
    },
    registration::CommandRegistration,
};
use crate::{
    permission::{OP_GROUP, PermissionSubjectState},
    player::player_data_storage::PersistenceUpdateOutcome,
    server::Server,
};

pub(super) fn op_registration() -> CommandRegistration<CommandSource> {
    CommandRegistration::new(Identifier::vanilla_static("op"), |_| op_command())
}

pub(super) fn deop_registration() -> CommandRegistration<CommandSource> {
    CommandRegistration::new(Identifier::vanilla_static("deop"), |_| deop_command())
}

fn op_command() -> CommandNodeBuilder<CommandSource, FotonCommandRuntime> {
    literal("op").then(
        argument("targets", FotonArgumentType::non_operator_profile())
            .executes_suspended(|context| start_operation(context, OperatorAction::Grant)),
    )
}

fn deop_command() -> CommandNodeBuilder<CommandSource, FotonCommandRuntime> {
    literal("deop").then(
        argument("targets", FotonArgumentType::operator_profile())
            .executes_suspended(|context| start_operation(context, OperatorAction::Revoke)),
    )
}

#[derive(Clone, Copy)]
enum OperatorAction {
    Grant,
    Revoke,
}

impl OperatorAction {
    const fn command_name(self) -> &'static str {
        match self {
            Self::Grant => "op",
            Self::Revoke => "deop",
        }
    }

    fn failed(self) -> TextComponent {
        match self {
            Self::Grant => TextComponent::from(&translations::COMMANDS_OP_FAILED),
            Self::Revoke => TextComponent::from(&translations::COMMANDS_DEOP_FAILED),
        }
    }

    fn success(self, name: String) -> TextComponent {
        match self {
            Self::Grant => translations::COMMANDS_OP_SUCCESS
                .message([TextComponent::plain(name)])
                .component(),
            Self::Revoke => translations::COMMANDS_DEOP_SUCCESS
                .message([TextComponent::plain(name)])
                .component(),
        }
    }
}

fn start_operation(
    context: &FotonCommandContext<CommandSource>,
    action: OperatorAction,
) -> Result<OperatorCommandSuspension, CommandSyntaxError> {
    let argument = context.game_profile_argument("targets").cloned()?;
    let source = context.source().clone();
    let task_source = source.clone();
    let (sender, receiver) = oneshot::channel();
    let task = tokio::spawn(async move {
        let result = run_operation(&task_source, argument, action).await;
        let _ = sender.send(result);
    });
    Ok(OperatorCommandSuspension {
        source,
        action,
        receiver,
        task: Some(task),
    })
}

struct OperatorCommandResult {
    changed_names: Vec<String>,
}

async fn run_operation(
    source: &CommandSource,
    argument: GameProfileArgument,
    action: OperatorAction,
) -> Result<OperatorCommandResult, CommandSyntaxError> {
    let targets = argument.resolve(source).await?;
    let mut changed_names = Vec::new();
    for target in targets {
        if update_operator_group(source.server(), target.uuid, action).await? {
            changed_names.push(target.name);
        }
    }
    if changed_names.is_empty() {
        return Err(CommandSyntaxError::dynamic(action.failed()));
    }
    Ok(OperatorCommandResult { changed_names })
}

async fn update_operator_group(
    server: &Arc<Server>,
    uuid: uuid::Uuid,
    action: OperatorAction,
) -> Result<bool, CommandSyntaxError> {
    let result = server
        .try_update_player_permissions(uuid, move |state| {
            let (mut groups, overrides, metadata) = state.into_parts();
            let changed = update_groups(&mut groups, action);
            Ok::<_, Infallible>((
                PermissionSubjectState::new_with_metadata(groups, overrides, metadata),
                changed,
            ))
        })
        .await
        .map_err(|error| CommandSyntaxError::dynamic(error.to_string()))?;
    Ok(complete_operator_update(uuid, action, result))
}

fn complete_operator_update(
    uuid: uuid::Uuid,
    action: OperatorAction,
    result: (PermissionSubjectState, bool, PersistenceUpdateOutcome),
) -> bool {
    log_operator_persistence_warning(uuid, action, &result.2);
    result.1
}

fn log_operator_persistence_warning(
    uuid: uuid::Uuid,
    action: OperatorAction,
    outcome: &PersistenceUpdateOutcome,
) {
    let PersistenceUpdateOutcome::CommittedWithError(error) = outcome else {
        return;
    };
    let operator = matches!(action, OperatorAction::Grant);
    tracing::error!(
        %uuid,
        operator,
        command = action.command_name(),
        %error,
        "Operator command update is visible, but its durability or backup rotation could not be confirmed"
    );
}

fn update_groups(groups: &mut Vec<String>, action: OperatorAction) -> bool {
    match action {
        OperatorAction::Grant => {
            if groups.iter().any(|group| group == OP_GROUP) {
                false
            } else {
                groups.push(OP_GROUP.to_owned());
                true
            }
        }
        OperatorAction::Revoke => {
            let old_len = groups.len();
            groups.retain(|group| group != OP_GROUP);
            groups.len() != old_len
        }
    }
}

struct OperatorCommandSuspension {
    source: CommandSource,
    action: OperatorAction,
    receiver: oneshot::Receiver<Result<OperatorCommandResult, CommandSyntaxError>>,
    task: Option<JoinHandle<()>>,
}

impl CommandResultSuspension for OperatorCommandSuspension {
    fn order(&self) -> CommandSuspensionOrder {
        CommandSuspensionOrder::Global
    }

    fn poll(&mut self) -> CommandResultSuspensionPoll {
        match self.receiver.try_recv() {
            Ok(result) => {
                self.task = None;
                CommandResultSuspensionPoll::Ready(result.map(|result| {
                    let changed = result.changed_names.len().min(i32::MAX as usize) as i32;
                    for name in result.changed_names {
                        self.source.send_success(&self.action.success(name), true);
                    }
                    changed
                }))
            }
            Err(oneshot::error::TryRecvError::Empty) => CommandResultSuspensionPoll::Pending,
            Err(oneshot::error::TryRecvError::Closed) => {
                self.task = None;
                CommandResultSuspensionPoll::Ready(Err(CommandSyntaxError::dynamic(format!(
                    "{} command task ended without a result",
                    self.action.command_name()
                ))))
            }
        }
    }

    fn cancel(&mut self) {
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{io, sync::Arc};

    use foton_protocol::packets::game::{
        ArgumentType as ProtocolArgumentType, SuggestionType as ProtocolSuggestionType,
    };
    use foton_registry::init_vanilla_registry;
    use foton_utils::locks::SyncMutex;
    use tracing::subscriber::with_default;
    use tracing_subscriber::fmt::MakeWriter;

    use super::{OperatorAction, complete_operator_update, update_groups};
    use crate::command::builtins::create_dispatcher;
    use crate::command::execution::FotonArgumentType;
    use crate::permission::PermissionSubjectState;
    use crate::player::player_data_storage::PersistenceUpdateOutcome;

    #[derive(Clone, Default)]
    struct CapturedLogs(Arc<SyncMutex<Vec<u8>>>);

    struct CapturedLogWriter(Arc<SyncMutex<Vec<u8>>>);

    impl io::Write for CapturedLogWriter {
        fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
            self.0.lock().extend_from_slice(buffer);
            Ok(buffer.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl<'writer> MakeWriter<'writer> for CapturedLogs {
        type Writer = CapturedLogWriter;

        fn make_writer(&'writer self) -> Self::Writer {
            CapturedLogWriter(Arc::clone(&self.0))
        }
    }

    impl CapturedLogs {
        fn contents(&self) -> String {
            let bytes = self.0.lock();
            String::from_utf8_lossy(&bytes).into_owned()
        }
    }

    #[test]
    fn operator_group_updates_are_idempotent_and_preserve_other_groups() {
        let mut groups = vec!["builder".to_owned()];
        assert!(update_groups(&mut groups, OperatorAction::Grant));
        assert_eq!(groups, ["builder", "op"]);
        assert!(!update_groups(&mut groups, OperatorAction::Grant));
        assert!(update_groups(&mut groups, OperatorAction::Revoke));
        assert_eq!(groups, ["builder"]);
        assert!(!update_groups(&mut groups, OperatorAction::Revoke));
    }

    #[test]
    fn direct_operator_command_logs_a_visible_commit_with_uncertain_durability() {
        let uuid = uuid::Uuid::from_u128(0x0000_4f50_4552_4154_4f52);
        let logs = CapturedLogs::default();
        let subscriber = tracing_subscriber::fmt()
            .without_time()
            .with_ansi(false)
            .with_writer(logs.clone())
            .finish();
        with_default(subscriber, || {
            assert!(complete_operator_update(
                uuid,
                OperatorAction::Grant,
                (
                    PermissionSubjectState::default(),
                    true,
                    PersistenceUpdateOutcome::CommittedWithError(io::Error::other(
                        "backup rotation failed",
                    )),
                ),
            ));
        });

        let output = logs.contents();
        assert!(output.contains(&uuid.to_string()));
        assert!(output.contains("operator=true"));
        assert!(output.contains("backup rotation failed"));
        assert!(output.contains("visible"));
    }

    #[test]
    fn operator_targets_use_vanillas_game_profile_argument() {
        init_vanilla_registry();
        let dispatcher = create_dispatcher();
        let Ok(dispatcher) = dispatcher else {
            panic!("built-in dispatcher should build");
        };
        for command_name in ["op", "deop"] {
            let root = dispatcher.children(dispatcher.root()).and_then(|children| {
                children.iter().copied().find(|child| {
                    dispatcher
                        .node(*child)
                        .is_some_and(|node| node.name() == command_name)
                })
            });
            let Some(root) = root else {
                panic!("{command_name} root should exist");
            };
            let target = dispatcher
                .children(root)
                .and_then(|children| children.first())
                .and_then(|target| dispatcher.node(*target));
            let Some(target) = target else {
                panic!("{command_name} target should exist");
            };
            let protocol = target
                .argument_type()
                .map(FotonArgumentType::protocol_argument);
            assert!(matches!(
                protocol,
                Some((
                    ProtocolArgumentType::GameProfile,
                    Some(ProtocolSuggestionType::AskServer),
                ))
            ));
            assert!(target.is_executable());
        }
    }
}
