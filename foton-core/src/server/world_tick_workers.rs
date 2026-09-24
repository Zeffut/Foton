use std::{io, sync::Arc, thread};

use foton_utils::Identifier;

use crossbeam::channel::{self, Sender};
use thiserror::Error;
use tokio::sync::oneshot;

use crate::server::worlds::WorldMapSnapshot;
use crate::world::WorldGameTickTimings;

struct WorldTickRequest {
    tick_count: u64,
    runs_normally: bool,
    response: oneshot::Sender<WorldGameTickTimings>,
}

struct WorldTickWorker {
    world_key: Arc<str>,
    requests: Option<Sender<WorldTickRequest>>,
    thread: Option<thread::JoinHandle<()>>,
}

impl WorldTickWorker {
    fn spawn(index: usize, snapshot: WorldMapSnapshot) -> io::Result<Self> {
        let world = Arc::clone(snapshot.world());
        let world_key = Arc::<str>::from(world.key.to_string());
        let (request_sender, request_receiver) = channel::bounded::<WorldTickRequest>(1);
        let thread = thread::Builder::new()
            .name(format!("world-tick-{index}"))
            .spawn(move || {
                while let Ok(request) = request_receiver.recv() {
                    if request.runs_normally {
                        world.chunk_map.tick_timed_tickets();
                    }
                    let timings = world.tick_game(request.tick_count, request.runs_normally);
                    let _ = request.response.send(timings);
                }
            })?;

        Ok(Self {
            world_key,
            requests: Some(request_sender),
            thread: Some(thread),
        })
    }

    fn start_tick(
        &self,
        tick_count: u64,
        runs_normally: bool,
    ) -> Result<oneshot::Receiver<WorldGameTickTimings>, WorldTickWorkerError> {
        let (response, receiver) = oneshot::channel();
        let Some(requests) = &self.requests else {
            return Err(WorldTickWorkerError::Unavailable {
                world: Arc::clone(&self.world_key),
            });
        };
        requests
            .send(WorldTickRequest {
                tick_count,
                runs_normally,
                response,
            })
            .map_err(|_| WorldTickWorkerError::Unavailable {
                world: Arc::clone(&self.world_key),
            })?;
        Ok(receiver)
    }
}

impl Drop for WorldTickWorker {
    fn drop(&mut self) {
        drop(self.requests.take());
        let Some(thread) = self.thread.take() else {
            return;
        };
        if thread.join().is_err() {
            log::error!(
                "World tick worker for {} panicked during execution",
                self.world_key
            );
        }
    }
}

#[derive(Debug, Error)]
pub(super) enum WorldTickWorkerError {
    #[error("world tick worker for {world} is unavailable")]
    Unavailable { world: Arc<str> },
    #[error("world tick worker for {world} stopped without returning timings")]
    MissingResponse { world: Arc<str> },
}

pub(super) struct WorldTickWorkers {
    workers: Vec<WorldTickWorker>,
}

pub(super) struct PendingWorldTickWorkerRemoval<'a> {
    workers: &'a mut WorldTickWorkers,
    index: usize,
}

impl PendingWorldTickWorkerRemoval<'_> {
    pub(super) fn commit(self) {
        drop(self.workers.workers.remove(self.index));
    }
}

impl WorldTickWorkers {
    pub(super) fn spawn<S>(worlds: impl IntoIterator<Item = S>) -> io::Result<Self>
    where
        S: Into<WorldMapSnapshot>,
    {
        let mut workers = Vec::new();
        for (index, world) in worlds.into_iter().map(Into::into).enumerate() {
            workers.push(WorldTickWorker::spawn(index, world)?);
        }
        Ok(Self { workers })
    }

    /// Adds a worker at a tick safe-point.
    pub(super) fn add<S>(&mut self, world: S) -> io::Result<()>
    where
        S: Into<WorldMapSnapshot>,
    {
        let index = self.workers.len();
        self.workers
            .push(WorldTickWorker::spawn(index, world.into())?);
        Ok(())
    }

    /// Preflights a worker removal without stopping or detaching the worker.
    pub(super) fn prepare_removal(
        &mut self,
        key: &Identifier,
    ) -> Option<PendingWorldTickWorkerRemoval<'_>> {
        let key = key.to_string();
        let index = self
            .workers
            .iter()
            .position(|worker| worker.world_key.as_ref() == key)?;
        Some(PendingWorldTickWorkerRemoval {
            workers: self,
            index,
        })
    }

    pub(super) async fn tick_all(
        &self,
        tick_count: u64,
        runs_normally: bool,
    ) -> Result<Vec<WorldGameTickTimings>, WorldTickWorkerError> {
        let mut responses = Vec::with_capacity(self.workers.len());
        for worker in &self.workers {
            responses.push(worker.start_tick(tick_count, runs_normally)?);
        }

        let mut timings = Vec::with_capacity(responses.len());
        for (worker, response) in self.workers.iter().zip(responses) {
            timings.push(
                response
                    .await
                    .map_err(|_| WorldTickWorkerError::MissingResponse {
                        world: Arc::clone(&worker.world_key),
                    })?,
            );
        }
        Ok(timings)
    }
}

#[cfg(test)]
mod tests {
    use futures::executor::block_on;

    use super::WorldTickWorkers;
    use crate::test_support::fresh_test_world;

    #[test]
    fn removing_worker_joins_it_and_keeps_remaining_worlds_ticking() {
        let first = fresh_test_world("removable_worker_first");
        let second = fresh_test_world("removable_worker_second");
        let Ok(mut workers) = WorldTickWorkers::spawn([&first, &second]) else {
            panic!("world tick workers should start");
        };
        assert!(block_on(workers.tick_all(1, true)).is_ok());

        let Some(removal) = workers.prepare_removal(&first.key) else {
            panic!("first world tick worker should be removable");
        };
        removal.commit();
        assert!(workers.prepare_removal(&first.key).is_none());
        assert!(workers.add(&first).is_ok());
        let Ok(timings) = block_on(workers.tick_all(2, true)) else {
            panic!("remaining worker should finish its tick");
        };
        assert_eq!(timings.len(), 2);
        assert_eq!(first.game_time(), 2);
        assert_eq!(second.game_time(), 2);
    }

    #[test]
    fn persistent_workers_tick_every_world_across_boundaries() {
        let first = fresh_test_world("persistent_worker_first");
        let second = fresh_test_world("persistent_worker_second");
        let Ok(workers) = WorldTickWorkers::spawn([&first, &second]) else {
            panic!("world tick workers should start");
        };

        let Ok(first_tick) = block_on(workers.tick_all(1, true)) else {
            panic!("world tick workers should finish the first tick");
        };
        assert_eq!(first_tick.len(), 2);
        assert_eq!(first.game_time(), 1);
        assert_eq!(second.game_time(), 1);

        let Ok(second_tick) = block_on(workers.tick_all(2, true)) else {
            panic!("world tick workers should finish the second tick");
        };
        assert_eq!(second_tick.len(), 2);
        assert_eq!(first.game_time(), 2);
        assert_eq!(second.game_time(), 2);
    }
}
