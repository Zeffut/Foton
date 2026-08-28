#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "custom scheduler control hooks are exercised by tests and reserved for keyed runtimes"
    )
)]

use std::{collections::VecDeque, mem, sync::Arc};

use steel_utils::locks::SyncMutex;

use crate::command::brigadier::{CommandSyntaxError, ContextChainStage};

use super::{
    CommandResultCallback, ExecutionCommandSource, SteelContextChain, SteelExecutor, SteelModifier,
};

const MAX_COMMAND_QUEUE_DEPTH: usize = 10_000_000;

/// Flags accumulated while traversing a command context chain.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ChainModifiers(u8);

impl ChainModifiers {
    const FORKED: u8 = 1;
    const RETURN: u8 = 2;

    pub(crate) const fn is_forked(self) -> bool {
        self.0 & Self::FORKED != 0
    }

    pub(crate) const fn is_return(self) -> bool {
        self.0 & Self::RETURN != 0
    }

    pub(crate) const fn with_forked(self) -> Self {
        Self(self.0 | Self::FORKED)
    }

    pub(crate) const fn with_return(self) -> Self {
        Self(self.0 | Self::RETURN)
    }
}

/// Why a command queue stopped running.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ExecutionStop {
    Completed,
    Suspended,
    CommandLimit,
    QueueOverflow,
}

#[derive(Clone)]
pub(crate) struct Frame {
    depth: usize,
    return_value_consumer: CommandResultCallback,
    discard: FrameDiscard,
}

#[derive(Clone, Copy)]
enum FrameDiscard {
    All,
    AtOrAbove(usize),
}

impl Frame {
    pub(crate) const fn depth(&self) -> usize {
        self.depth
    }

    pub(crate) fn return_success(&self, value: i32) {
        self.return_value_consumer.on_result(true, value);
    }

    pub(crate) fn return_failure(&self) {
        self.return_value_consumer.on_result(false, 0);
    }

    /// Returns the callback this frame reports its result to.
    ///
    /// Vanilla parity: `Frame.returnValueConsumer`, which `/function` chains
    /// onto its own callback when it runs inside a `return run`.
    pub(crate) fn return_value_consumer(&self) -> CommandResultCallback {
        self.return_value_consumer.clone()
    }
}

pub(crate) trait EntryAction<S>: Send + 'static
where
    S: ExecutionCommandSource,
{
    fn execute(self: Box<Self>, context: &mut CommandExecutionContext<S>, frame: Frame);

    fn runs_after_command_limit(&self) -> bool {
        false
    }

    fn cancel(&mut self) {}
}

/// Poll result for a normal command whose result is produced across ticks.
pub(crate) enum CommandResultSuspensionPoll {
    Pending,
    Ready(Result<i32, CommandSyntaxError>),
}

/// Ordering barrier retained while a top-level command is suspended.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CommandSuspensionOrder {
    /// Only later commands from the same source wait for this suspension.
    #[default]
    Source,
    /// Every later command waits because the suspended work mutates shared command authority.
    Global,
}

/// Cross-tick work that retains ordinary command result and error semantics.
pub(crate) trait CommandResultSuspension: Send + 'static {
    fn order(&self) -> CommandSuspensionOrder {
        CommandSuspensionOrder::Source
    }

    fn poll(&mut self) -> CommandResultSuspensionPoll;

    fn cancel(&mut self) {}
}

/// Poll result for work that suspended a command execution.
pub(crate) enum CommandSuspensionPoll<S>
where
    S: ExecutionCommandSource,
{
    Pending,
    Ready(Box<dyn EntryAction<S>>),
}

impl<S> CommandSuspensionPoll<S>
where
    S: ExecutionCommandSource,
{
    pub(crate) fn resume(action: impl EntryAction<S>) -> Self {
        Self::Ready(Box::new(action))
    }
}

/// Cross-tick work that eventually produces the next action for the same command frame.
pub(crate) trait CommandSuspension<S>: Send + 'static
where
    S: ExecutionCommandSource,
{
    fn order(&self) -> CommandSuspensionOrder {
        CommandSuspensionOrder::Source
    }

    fn poll(&mut self) -> CommandSuspensionPoll<S>;

    fn cancel(&mut self) {}
}

struct CommandQueueEntry<S>
where
    S: ExecutionCommandSource,
{
    frame: Frame,
    action: Box<dyn EntryAction<S>>,
}

struct ActiveSuspension<S>
where
    S: ExecutionCommandSource,
{
    frame: Frame,
    suspension: Box<dyn CommandSuspension<S>>,
}

struct SuspensionResumeAction<S>
where
    S: ExecutionCommandSource,
{
    action: Box<dyn EntryAction<S>>,
}

impl<S> EntryAction<S> for SuspensionResumeAction<S>
where
    S: ExecutionCommandSource,
{
    fn execute(self: Box<Self>, context: &mut CommandExecutionContext<S>, frame: Frame) {
        self.action.execute(context, frame);
    }

    fn runs_after_command_limit(&self) -> bool {
        true
    }

    fn cancel(&mut self) {
        self.action.cancel();
    }
}

/// Vanilla-style command action queue retained only while explicitly suspended.
pub(crate) struct CommandExecutionContext<S>
where
    S: ExecutionCommandSource,
{
    command_limit: usize,
    fork_limit: usize,
    queue_limit: usize,
    command_quota: usize,
    queue_overflow: bool,
    command_queue: VecDeque<CommandQueueEntry<S>>,
    new_top_commands: Vec<CommandQueueEntry<S>>,
    suspension: Option<ActiveSuspension<S>>,
    current_frame_depth: usize,
}

impl<S> CommandExecutionContext<S>
where
    S: ExecutionCommandSource,
{
    pub(crate) fn new(command_limit: usize, fork_limit: usize) -> Self {
        let command_limit = command_limit.max(1);
        Self {
            command_limit,
            fork_limit,
            queue_limit: MAX_COMMAND_QUEUE_DEPTH,
            command_quota: command_limit,
            queue_overflow: false,
            command_queue: VecDeque::new(),
            new_top_commands: Vec::new(),
            suspension: None,
            current_frame_depth: 0,
        }
    }

    #[cfg(test)]
    pub(super) fn with_queue_limit(
        command_limit: usize,
        fork_limit: usize,
        queue_limit: usize,
    ) -> Self {
        let mut context = Self::new(command_limit, fork_limit);
        context.queue_limit = queue_limit;
        context
    }

    /// Queues one loaded function as a top-level execution.
    ///
    /// Vanilla parity: `ExecutionContext.queueInitialFunctionCall`.
    pub(crate) fn queue_initial_function_call(
        &mut self,
        entries: FunctionEntries<S>,
        sender: S,
        function_return: CommandResultCallback,
    ) {
        let sender = Arc::new(sender);
        let result_callback = sender.callback();
        let frame = self.create_top_frame(function_return);
        self.queue_entry(CommandQueueEntry {
            frame,
            action: Box::new(CallFunctionAction {
                entries,
                sender,
                result_callback,
                return_parent_frame: false,
            }),
        });
    }

    pub(crate) fn queue_initial_command(
        &mut self,
        chain: SteelContextChain<S>,
        source: S,
        return_value_consumer: CommandResultCallback,
    ) {
        let source = Arc::new(source);
        let frame = self.create_top_frame(return_value_consumer);
        self.queue_entry(CommandQueueEntry {
            frame,
            action: Box::new(BuildContextsAction {
                chain,
                original_source: Arc::clone(&source),
                sources: vec![source],
                modifiers: ChainModifiers::default(),
            }),
        });
    }

    pub(crate) fn run(&mut self) -> ExecutionStop {
        if self.suspension.is_some() {
            return ExecutionStop::Suspended;
        }
        if self.queue_overflow {
            log::error!(
                "Command execution stopped due to command queue overflow (max {})",
                self.queue_limit
            );
            return ExecutionStop::QueueOverflow;
        }

        self.push_new_commands();
        let stop = loop {
            if self.command_quota == 0
                && !self
                    .command_queue
                    .front()
                    .is_some_and(|entry| entry.action.runs_after_command_limit())
            {
                log::info!(
                    "Command execution stopped due to limit (executed {} commands)",
                    self.command_limit
                );
                break ExecutionStop::CommandLimit;
            }

            let Some(entry) = self.command_queue.pop_front() else {
                break ExecutionStop::Completed;
            };
            self.current_frame_depth = entry.frame.depth;
            entry.action.execute(self, entry.frame);
            if self.queue_overflow {
                log::error!(
                    "Command execution stopped due to command queue overflow (max {})",
                    self.queue_limit
                );
                break ExecutionStop::QueueOverflow;
            }
            if self.suspension.is_some() {
                break ExecutionStop::Suspended;
            }
            self.push_new_commands();
        };
        self.current_frame_depth = 0;
        stop
    }

    /// Polls the active suspension once and resumes the retained queue when it is ready.
    pub(crate) fn poll_suspension(&mut self) -> ExecutionStop {
        let Some(mut active) = self.suspension.take() else {
            return self.run();
        };

        match active.suspension.poll() {
            CommandSuspensionPoll::Pending => {
                self.suspension = Some(active);
                ExecutionStop::Suspended
            }
            CommandSuspensionPoll::Ready(action) => {
                self.queue_next(active.frame, SuspensionResumeAction { action });
                self.run()
            }
        }
    }

    pub(crate) fn suspension_order(&self) -> Option<CommandSuspensionOrder> {
        self.suspension
            .as_ref()
            .map(|active| active.suspension.order())
    }

    /// Cancels active and queued suspension work and discards the retained command queue.
    pub(crate) fn cancel(&mut self) {
        if let Some(mut active) = self.suspension.take() {
            active.suspension.cancel();
        }
        self.cancel_command_queue();
        self.cancel_new_top_commands();
        self.queue_overflow = false;
        self.current_frame_depth = 0;
    }

    pub(crate) const fn fork_limit(&self) -> usize {
        self.fork_limit
    }

    pub(crate) const fn increment_cost(&mut self) {
        self.command_quota = self.command_quota.saturating_sub(1);
    }

    const fn create_top_frame(&self, return_value_consumer: CommandResultCallback) -> Frame {
        if self.current_frame_depth == 0 {
            return Frame {
                depth: 0,
                return_value_consumer,
                discard: FrameDiscard::All,
            };
        }

        let depth = self.current_frame_depth + 1;
        Frame {
            depth,
            return_value_consumer,
            discard: FrameDiscard::AtOrAbove(depth),
        }
    }

    fn queue_next(&mut self, frame: Frame, action: impl EntryAction<S>) {
        self.queue_entry(CommandQueueEntry {
            frame,
            action: Box::new(action),
        });
    }

    fn queue_boxed(&mut self, frame: Frame, action: Box<dyn EntryAction<S>>) {
        self.queue_entry(CommandQueueEntry { frame, action });
    }

    fn queue_entry(&mut self, mut entry: CommandQueueEntry<S>) {
        if self
            .new_top_commands
            .len()
            .saturating_add(self.command_queue.len())
            > self.queue_limit
        {
            self.queue_overflow = true;
            self.cancel_new_top_commands();
            self.cancel_command_queue();
        }
        if self.queue_overflow {
            entry.action.cancel();
            return;
        }
        self.new_top_commands.push(entry);
    }

    fn push_new_commands(&mut self) {
        while let Some(command) = self.new_top_commands.pop() {
            self.command_queue.push_front(command);
        }
    }

    fn discard(&mut self, frame: &Frame) {
        match frame.discard {
            FrameDiscard::All => self.cancel_command_queue(),
            FrameDiscard::AtOrAbove(depth) => {
                while self
                    .command_queue
                    .front()
                    .is_some_and(|entry| entry.frame.depth >= depth)
                {
                    if let Some(mut entry) = self.command_queue.pop_front() {
                        entry.action.cancel();
                    }
                }
            }
        }
    }

    fn cancel_command_queue(&mut self) {
        for mut entry in self.command_queue.drain(..) {
            entry.action.cancel();
        }
    }

    fn cancel_new_top_commands(&mut self) {
        for mut entry in self.new_top_commands.drain(..) {
            entry.action.cancel();
        }
    }
}

impl<S> Drop for CommandExecutionContext<S>
where
    S: ExecutionCommandSource,
{
    fn drop(&mut self) {
        self.cancel();
    }
}

/// Queue and frame operations available only to custom internal executors.
pub(crate) struct ExecutionControl<'context, S>
where
    S: ExecutionCommandSource,
{
    context: &'context mut CommandExecutionContext<S>,
    frame: Frame,
}

impl<'context, S> ExecutionControl<'context, S>
where
    S: ExecutionCommandSource,
{
    pub(crate) const fn new(
        context: &'context mut CommandExecutionContext<S>,
        frame: Frame,
    ) -> Self {
        Self { context, frame }
    }

    pub(crate) const fn current_frame(&self) -> &Frame {
        &self.frame
    }

    pub(crate) fn queue_next(&mut self, action: impl EntryAction<S>) {
        self.context.queue_next(self.frame.clone(), action);
    }

    /// Suspends at this queue position until the supplied work produces a resume action.
    pub(crate) fn suspend(&mut self, suspension: impl CommandSuspension<S>) {
        self.queue_next(SuspendAction {
            suspension: Box::new(suspension),
        });
    }

    pub(crate) fn queue_contexts(
        &mut self,
        chain: SteelContextChain<S>,
        original_source: Arc<S>,
        sources: Vec<Arc<S>>,
        modifiers: ChainModifiers,
    ) {
        self.context.queue_next(
            self.frame.clone(),
            BuildContextsAction {
                chain,
                original_source,
                sources,
                modifiers,
            },
        );
    }

    /// Queues the rest of a chain against sources that earlier queued work fills in.
    ///
    /// Vanilla parity: the `BuildContexts.Continuation` that
    /// `execute if function` queues over the list its isolated calls append
    /// their passing sources to. The list has to stay shared: the continuation
    /// is queued before those calls have run.
    pub(crate) fn queue_deferred_contexts(
        &mut self,
        chain: SteelContextChain<S>,
        original_source: Arc<S>,
        sources: Arc<SyncMutex<Vec<Arc<S>>>>,
        modifiers: ChainModifiers,
    ) {
        self.queue_next(DeferredBuildContextsAction {
            chain,
            original_source,
            sources,
            modifiers,
        });
    }

    pub(crate) fn discard_frame(&mut self) {
        self.context.discard(&self.frame);
    }

    pub(crate) fn queue_fallthrough(&mut self) {
        self.queue_next(FallthroughAction);
    }

    /// Queues a plain callback at this queue position.
    ///
    /// Vanilla parity: the lambda `EntryAction` `/function` queues to report the
    /// summed result of several functions once they have all run.
    pub(crate) fn queue_callback(&mut self, callback: impl FnOnce() + Send + 'static) {
        self.queue_next(CallbackAction {
            callback: Box::new(callback),
        });
    }

    /// Queues work that runs in its own frame and reports that frame's result
    /// to `output` instead of to the caller's frame.
    ///
    /// Vanilla parity: `IsolatedCall`.
    pub(crate) fn queue_isolated(
        &mut self,
        output: CommandResultCallback,
        task: impl FnOnce(&mut ExecutionControl<'_, S>) + Send + 'static,
    ) {
        self.queue_next(IsolatedCallAction {
            task: Box::new(task),
            output,
        });
    }

    /// Queues one loaded function's entries in a frame of their own.
    ///
    /// Vanilla parity: `CallFunction`. `return_parent_frame` keeps the caller's
    /// frame control so a `/return` inside the function ends the caller too,
    /// which is what `execute if function` and `return run function` rely on.
    pub(crate) fn queue_function_call(
        &mut self,
        entries: FunctionEntries<S>,
        sender: Arc<S>,
        result_callback: CommandResultCallback,
        return_parent_frame: bool,
    ) {
        self.queue_next(CallFunctionAction {
            entries,
            sender,
            result_callback,
            return_parent_frame,
        });
    }

    pub(crate) fn return_success(&mut self, result: i32) {
        self.frame.return_success(result);
        self.context.discard(&self.frame);
    }

    pub(crate) fn return_failure(&mut self) {
        self.frame.return_failure();
        self.context.discard(&self.frame);
    }
}

struct SuspendAction<S>
where
    S: ExecutionCommandSource,
{
    suspension: Box<dyn CommandSuspension<S>>,
}

impl<S> EntryAction<S> for SuspendAction<S>
where
    S: ExecutionCommandSource,
{
    fn execute(self: Box<Self>, context: &mut CommandExecutionContext<S>, frame: Frame) {
        assert!(
            context.suspension.is_none(),
            "a command execution cannot activate two suspensions at once"
        );
        context.suspension = Some(ActiveSuspension {
            frame,
            suspension: self.suspension,
        });
    }

    fn runs_after_command_limit(&self) -> bool {
        true
    }

    fn cancel(&mut self) {
        self.suspension.cancel();
    }
}

struct BuildContextsAction<S>
where
    S: ExecutionCommandSource,
{
    chain: SteelContextChain<S>,
    original_source: Arc<S>,
    sources: Vec<Arc<S>>,
    modifiers: ChainModifiers,
}

impl<S> EntryAction<S> for BuildContextsAction<S>
where
    S: ExecutionCommandSource,
{
    fn execute(self: Box<Self>, context: &mut CommandExecutionContext<S>, frame: Frame) {
        let Self {
            chain,
            original_source,
            sources,
            modifiers,
        } = *self;
        build_contexts(context, frame, chain, original_source, sources, modifiers);
    }
}

/// Walks a context chain's modifier stages and schedules its terminal executor.
///
/// Vanilla parity: `BuildContexts.execute`, shared by the queued entry action
/// and by the unbound entry a loaded function keeps for each of its lines.
fn build_contexts<S>(
    context: &mut CommandExecutionContext<S>,
    frame: Frame,
    chain: SteelContextChain<S>,
    original_source: Arc<S>,
    sources: Vec<Arc<S>>,
    modifiers: ChainModifiers,
) where
    S: ExecutionCommandSource,
{
    let mut chain = chain;
    let mut sources = sources;
    let mut modifiers = modifiers;

    while chain.stage() == ContextChainStage::Modify {
        sources.retain(|source| source.execution_is_current());
        if sources.is_empty() {
            if modifiers.is_return() {
                context.queue_next(frame, FallthroughAction);
            }
            return;
        }
        if chain.top_context().is_forked() {
            modifiers = modifiers.with_forked();
        }

        match chain.top_context().modifier() {
            Some(SteelModifier::Custom(modifier)) => {
                let mut control = ExecutionControl::new(context, frame);
                modifier.apply(original_source, sources, &chain, modifiers, &mut control);
                return;
            }
            Some(SteelModifier::Standard(modifier)) => {
                context.increment_cost();
                let mut next_sources = Vec::new();
                for source in sources {
                    let command_context = chain.top_context().copy_for(Arc::clone(&source));
                    let new_sources = match modifier(&command_context) {
                        Ok(sources) => sources,
                        Err(error) => {
                            if modifiers.is_forked() {
                                continue;
                            }
                            source.handle_error(&error, false);
                            return;
                        }
                    };
                    if next_sources.len().saturating_add(new_sources.len()) >= context.fork_limit()
                    {
                        let error = CommandSyntaxError::dynamic(format!(
                            "Command fork limit reached ({})",
                            context.fork_limit()
                        ));
                        original_source.handle_error(&error, modifiers.is_forked());
                        return;
                    }
                    next_sources.extend(new_sources.into_iter().map(Arc::new));
                }
                sources = next_sources;
            }
            None => {}
        }

        let Some(next_stage) = chain.next_stage() else {
            unreachable!("a modifying command stage always has a following stage")
        };
        chain = next_stage;
    }

    sources.retain(|source| source.execution_is_current());
    if sources.is_empty() {
        if modifiers.is_return() {
            context.queue_next(frame, FallthroughAction);
        }
        return;
    }

    let Some(executor) = chain.top_context().executor() else {
        unreachable!("a context chain's final stage is always executable")
    };
    match executor {
        SteelExecutor::Custom(executor) => {
            for source in sources {
                let mut control = ExecutionControl::new(context, frame.clone());
                executor.run(source, &chain, modifiers, &mut control);
            }
        }
        SteelExecutor::Standard(_) | SteelExecutor::Suspended(_) => {
            if modifiers.is_return() {
                let Some(source) = sources.into_iter().next() else {
                    unreachable!("empty source lists return before terminal scheduling")
                };
                let callback = CommandResultCallback::chain(
                    source.callback(),
                    frame.return_value_consumer.clone(),
                );
                let source = Arc::new(source.with_callback(callback));
                schedule_executions(context, frame, chain, vec![source], modifiers);
            } else {
                schedule_executions(context, frame, chain, sources, modifiers);
            }
        }
    }
}

fn schedule_executions<S>(
    context: &mut CommandExecutionContext<S>,
    frame: Frame,
    chain: SteelContextChain<S>,
    sources: Vec<Arc<S>>,
    modifiers: ChainModifiers,
) where
    S: ExecutionCommandSource,
{
    match sources.len() {
        0 => {}
        1 | 2 => {
            for source in sources {
                context.queue_next(
                    frame.clone(),
                    ExecuteAction {
                        chain: chain.clone(),
                        source,
                        modifiers,
                    },
                );
            }
        }
        _ => context.queue_next(
            frame,
            ExecuteContinuation {
                chain,
                sources: sources.into(),
                modifiers,
            },
        ),
    }
}

struct ExecuteContinuation<S>
where
    S: ExecutionCommandSource,
{
    chain: SteelContextChain<S>,
    sources: VecDeque<Arc<S>>,
    modifiers: ChainModifiers,
}

impl<S> EntryAction<S> for ExecuteContinuation<S>
where
    S: ExecutionCommandSource,
{
    fn execute(mut self: Box<Self>, context: &mut CommandExecutionContext<S>, frame: Frame) {
        let Some(source) = self.sources.pop_front() else {
            return;
        };
        context.queue_next(
            frame.clone(),
            ExecuteAction {
                chain: self.chain.clone(),
                source,
                modifiers: self.modifiers,
            },
        );
        if !self.sources.is_empty() {
            context.queue_boxed(frame, self);
        }
    }
}

struct ExecuteAction<S>
where
    S: ExecutionCommandSource,
{
    chain: SteelContextChain<S>,
    source: Arc<S>,
    modifiers: ChainModifiers,
}

impl<S> EntryAction<S> for ExecuteAction<S>
where
    S: ExecutionCommandSource,
{
    fn execute(self: Box<Self>, context: &mut CommandExecutionContext<S>, frame: Frame) {
        let Self {
            chain,
            source,
            modifiers,
        } = *self;
        if !source.execution_is_current() {
            source.callback().on_result(false, 0);
            return;
        }
        context.increment_cost();
        let command_context = chain.top_context().copy_for(Arc::clone(&source));
        let Some(executor) = command_context.executor() else {
            unreachable!("a scheduled execute action always has a terminal executor")
        };
        match executor {
            SteelExecutor::Standard(executor) => {
                complete_command_result(source.as_ref(), modifiers, executor(&command_context));
            }
            SteelExecutor::Suspended(executor) => match executor(&command_context) {
                Ok(suspension) => context.queue_next(
                    frame,
                    SuspendAction {
                        suspension: Box::new(CommandResultSuspensionAdapter {
                            suspension,
                            source,
                            modifiers,
                        }),
                    },
                ),
                Err(error) => complete_command_result(source.as_ref(), modifiers, Err(error)),
            },
            SteelExecutor::Custom(_) => {
                unreachable!("custom executors run directly while building contexts")
            }
        }
    }
}

struct CommandResultSuspensionAdapter<S>
where
    S: ExecutionCommandSource,
{
    suspension: Box<dyn CommandResultSuspension>,
    source: Arc<S>,
    modifiers: ChainModifiers,
}

impl<S> CommandSuspension<S> for CommandResultSuspensionAdapter<S>
where
    S: ExecutionCommandSource,
{
    fn order(&self) -> CommandSuspensionOrder {
        self.suspension.order()
    }

    fn poll(&mut self) -> CommandSuspensionPoll<S> {
        if !self.source.execution_is_current() {
            self.suspension.cancel();
            return CommandSuspensionPoll::resume(UnavailableCommandResultAction {
                source: Arc::clone(&self.source),
            });
        }
        match self.suspension.poll() {
            CommandResultSuspensionPoll::Pending => CommandSuspensionPoll::Pending,
            CommandResultSuspensionPoll::Ready(result) => {
                CommandSuspensionPoll::resume(CompleteCommandResultAction {
                    source: Arc::clone(&self.source),
                    modifiers: self.modifiers,
                    result,
                })
            }
        }
    }

    fn cancel(&mut self) {
        self.suspension.cancel();
    }
}

struct UnavailableCommandResultAction<S>
where
    S: ExecutionCommandSource,
{
    source: Arc<S>,
}

impl<S> EntryAction<S> for UnavailableCommandResultAction<S>
where
    S: ExecutionCommandSource,
{
    fn execute(self: Box<Self>, _context: &mut CommandExecutionContext<S>, _frame: Frame) {
        self.source.callback().on_result(false, 0);
    }
}

struct CompleteCommandResultAction<S>
where
    S: ExecutionCommandSource,
{
    source: Arc<S>,
    modifiers: ChainModifiers,
    result: Result<i32, CommandSyntaxError>,
}

impl<S> EntryAction<S> for CompleteCommandResultAction<S>
where
    S: ExecutionCommandSource,
{
    fn execute(self: Box<Self>, _context: &mut CommandExecutionContext<S>, _frame: Frame) {
        complete_command_result(self.source.as_ref(), self.modifiers, self.result);
    }
}

fn complete_command_result<S>(
    source: &S,
    modifiers: ChainModifiers,
    result: Result<i32, CommandSyntaxError>,
) where
    S: ExecutionCommandSource,
{
    match result {
        Ok(result) => source.callback().on_result(true, result),
        Err(error) => {
            source.callback().on_result(false, 0);
            if !modifiers.is_forked() {
                source.handle_error(&error, false);
            }
        }
    }
}

struct FallthroughAction;

impl<S> EntryAction<S> for FallthroughAction
where
    S: ExecutionCommandSource,
{
    fn execute(self: Box<Self>, context: &mut CommandExecutionContext<S>, frame: Frame) {
        frame.return_failure();
        context.discard(&frame);
    }
}

/// A queued action bound to its command source only when it runs.
///
/// Vanilla parity: `UnboundEntryAction`. A loaded function keeps one action per
/// command line and binds every caller's source to it, so unlike [`EntryAction`]
/// the action is shared and runs from `&self`.
pub(crate) trait UnboundEntryAction<S>: Send + Sync
where
    S: ExecutionCommandSource,
{
    fn execute(&self, sender: Arc<S>, context: &mut CommandExecutionContext<S>, frame: Frame);
}

/// The compiled lines of one function, shared by every call to it.
pub(crate) type FunctionEntries<S> = Arc<[Arc<dyn UnboundEntryAction<S>>]>;

/// One parsed function line, ready to run against any source.
///
/// Vanilla parity: `BuildContexts.Unbound`.
pub(crate) struct UnboundCommand<S>
where
    S: ExecutionCommandSource,
{
    chain: SteelContextChain<S>,
}

impl<S> UnboundCommand<S>
where
    S: ExecutionCommandSource,
{
    pub(crate) const fn new(chain: SteelContextChain<S>) -> Self {
        Self { chain }
    }
}

impl<S> UnboundEntryAction<S> for UnboundCommand<S>
where
    S: ExecutionCommandSource,
{
    fn execute(&self, sender: Arc<S>, context: &mut CommandExecutionContext<S>, frame: Frame) {
        build_contexts(
            context,
            frame,
            self.chain.clone(),
            Arc::clone(&sender),
            vec![sender],
            ChainModifiers::default(),
        );
    }
}

struct BoundEntryAction<S>
where
    S: ExecutionCommandSource,
{
    action: Arc<dyn UnboundEntryAction<S>>,
    sender: Arc<S>,
}

impl<S> EntryAction<S> for BoundEntryAction<S>
where
    S: ExecutionCommandSource,
{
    fn execute(self: Box<Self>, context: &mut CommandExecutionContext<S>, frame: Frame) {
        self.action.execute(self.sender, context, frame);
    }
}

struct CallFunctionAction<S>
where
    S: ExecutionCommandSource,
{
    entries: FunctionEntries<S>,
    sender: Arc<S>,
    result_callback: CommandResultCallback,
    return_parent_frame: bool,
}

impl<S> EntryAction<S> for CallFunctionAction<S>
where
    S: ExecutionCommandSource,
{
    fn execute(self: Box<Self>, context: &mut CommandExecutionContext<S>, frame: Frame) {
        context.increment_cost();
        let depth = frame.depth + 1;
        let discard = if self.return_parent_frame {
            frame.discard
        } else {
            FrameDiscard::AtOrAbove(depth)
        };
        let function_frame = Frame {
            depth,
            return_value_consumer: self.result_callback,
            discard,
        };
        schedule_function_entries(context, function_frame, self.entries, self.sender);
    }
}

/// Queues a function's entries in order, deferring long bodies to a continuation.
///
/// Vanilla parity: `ContinuationTask.schedule`.
fn schedule_function_entries<S>(
    context: &mut CommandExecutionContext<S>,
    frame: Frame,
    entries: FunctionEntries<S>,
    sender: Arc<S>,
) where
    S: ExecutionCommandSource,
{
    match entries.len() {
        0 => {}
        1 | 2 => {
            for entry in entries.iter() {
                context.queue_next(
                    frame.clone(),
                    BoundEntryAction {
                        action: Arc::clone(entry),
                        sender: Arc::clone(&sender),
                    },
                );
            }
        }
        _ => context.queue_next(
            frame,
            FunctionEntryContinuation {
                entries,
                sender,
                index: 0,
            },
        ),
    }
}

struct FunctionEntryContinuation<S>
where
    S: ExecutionCommandSource,
{
    entries: FunctionEntries<S>,
    sender: Arc<S>,
    index: usize,
}

impl<S> EntryAction<S> for FunctionEntryContinuation<S>
where
    S: ExecutionCommandSource,
{
    fn execute(mut self: Box<Self>, context: &mut CommandExecutionContext<S>, frame: Frame) {
        let Some(entry) = self.entries.get(self.index) else {
            return;
        };
        context.queue_next(
            frame.clone(),
            BoundEntryAction {
                action: Arc::clone(entry),
                sender: Arc::clone(&self.sender),
            },
        );
        self.index += 1;
        if self.index < self.entries.len() {
            context.queue_boxed(frame, self);
        }
    }
}

struct DeferredBuildContextsAction<S>
where
    S: ExecutionCommandSource,
{
    chain: SteelContextChain<S>,
    original_source: Arc<S>,
    sources: Arc<SyncMutex<Vec<Arc<S>>>>,
    modifiers: ChainModifiers,
}

impl<S> EntryAction<S> for DeferredBuildContextsAction<S>
where
    S: ExecutionCommandSource,
{
    fn execute(self: Box<Self>, context: &mut CommandExecutionContext<S>, frame: Frame) {
        let sources = mem::take(&mut *self.sources.lock());
        build_contexts(
            context,
            frame,
            self.chain,
            self.original_source,
            sources,
            self.modifiers,
        );
    }
}

struct CallbackAction {
    callback: Box<dyn FnOnce() + Send>,
}

impl<S> EntryAction<S> for CallbackAction
where
    S: ExecutionCommandSource,
{
    fn execute(self: Box<Self>, _context: &mut CommandExecutionContext<S>, _frame: Frame) {
        (self.callback)();
    }
}

type IsolatedTask<S> = dyn FnOnce(&mut ExecutionControl<'_, S>) + Send;

struct IsolatedCallAction<S>
where
    S: ExecutionCommandSource,
{
    task: Box<IsolatedTask<S>>,
    output: CommandResultCallback,
}

impl<S> EntryAction<S> for IsolatedCallAction<S>
where
    S: ExecutionCommandSource,
{
    fn execute(self: Box<Self>, context: &mut CommandExecutionContext<S>, frame: Frame) {
        let depth = frame.depth + 1;
        let isolated_frame = Frame {
            depth,
            return_value_consumer: self.output,
            discard: FrameDiscard::AtOrAbove(depth),
        };
        let mut control = ExecutionControl::new(context, isolated_frame);
        (self.task)(&mut control);
    }
}
