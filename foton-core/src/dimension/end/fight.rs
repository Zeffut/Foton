//! The orchestrator of the End.
//!
//! Vanilla parity: `EnderDragonFight`, a `SavedData` the End level builds in
//! its constructor and ticks before its entities. Foton keeps it on
//! [`World`](crate::world::World) the way vanilla keeps it on `ServerLevel`,
//! and persists it as `data/ender_dragon_fight.toml` beside `raids.toml`.
//!
//! Almost nothing about the End works without it. The dragon asks it how many
//! crystals are still standing, and its pathfinder branches on the answer; the
//! podium and the exit portal have no other caller; the twelve thousand
//! experience of a first kill and the dragon egg are its to award; and the
//! four-crystal ritual is a state machine it owns. See
//! [`DragonRespawnStage`](super::DragonRespawnStage) for the ritual itself.

use std::f64::consts::PI;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};

use foton_registry::REGISTRY;
use foton_registry::blocks::block_state_ext::BlockStateExt as _;
use foton_registry::feature::EndSpike;
use foton_registry::{
    level_events, vanilla_block_entity_types, vanilla_blocks, vanilla_configured_features,
    vanilla_entities,
};
use foton_utils::locks::SyncMutex;
use foton_utils::random::worldgen_random::WorldgenRandom;
use foton_utils::random::{Random as _, legacy_random::LegacyRandom};
use foton_utils::types::UpdateFlags;
use foton_utils::{BlockPos, ChunkPos, Direction, Downcast as _, SectionPos, WorldAabb};
use glam::DVec3;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::respawn_stage::DragonRespawnStage;
use crate::boss_event::ServerBossEvent;
use crate::chunk::heightmap::HeightmapType;
use crate::entity::damage::DamageSource;
use crate::entity::entities::mobs::bosses::ender_dragon::phases::EnderDragonPhase;
use crate::entity::entities::{EndCrystalEntity, EnderDragon};
use crate::entity::{
    ENTITIES, Entity, LivingEntity, RemovalReason, SharedEntity, entity_type_name, next_entity_id,
};
use crate::world::block_pattern::{
    BlockPattern, BlockPatternBuilder, BlockPatternMatch, has_state,
};
use crate::world::{LevelAccessor as _, LevelReader as _, World};
use crate::worldgen::feature::FeatureDecorationRunner;
use crate::worldgen::feature::features::end_podium;

use foton_protocol::packets::game::{BossBarColor, BossBarOverlay};

/// How long the fight waits for its dragon before assuming it is gone.
///
/// Vanilla parity: `EnderDragonFight.MAX_TICKS_BEFORE_DRAGON_RESPAWN`.
const MAX_TICKS_BEFORE_DRAGON_RESPAWN: i32 = 1200;

/// Vanilla parity: `EnderDragonFight.TIME_BETWEEN_CRYSTAL_SCANS`.
const TIME_BETWEEN_CRYSTAL_SCANS: i32 = 100;

/// Vanilla parity: `EnderDragonFight.TIME_BETWEEN_PLAYER_SCANS`.
const TIME_BETWEEN_PLAYER_SCANS: i32 = 20;

/// Half-width of the arena, in chunks.
///
/// Vanilla parity: `EnderDragonFight.ARENA_SIZE_CHUNKS`.
const ARENA_SIZE_CHUNKS: i32 = 8;

/// Full-chunk radius the arena ticket holds loaded and simulated.
///
/// Vanilla parity: the `addTicketWithRadius(TicketType.DRAGON, ChunkPos.ZERO, 9)`
/// of `tick`. Vanilla's `TicketType.DRAGON` loads *and* simulates, which is why
/// this is a simulation ticket rather than a loading one.
const ARENA_TICKET_RADIUS: u8 = 9;

/// How many gateways a world has to give out, one per dragon killed.
///
/// Vanilla parity: `EnderDragonFight.GATEWAY_COUNT`.
const GATEWAY_COUNT: i32 = 20;

/// How far from the origin the gateways stand.
///
/// Vanilla parity: `EnderDragonFight.GATEWAY_DISTANCE`.
const GATEWAY_DISTANCE: f64 = 96.0;

/// Y the gateways stand at.
///
/// Vanilla parity: the `new BlockPos(x, 75, z)` of `spawnNewGateway`.
const GATEWAY_Y: i32 = 75;

/// How high above the origin a new dragon appears.
///
/// Vanilla parity: `EnderDragonFight.DRAGON_SPAWN_Y`.
const DRAGON_SPAWN_Y: i32 = 128;

/// How far from the arena a player still sees the boss bar.
///
/// Vanilla parity: the `withinDistance(origin.getX(), 128 + origin.getY(),
/// origin.getZ(), 192.0)` of `init`.
const BOSS_BAR_RANGE: f64 = 192.0;

/// Lowest Y the exit portal is allowed to sink to while looking for ground.
///
/// Vanilla parity: the `exitPortalLocation.getY() > 63` of `spawnExitPortal`.
const EXIT_PORTAL_MIN_SEARCH_Y: i32 = 63;

/// Bottom of any dimension, and so the bottom of a spike's crystal box.
///
/// Vanilla parity: `DimensionType.MIN_Y`, which `EndSpike` uses to make its top
/// bounding box span the whole column.
const DIMENSION_MIN_Y: f64 = -2032.0;

/// Top of any dimension.
///
/// Vanilla parity: `DimensionType.MAX_Y`.
const DIMENSION_MAX_Y: f64 = 2031.0;

/// The fight one End is running.
///
/// Vanilla parity: `EnderDragonFight`.
pub struct EnderDragonFight {
    /// Vanilla parity: `EnderDragonFight.origin`, always `BlockPos.ZERO` in an
    /// unmodified game -- `ServerLevel` passes it in its constructor.
    origin: BlockPos,
    /// The purple bar with the notches in it.
    ///
    /// Vanilla parity: `EnderDragonFight.dragonEvent`. The dragon does not own
    /// a bar of its own in vanilla, and does not in Foton either: membership is
    /// by distance from the arena, not by who can see the entity.
    dragon_event: ServerBossEvent,
    /// Vanilla parity: `EnderDragonFight.exitPortalPattern`.
    exit_portal_pattern: BlockPattern,
    /// Vanilla parity: `EnderDragonFight.aliveCrystals`.
    ///
    /// Kept out of [`FightState`] because the dragon reads it from its own tick
    /// -- through [`EnderDragon::alive_crystals`] and again from inside the
    /// pathfinder -- and taking the fight's lock from there would be one more
    /// thing to reason about for a number that is recomputed from scratch every
    /// five seconds and never persisted.
    alive_crystals: AtomicI32,
    /// Whether the arena chunk ticket is currently held.
    ///
    /// Vanilla adds the ticket on every tick a player is on the bar and lets
    /// its ticket set collapse the duplicates. Foton's ticket store keeps one
    /// entry per `add`, so the fight tracks the edge itself and adds or removes
    /// once. Same chunks held, same ticks.
    arena_ticket_held: AtomicBool,
    state: SyncMutex<FightState>,
}

/// The fight's own mutable state.
struct FightState {
    /// Vanilla parity: `EnderDragonFight.ticksSinceDragonSeen`.
    ticks_since_dragon_seen: i32,
    /// Vanilla parity: `EnderDragonFight.ticksSinceCrystalsScanned`.
    ticks_since_crystals_scanned: i32,
    /// Vanilla parity: `EnderDragonFight.ticksSinceLastPlayerScan`, which
    /// vanilla starts at 21 so the first tick scans.
    ticks_since_last_player_scan: i32,
    /// Vanilla parity: `EnderDragonFight.dragonKilled`.
    dragon_killed: bool,
    /// Vanilla parity: `EnderDragonFight.hasPreviouslyKilledDragon`.
    has_previously_killed_dragon: bool,
    /// Vanilla parity: `EnderDragonFight.dragonUUID`.
    dragon_uuid: Option<Uuid>,
    /// Vanilla parity: `EnderDragonFight.needsStateScanning`.
    needs_state_scanning: bool,
    /// Vanilla parity: `EnderDragonFight.exitPortalLocation`.
    exit_portal_location: Option<BlockPos>,
    /// Vanilla parity: `EnderDragonFight.respawnStage`.
    respawn_stage: Option<DragonRespawnStage>,
    /// Vanilla parity: `EnderDragonFight.respawnTime`.
    respawn_time: i32,
    /// Vanilla parity: `EnderDragonFight.respawnCrystals`, held as UUIDs
    /// because that is what an `EntityReference` resolves through.
    respawn_crystals: Vec<Uuid>,
    /// Vanilla parity: `EnderDragonFight.gateways`, shuffled once and then
    /// popped from the back, one per dragon killed.
    gateways: Vec<i32>,
}

impl EnderDragonFight {
    /// Rebuilds a fight from its saved data and centers it.
    ///
    /// Vanilla parity: the codec constructor plus `EnderDragonFight.init`,
    /// which `ServerLevel` calls immediately after loading the saved data
    /// with the level, its seed and `BlockPos.ZERO`. Foton has no separate
    /// init step because a `World` never hands out a fight before it has
    /// one, and the level itself arrives per call rather than being held.
    #[must_use]
    pub fn from_persistent(
        persistent: PersistentEnderDragonFight,
        seed: i64,
        origin: BlockPos,
    ) -> Self {
        let dragon_event = ServerBossEvent::with_random_id(
            entity_type_name(&vanilla_entities::ENDER_DRAGON),
            BossBarColor::Pink,
            BossBarOverlay::Progress,
        );
        dragon_event.set_play_boss_music(true);
        dragon_event.set_create_world_fog(true);

        let mut gateways = persistent.gateways;
        if gateways.is_empty() {
            gateways = Self::shuffled_gateways(seed);
        }

        Self {
            origin,
            dragon_event,
            exit_portal_pattern: exit_portal_pattern(),
            alive_crystals: AtomicI32::new(0),
            arena_ticket_held: AtomicBool::new(false),
            state: SyncMutex::new(FightState {
                ticks_since_dragon_seen: 0,
                ticks_since_crystals_scanned: 0,
                ticks_since_last_player_scan: TIME_BETWEEN_PLAYER_SCANS + 1,
                dragon_killed: persistent.dragon_killed,
                has_previously_killed_dragon: persistent.previously_killed,
                dragon_uuid: persistent.dragon_uuid,
                needs_state_scanning: persistent.needs_state_scanning,
                exit_portal_location: persistent
                    .exit_portal_location
                    .map(|pos| BlockPos::new(pos[0], pos[1], pos[2])),
                respawn_stage: persistent.respawn_stage,
                respawn_time: persistent.respawn_time,
                respawn_crystals: persistent.respawn_crystals,
                gateways,
            }),
        }
    }

    /// Snapshots the fight for its saved data.
    #[must_use]
    pub fn to_persistent(&self) -> PersistentEnderDragonFight {
        let state = self.state.lock();
        PersistentEnderDragonFight {
            needs_state_scanning: state.needs_state_scanning,
            dragon_killed: state.dragon_killed,
            previously_killed: state.has_previously_killed_dragon,
            respawn_stage: state.respawn_stage,
            respawn_time: state.respawn_time,
            dragon_uuid: state.dragon_uuid,
            exit_portal_location: state
                .exit_portal_location
                .map(|pos| [pos.x(), pos.y(), pos.z()]),
            gateways: state.gateways.clone(),
            respawn_crystals: state.respawn_crystals.clone(),
        }
    }

    /// Shuffles the twenty gateway slots for a world.
    ///
    /// Vanilla parity: the `Util.shuffle(newGateways,
    /// RandomSource.createThreadLocalInstance(seed))` of `init`, which is the
    /// same back-to-front swap `EndSpikeFeature` uses for its pillar sizes.
    fn shuffled_gateways(seed: i64) -> Vec<i32> {
        let mut random = LegacyRandom::from_seed(seed as u64);
        let mut gateways: Vec<i32> = (0..GATEWAY_COUNT).collect();
        for bound in (2..=GATEWAY_COUNT).rev() {
            let swap_to = random.next_i32_bounded(bound) as usize;
            gateways.swap(bound as usize - 1, swap_to);
        }
        gateways
    }

    /// Returns where this fight is centered.
    #[must_use]
    pub const fn origin(&self) -> BlockPos {
        self.origin
    }

    /// Returns how many pillar crystals are still standing.
    ///
    /// Vanilla parity: `EnderDragonFight.aliveCrystals`.
    #[must_use]
    pub fn alive_crystals(&self) -> i32 {
        self.alive_crystals.load(Ordering::Relaxed)
    }

    /// Returns the bar the players in the arena see.
    #[must_use]
    pub const fn boss_event(&self) -> &ServerBossEvent {
        &self.dragon_event
    }

    /// Returns which dragon this fight is following.
    ///
    /// Vanilla parity: `EnderDragonFight.dragonUUID`.
    #[must_use]
    pub fn dragon_uuid(&self) -> Option<Uuid> {
        self.state.lock().dragon_uuid
    }

    /// Returns whether a dragon has ever died in this world.
    ///
    /// Vanilla parity: `EnderDragonFight.hasPreviouslyKilledDragon`, which is
    /// what decides between five hundred experience and twelve thousand.
    #[must_use]
    pub fn has_previously_killed_dragon(&self) -> bool {
        self.state.lock().has_previously_killed_dragon
    }

    /// Returns whether the fight considers its dragon dead.
    #[must_use]
    pub fn is_dragon_killed(&self) -> bool {
        self.state.lock().dragon_killed
    }

    /// Returns the stage a running respawn ritual is in.
    ///
    /// Vanilla parity: `EnderDragonFight.respawnStage`, which vanilla only ever
    /// reads through `setRespawnStage`'s null check.
    #[must_use]
    pub fn respawn_stage(&self) -> Option<DragonRespawnStage> {
        self.state.lock().respawn_stage
    }

    /// Runs one tick of the fight.
    ///
    /// Vanilla parity: `EnderDragonFight.tick`, called from `ServerLevel.tick`
    /// just ahead of the entity tick.
    pub fn tick(&self, world: &Arc<World>) {
        self.dragon_event
            .set_visible(!self.state.lock().dragon_killed);

        let scan_players = {
            let mut state = self.state.lock();
            state.ticks_since_last_player_scan += 1;
            if state.ticks_since_last_player_scan >= TIME_BETWEEN_PLAYER_SCANS {
                state.ticks_since_last_player_scan = 0;
                true
            } else {
                false
            }
        };
        if scan_players {
            self.update_players(world);
        }

        if !self.dragon_event.has_players() {
            self.hold_arena_ticket(world, false);
            return;
        }

        self.hold_arena_ticket(world, true);
        if !self.is_arena_loaded(world) {
            return;
        }

        if self.state.lock().needs_state_scanning {
            self.scan_state(world);
            self.state.lock().needs_state_scanning = false;
        }

        if !self.tick_respawn_stage(world) {
            return;
        }

        if self.state.lock().dragon_killed {
            return;
        }

        let look_for_dragon = {
            let mut state = self.state.lock();
            state.ticks_since_dragon_seen += 1;
            state.dragon_uuid.is_none()
                || state.ticks_since_dragon_seen >= MAX_TICKS_BEFORE_DRAGON_RESPAWN
        };
        if look_for_dragon {
            self.find_or_create_dragon(world);
            self.state.lock().ticks_since_dragon_seen = 0;
        }

        let scan_crystals = {
            let mut state = self.state.lock();
            state.ticks_since_crystals_scanned += 1;
            state.ticks_since_crystals_scanned >= TIME_BETWEEN_CRYSTAL_SCANS
        };
        if scan_crystals {
            self.update_crystal_count(world);
        }
    }

    /// Takes or drops the arena chunk ticket, once per transition.
    fn hold_arena_ticket(&self, world: &Arc<World>, held: bool) {
        if self.arena_ticket_held.swap(held, Ordering::Relaxed) == held {
            return;
        }

        let center = arena_center(self.origin);
        if held {
            world
                .chunk_map
                .add_arena_ticket(center, ARENA_TICKET_RADIUS);
        } else {
            world
                .chunk_map
                .remove_arena_ticket(center, ARENA_TICKET_RADIUS);
        }
    }

    /// Advances a running ritual by one tick.
    ///
    /// Vanilla parity: the `if (this.respawnStage != null)` block of `tick`.
    /// Returns whether the caller should carry on into the dragon checks, which
    /// vanilla expresses by returning out of `tick` when the ritual's crystals
    /// have all been broken.
    fn tick_respawn_stage(&self, world: &Arc<World>) -> bool {
        let (stage, time, crystal_uuids) = {
            let fight = self.state.lock();
            let Some(stage) = fight.respawn_stage else {
                return true;
            };
            (stage, fight.respawn_time, fight.respawn_crystals.clone())
        };

        let crystals: Vec<SharedEntity> = crystal_uuids
            .iter()
            .filter_map(|uuid| world.get_entity_by_uuid(uuid))
            .filter(|entity| entity.downcast_ref::<EndCrystalEntity>().is_some())
            .collect();
        if crystals.is_empty() {
            self.abort_respawn_sequence(world);
            return false;
        }

        self.state.lock().respawn_time += 1;
        stage.tick(world, self, &crystals, time);
        true
    }

    /// Works out what a world that has never run a fight is already in.
    ///
    /// Vanilla parity: `EnderDragonFight.scanState`.
    fn scan_state(&self, world: &Arc<World>) {
        let active_portal_exists = Self::has_active_exit_portal(world);
        if active_portal_exists {
            log::info!("Found that the dragon has been killed in this world already.");
            self.state.lock().has_previously_killed_dragon = true;
        } else {
            log::info!("Found that the dragon has not yet been killed in this world.");
            self.state.lock().has_previously_killed_dragon = false;
            if self.find_exit_portal(world).is_none() {
                self.spawn_exit_portal(world, false);
            }
        }

        let dragons = dragons_in(world);
        if let Some(dragon) = dragons.first() {
            log::info!("Found that there's a dragon still alive ({})", dragon.id());
            let mut state = self.state.lock();
            state.dragon_uuid = Some(dragon.uuid());
            state.dragon_killed = false;
            drop(state);
            if !active_portal_exists {
                log::info!("But we didn't have a portal, let's remove it.");
                dragon.set_removed(RemovalReason::Discarded);
                self.state.lock().dragon_uuid = None;
            }
        } else {
            self.state.lock().dragon_killed = true;
        }

        let mut state = self.state.lock();
        if !state.has_previously_killed_dragon && state.dragon_killed {
            state.dragon_killed = false;
        }
    }

    /// Vanilla parity: `EnderDragonFight.findOrCreateDragon`.
    fn find_or_create_dragon(&self, world: &Arc<World>) {
        let dragons = dragons_in(world);
        if let Some(dragon) = dragons.first() {
            self.state.lock().dragon_uuid = Some(dragon.uuid());
        } else {
            self.create_new_dragon(world);
        }
    }

    /// Moves the ritual to its next stage.
    ///
    /// Vanilla parity: `EnderDragonFight.setRespawnStage`. Reaching
    /// [`DragonRespawnStage::End`] is what actually brings the dragon back.
    ///
    /// # Panics
    ///
    /// Panics when no ritual is running, the way vanilla throws: the stages are
    /// the only callers, and one of them running with no ritual would mean the
    /// fight lost its own state mid-tick.
    pub fn set_respawn_stage(&self, world: &Arc<World>, stage: DragonRespawnStage) {
        let mut fight = self.state.lock();
        assert!(
            fight.respawn_stage.is_some(),
            "dragon respawn isn't in progress, can't skip ahead in the animation"
        );
        fight.respawn_time = 0;
        if stage != DragonRespawnStage::End {
            fight.respawn_stage = Some(stage);
            return;
        }

        fight.respawn_stage = None;
        fight.dragon_killed = false;
        drop(fight);
        self.create_new_dragon(world);
    }

    /// Returns whether the arena already holds a lit exit portal.
    ///
    /// Vanilla parity: `EnderDragonFight.hasActiveExitPortal`. Vanilla scans
    /// chunks `-8..=8` here rather than around the origin -- the only place in
    /// the class that does -- and that is kept.
    fn has_active_exit_portal(world: &Arc<World>) -> bool {
        for x in -ARENA_SIZE_CHUNKS..=ARENA_SIZE_CHUNKS {
            for z in -ARENA_SIZE_CHUNKS..=ARENA_SIZE_CHUNKS {
                let found = world
                    .chunk_map
                    .with_full_chunk(ChunkPos::new(x, z), |chunk| {
                        chunk.get_block_entities().iter().any(|block_entity| {
                            block_entity.get_type() == &vanilla_block_entity_types::END_PORTAL
                        })
                    })
                    .unwrap_or(false);
                if found {
                    return true;
                }
            }
        }

        false
    }

    /// Finds the exit portal's bedrock frame, and remembers where it stands.
    ///
    /// Vanilla parity: `EnderDragonFight.findExitPortal`.
    fn find_exit_portal<'level>(
        &self,
        world: &'level Arc<World>,
    ) -> Option<BlockPatternMatch<'level>> {
        let level: &'level World = world.as_ref();
        let chunk_origin = ChunkPos::from_block_pos(self.origin);

        for x in chunk_origin.0.x - ARENA_SIZE_CHUNKS..=chunk_origin.0.x + ARENA_SIZE_CHUNKS {
            for z in chunk_origin.0.y - ARENA_SIZE_CHUNKS..=chunk_origin.0.y + ARENA_SIZE_CHUNKS {
                let portal_positions = world
                    .chunk_map
                    .with_full_chunk(ChunkPos::new(x, z), |chunk| {
                        chunk
                            .get_block_entities()
                            .iter()
                            .filter(|block_entity| {
                                block_entity.get_type() == &vanilla_block_entity_types::END_PORTAL
                            })
                            .map(|block_entity| block_entity.get_block_pos())
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();

                for pos in portal_positions {
                    if let Some(found) = self.remember_exit_portal(level, pos) {
                        return Some(found);
                    }
                }
            }
        }

        let podium = end_podium::location(self.origin);
        let max_y = world
            .heightmap_pos(HeightmapType::MotionBlocking, podium)
            .y();
        for y in (level.min_y()..=max_y).rev() {
            if let Some(found) =
                self.remember_exit_portal(level, BlockPos::new(podium.x(), y, podium.z()))
            {
                return Some(found);
            }
        }

        None
    }

    /// Vanilla parity: the shared body of both `findExitPortal` searches -- run
    /// the pattern, and record the center block the first time one matches.
    fn remember_exit_portal<'level>(
        &self,
        level: &'level World,
        pos: BlockPos,
    ) -> Option<BlockPatternMatch<'level>> {
        let found = self.exit_portal_pattern.find(level, pos)?;
        let mut state = self.state.lock();
        if state.exit_portal_location.is_none() {
            state.exit_portal_location = Some(found.block(3, 3, 3).pos());
        }
        Some(found)
    }

    /// Returns whether the arena is loaded far enough to run the fight in.
    ///
    /// Vanilla parity: `EnderDragonFight.isArenaLoaded`. Vanilla's `z` loop runs
    /// from `8 + chunkOrigin.z()` rather than `-8 + chunkOrigin.z()`, so only a
    /// single row of chunks is ever checked; that is a vanilla bug, and it is
    /// kept because widening it would hold the fight back on worlds vanilla
    /// starts.
    fn is_arena_loaded(&self, world: &Arc<World>) -> bool {
        let chunk_origin = ChunkPos::from_block_pos(self.origin);
        for x in chunk_origin.0.x - ARENA_SIZE_CHUNKS..=chunk_origin.0.x + ARENA_SIZE_CHUNKS {
            let z = chunk_origin.0.y + ARENA_SIZE_CHUNKS;
            if !world
                .chunk_map
                .is_block_ticking_full_chunk_loaded(ChunkPos::new(x, z))
            {
                return false;
            }
        }

        for x in chunk_origin.0.x - 1..=chunk_origin.0.x + 1 {
            for z in chunk_origin.0.y - 1..=chunk_origin.0.y + 1 {
                if !world.entity_manager().is_chunk_loaded(ChunkPos::new(x, z)) {
                    return false;
                }
            }
        }

        true
    }

    /// Puts every player near the arena on the boss bar, and takes the rest off.
    ///
    /// Vanilla parity: `EnderDragonFight.updatePlayers`.
    fn update_players(&self, world: &Arc<World>) {
        let center = DVec3::new(
            f64::from(self.origin.x()),
            f64::from(DRAGON_SPAWN_Y + self.origin.y()),
            f64::from(self.origin.z()),
        );
        let range_sqr = BOSS_BAR_RANGE * BOSS_BAR_RANGE;

        let mut in_range = Vec::new();
        world.players.iter_players(|_, player| {
            if Entity::is_alive(player.as_ref())
                && player.position().distance_squared(center) <= range_sqr
            {
                in_range.push(Arc::clone(player));
            }
            true
        });

        for player in &in_range {
            self.dragon_event.add_player(player);
        }

        for player in self.dragon_event.players() {
            if !in_range.iter().any(|kept| kept.id() == player.id()) {
                self.dragon_event.remove_player(&player);
            }
        }
    }

    /// Recounts the crystals still standing on the pillars.
    ///
    /// Vanilla parity: `EnderDragonFight.updateCrystalCount`.
    fn update_crystal_count(&self, world: &Arc<World>) {
        self.state.lock().ticks_since_crystals_scanned = 0;
        let mut alive = 0;
        for spike in FeatureDecorationRunner::end_spikes_for_level(world.seed()) {
            alive += world
                .get_entities_in_aabb(&spike_top_bounding_box(&spike))
                .iter()
                .filter(|entity| entity.downcast_ref::<EndCrystalEntity>().is_some())
                .count() as i32;
        }
        self.alive_crystals.store(alive, Ordering::Relaxed);
    }

    /// Closes the fight out: the bar goes, the portal opens, the egg drops.
    ///
    /// Vanilla parity: `EnderDragonFight.setDragonKilled`.
    pub fn set_dragon_killed(&self, world: &Arc<World>, dragon: &EnderDragon) {
        if self.state.lock().dragon_uuid != Some(dragon.uuid()) {
            return;
        }

        self.dragon_event.set_progress(0.0);
        self.dragon_event.set_visible(false);
        self.spawn_exit_portal(world, true);
        self.spawn_new_gateway(world);

        if !self.state.lock().has_previously_killed_dragon {
            let egg = world.heightmap_pos(
                HeightmapType::MotionBlocking,
                end_podium::location(self.origin),
            );
            world.set_block_state(
                egg,
                vanilla_blocks::DRAGON_EGG.default_state(),
                UpdateFlags::UPDATE_ALL,
            );
        }

        let mut state = self.state.lock();
        state.has_previously_killed_dragon = true;
        state.dragon_killed = true;
    }

    /// Opens the next gateway of the twenty.
    ///
    /// Vanilla parity: the no-argument `EnderDragonFight.spawnNewGateway`.
    fn spawn_new_gateway(&self, world: &Arc<World>) {
        let Some(gateway) = self.state.lock().gateways.pop() else {
            return;
        };

        let angle = 2.0 * (-PI + (PI / 20.0) * f64::from(gateway));
        let x = (GATEWAY_DISTANCE * angle.cos()).floor() as i32;
        let z = (GATEWAY_DISTANCE * angle.sin()).floor() as i32;
        Self::spawn_gateway_at(world, BlockPos::new(x, GATEWAY_Y, z));
    }

    /// Vanilla parity: `EnderDragonFight.spawnNewGateway(BlockPos)`.
    fn spawn_gateway_at(world: &Arc<World>, pos: BlockPos) {
        world.level_event(level_events::ANIMATION_END_GATEWAY_SPAWN, pos, 0, None);
        let mut random = WorldgenRandom::from_seed(rand::random());
        FeatureDecorationRunner::place_configured_feature_kind(
            world,
            &REGISTRY,
            &mut random,
            &vanilla_configured_features::END_GATEWAY_DELAYED.kind,
            pos,
            world.biome_zoom_seed(),
        );
    }

    /// Builds the podium, and lights its portal when `activated`.
    ///
    /// Vanilla parity: `EnderDragonFight.spawnExitPortal`.
    fn spawn_exit_portal(&self, world: &Arc<World>, activated: bool) {
        let known = self.state.lock().exit_portal_location;
        let location = if let Some(location) = known {
            location
        } else {
            let found = self.find_exit_portal_ground(world);
            self.state.lock().exit_portal_location = Some(found);
            found
        };

        // Vanilla follows a successful placement with `waitForLightBeforeSending`,
        // which only delays the chunk packet so the client never renders the
        // portal unlit. Foton has no such hold, and the podium is a live block
        // write that relights like any other.
        end_podium::place(world, location, activated);
    }

    /// Vanilla parity: the `exitPortalLocation == null` branch of
    /// `spawnExitPortal` -- drop through the surface until the bedrock ends.
    fn find_exit_portal_ground(&self, world: &Arc<World>) -> BlockPos {
        let mut location = world
            .heightmap_pos(
                HeightmapType::MotionBlockingNoLeaves,
                end_podium::location(self.origin),
            )
            .below();

        while world.get_block_state(location).get_block() == &vanilla_blocks::BEDROCK
            && location.y() > EXIT_PORTAL_MIN_SEARCH_Y
        {
            location = location.below();
        }

        location.at_y(location.y().max(world.as_ref().min_y() + 1))
    }

    /// Puts a fresh dragon in the sky over the podium.
    ///
    /// Vanilla parity: `EnderDragonFight.createNewDragon`.
    fn create_new_dragon(&self, world: &Arc<World>) {
        let position = DVec3::new(
            f64::from(self.origin.x()),
            f64::from(DRAGON_SPAWN_Y + self.origin.y()),
            f64::from(self.origin.z()),
        );
        let Some(entity) = ENTITIES.create(
            &vanilla_entities::ENDER_DRAGON,
            next_entity_id(),
            position,
            Arc::downgrade(world),
        ) else {
            return;
        };

        let uuid = entity.uuid();
        if let Some(dragon) = entity.downcast_ref::<EnderDragon>() {
            dragon.set_fight_origin(self.origin);
            dragon
                .phase_manager()
                .set_phase(dragon, EnderDragonPhase::HoldingPattern);
            dragon.set_rotation((rand::random::<f32>() * 360.0, 0.0));
        }

        if world.try_add_entity(entity).is_err() {
            return;
        }
        self.state.lock().dragon_uuid = Some(uuid);
    }

    /// Follows the fight's dragon: the bar, and the watchdog that respawns one.
    ///
    /// Vanilla parity: `EnderDragonFight.updateDragon`.
    pub fn update_dragon(&self, dragon: &EnderDragon) {
        if self.state.lock().dragon_uuid != Some(dragon.uuid()) {
            return;
        }

        self.dragon_event
            .set_progress(dragon.get_health() / dragon.get_max_health());
        self.state.lock().ticks_since_dragon_seen = 0;
        if dragon.custom_name().is_some() {
            self.dragon_event.set_name(dragon.display_name());
        }
    }

    /// Reacts to one of the End's crystals blowing up.
    ///
    /// Vanilla parity: `EnderDragonFight.onCrystalDestroyed`. A ritual crystal
    /// aborts the ritual; any other one is recounted and handed to the dragon.
    pub fn on_crystal_destroyed(
        &self,
        world: &Arc<World>,
        crystal: &EndCrystalEntity,
        source: &DamageSource,
    ) {
        let is_respawn_crystal = {
            let state = self.state.lock();
            state.respawn_stage.is_some() && state.respawn_crystals.contains(&crystal.uuid())
        };
        if is_respawn_crystal {
            self.abort_respawn_sequence(world);
            return;
        }

        self.update_crystal_count(world);
        let Some(dragon_uuid) = self.dragon_uuid() else {
            return;
        };
        let Some(entity) = world.get_entity_by_uuid(&dragon_uuid) else {
            return;
        };
        let Some(dragon) = entity.downcast_ref::<EnderDragon>() else {
            return;
        };
        dragon.on_crystal_destroyed(world, crystal, crystal.block_position(), source, None);
    }

    /// Vanilla parity: `EnderDragonFight.abortRespawnSequence`.
    fn abort_respawn_sequence(&self, world: &Arc<World>) {
        log::debug!("Aborting respawn sequence");
        {
            let mut state = self.state.lock();
            state.respawn_stage = None;
            state.respawn_time = 0;
        }
        Self::reset_spike_crystals(world);
        self.spawn_exit_portal(world, true);
    }

    /// Starts the ritual when four crystals stand on the portal's rim.
    ///
    /// Vanilla parity: `EnderDragonFight.tryRespawn`, which the end crystal item
    /// calls after it places its fourth crystal.
    pub fn try_respawn(&self, world: &Arc<World>) {
        {
            let state = self.state.lock();
            if !state.dragon_killed || state.respawn_stage.is_some() {
                return;
            }
        }

        let mut location = self.state.lock().exit_portal_location;
        if location.is_none() {
            log::debug!("Tried to respawn, but need to find the portal first.");
            if self.find_exit_portal(world).is_none() {
                log::debug!("Couldn't find a portal, so we made one.");
                self.spawn_exit_portal(world, true);
            } else {
                log::debug!("Found the exit portal & saved its location for next time.");
            }
            location = self.state.lock().exit_portal_location;
        }

        let Some(location) = location else {
            return;
        };

        let center = location.above();
        let mut crystals = Vec::new();
        for direction in Direction::HORIZONTAL {
            let found = crystals_at_block(world, center.relative_n(direction, 3));
            if found.is_empty() {
                return;
            }
            crystals.extend(found);
        }

        log::debug!("Found all crystals, respawning dragon.");
        self.respawn_dragon(world, &crystals);
    }

    /// Tears the lit portal back out and starts the ritual.
    ///
    /// Vanilla parity: `EnderDragonFight.respawnDragon`.
    fn respawn_dragon(&self, world: &Arc<World>, crystals: &[SharedEntity]) {
        {
            let state = self.state.lock();
            if !state.dragon_killed || state.respawn_stage.is_some() {
                return;
            }
        }

        let end_stone = vanilla_blocks::END_STONE.default_state();
        while let Some(portal) = self.find_exit_portal(world) {
            let mut replaced = Vec::new();
            for x in 0..self.exit_portal_pattern.width() as i32 {
                for y in 0..self.exit_portal_pattern.height() as i32 {
                    for z in 0..self.exit_portal_pattern.depth() as i32 {
                        let block = portal.block(x, y, z);
                        let owner = block.state().get_block();
                        if owner == &vanilla_blocks::BEDROCK || owner == &vanilla_blocks::END_PORTAL
                        {
                            replaced.push(block.pos());
                        }
                    }
                }
            }
            drop(portal);

            if replaced.is_empty() {
                break;
            }
            for pos in replaced {
                world.set_block_state(pos, end_stone, UpdateFlags::UPDATE_ALL);
            }
        }

        let mut state = self.state.lock();
        state.respawn_stage = Some(DragonRespawnStage::Start);
        state.respawn_time = 0;
        state.respawn_crystals = crystals.iter().map(|crystal| crystal.uuid()).collect();
        drop(state);
        self.spawn_exit_portal(world, false);
    }

    /// Makes the pillar crystals breakable again and cuts their beams.
    ///
    /// Vanilla parity: `EnderDragonFight.resetSpikeCrystals`.
    pub fn reset_spike_crystals(world: &Arc<World>) {
        for spike in FeatureDecorationRunner::end_spikes_for_level(world.seed()) {
            for entity in world.get_entities_in_aabb(&spike_top_bounding_box(&spike)) {
                let Some(crystal) = entity.downcast_ref::<EndCrystalEntity>() else {
                    continue;
                };
                crystal.set_invulnerable(false);
                crystal.set_beam_target(None);
            }
        }
    }
}

/// Returns the crystals standing in one block's cube.
///
/// Vanilla parity: the `getEntitiesOfClass(EndCrystal.class, new AABB(pos))` of
/// `tryRespawn`.
fn crystals_at_block(world: &Arc<World>, pos: BlockPos) -> Vec<SharedEntity> {
    let box_ = WorldAabb::new(
        f64::from(pos.x()),
        f64::from(pos.y()),
        f64::from(pos.z()),
        f64::from(pos.x() + 1),
        f64::from(pos.y() + 1),
        f64::from(pos.z() + 1),
    );
    world
        .get_entities_in_aabb(&box_)
        .into_iter()
        .filter(|entity| entity.downcast_ref::<EndCrystalEntity>().is_some())
        .collect()
}

/// The column a pillar's crystal is looked for in.
///
/// Vanilla parity: `EndSpikeFeature.EndSpike.getTopBoundingBox`, which spans the
/// whole dimension vertically so a crystal knocked off its cage still counts.
fn spike_top_bounding_box(spike: &EndSpike) -> WorldAabb {
    WorldAabb::new(
        f64::from(spike.center_x - spike.radius),
        DIMENSION_MIN_Y,
        f64::from(spike.center_z - spike.radius),
        f64::from(spike.center_x + spike.radius),
        DIMENSION_MAX_Y,
        f64::from(spike.center_z + spike.radius),
    )
}

/// Returns the dragons a world is running, alive ones only.
///
/// Vanilla parity: `ServerLevel.getDragons`.
fn dragons_in(world: &Arc<World>) -> Vec<SharedEntity> {
    world
        .entity_manager()
        .get_accessible_entities()
        .into_iter()
        .filter(|entity| {
            entity
                .downcast_ref::<EnderDragon>()
                .is_some_and(LivingEntity::is_alive)
        })
        .collect()
}

/// The chunk the arena ticket is centered on.
///
/// Vanilla parity: the `new ChunkPos(0, 0)` of `tick`. Vanilla centers the
/// ticket on the world origin rather than on the fight origin, which only
/// differ in a modified game; the origin is used here so the two cannot drift.
const fn arena_center(origin: BlockPos) -> ChunkPos {
    ChunkPos::new(
        SectionPos::block_to_section_coord(origin.x()),
        SectionPos::block_to_section_coord(origin.z()),
    )
}

/// The bedrock frame around the exit portal.
///
/// Vanilla parity: `EnderDragonFight.exitPortalPattern`.
fn exit_portal_pattern() -> BlockPattern {
    let column = &[
        "       ", "       ", "       ", "   #   ", "       ", "       ", "       ",
    ];
    BlockPatternBuilder::start()
        .aisle(column)
        .aisle(column)
        .aisle(column)
        .aisle(&[
            "  ###  ", " #   # ", "#     #", "#  #  #", "#     #", " #   # ", "  ###  ",
        ])
        .aisle(&[
            "       ", "  ###  ", " ##### ", " ##### ", " ##### ", "  ###  ", "       ",
        ])
        .where_char(
            '#',
            has_state(|state| state.get_block() == &vanilla_blocks::BEDROCK),
        )
        .build()
}

/// The saved form of a fight.
///
/// The field names follow vanilla's `EnderDragonFight.CODEC` so a reader who
/// knows the vanilla file knows this one. The origin is not among them, here
/// as in vanilla: the level supplies it on every load.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersistentEnderDragonFight {
    #[serde(default = "yes")]
    needs_state_scanning: bool,
    #[serde(default)]
    dragon_killed: bool,
    #[serde(default)]
    previously_killed: bool,
    #[serde(default)]
    respawn_time: i32,
    #[serde(default)]
    gateways: Vec<i32>,
    #[serde(default)]
    respawn_crystals: Vec<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    respawn_stage: Option<DragonRespawnStage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    dragon_uuid: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    exit_portal_location: Option<[i32; 3]>,
}

const fn yes() -> bool {
    true
}

impl Default for PersistentEnderDragonFight {
    /// Vanilla parity: `EnderDragonFight.createDefault`, which starts a fight
    /// needing a state scan and nothing else.
    fn default() -> Self {
        Self {
            needs_state_scanning: true,
            dragon_killed: false,
            previously_killed: false,
            respawn_time: 0,
            gateways: Vec::new(),
            respawn_crystals: Vec::new(),
            respawn_stage: None,
            dragon_uuid: None,
            exit_portal_location: None,
        }
    }
}
