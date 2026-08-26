//! The game-event listener that turns events into vibrations, and its ticker.

use std::sync::Arc;

use glam::DVec3;
use simdnbt::borrow::NbtCompound as NbtCompoundView;
use simdnbt::owned::NbtCompound;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::game_events::GameEventRef;
use steel_registry::particle_type::{ParticleData, VibrationParticleOption};
use steel_registry::vanilla_block_tags::BlockTag;
use steel_registry::{REGISTRY, TaggedRegistryExt as _, vanilla_particle_types};
use steel_utils::locks::SyncMutex;
use steel_utils::{BlockPos, ChunkPos, Direction};

use super::{VibrationData, VibrationInfo, VibrationUser};
use crate::world::World;
use crate::world::game_event::{GameEventContext, GameEventListener};
use crate::world::raycast::RaytraceAction;

/// Vanilla `VibrationSystem` -- its `Data`, its `Listener` and its `Ticker` in one place.
///
/// Vanilla splits the three across an interface implemented by the listening block entity or
/// mob, which then owns all of them. Rust has no such back-reference without a cycle, so the
/// listener owns the data and the user instead, and the block entity or mob owns the
/// listener. What each piece does is unchanged.
pub struct VibrationListener {
    data: SyncMutex<VibrationData>,
    user: Arc<dyn VibrationUser>,
}

impl VibrationListener {
    /// Creates a listener for `user` with no vibration in flight.
    #[must_use]
    pub fn new(user: Arc<dyn VibrationUser>) -> Self {
        Self {
            data: SyncMutex::new(VibrationData::default()),
            user,
        }
    }

    /// Returns the user this listener belongs to.
    #[must_use]
    pub fn user(&self) -> &Arc<dyn VibrationUser> {
        &self.user
    }

    /// Writes the vanilla `listener` compound.
    pub fn save(&self, nbt: &mut NbtCompound) {
        self.data.lock().save(nbt);
    }

    /// Reads the vanilla `listener` compound, or resets to a fresh listener without one.
    pub fn load(&self, nbt: Option<&NbtCompoundView<'_, '_>>) {
        *self.data.lock() = nbt.map_or_else(VibrationData::default, VibrationData::load);
    }

    /// Returns whether a vibration is currently travelling toward this listener.
    #[must_use]
    pub fn has_vibration_in_flight(&self) -> bool {
        self.data.lock().current_vibration().is_some()
    }

    /// Returns how many ticks the vibration in flight still needs.
    #[must_use]
    pub fn travel_time_in_ticks(&self) -> i32 {
        self.data.lock().travel_time_in_ticks()
    }

    /// Vanilla `VibrationSystem.Listener.forceScheduleVibration`.
    pub fn force_schedule_vibration(
        &self,
        world: &Arc<World>,
        event: GameEventRef,
        context: &GameEventContext<'_>,
        origin: DVec3,
    ) {
        let Some(destination) = self.user.position_source().resolve() else {
            return;
        };
        self.schedule_vibration(world, event, context, origin, destination);
    }

    /// Vanilla `VibrationSystem.Listener.scheduleVibration`.
    fn schedule_vibration(
        &self,
        world: &Arc<World>,
        event: GameEventRef,
        context: &GameEventContext<'_>,
        origin: DVec3,
        destination: DVec3,
    ) {
        let candidate = VibrationInfo::new(
            event,
            origin.distance(destination) as f32,
            origin,
            context.source_entity(),
            world,
        );
        self.data
            .lock()
            .selection_strategy()
            .add_candidate(candidate, world.game_time());
    }

    /// Vanilla `VibrationSystem.Ticker.tick`.
    ///
    /// The lock on the data is taken and released around every user callback: a callback can
    /// re-enter this listener through a game event of its own, and vanilla's data is visibly
    /// unchanged for the whole of `onReceiveVibration`.
    pub fn tick(&self, world: &Arc<World>) {
        if !self.has_vibration_in_flight() {
            self.try_select_and_schedule_vibration(world);
        }
        if !self.has_vibration_in_flight() {
            return;
        }

        let mut has_changed = self.travel_time_in_ticks() > 0;
        self.try_reload_vibration_particle(world);
        self.data.lock().decrement_travel_time();
        if self.travel_time_in_ticks() <= 0 {
            has_changed = self.receive_vibration(world);
        }

        if has_changed {
            self.user.on_data_changed();
        }
    }

    /// Vanilla `VibrationSystem.Ticker.trySelectAndScheduleVibration`.
    fn try_select_and_schedule_vibration(&self, world: &Arc<World>) {
        let game_time = world.game_time();
        let Some((origin, travel_time_in_ticks)) = ({
            let mut data = self.data.lock();
            let chosen = data
                .selection_strategy()
                .chosen_candidate(game_time)
                .cloned();
            chosen.map(|chosen| {
                let origin = chosen.pos();
                let travel_time_in_ticks =
                    self.user.calculate_travel_time_in_ticks(chosen.distance());
                data.set_current_vibration(Some(chosen));
                data.set_travel_time_in_ticks(travel_time_in_ticks);
                (origin, travel_time_in_ticks)
            })
        }) else {
            return;
        };

        self.send_vibration_particle(world, origin, travel_time_in_ticks);
        self.user.on_data_changed();
        self.data.lock().selection_strategy().start_over();
    }

    /// Vanilla `VibrationSystem.Ticker.tryReloadVibrationParticle`.
    ///
    /// A vibration loaded from disk was already in flight, so its particle has to be sent
    /// again from wherever it has reached by now.
    fn try_reload_vibration_particle(&self, world: &Arc<World>) {
        let Some((origin, distance, travel_time_in_ticks)) = ({
            let data = self.data.lock();
            if !data.should_reload_vibration_particle() {
                return;
            }
            data.current_vibration().map(|current| {
                (
                    current.pos(),
                    current.distance(),
                    data.travel_time_in_ticks(),
                )
            })
        }) else {
            self.data.lock().set_reload_vibration_particle(false);
            return;
        };

        let destination = self.user.position_source().resolve().unwrap_or(origin);
        let initial_travel_time = self.user.calculate_travel_time_in_ticks(distance);
        // Vanilla divides by the initial travel time without guarding it; a vibration born
        // less than a block away has an initial time of zero, which would put the particle at
        // NaN. Such a vibration is already at its destination, so send it from there.
        let alpha = if initial_travel_time > 0 {
            1.0 - f64::from(travel_time_in_ticks) / f64::from(initial_travel_time)
        } else {
            1.0
        };
        let particle_was_sent = self.send_vibration_particle(
            world,
            origin.lerp(destination, alpha),
            travel_time_in_ticks,
        ) > 0;
        if particle_was_sent {
            self.data.lock().set_reload_vibration_particle(false);
        }
    }

    fn send_vibration_particle(
        &self,
        world: &Arc<World>,
        position: DVec3,
        arrival_in_ticks: i32,
    ) -> i32 {
        world.send_particles(
            ParticleData::new(
                &vanilla_particle_types::VIBRATION,
                VibrationParticleOption::new(
                    self.user.position_source().to_particle_source(),
                    arrival_in_ticks,
                ),
            ),
            position,
            1,
            DVec3::ZERO,
            0.0,
        )
    }

    /// Vanilla `VibrationSystem.Ticker.receiveVibration`.
    fn receive_vibration(&self, world: &Arc<World>) -> bool {
        let Some(current_vibration) = self.data.lock().current_vibration().cloned() else {
            return false;
        };

        let origin = BlockPos::from(current_vibration.pos());
        let destination = self
            .user
            .position_source()
            .resolve()
            .map_or(origin, BlockPos::from);
        if self.user.requires_adjacent_chunks_to_be_ticking()
            && !adjacent_chunks_ticking(world, destination)
        {
            return false;
        }

        let source_entity = current_vibration.get_entity(world);
        let projectile_owner = current_vibration.get_projectile_owner(world);
        self.user.on_receive_vibration(
            world,
            origin,
            current_vibration.game_event(),
            source_entity.as_deref(),
            projectile_owner.as_deref(),
            distance_between_in_blocks(origin, destination),
        );
        self.data.lock().set_current_vibration(None);
        true
    }
}

impl GameEventListener for VibrationListener {
    fn listener_pos(&self) -> Option<DVec3> {
        self.user.position_source().resolve()
    }

    fn listener_radius(&self) -> i32 {
        self.user.listener_radius()
    }

    /// Vanilla `VibrationSystem.Listener.handleGameEvent`.
    fn handle_game_event(
        &self,
        world: &Arc<World>,
        event: GameEventRef,
        context: &GameEventContext<'_>,
        source_pos: DVec3,
    ) -> bool {
        if self.has_vibration_in_flight() {
            return false;
        }
        if !self.user.is_valid_vibration(event, context) {
            return false;
        }
        let Some(destination) = self.user.position_source().resolve() else {
            return false;
        };
        if !self
            .user
            .can_receive_vibration(world, BlockPos::from(source_pos), event, context)
        {
            return false;
        }
        if is_occluded(world, source_pos, destination) {
            return false;
        }

        self.schedule_vibration(world, event, context, source_pos, destination);
        true
    }
}

/// Vanilla `VibrationSystem.Listener.distanceBetweenInBlocks`.
#[must_use]
pub fn distance_between_in_blocks(origin: BlockPos, destination: BlockPos) -> f32 {
    let delta = origin.0 - destination.0;
    let distance_sq = f64::from(delta.x) * f64::from(delta.x)
        + f64::from(delta.y) * f64::from(delta.y)
        + f64::from(delta.z) * f64::from(delta.z);
    distance_sq.sqrt() as f32
}

/// Vanilla `VibrationSystem.Listener.isOccluded`.
///
/// Wool between the source and the listener stops the vibration, but only when it stops
/// every one of the six rays leaving the source block, which is why a single wool block
/// beside a sensor does not deafen it.
fn is_occluded(world: &Arc<World>, origin: DVec3, destination: DVec3) -> bool {
    let from = block_center(origin);
    let to = block_center(destination);

    for direction in Direction::ALL {
        let nudged_source = from + direction.offset_vec().as_dvec3() * 1.0e-5;
        let (hit, _) = world.raytrace(nudged_source, to, |pos, world| {
            if REGISTRY.blocks.is_in_tag(
                world.get_block_state(pos).get_block(),
                &BlockTag::OCCLUDES_VIBRATION_SIGNALS,
            ) {
                RaytraceAction::ImmediateHit
            } else {
                RaytraceAction::Pass
            }
        });
        if hit.is_none() {
            return false;
        }
    }

    true
}

fn block_center(position: DVec3) -> DVec3 {
    DVec3::new(
        position.x.floor() + 0.5,
        position.y.floor() + 0.5,
        position.z.floor() + 0.5,
    )
}

/// Vanilla `VibrationSystem.Ticker.areAdjacentChunksTicking`.
fn adjacent_chunks_ticking(world: &Arc<World>, listener_pos: BlockPos) -> bool {
    let listener_chunk_pos = ChunkPos::from_block_pos(listener_pos);
    for x in listener_chunk_pos.0.x - 1..=listener_chunk_pos.0.x + 1 {
        for z in listener_chunk_pos.0.y - 1..=listener_chunk_pos.0.y + 1 {
            if !world
                .chunk_map
                .is_block_ticking_full_chunk_loaded(ChunkPos::new(x, z))
            {
                return false;
            }
        }
    }
    true
}
