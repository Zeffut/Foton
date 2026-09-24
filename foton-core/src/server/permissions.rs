use crate::player::player_data_storage::{GlobalPlayerData, PersistenceUpdateOutcome};
use std::{convert::Infallible, future::Future, sync::LazyLock};
use tokio::runtime::Handle;
use tokio_util::task::TaskTracker;

use super::{
    Arc, COMMAND_BLOCK_GROUP, FnServerJob, OP_GROUP, PermissionGroupManager,
    PermissionGroupManagerError, PermissionGroupUpdateError, PermissionGroupsConfig, PermissionSet,
    PermissionSubjectState, Player, PlayerPermissionUpdateError, Server, ServerJobContext,
    SyncMutex, Uuid,
};
use crate::permission::PermissionGroups;

pub(super) struct PluginPersistenceTasks {
    accepting: SyncMutex<bool>,
    tasks: TaskTracker,
}

impl PluginPersistenceTasks {
    pub(super) fn new() -> Self {
        Self {
            accepting: SyncMutex::new(true),
            tasks: TaskTracker::new(),
        }
    }

    fn spawn_on<F, Fut>(&self, runtime: &Handle, build: F) -> bool
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let accepting = self.accepting.lock();
        if !*accepting {
            return false;
        }
        self.tasks.spawn_on(build(), runtime);
        true
    }

    pub(super) fn close(&self) {
        let mut accepting = self.accepting.lock();
        *accepting = false;
        self.tasks.close();
    }

    pub(super) async fn wait(&self) {
        self.tasks.wait().await;
    }
}

/// The built-in `command_block` group, used when `groups.toml` defines none.
///
/// A server whose config predates the group would otherwise give its command
/// blocks an empty permission set and leave every command silently refused.
static BUILT_IN_COMMAND_BLOCK_GROUPS: LazyLock<Option<PermissionGroups>> =
    LazyLock::new(
        || match PermissionGroups::from_config(PermissionGroupsConfig::default()) {
            Ok(groups) => Some(groups),
            Err(error) => {
                log::error!("built-in permission groups do not resolve: {error}");
                None
            }
        },
    );

pub(super) fn validate_player_permission_group_update<E>(
    manager: &PermissionGroupManager,
    previous_groups: &[String],
    updated_groups: &[String],
) -> Result<(), PlayerPermissionUpdateError<E>> {
    for group in updated_groups {
        let already_assigned = previous_groups.iter().any(|current| current == group);
        if !already_assigned && !manager.contains_group(group) {
            return Err(PlayerPermissionUpdateError::UnknownGroup(group.clone()));
        }
    }
    Ok(())
}

impl Server {
    pub(super) fn begin_queued_whitelist_update(&self, uuid: Uuid) -> Arc<()> {
        let revision = Arc::new(());
        self.queued_whitelist_update_revisions
            .lock()
            .insert(uuid, Arc::clone(&revision));
        revision
    }

    pub(super) fn queued_whitelist_update_is_current(
        &self,
        uuid: Uuid,
        revision: &Arc<()>,
    ) -> bool {
        self.queued_whitelist_update_revisions
            .lock()
            .get(&uuid)
            .is_some_and(|current| Arc::ptr_eq(current, revision))
    }

    fn finish_queued_whitelist_update(&self, uuid: Uuid, revision: &Arc<()>) {
        let mut revisions = self.queued_whitelist_update_revisions.lock();
        if revisions
            .get(&uuid)
            .is_some_and(|current| Arc::ptr_eq(current, revision))
        {
            revisions.remove(&uuid);
        }
    }

    pub(super) fn begin_queued_operator_update(&self, uuid: Uuid) -> Arc<()> {
        let revision = Arc::new(());
        self.queued_operator_update_revisions
            .lock()
            .insert(uuid, Arc::clone(&revision));
        revision
    }

    pub(super) fn queued_operator_update_is_current(&self, uuid: Uuid, revision: &Arc<()>) -> bool {
        self.queued_operator_update_revisions
            .lock()
            .get(&uuid)
            .is_some_and(|current| Arc::ptr_eq(current, revision))
    }

    fn finish_queued_operator_update(&self, uuid: Uuid, revision: &Arc<()>) {
        let mut revisions = self.queued_operator_update_revisions.lock();
        if revisions
            .get(&uuid)
            .is_some_and(|current| Arc::ptr_eq(current, revision))
        {
            revisions.remove(&uuid);
        }
    }

    /// Returns whether a player may join under the configured whitelist.
    #[must_use]
    pub fn is_player_whitelisted(&self, uuid: Uuid) -> bool {
        !self.config.whitelist_enabled
            || self
                .global_player_data(uuid)
                .is_some_and(|data| data.whitelisted)
    }

    /// Queues a persistent whitelist update and publishes it to synchronous lookups.
    pub fn queue_player_whitelist_update(self: &Arc<Self>, uuid: Uuid, whitelisted: bool) {
        let server = Arc::clone(self);
        let accepted = self.plugin_persistence_tasks.spawn_on(
            self.chunk_runtime.handle(),
            move || {
                let revision = server.begin_queued_whitelist_update(uuid);
                async move {
                    let data =
                        server
                            .global_player_data(uuid)
                            .unwrap_or_else(|| GlobalPlayerData {
                                last_active_domain: server.worlds.default_domain().to_owned(),
                                first_played: 0,
                                last_played: 0,
                                statistics: Vec::new(),
                                whitelisted: false,
                            });
                    let current_server = Arc::clone(&server);
                    let current_revision = Arc::clone(&revision);
                    let publication_server = Arc::clone(&server);
                    match server
                        .player_data_storage
                        .set_player_whitelisted(
                            uuid,
                            whitelisted,
                            &data,
                            move || {
                                current_server
                                    .queued_whitelist_update_is_current(uuid, &current_revision)
                            },
                            move |data| {
                                publication_server.publish_player_whitelist(uuid, data);
                            },
                        )
                        .await
                    {
                        Ok(Some(PersistenceUpdateOutcome::Durable) | None) => {}
                        Ok(Some(PersistenceUpdateOutcome::CommittedWithError(error))) => tracing::error!(
                            %uuid,
                            whitelisted,
                            %error,
                            "Whitelist update is visible, but its durability or backup rotation could not be confirmed"
                        ),
                        Err(error) => tracing::error!(
                            %uuid,
                            whitelisted,
                            %error,
                            "Whitelist update failed before the new value became visible"
                        ),
                    }
                    server.finish_queued_whitelist_update(uuid, &revision);
                }
            },
        );
        if !accepted {
            tracing::warn!(
                %uuid,
                whitelisted,
                "Whitelist update rejected because server persistence is shutting down"
            );
        }
    }

    /// Queues a persistent operator-group update for a plugin caller.
    pub fn queue_player_operator_update(self: &Arc<Self>, uuid: Uuid, operator: bool) {
        let server = Arc::clone(self);
        let accepted = self.plugin_persistence_tasks.spawn_on(
            self.chunk_runtime.handle(),
            move || {
                let revision = server.begin_queued_operator_update(uuid);
                async move {
                    let current_server = Arc::clone(&server);
                    let current_revision = Arc::clone(&revision);
                    let result = server
                        .try_update_player_permissions_if_current(
                            uuid,
                            move || {
                                current_server
                                    .queued_operator_update_is_current(uuid, &current_revision)
                            },
                            move |state| {
                                let (mut groups, overrides, metadata) = state.into_parts();
                                let already = groups.iter().any(|group| group == OP_GROUP);
                                if operator && !already {
                                    groups.push(OP_GROUP.to_owned());
                                } else if !operator && already {
                                    groups.retain(|group| group != OP_GROUP);
                                }
                                Ok::<_, Infallible>((
                                    PermissionSubjectState::new_with_metadata(
                                        groups, overrides, metadata,
                                    ),
                                    (),
                                ))
                            },
                        )
                        .await;
                    match result {
                        Ok((_, (), PersistenceUpdateOutcome::Durable))
                        | Err(PlayerPermissionUpdateError::Superseded) => {}
                        Ok((_, (), PersistenceUpdateOutcome::CommittedWithError(error))) => {
                            tracing::error!(
                                %uuid,
                                operator,
                                %error,
                                "Operator update is visible, but its durability or backup rotation could not be confirmed"
                            );
                        }
                        Err(error) => tracing::error!(
                            %uuid,
                            operator,
                            %error,
                            "Operator update failed before the new value became visible"
                        ),
                    }
                    server.finish_queued_operator_update(uuid, &revision);
                }
            },
        );
        if !accepted {
            tracing::warn!(
                %uuid,
                operator,
                "Operator update rejected because server persistence is shutting down"
            );
        }
    }

    fn close_plugin_persistence_updates(&self) {
        self.plugin_persistence_tasks.close();
    }

    async fn drain_plugin_persistence_updates(&self) {
        self.plugin_persistence_tasks.wait().await;
    }

    /// Closes plugin-originated player-list persistence and waits for every
    /// update admitted before the boundary to finish.
    ///
    /// This is safe to repeat so the server task's panic fallback can enforce
    /// the same boundary as its ordinary shutdown path.
    pub async fn close_and_drain_plugin_persistence_updates(&self) {
        self.close_plugin_persistence_updates();
        self.drain_plugin_persistence_updates().await;
    }

    pub(super) fn apply_cached_or_default_permission_state(&self, player: &Player) -> u64 {
        let state = self
            .player_permission_states
            .read()
            .get(player.gameprofile.id)
            .cloned()
            .unwrap_or_default();
        self.apply_player_permission_state(player, state)
    }

    fn apply_player_permission_state(&self, player: &Player, state: PermissionSubjectState) -> u64 {
        let (groups, overrides, metadata_overrides) = state.into_parts();
        for group in &groups {
            if !self.permission_groups.contains_group(group) {
                log::warn!(
                    "Player {} has unknown permission group {group}",
                    player.gameprofile.name
                );
            }
        }
        let effective = self
            .permission_groups
            .effective_permissions(&groups, &overrides);
        let effective_metadata = self
            .permission_groups
            .effective_metadata(&groups, &metadata_overrides);
        player.set_permission_state(
            groups,
            overrides,
            metadata_overrides,
            effective,
            effective_metadata,
        )
    }

    /// Returns one player's cached persisted permission state.
    #[must_use]
    pub fn player_permission_state(&self, uuid: Uuid) -> Option<PermissionSubjectState> {
        self.player_permission_states.read().get(uuid).cloned()
    }

    /// Returns whether the latest published subject state assigns the operator group.
    #[must_use]
    pub fn is_operator(&self, uuid: Uuid) -> bool {
        self.player_permission_states
            .read()
            .get(uuid)
            .is_some_and(|state| state.groups().iter().any(|group| group == OP_GROUP))
    }

    /// Captures effective command permissions from the latest published subject and group state.
    #[must_use]
    pub(crate) fn command_permission_snapshot(&self, uuid: Uuid) -> PermissionSet {
        let subject = self.player_permission_state(uuid).unwrap_or_default();
        self.permission_groups
            .effective_permissions(subject.groups(), subject.overrides())
    }

    /// Captures the permissions a command block runs its command with.
    ///
    /// Vanilla parity: the `LevelBasedPermissionSet.GAMEMASTER` a
    /// `BaseCommandBlock` builds its command source with. Foton resolves the
    /// `command_block` group instead, so the same authority is nameable and
    /// retunable; a config without that group falls back to the built-in
    /// definition rather than to nothing.
    #[must_use]
    pub(crate) fn command_block_permission_snapshot(&self) -> PermissionSet {
        let assigned = [COMMAND_BLOCK_GROUP.to_owned()];
        if self.permission_groups.contains_group(COMMAND_BLOCK_GROUP) {
            return self
                .permission_groups
                .effective_permissions(&assigned, &PermissionSet::default());
        }
        BUILT_IN_COMMAND_BLOCK_GROUPS
            .as_ref()
            .map(|groups| groups.effective_permissions(&assigned, &PermissionSet::default()))
            .unwrap_or_default()
    }

    /// Atomically edits one player's persisted permission state.
    ///
    /// Persistence completes before the cache is published. An online player is
    /// refreshed from the latest cached snapshot at the server job tick stage.
    ///
    /// # Errors
    ///
    /// Returns an edit error, an unknown newly assigned group, or a storage error.
    pub async fn try_update_player_permissions<T, E>(
        self: &Arc<Self>,
        uuid: Uuid,
        update: impl FnOnce(PermissionSubjectState) -> Result<(PermissionSubjectState, T), E> + Send,
    ) -> Result<(PermissionSubjectState, T, PersistenceUpdateOutcome), PlayerPermissionUpdateError<E>>
    where
        T: Send,
        E: Send,
    {
        self.try_update_player_permissions_if_current(uuid, || true, update)
            .await
    }

    pub(super) async fn try_update_player_permissions_if_current<T, E>(
        self: &Arc<Self>,
        uuid: Uuid,
        is_current: impl FnOnce() -> bool + Send,
        update: impl FnOnce(PermissionSubjectState) -> Result<(PermissionSubjectState, T), E> + Send,
    ) -> Result<(PermissionSubjectState, T, PersistenceUpdateOutcome), PlayerPermissionUpdateError<E>>
    where
        T: Send,
        E: Send,
    {
        let _guard = self.player_permission_updates.lock().await;
        if !is_current() {
            return Err(PlayerPermissionUpdateError::Superseded);
        }
        let mut states = self.player_permission_states.read().clone();
        let current = states.get(uuid).cloned().unwrap_or_default();
        let previous_groups = current.groups().to_vec();
        let (updated, result) = update(current).map_err(PlayerPermissionUpdateError::Edit)?;
        validate_player_permission_group_update(
            &self.permission_groups,
            &previous_groups,
            updated.groups(),
        )?;

        if updated.is_empty() {
            states.remove(uuid);
        } else {
            states.set(uuid, updated.clone());
        }
        let outcome = self
            .player_data_storage
            .save_permission_subjects(&states)
            .await?;

        *self.player_permission_states.write() = states;
        self.queue_player_permission_refresh(uuid);
        Ok((updated, result, outcome))
    }

    /// Replaces the complete permission group config and refreshes online players.
    ///
    /// # Errors
    ///
    /// Returns an error when validation or persistence fails.
    pub async fn replace_permission_groups(
        self: &Arc<Self>,
        config: PermissionGroupsConfig,
    ) -> Result<(), PermissionGroupManagerError> {
        self.permission_groups.replace_config(config).await?;
        self.queue_online_permission_group_refresh();
        Ok(())
    }

    /// Edits the latest permission group config and refreshes online players.
    ///
    /// # Errors
    ///
    /// Returns an error when validation or persistence fails.
    pub async fn update_permission_groups(
        self: &Arc<Self>,
        update: impl FnOnce(&mut PermissionGroupsConfig) + Send,
    ) -> Result<(), PermissionGroupManagerError> {
        self.permission_groups.update_config(update).await?;
        self.queue_online_permission_group_refresh();
        Ok(())
    }

    /// Applies a fallible permission group edit and refreshes online players.
    ///
    /// # Errors
    ///
    /// Returns the caller edit error or a validation/persistence error.
    pub async fn try_update_permission_groups<T, E>(
        self: &Arc<Self>,
        update: impl FnOnce(&mut PermissionGroupsConfig) -> Result<T, E> + Send,
    ) -> Result<T, PermissionGroupUpdateError<E>>
    where
        T: Send,
        E: Send,
    {
        let result = self.permission_groups.try_update_config(update).await?;
        self.queue_online_permission_group_refresh();
        Ok(result)
    }

    fn queue_player_permission_refresh(self: &Arc<Self>, uuid: Uuid) {
        self.jobs
            .spawn(FnServerJob::new(move |context: &mut ServerJobContext| {
                if let Some(server) = context.server() {
                    server.refresh_player_permission_state(uuid);
                }
            }));
    }

    pub(crate) fn refresh_player_permission_state(self: &Arc<Self>, uuid: Uuid) {
        let Some(player) = self.online_players.get_by_uuid(&uuid) else {
            return;
        };
        let state = self.player_permission_state(uuid).unwrap_or_default();
        self.apply_player_permission_state(&player, state);
        self.resend_player_permission_context(&player);
    }

    fn queue_online_permission_group_refresh(self: &Arc<Self>) {
        self.jobs
            .spawn(FnServerJob::new(|context: &mut ServerJobContext| {
                if let Some(server) = context.server() {
                    server.refresh_online_permission_groups();
                }
            }));
    }

    fn refresh_online_permission_groups(self: &Arc<Self>) {
        for player in self.get_players() {
            let state = self
                .player_permission_state(player.gameprofile.id)
                .unwrap_or_default();
            self.apply_player_permission_state(&player, state);
            self.resend_player_permission_context(&player);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::atomic::{AtomicBool, Ordering},
        thread,
        time::Duration,
    };
    use tokio::{sync::oneshot, task::yield_now, time::timeout};

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn update_admitted_from_non_runtime_thread_just_before_close_is_drained() {
        let tasks = Arc::new(PluginPersistenceTasks::new());
        let completed = Arc::new(AtomicBool::new(false));
        let runtime = Handle::current();
        let caller_tasks = Arc::clone(&tasks);
        let caller_completed = Arc::clone(&completed);

        let caller = thread::spawn(move || {
            assert!(Handle::try_current().is_err());
            caller_tasks.spawn_on(&runtime, move || async move {
                yield_now().await;
                caller_completed.store(true, Ordering::Release);
            })
        });
        assert!(caller.join().is_ok_and(|accepted| accepted));

        tasks.close();
        tasks.wait().await;
        assert!(completed.load(Ordering::Acquire));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn close_waits_for_update_already_writing() {
        let tasks = PluginPersistenceTasks::new();
        let (started_sender, started_receiver) = oneshot::channel();
        let (release_sender, release_receiver) = oneshot::channel();
        assert!(tasks.spawn_on(&Handle::current(), move || async move {
            let _ = started_sender.send(());
            let _ = release_receiver.await;
        }));
        assert!(started_receiver.await.is_ok());

        tasks.close();
        let drain = tasks.wait();
        tokio::pin!(drain);
        assert!(
            timeout(Duration::from_millis(20), drain.as_mut())
                .await
                .is_err()
        );

        assert!(release_sender.send(()).is_ok());
        assert!(timeout(Duration::from_secs(1), drain).await.is_ok());
    }

    #[tokio::test]
    async fn update_after_close_is_rejected_without_building_its_revision() {
        let tasks = PluginPersistenceTasks::new();
        let built = Arc::new(AtomicBool::new(false));
        tasks.close();

        let attempted = Arc::clone(&built);
        assert!(!tasks.spawn_on(&Handle::current(), move || {
            attempted.store(true, Ordering::Release);
            async {}
        }));
        tasks.wait().await;
        assert!(!built.load(Ordering::Acquire));
    }
}
