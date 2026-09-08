use super::world_tick_workers::{WorldTickWorkerError, WorldTickWorkers};
use super::{
    AUTOSAVE_INTERVAL, Arc, CCommandSuggestions, CHUNK_SENDING_TPS, COMMAND_REQUESTS_PER_TICK,
    COMMAND_RESUMPTIONS_PER_TICK, CancellationToken, ChunkPos, ChunkSender,
    CommandExecutionContext, CommandExecutionOwner, CommandRequest, CommandResultCallback,
    CommandSender, CommandSource, Duration, EncodedChunk, ExecutionCommandSource, ExecutionStop,
    GameTickTaskGuard, Instant, JoinSet, NetworkConnection, OVERLOADED_THRESHOLD,
    OVERLOADED_WARNING_INTERVAL, PendingCommandExecutionQueue, Player, SEND_PLAYER_INFO_INTERVAL,
    SLOW_CHUNK_TICK_THRESHOLD, Server, StringReader, SuggestionError, Suggestions,
    TAB_LIST_UPDATE_INTERVAL, TabListTickStats, ThreadPool, World, WorldRemovalRequest,
    command_suggestions_packet, configured_packet_workers, plugin_suggestions_packet, sleep,
    spawn_blocking,
};
use crate::command::functions::CommandFunction;
use crate::event::{AsyncTabCompleteEvent, CommandEvent, ServerTickEvent};
use foton_utils::Identifier;
use std::mem::take;
use tokio::sync::oneshot::{Receiver, channel};

fn detach_world(
    worlds: &super::WorldMap,
    workers: &mut WorldTickWorkers,
    key: &Identifier,
) -> Result<Arc<World>, String> {
    worlds
        .validate_removal(key)
        .map_err(|error| format!("{error:?}"))?;
    let Some(worker_removal) = workers.prepare_removal(key) else {
        log::error!(
            target: "foton::server::world_removal",
            "Requested world {key} has no tick worker"
        );
        return Err("world has no tick worker".to_owned());
    };
    let world = worlds.remove(key).map_err(|error| format!("{error:?}"))?;
    worker_removal.commit();
    Ok(world)
}

impl Server {
    /// Runs gameplay packets, game ticks, and chunk sending. Game-tick boundaries
    /// fork background chunk-scheduling epochs through each world's task tracker.
    pub async fn run(self: Arc<Self>, cancel_token: CancellationToken) {
        // A world is built before the server that owns it, so the link back is
        // filled in here, before anything can tick.
        self.attach_worlds();
        // Functions are compiled against a command source, and a source needs
        // the server, so the datapack load cannot happen before this point.
        let report = self.reload_functions();
        for error in &report.errors {
            log::error!("Datapack load: {error}");
        }
        log::info!(
            "Loaded {} function(s) and {} function tag(s) from {}",
            report.functions,
            report.tags,
            self.functions.root().display()
        );
        self.packet_processor.open_after_tick();
        let packet_worker_count = configured_packet_workers(self.config.packet_workers);
        let mut packet_handles = Vec::with_capacity(packet_worker_count);
        for worker_id in 0..packet_worker_count {
            let s = self.clone();
            let t = cancel_token.clone();
            packet_handles.push(tokio::spawn(async move {
                if let Err(error) = spawn_blocking(move || s.packet_processor.run(&s)).await {
                    log::error!("Gameplay packet worker {worker_id} failed: {error}");
                    t.cancel();
                }
            }));
        }
        let packet_supervisor_cancel = cancel_token.clone();
        let packet_workers = async move {
            for handle in packet_handles {
                if let Err(error) = handle.await {
                    log::error!("Gameplay packet supervisor failed: {error}");
                    packet_supervisor_cancel.cancel();
                }
            }
        };
        let game_handle = {
            let s = self.clone();
            let t = cancel_token.clone();
            let task_guard = GameTickTaskGuard::new(self.clone(), cancel_token.clone());
            tokio::spawn(async move {
                let _task_guard = task_guard;
                s.run_game_tick(t).await;
            })
        };
        let chunk_send_handle = {
            let s = self.clone();
            let t = cancel_token.clone();
            tokio::spawn(async move { s.run_chunk_sending_tick(t).await })
        };
        let ((), game_result, chunk_send_result) =
            tokio::join!(packet_workers, game_handle, chunk_send_handle);
        for (task, result) in [
            ("Game tick", game_result),
            ("Chunk sending tick", chunk_send_result),
        ] {
            if let Err(error) = result {
                log::error!("{task} task failed: {error}");
            }
        }
    }

    /// The main game tick loop (20 TPS, governed by tick rate manager).
    #[expect(
        clippy::too_many_lines,
        reason = "the ordered tick phases and their shutdown joins remain easier to audit together"
    )]
    async fn run_game_tick(self: Arc<Self>, cancel_token: CancellationToken) {
        let mut world_tick_workers = match WorldTickWorkers::spawn(self.worlds.snapshots()) {
            Ok(workers) => workers,
            Err(error) => {
                log::error!("Failed to start world tick workers: {error}");
                cancel_token.cancel();
                return;
            }
        };
        let mut next_tick_time = Instant::now();
        // `None` is vanilla's zero-initialized `lastOverloadWarningNanos`: the
        // first overload is always allowed to report and to drop its debt.
        let mut last_overload_warning: Option<Instant> = None;
        let mut next_command_data_autosave = Instant::now() + AUTOSAVE_INTERVAL;
        let mut player_info_ticks = 0_u64;
        let mut pending_command_executions = PendingCommandExecutionQueue::<CommandSource>::new();
        let mut player_disconnect_saves = JoinSet::new();
        let mut command_data_autosaves = JoinSet::new();

        loop {
            if cancel_token.is_cancelled() {
                break;
            }

            let (nanoseconds_per_tick, should_sprint_this_tick) = {
                let mut tick_manager = self.tick_rate_manager.write();
                let nanoseconds_per_tick = tick_manager.nanoseconds_per_tick;
                let (should_sprint, sprint_report) = tick_manager.check_should_sprint_this_tick();
                drop(tick_manager);

                if let Some(report) = sprint_report {
                    self.broadcast_sprint_report(&report);
                    self.broadcast_ticking_state();
                }

                (nanoseconds_per_tick, should_sprint)
            };

            if should_sprint_this_tick {
                next_tick_time = Instant::now();
                last_overload_warning = Some(next_tick_time);
            } else {
                let tick_duration = Duration::from_nanos(nanoseconds_per_tick);
                let now = Instant::now();

                // A tick that overran leaves `next_tick_time` in the past, and
                // the loop then runs flat out until it has paid the whole debt
                // back -- replaying every missed tick, so mobs, redstone,
                // growth and the day cycle all fast-forward, and on a server
                // that is merely slow the debt never stops growing. Vanilla
                // gives up on the debt instead, and says so.
                let behind = now.saturating_duration_since(next_tick_time);
                let may_warn = last_overload_warning.is_none_or(|last| {
                    next_tick_time.saturating_duration_since(last)
                        >= OVERLOADED_WARNING_INTERVAL + 100 * tick_duration
                });
                if behind > OVERLOADED_THRESHOLD + 20 * tick_duration && may_warn {
                    let ticks_behind = behind.as_nanos() / u128::from(nanoseconds_per_tick.max(1));
                    log::warn!(
                        "Can't keep up! Is the server overloaded? Running {}ms or {ticks_behind} ticks behind",
                        behind.as_millis()
                    );
                    next_tick_time +=
                        tick_duration * u32::try_from(ticks_behind).unwrap_or(u32::MAX);
                    last_overload_warning = Some(next_tick_time);
                }

                if now < next_tick_time {
                    tokio::select! {
                        () = cancel_token.cancelled() => break,
                        () = sleep(next_tick_time - now) => {}
                    }
                }
                next_tick_time += tick_duration;
            }

            if cancel_token.is_cancelled() {
                break;
            }

            let tick_start = Instant::now();
            self.packet_processor.close_for_tick().await;
            self.advance_chunk_scheduling();
            self.start_player_disconnect_saves(&mut player_disconnect_saves);

            let (tick_count, runs_normally) = {
                let mut tick_manager = self.tick_rate_manager.write();
                tick_manager.tick();
                let runs_normally = tick_manager.runs_normally();
                tick_manager.increment_tick_count();
                (tick_manager.tick_count, runs_normally)
            };

            self.tick_pending_command_executions(&mut pending_command_executions);
            self.tick_command_requests(&mut pending_command_executions);
            self.tick_functions(&mut pending_command_executions, runs_normally);
            if let Err(error) = self
                .tick_worlds_game(&world_tick_workers, tick_count, runs_normally)
                .await
            {
                log::error!("World game tick failed: {error}");
                cancel_token.cancel();
                break;
            }
            self.process_world_removals(&mut world_tick_workers);
            self.tick_player_connections();
            player_info_ticks += 1;
            if player_info_ticks > SEND_PLAYER_INFO_INTERVAL {
                let _span = tracing::trace_span!("broadcast_latency").entered();
                self.broadcast_player_latency_updates();
                player_info_ticks = 0;
            }
            self.tick_jobs(tick_count, runs_normally);
            self.process_player_joins();

            {
                let server = self.clone();
                let _ =
                    spawn_blocking(move || server.process_world_changes(tick_count, runs_normally))
                        .await;
            }

            self.process_world_additions(&mut world_tick_workers);
            self.process_domain_switches();

            self.tick_command_data_autosave(
                &mut next_command_data_autosave,
                &mut command_data_autosaves,
            );

            let tab_list_tick_stats = self.record_tick_and_capture_tab_stats(
                tick_count,
                tick_start.elapsed().as_nanos() as u64,
            );

            if let Some(tick_stats) = tab_list_tick_stats {
                self.broadcast_tab_list(tick_stats);
            }

            if should_sprint_this_tick {
                let mut tick_manager = self.tick_rate_manager.write();
                tick_manager.end_tick_work();
            }

            // Last, so that anything listening sees a tick that has already
            // done its own work rather than one still in the middle of it.
            self.events.fire(&mut ServerTickEvent::new(tick_count));

            self.packet_processor.open_after_tick();
            if should_sprint_this_tick || Instant::now() >= next_tick_time {
                self.packet_processor.wait_for_overload_progress().await;
            }
        }

        self.jobs.cancel_all();
        pending_command_executions.cancel_all();
        self.command_requests.clear();
        // A compiled function holds the source it was parsed with, and that
        // source holds the server; dropping the library breaks the cycle.
        self.functions.unload();
        self.packet_processor.stop();
        self.start_player_disconnect_saves(&mut player_disconnect_saves);
        self.pending_player_disconnects.clear();
        while let Some(result) = player_disconnect_saves.join_next().await {
            if let Err(error) = result {
                log::error!("Player disconnect save task failed during shutdown: {error}");
            }
        }
        while let Some(result) = command_data_autosaves.join_next().await {
            if let Err(error) = result {
                log::error!("Command data autosave task failed during shutdown: {error}");
            }
        }
    }

    /// Ticks every online player's connection, world membership or not.
    ///
    /// Vanilla parity: `MinecraftServer.tickConnection` ->
    /// `ServerConnectionListener.tick`, which walks its own connection list and
    /// not the player list. That distinction is the whole point of the call.
    /// Foton ticked the connection from `Player::tick`, which only runs for a
    /// player some world still ticks -- so a player who reached the End credits,
    /// and whom `show_end_credits` removes from the world, stopped being sent
    /// keep-alives and stopped being timed out. A client whose link then died
    /// without a FIN left its socket, both halves of the connection task and the
    /// whole `Player` alive until restart, with the name still in the tab list;
    /// and because a duplicate login is refused while the ghost holds the UUID,
    /// the person behind it could never get back in either.
    fn tick_player_connections(&self) {
        self.online_players().iter_players(|_, player| {
            player.connection.tick();
            true
        });
    }

    fn record_tick_and_capture_tab_stats(
        &self,
        tick_count: u64,
        tick_duration_nanos: u64,
    ) -> Option<TabListTickStats> {
        let mut tick_manager = self.tick_rate_manager.write();
        tick_manager.record_tick_time(tick_duration_nanos);
        tick_count
            .is_multiple_of(TAB_LIST_UPDATE_INTERVAL)
            .then(|| TabListTickStats::capture(&tick_manager))
    }

    async fn autosave_command_data(self: &Arc<Self>) {
        tracing::debug!("Autosave started");

        // Vanilla parity: `MinecraftServer.autoSave` runs
        // `saveEverything(true, false, false)` -- chunks, level data and players
        // -- every 6000 ticks. Foton's timer only ever covered the command-owned
        // data below, so nothing else was written until a chunk unloaded, a
        // player disconnected, or the server shut down cleanly. Around spawn
        // (`keep_spawn_in_memory`) and inside any player's radius chunks never
        // unload at all, so a long session could reach the end of the day having
        // written nothing, and any unclean stop took the lot.
        for world in self.worlds.values() {
            world.request_save();
        }
        self.request_save_players();

        let results = self.save_command_data().await;
        match results.scoreboards {
            Ok(saved) => tracing::debug!(saved, "Domain scoreboard autosave completed"),
            Err(error) => tracing::error!(%error, "Domain scoreboard autosave failed"),
        }
        match results.storage {
            Ok(saved) => tracing::debug!(saved, "Domain command-storage autosave completed"),
            Err(error) => tracing::error!(%error, "Domain command-storage autosave failed"),
        }
        match results.maps {
            Ok(saved) => tracing::debug!(saved, "Domain map autosave completed"),
            Err(error) => tracing::error!(%error, "Domain map autosave failed"),
        }
        match results.boss_bars {
            Ok(saved) => tracing::debug!(saved, "Domain boss-bar autosave completed"),
            Err(error) => tracing::error!(%error, "Domain boss-bar autosave failed"),
        }
    }

    fn tick_command_data_autosave(
        self: &Arc<Self>,
        next_autosave: &mut Instant,
        saves: &mut JoinSet<()>,
    ) {
        while let Some(result) = saves.try_join_next() {
            if let Err(error) = result {
                log::error!("Command data autosave task failed: {error}");
            }
        }
        if Instant::now() < *next_autosave {
            return;
        }
        if saves.is_empty() {
            let server = Arc::clone(self);
            saves.spawn(async move {
                server.autosave_command_data().await;
            });
        } else {
            tracing::warn!("Skipping command data autosave while the previous save runs");
        }
        *next_autosave = Instant::now() + AUTOSAVE_INTERVAL;
    }

    fn tick_pending_command_executions(
        &self,
        pending: &mut PendingCommandExecutionQueue<CommandSource>,
    ) {
        let stats = pending.tick(COMMAND_RESUMPTIONS_PER_TICK, |owner| owner.is_current(self));
        if stats.polled == COMMAND_RESUMPTIONS_PER_TICK && stats.pending > 0 {
            tracing::debug!(
                polled = stats.polled,
                finished = stats.finished,
                pending = stats.pending,
                "Command resumption tick reached per-tick processing limit"
            );
        }
    }

    /// Runs the `#minecraft:load` and `#minecraft:tick` function tags.
    ///
    /// Vanilla parity: `ServerFunctionManager.tick`, which skips the tags
    /// entirely while the tick rate manager is frozen or stepping.
    fn tick_functions(
        self: &Arc<Self>,
        pending: &mut PendingCommandExecutionQueue<CommandSource>,
        runs_normally: bool,
    ) {
        if !runs_normally {
            return;
        }
        let load = self.functions.take_load_functions();
        let tick = self.functions.ticking_functions();
        if load.is_empty() && tick.is_empty() {
            return;
        }
        for function in load.into_iter().chain(tick) {
            self.run_function_now(pending, &function);
        }
    }

    /// Starts one function as its own top-level execution.
    fn run_function_now(
        self: &Arc<Self>,
        pending: &mut PendingCommandExecutionQueue<CommandSource>,
        function: &CommandFunction,
    ) {
        let owner = CommandExecutionOwner::capture(CommandSender::Console, self);
        let source = self.function_source();
        // A macro function in a tag has no arguments to fill in, so it fails
        // here the way vanilla's does rather than running half-substituted.
        let entries = match self
            .with_command_dispatcher(|dispatcher| function.instantiate(None, dispatcher))
        {
            Ok(entries) => entries,
            Err(reason) => {
                log::error!("Failed to instantiate function {}: {reason}", function.id());
                return;
            }
        };
        let mut execution = CommandExecutionContext::for_source(&source);
        execution.queue_initial_function_call(entries, source, CommandResultCallback::empty());
        if execution.run() == ExecutionStop::Suspended && !pending.push_suspended(owner, execution)
        {
            tracing::error!(
                function = %function.id(),
                "suspended function execution could not be retained"
            );
        }
    }

    fn tick_command_requests(
        self: &Arc<Self>,
        pending: &mut PendingCommandExecutionQueue<CommandSource>,
    ) {
        let mut handled = 0;
        for _ in 0..COMMAND_REQUESTS_PER_TICK {
            let Some(request) = self
                .command_requests
                .pop_front_runnable(|owner| !pending.blocks(owner.key()))
            else {
                break;
            };
            handled += 1;

            match request {
                CommandRequest::Execute { owner, command } => {
                    if !owner.is_current(self) {
                        continue;
                    }
                    self.execute_command_request(pending, owner, &command);
                }
                CommandRequest::Suggestions {
                    owner,
                    transaction_id,
                    input,
                } => {
                    if !owner.is_current(self) {
                        continue;
                    }
                    let Some(player) = owner.sender().get_player() else {
                        tracing::error!("command suggestion request has a non-player owner");
                        continue;
                    };
                    self.send_command_suggestions(player, transaction_id, &input);
                }
            }
        }

        if handled == COMMAND_REQUESTS_PER_TICK {
            tracing::debug!(handled, "Command request tick reached its processing limit");
        }
    }

    fn execute_command_request(
        self: &Arc<Self>,
        pending: &mut PendingCommandExecutionQueue<CommandSource>,
        owner: CommandExecutionOwner,
        command: &str,
    ) {
        let command = command.strip_prefix('/').unwrap_or(command);

        // Before the dispatcher, because a plugin's command is not in the
        // Brigadier tree and never will be: the tree is built at startup from
        // types the server knows. A listener that claims the name runs it
        // itself, and the server does not go looking for a command it has
        // never heard of only to report that it does not exist.
        let mut event = CommandEvent::new(owner.sender().get_player().cloned(), command);
        self.events.fire(&mut event);
        if event.is_handled() {
            return;
        }

        let source = CommandSource::new(owner.sender().clone(), Arc::clone(self));
        let chain = {
            let dispatcher = self.command_dispatcher.read();
            let parse = dispatcher.parse(command, source.clone());
            dispatcher.context_chain(parse)
        };
        let chain = match chain {
            Ok(chain) => chain,
            Err(error) => {
                source.handle_error(&error, false);
                return;
            }
        };

        let mut execution = CommandExecutionContext::for_source(&source);
        execution.queue_initial_command(chain, source, CommandResultCallback::empty());
        if execution.run() == ExecutionStop::Suspended && !pending.push_suspended(owner, execution)
        {
            tracing::error!("suspended command execution could not be retained");
        }
    }

    fn send_command_suggestions(
        self: &Arc<Self>,
        player: &Arc<Player>,
        transaction_id: i32,
        input: &str,
    ) {
        // Paper's `AsyncTabCompleteEvent`, fired before the tree is consulted:
        // a plugin's command is not in the Brigadier tree and never will be, so
        // completing one is only possible ahead of the server's own answer.
        let mut event = AsyncTabCompleteEvent::new(Some(Arc::clone(player)), input.to_owned());
        self.events.fire(&mut event);
        if event.is_cancelled() {
            player.send_packet(CCommandSuggestions::new(transaction_id, 0, 0, Vec::new()));
            return;
        }
        if event.is_handled() {
            player.send_packet(plugin_suggestions_packet(
                transaction_id,
                input,
                event.completions(),
            ));
            return;
        }

        let suggestions =
            self.build_command_suggestions(CommandSender::Player(Arc::clone(player)), input);
        match suggestions {
            Ok(suggestions) => {
                player.send_packet(command_suggestions_packet(transaction_id, &suggestions));
            }
            Err(error) => {
                tracing::warn!(%error, "failed to build command suggestions");
                player.send_packet(CCommandSuggestions::new(transaction_id, 0, 0, Vec::new()));
            }
        }
    }

    pub(super) fn build_command_suggestions(
        self: &Arc<Self>,
        sender: CommandSender,
        input: &str,
    ) -> Result<Suggestions, SuggestionError> {
        let source = CommandSource::new(sender, Arc::clone(self));
        let mut reader = StringReader::new(input);
        if reader.peek() == Some('/') {
            reader.skip();
        }
        let dispatcher = self.command_dispatcher.read();
        let parse = dispatcher.parse_reader(reader, source);
        dispatcher.completion_suggestions(&parse)
    }

    /// Chunk sending tick loop — encodes and sends chunks to players independently.
    async fn run_chunk_sending_tick(self: Arc<Self>, cancel_token: CancellationToken) {
        let nanos_per_tick = 1_000_000_000 / CHUNK_SENDING_TPS;
        let mut next_tick_time = Instant::now();

        loop {
            if cancel_token.is_cancelled() {
                break;
            }

            let now = Instant::now();
            if now < next_tick_time {
                tokio::select! {
                    () = cancel_token.cancelled() => break,
                    () = sleep(next_tick_time - now) => {}
                }
            }
            next_tick_time += Duration::from_nanos(nanos_per_tick);

            if cancel_token.is_cancelled() {
                break;
            }

            let server = self.clone();
            let _ = spawn_blocking(move || {
                server.tick_chunk_sending();
            })
            .await;
        }
    }

    /// Executes one chunk sending tick across all worlds and players.
    ///
    /// A per-world per-tick encode cache is used so overlapping view areas
    /// don't re-encode the same chunk within a single tick.
    fn tick_chunk_sending(&self) {
        let tick_start = Instant::now();
        for snapshot in self.worlds.snapshots() {
            let world = snapshot.world();
            let mut encode_cache = rustc_hash::FxHashMap::default();
            world.players.iter_players(|_uuid, player| {
                Self::send_chunks_for_player(
                    player,
                    world,
                    &mut encode_cache,
                    self.chunk_encoding_pool.as_ref(),
                );
                true
            });
        }

        let elapsed = tick_start.elapsed();
        if elapsed >= SLOW_CHUNK_TICK_THRESHOLD {
            tracing::warn!(?elapsed, "Chunk sending tick slow");
        }
    }

    /// Three-phase chunk send for a single player: prepare (lock briefly),
    /// encode (no lock), commit (lock briefly + generation check).
    fn send_chunks_for_player(
        player: &Arc<Player>,
        world: &Arc<World>,
        encode_cache: &mut rustc_hash::FxHashMap<ChunkPos, EncodedChunk>,
        encoding_pool: &ThreadPool,
    ) {
        let chunk_pos = *player.last_chunk_pos.lock();
        let connection = &player.connection;

        // Phase 1: prepare (brief lock)
        let prepared = {
            let mut sender = player.chunk_sender.lock();
            sender.prepare_batch(world, chunk_pos, &player.chunk_send_epoch)
        };

        let Some(batch) = prepared else {
            return;
        };

        // Phase 2: encode (no lock held — uses per-tick local cache)
        let compression = connection.compression();
        let encoded = ChunkSender::encode_batch(&batch, encode_cache, compression, encoding_pool);

        // Phase 3: commit while holding the tracking view that world detachment invalidates.
        // This makes membership validation, packet commit, and tracker refresh one side of
        // the same synchronization boundary.
        let tracking_view = player.last_tracking_view.lock();
        let Some(view) = *tracking_view else {
            return;
        };
        if !world.contains_player(player) {
            return;
        }
        let sent_chunks = {
            let mut sender = player.chunk_sender.lock();
            sender.commit_batch(&batch, encoded, connection, &player.chunk_send_epoch)
        };

        if sent_chunks.is_empty() {
            return;
        }

        let sent_chunks = player.chunk_sender.lock().sent_chunks_snapshot();
        world
            .entity_tracker()
            .update_player(player, &view, |chunk| sent_chunks.contains(&chunk));
    }

    /// Commits ready chunk lifecycle epochs and forks the next background work.
    fn advance_chunk_scheduling(&self) {
        for (i, snapshot) in self.worlds.snapshots().into_iter().enumerate() {
            let world = snapshot.world();
            let timings = world.chunk_map.advance_scheduling();

            let background_elapsed = timings.ticket_updates
                + timings.schedule_generation
                + timings.run_generation
                + timings.process_unloads;
            let boundary_elapsed = timings.block_entity_unloads
                + timings.readiness_demotions
                + timings.lifecycle_commit
                + timings.readiness_reconcile
                + timings.ticking_snapshot_rebuild;
            let work_elapsed = background_elapsed + boundary_elapsed;

            if work_elapsed >= SLOW_CHUNK_TICK_THRESHOLD {
                tracing::warn!(
                    world = i,
                    work_elapsed = ?work_elapsed,
                    background_elapsed = ?background_elapsed,
                    boundary_elapsed = ?boundary_elapsed,
                    ticket_updates = ?timings.ticket_updates,
                    block_entity_unloads = ?timings.block_entity_unloads,
                    readiness_demotions = ?timings.readiness_demotions,
                    lifecycle_commit = ?timings.lifecycle_commit,
                    readiness_reconcile = ?timings.readiness_reconcile,
                    post_process_generation = ?timings.post_process_generation,
                    post_process_chunk_count = timings.post_process_chunk_count,
                    post_process_position_count = timings.post_process_position_count,
                    readiness_candidate_count = timings.readiness_candidate_count,
                    ticking_snapshot_rebuild = ?timings.ticking_snapshot_rebuild,
                    rebuilt_ticking_chunk_count = timings.rebuilt_ticking_chunk_count,
                    lookup_cache_holder_hits = timings.lookup_cache.holder_hits,
                    lookup_cache_missing_hits = timings.lookup_cache.missing_hits,
                    lookup_cache_scc_lookups = timings.lookup_cache.scc_lookups,
                    lookup_cache_foreign_map_bypasses = timings.lookup_cache.foreign_map_bypasses,
                    lookup_cache_evictions = timings.lookup_cache.evictions,
                    schedule_generation = ?timings.schedule_generation,
                    scheduled_count = timings.scheduled_count,
                    // A non-zero count is the lifecycle boundary and the
                    // scheduler disagreeing about which chunks are loaded --
                    // the state that used to end the epoch in a panic.
                    deferred_schedule_count = timings.deferred_schedule_count,
                    run_generation = ?timings.run_generation,
                    process_unloads = ?timings.process_unloads,
                    "Chunk scheduling epoch slow"
                );
            }
        }
    }

    /// Attaches worlds constructed asynchronously at a tick safe-point.
    fn process_world_additions(&self, workers: &mut WorldTickWorkers) {
        let worlds = take(&mut *self.pending_world_additions.lock());
        for (world, completion) in worlds {
            let key = world.key.clone();
            if let Err(error) = self.worlds.insert(key.clone(), world) {
                let _ = completion.send(Err(error));
                continue;
            }
            let Some(world) = self.worlds.get(&key) else {
                let _ = completion.send(Err(format!("world {key} was not attached")));
                continue;
            };
            if let Err(error) = workers.add(&world) {
                log::error!("Failed to start tick worker for {key}: {error}");
                let _ = self.worlds.remove(&key);
                let _ = completion.send(Err(error.to_string()));
            } else {
                let _ = completion.send(Ok(()));
            }
        }
    }

    /// Queues a loaded-world removal for the next tick safe-point.
    pub fn request_world_removal(&self, key: Identifier) -> bool {
        if self.worlds.get(&key).is_none() {
            return false;
        }
        self.pending_world_removals
            .lock()
            .push(WorldRemovalRequest {
                key,
                save: true,
                completion: None,
            });
        true
    }

    /// Queues a world removal with explicit persistence choice.
    pub fn request_world_removal_with_save(&self, key: Identifier, save: bool) -> bool {
        if self.worlds.get(&key).is_none() {
            return false;
        }
        self.pending_world_removals
            .lock()
            .push(WorldRemovalRequest {
                key,
                save,
                completion: None,
            });
        true
    }

    /// Queues a removal and returns a receiver completed after persistence.
    pub fn request_world_removal_with_completion(
        &self,
        key: Identifier,
        save: bool,
    ) -> Option<Receiver<Result<usize, String>>> {
        self.worlds.get(&key)?;
        let (sender, receiver) = channel();
        self.pending_world_removals
            .lock()
            .push(WorldRemovalRequest {
                key,
                save,
                completion: Some(sender),
            });
        Some(receiver)
    }

    /// Detaches worlds only after the current tick has completed.
    fn process_world_removals(&self, workers: &mut WorldTickWorkers) {
        let requests = take(&mut *self.pending_world_removals.lock());
        for request in requests {
            let WorldRemovalRequest {
                key,
                save,
                completion,
            } = request;
            let world = match detach_world(&self.worlds, workers, &key) {
                Ok(world) => world,
                Err(error) => {
                    log::debug!("World removal deferred for {key}: {error}");
                    if let Some(sender) = completion {
                        let _ = sender.send(Err(error));
                    }
                    continue;
                }
            };
            // Persistence is I/O and must not stall the serialized game tick.
            self.world_cleanup_tasks.spawn_on(async move {
                let result = if save {
                    let mut saved = 0;
                    match world.cleanup(&mut saved).await {
                        Ok(()) => Ok(saved),
                        Err(error) => Err(error.to_string()),
                    }
                } else {
                    world.cleanup_without_save().await;
                    Ok(0)
                };
                match result {
                    Ok(saved) => {
                        tracing::debug!(world = %key, saved_chunks = saved, "World cleanup finished");
                        if let Some(sender) = completion {
                            let _ = sender.send(Ok(saved));
                        }
                    }
                    Err(error) => {
                        log::error!("World cleanup failed for {key}: {error}");
                        if let Some(sender) = completion {
                            let _ = sender.send(Err(error));
                        }
                    }
                }
            }, self.chunk_runtime.handle());
        }
    }

    #[tracing::instrument(level = "trace", skip(self, workers), name = "tick_worlds")]
    async fn tick_worlds_game(
        &self,
        workers: &WorldTickWorkers,
        tick_count: u64,
        runs_normally: bool,
    ) -> Result<(), WorldTickWorkerError> {
        let all_timings = workers.tick_all(tick_count, runs_normally).await?;
        for (i, timings) in all_timings.iter().enumerate() {
            if timings.elapsed.as_millis() < 50 {
                continue;
            }
            let cm = &timings.chunk_map;
            tracing::warn!(
                world = i,
                elapsed = ?timings.elapsed,
                tick_count,
                entity_tick = ?timings.entity_tick,
                broadcast_changes = ?cm.broadcast_changes,
                collect_tickable = ?cm.collect_tickable,
                tick_chunks = ?cm.tick_chunks,
                tick_block_entities = ?cm.tick_block_entities,
                tickable_count = cm.tickable_count,
                total_chunks = cm.total_chunks,
                lookup_cache_holder_hits = cm.lookup_cache.holder_hits,
                lookup_cache_missing_hits = cm.lookup_cache.missing_hits,
                lookup_cache_scc_lookups = cm.lookup_cache.scc_lookups,
                lookup_cache_foreign_map_bypasses = cm.lookup_cache.foreign_map_bypasses,
                lookup_cache_evictions = cm.lookup_cache.evictions,
                "Game tick slow"
            );
        }
        Ok(())
    }

    pub(super) fn tick_jobs(self: &Arc<Self>, tick_count: u64, runs_normally: bool) {
        let stats = self
            .jobs
            .tick(Arc::downgrade(self), tick_count, runs_normally);
        if stats.polled > 0 && stats.pending > 0 && tick_count.is_multiple_of(100) {
            tracing::debug!(
                polled = stats.polled,
                finished = stats.finished,
                pending = stats.pending,
                "Server jobs pending"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        iter::empty,
        sync::{Arc, Mutex, Once},
    };

    use super::{Server, detach_world};
    use crate::{
        config::ResolvedDomainConfig,
        player::ResetReason,
        server::{WorldMap, world_tick_workers::WorldTickWorkers},
        test_support::{
            TestPlayerBuilder, fresh_test_world, fresh_test_world_in_domain,
            insert_ready_full_chunk,
        },
        world::World,
    };
    use foton_utils::{ChunkPos, Identifier};
    use futures::executor::block_on;
    use log::{Level, Log, Metadata, Record};
    use rustc_hash::FxHashMap;

    struct WorldRemovalLogger;

    static WORLD_REMOVAL_LOGGER: WorldRemovalLogger = WorldRemovalLogger;
    static WORLD_REMOVAL_LOGGER_INIT: Once = Once::new();
    static WORLD_REMOVAL_LOGS: Mutex<Vec<(Level, String)>> = Mutex::new(Vec::new());

    impl Log for WorldRemovalLogger {
        fn enabled(&self, metadata: &Metadata<'_>) -> bool {
            metadata.target() == "foton::server::world_removal"
        }

        fn log(&self, record: &Record<'_>) {
            if !self.enabled(record.metadata()) {
                return;
            }
            let Ok(mut logs) = WORLD_REMOVAL_LOGS.lock() else {
                return;
            };
            logs.push((record.level(), record.args().to_string()));
        }

        fn flush(&self) {}
    }

    fn capture_world_removal_logs() {
        WORLD_REMOVAL_LOGGER_INIT.call_once(|| {
            let _ = log::set_logger(&WORLD_REMOVAL_LOGGER);
            log::set_max_level(log::LevelFilter::Trace);
        });
        let Ok(mut logs) = WORLD_REMOVAL_LOGS.lock() else {
            panic!("world removal log capture should remain available");
        };
        logs.clear();
    }

    fn domain(default_world: Identifier) -> ResolvedDomainConfig {
        ResolvedDomainConfig {
            name: "main".to_owned(),
            default_world: default_world.clone(),
            worlds: vec![default_world],
        }
    }

    #[test]
    fn rejecting_default_world_removal_keeps_its_tick_worker() {
        let world = fresh_test_world_in_domain("main", "spawn");
        let key = world.key.clone();
        let worlds = WorldMap::new("main".to_owned(), &[domain(key.clone())], &[]);
        assert!(worlds.insert(key.clone(), Arc::clone(&world)).is_ok());
        let Ok(mut workers) = WorldTickWorkers::spawn([&world]) else {
            panic!("world tick worker should start");
        };

        let Err(error) = detach_world(&worlds, &mut workers, &key) else {
            panic!("default world removal must be rejected");
        };
        assert_eq!(error, "DefaultWorld");
        assert!(worlds.get(&key).is_some());
        assert!(block_on(workers.tick_all(1, true)).is_ok());
        assert_eq!(world.game_time(), 1);
    }

    #[test]
    fn rejecting_world_with_players_removal_keeps_its_tick_worker() {
        let world = fresh_test_world_in_domain("main", "arena");
        let key = world.key.clone();
        let worlds = WorldMap::new(
            "main".to_owned(),
            &[domain(Identifier::new("main", "spawn"))],
            &[],
        );
        let player = TestPlayerBuilder::new(Arc::clone(&world), "player", 1).build();
        assert!(world.players.insert(player));
        assert!(worlds.insert(key.clone(), Arc::clone(&world)).is_ok());
        let Ok(mut workers) = WorldTickWorkers::spawn([&world]) else {
            panic!("world tick worker should start");
        };

        let Err(error) = detach_world(&worlds, &mut workers, &key) else {
            panic!("world removal with players must be rejected");
        };
        assert_eq!(error, "PlayersPresent(1)");
        assert!(worlds.get(&key).is_some());
        assert!(block_on(workers.tick_all(1, true)).is_ok());
        assert_eq!(world.game_time(), 1);
    }

    #[test]
    fn missing_tick_worker_does_not_detach_live_world() {
        let world = fresh_test_world_in_domain("main", "arena");
        let key = world.key.clone();
        let worlds = WorldMap::new(
            "main".to_owned(),
            &[domain(Identifier::new("main", "spawn"))],
            &[],
        );
        assert!(worlds.insert(key.clone(), Arc::clone(&world)).is_ok());
        let Ok(mut workers) = WorldTickWorkers::spawn(empty::<&Arc<World>>()) else {
            panic!("empty worker set should initialize");
        };

        let Err(error) = detach_world(&worlds, &mut workers, &key) else {
            panic!("missing worker must reject world removal");
        };
        assert_eq!(error, "world has no tick worker");
        let Some(remaining_world) = worlds.get(&key) else {
            panic!("missing worker must not detach the live world");
        };
        assert!(Arc::ptr_eq(&remaining_world, &world));
    }

    #[test]
    fn missing_tick_worker_is_logged_as_an_error() {
        capture_world_removal_logs();
        let world = fresh_test_world_in_domain("main", "arena_without_worker");
        let key = world.key.clone();
        let worlds = WorldMap::new(
            "main".to_owned(),
            &[domain(Identifier::new("main", "spawn"))],
            &[],
        );
        assert!(worlds.insert(key.clone(), world).is_ok());
        let Ok(mut workers) = WorldTickWorkers::spawn(empty::<&Arc<World>>()) else {
            panic!("empty worker set should initialize");
        };

        assert!(detach_world(&worlds, &mut workers, &key).is_err());

        let Ok(logs) = WORLD_REMOVAL_LOGS.lock() else {
            panic!("world removal logs should remain available");
        };
        assert!(logs.iter().any(|(level, message)| {
            *level == Level::Error
                && message == "Requested world main:arena_without_worker has no tick worker"
        }));
    }

    #[test]
    fn successful_detach_removes_world_and_worker_while_other_world_keeps_ticking() {
        let removed = fresh_test_world_in_domain("main", "arena");
        let remaining = fresh_test_world_in_domain("main", "lobby");
        let removed_key = removed.key.clone();
        let remaining_key = remaining.key.clone();
        let worlds = WorldMap::new(
            "main".to_owned(),
            &[domain(Identifier::new("main", "spawn"))],
            &[],
        );
        assert!(
            worlds
                .insert(removed_key.clone(), Arc::clone(&removed))
                .is_ok()
        );
        assert!(
            worlds
                .insert(remaining_key.clone(), Arc::clone(&remaining))
                .is_ok()
        );
        let Ok(mut workers) = WorldTickWorkers::spawn([&removed, &remaining]) else {
            panic!("world tick workers should start");
        };

        let Ok(detached) = detach_world(&worlds, &mut workers, &removed_key) else {
            panic!("eligible world should detach");
        };

        assert!(Arc::ptr_eq(&detached, &removed));
        assert!(worlds.get(&removed_key).is_none());
        assert!(worlds.get(&remaining_key).is_some());
        let Ok(timings) = block_on(workers.tick_all(1, true)) else {
            panic!("remaining world worker should tick");
        };
        assert_eq!(timings.len(), 1);
        assert_eq!(removed.game_time(), 0);
        assert_eq!(remaining.game_time(), 1);
    }

    #[test]
    fn chunk_send_commit_rechecks_live_world_membership() {
        let world = fresh_test_world("chunk_send_membership_revalidation");
        let center = ChunkPos::new(0, 0);
        insert_ready_full_chunk(&world, center);
        let player = TestPlayerBuilder::new(Arc::clone(&world), "ChunkTester", 1).build();
        assert!(world.add_player(Arc::clone(&player), ResetReason::InitialJoin));
        assert!(world.players.remove_player_sync(&player).is_some());

        let encoding_pool = rayon::ThreadPoolBuilder::new().num_threads(1).build();
        let Ok(encoding_pool) = encoding_pool else {
            panic!("test chunk encoding pool should initialize");
        };
        let mut encode_cache = FxHashMap::default();
        Server::send_chunks_for_player(&player, &world, &mut encode_cache, &encoding_pool);

        let sender = player.chunk_sender.lock();
        assert!(sender.pending_chunks.contains(&center));
        assert!(!sender.is_chunk_sent(center));
        assert_eq!(sender.unacknowledged_batches, 0);
        drop(sender);

        assert!(world.players.insert(Arc::clone(&player)));
        world.remove_player_for_world_change(&player);
    }
}
