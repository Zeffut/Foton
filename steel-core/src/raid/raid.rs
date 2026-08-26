//! One raid on one village.
//!
//! Vanilla parity: `net.minecraft.world.entity.raid.Raid`.
//!
//! A raid is a small state machine hung off a village center. It counts down
//! three hundred ticks, finds a spawn ring outside the village, drops a wave of
//! illagers on it, and waits until they are all dead before counting down
//! again. The boss bar is the whole of what a player sees of that machine: the
//! countdown fills it, the wave's remaining health empties it, and the last
//! twenty seconds after the final raider dies turn it into a victory banner.
//!
//! ## How this differs in shape from vanilla
//!
//! Vanilla holds `Set<Raider>` -- strong references to the mobs. Steel holds
//! entity ids and resolves them through the world, because a raid outlives the
//! chunk its raiders sit in and a strong reference would keep a removed entity
//! alive. An id that no longer resolves is treated as a raider that was
//! removed, which is what vanilla's `level.getEntity(uuid) == null` branch
//! decides anyway.
//!
//! The three flags a raider reads -- active, loss, over -- live in atomics
//! rather than under the state lock, so a mob asking what its raid is doing
//! from inside its own tick can never contend with the raid's tick. Nothing
//! else may be read that way.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

use core::f32::consts::{PI, TAU};

use glam::DVec3;
use rustc_hash::{FxHashMap, FxHashSet};
use steel_math::trig;
use steel_protocol::packets::game::{BossBarColor, BossBarOverlay, CSound, SoundSource};
use steel_registry::{sound_events, vanilla_entities, vanilla_mob_effects};
use steel_utils::locks::SyncMutex;
use steel_utils::types::Difficulty;
use steel_utils::{BlockPos, ChunkPos, SectionPos};
use text_components::{Modifier as _, TextComponent, translation::TranslatedMessage};
use uuid::Uuid;

use super::wave::{RaiderType, num_groups};
use crate::boss_event::ServerBossEvent;
use crate::chunk::heightmap::HeightmapType;
use crate::entity::raider::{
    MAX_NO_ACTION_TIME, OMINOUS_BANNER_DROP_CHANCE, RaidStatus, Raider, ominous_banner,
};
use crate::entity::{
    ENTITIES, Entity, EntitySpawnReason, LivingEntity, MobEffectInstance, next_entity_id,
};
use crate::inventory::equipment::EquipmentSlot;
use crate::player::Player;
use crate::world::World;
use crate::world::spawn_placement::spawn_placement_for;

/// Ticks a raid waits before its first wave, and between waves.
///
/// Vanilla parity: `Raid.DEFAULT_PRE_RAID_TICKS`.
pub const DEFAULT_PRE_RAID_TICKS: i32 = 300;

/// Ticks a raid may run before it gives up.
///
/// Vanilla parity: `Raid.RAID_TIMEOUT_TICKS`.
const RAID_TIMEOUT_TICKS: i64 = 48_000;

/// Ticks the bar lingers after the last raider dies, before victory is called.
///
/// Vanilla parity: `Raid.POST_RAID_TICK_LIMIT`.
const POST_RAID_TICK_LIMIT: i32 = 40;

/// Ticks a finished raid keeps its bar on screen.
///
/// Vanilla parity: `Raid.MAX_CELEBRATION_TICKS`.
const MAX_CELEBRATION_TICKS: i32 = 600;

/// Ticks a raider may spend away from the village before it is dropped.
///
/// Vanilla parity: `Raid.OUTSIDE_RAID_BOUNDS_TIMEOUT`.
const OUTSIDE_RAID_BOUNDS_TIMEOUT: i32 = 30;

/// The highest raid omen level a raid can absorb.
///
/// Vanilla parity: `Raid.DEFAULT_MAX_RAID_OMEN_LEVEL`.
pub const DEFAULT_MAX_RAID_OMEN_LEVEL: i32 = 5;

/// How many raiders still count as "nearly over" on the bar.
///
/// Vanilla parity: `Raid.LOW_MOB_THRESHOLD`.
const LOW_MOB_THRESHOLD: i32 = 2;

/// Ticks of Hero of the Village a victory awards.
///
/// Vanilla parity: `Raid.HERO_OF_THE_VILLAGE_DURATION`.
const HERO_OF_THE_VILLAGE_DURATION: i32 = 48_000;

/// Squared distance within which a position belongs to a raid.
///
/// Vanilla parity: `Raid.VALID_RAID_RADIUS_SQR`, the radius `getRaidAt` uses.
pub const VALID_RAID_RADIUS_SQR: f64 = 9216.0;

/// Vertical slack a spawn ring position may have from the raid center.
///
/// Vanilla parity: `Raid.VALID_RAID_RADIUS`.
const VALID_RAID_RADIUS: i32 = 96;

/// Squared distance past which a raider stops belonging to its raid.
///
/// Vanilla parity: `Raid.RAID_REMOVAL_THRESHOLD_SQR`.
const RAID_REMOVAL_THRESHOLD_SQR: f64 = 12_544.0;

/// Section radius searched when the village center has drifted.
///
/// Vanilla parity: `Raid.SECTION_RADIUS_FOR_FINDING_NEW_VILLAGE_CENTER`.
const SECTION_RADIUS_FOR_FINDING_NEW_VILLAGE_CENTER: i32 = 2;

/// How far out the spawn ring is thrown, before the countdown scaling.
///
/// Vanilla parity: `Raid.VILLAGE_SEARCH_RADIUS`.
const VILLAGE_SEARCH_RADIUS: f32 = 32.0;

/// Seconds left on the countdown below which raiders may spawn inside the village.
///
/// Vanilla parity: `Raid.ALLOW_SPAWNING_WITHIN_VILLAGE_SECONDS_THRESHOLD`.
const ALLOW_SPAWNING_WITHIN_VILLAGE_SECONDS_THRESHOLD: i32 = 7;

/// How many failed spawn-position searches stop a raid.
///
/// Vanilla parity: `Raid.NUM_SPAWN_ATTEMPTS`.
const NUM_SPAWN_ATTEMPTS: i32 = 5;

/// Ticks after which a raider that never acted is checked for drift.
///
/// Vanilla parity: the `raider.tickCount > 600` of `Raid.updateRaiders`.
const RAIDER_SETTLE_TICKS: i32 = 600;

/// Half-width of the block square that has to be loaded around a spawn position.
///
/// Vanilla parity: the `int delta = 10` of `Raid.findRandomSpawnPos`.
const SPAWN_POS_LOADED_MARGIN: i32 = 10;

/// How far from a player the raid horn is placed, so it comes from the raid.
///
/// Vanilla parity: the `float distAway = 13.0F` of `Raid.playSound`.
const RAID_HORN_OFFSET: f64 = 13.0;

/// How far a player can be and still hear the raid horn.
///
/// Vanilla parity: the `int range = 64` of `Raid.playSound`.
const RAID_HORN_RANGE: f64 = 64.0;

/// Volume the raid horn is sent at.
const RAID_HORN_VOLUME: f32 = 64.0;

/// Where a raid is in its life.
///
/// Vanilla parity: `Raid.RaidStatus`, renamed because Steel already calls the
/// three flags a raider reads [`RaidStatus`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RaidPhase {
    /// Vanilla parity: `RaidStatus.ONGOING`.
    Ongoing,
    /// Vanilla parity: `RaidStatus.VICTORY`.
    Victory,
    /// Vanilla parity: `RaidStatus.LOSS`.
    Loss,
    /// Vanilla parity: `RaidStatus.STOPPED`.
    Stopped,
}

impl RaidPhase {
    const fn to_bits(self) -> u8 {
        match self {
            Self::Ongoing => 0,
            Self::Victory => 1,
            Self::Loss => 2,
            Self::Stopped => 3,
        }
    }

    const fn from_bits(bits: u8) -> Self {
        match bits {
            1 => Self::Victory,
            2 => Self::Loss,
            3 => Self::Stopped,
            _ => Self::Ongoing,
        }
    }
}

/// The mutable half of a [`Raid`], behind one lock.
///
/// Nothing here may be read while entity code runs: every method that calls
/// into a mob takes the lock, copies what it needs, and drops it first.
#[derive(Debug)]
struct RaidState {
    /// Vanilla parity: `Raid.groupToLeaderMap`, by raider entity id.
    group_to_leader: FxHashMap<i32, i32>,
    /// Vanilla parity: `Raid.groupRaiderMap`, by raider entity id.
    group_raiders: FxHashMap<i32, FxHashSet<i32>>,
    /// Vanilla parity: `Raid.heroesOfTheVillage`.
    heroes_of_the_village: FxHashSet<Uuid>,
    /// Vanilla parity: `Raid.ticksActive`.
    ticks_active: i64,
    /// Vanilla parity: `Raid.started`.
    started: bool,
    /// Vanilla parity: `Raid.totalHealth`.
    total_health: f32,
    /// Vanilla parity: `Raid.raidOmenLevel`.
    raid_omen_level: i32,
    /// Vanilla parity: `Raid.groupsSpawned`.
    groups_spawned: i32,
    /// Vanilla parity: `Raid.postRaidTicks`.
    post_raid_ticks: i32,
    /// Vanilla parity: `Raid.raidCooldownTicks`.
    raid_cooldown_ticks: i32,
    /// Vanilla parity: `Raid.celebrationTicks`.
    celebration_ticks: i32,
    /// Vanilla parity: `Raid.waveSpawnPos`.
    wave_spawn_pos: Option<BlockPos>,
}

/// A raid on one village.
///
/// Vanilla parity: `Raid`.
#[derive(Debug)]
pub struct Raid {
    /// The key this raid is filed under in [`super::Raids`].
    ///
    /// Vanilla looks this up with `Raids.getId` by identity. Steel stores it,
    /// because identity comparison against an `Arc` would need the map lock
    /// from inside a tick that already released it.
    id: i32,
    /// Vanilla parity: `Raid.numGroups`, fixed by the difficulty at creation.
    num_groups: i32,
    /// Vanilla parity: `Raid.status`.
    phase: AtomicU8,
    /// Vanilla parity: `Raid.active`.
    active: AtomicBool,
    /// Vanilla parity: `Raid.center`.
    center: SyncMutex<BlockPos>,
    state: SyncMutex<RaidState>,
    /// Vanilla parity: `Raid.raidEvent`.
    boss_bar: ServerBossEvent,
}

impl Raid {
    /// Starts a raid on `center`.
    ///
    /// Vanilla parity: the public `Raid(BlockPos, Difficulty)` constructor.
    #[must_use]
    pub fn new(id: i32, center: BlockPos, difficulty: Difficulty) -> Self {
        let raid = Self::from_parts(
            id,
            center,
            num_groups(difficulty),
            RaidPhase::Ongoing,
            true,
            RaidState {
                group_to_leader: FxHashMap::default(),
                group_raiders: FxHashMap::default(),
                heroes_of_the_village: FxHashSet::default(),
                ticks_active: 0,
                started: false,
                total_health: 0.0,
                raid_omen_level: 0,
                groups_spawned: 0,
                post_raid_ticks: 0,
                raid_cooldown_ticks: DEFAULT_PRE_RAID_TICKS,
                celebration_ticks: 0,
                wave_spawn_pos: None,
            },
        );
        raid.boss_bar.set_progress(0.0);
        raid
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "one argument per persisted field, which is what the vanilla codec's \
                  private constructor also takes"
    )]
    pub(super) fn from_saved(
        id: i32,
        center: BlockPos,
        num_groups: i32,
        phase: RaidPhase,
        active: bool,
        started: bool,
        ticks_active: i64,
        raid_omen_level: i32,
        groups_spawned: i32,
        raid_cooldown_ticks: i32,
        post_raid_ticks: i32,
        total_health: f32,
        heroes_of_the_village: FxHashSet<Uuid>,
    ) -> Self {
        Self::from_parts(
            id,
            center,
            num_groups,
            phase,
            active,
            RaidState {
                group_to_leader: FxHashMap::default(),
                group_raiders: FxHashMap::default(),
                heroes_of_the_village,
                ticks_active,
                started,
                total_health,
                raid_omen_level,
                groups_spawned,
                post_raid_ticks,
                raid_cooldown_ticks,
                celebration_ticks: 0,
                wave_spawn_pos: None,
            },
        )
    }

    fn from_parts(
        id: i32,
        center: BlockPos,
        num_groups: i32,
        phase: RaidPhase,
        active: bool,
        state: RaidState,
    ) -> Self {
        Self {
            id,
            num_groups,
            phase: AtomicU8::new(phase.to_bits()),
            active: AtomicBool::new(active),
            center: SyncMutex::new(center),
            state: SyncMutex::new(state),
            boss_bar: ServerBossEvent::with_random_id(
                raid_name(),
                BossBarColor::Red,
                BossBarOverlay::Notched10,
            ),
        }
    }

    /// Returns the key this raid is filed under.
    #[must_use]
    pub const fn id(&self) -> i32 {
        self.id
    }

    /// Returns where the raid is centered.
    ///
    /// Vanilla parity: `Raid.getCenter`.
    #[must_use]
    pub fn center(&self) -> BlockPos {
        *self.center.lock()
    }

    /// Returns how many waves this raid runs before its bonus wave.
    #[must_use]
    pub const fn num_groups(&self) -> i32 {
        self.num_groups
    }

    /// Returns the phase this raid is in.
    #[must_use]
    pub fn phase(&self) -> RaidPhase {
        RaidPhase::from_bits(self.phase.load(Ordering::Relaxed))
    }

    fn set_phase(&self, phase: RaidPhase) {
        self.phase.store(phase.to_bits(), Ordering::Relaxed);
    }

    /// Vanilla parity: `Raid.isActive`.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::Relaxed)
    }

    /// Vanilla parity: `Raid.isStopped`.
    #[must_use]
    pub fn is_stopped(&self) -> bool {
        self.phase() == RaidPhase::Stopped
    }

    /// Vanilla parity: `Raid.isVictory`.
    #[must_use]
    pub fn is_victory(&self) -> bool {
        self.phase() == RaidPhase::Victory
    }

    /// Vanilla parity: `Raid.isLoss`.
    #[must_use]
    pub fn is_loss(&self) -> bool {
        self.phase() == RaidPhase::Loss
    }

    /// Vanilla parity: `Raid.isOver`.
    #[must_use]
    pub fn is_over(&self) -> bool {
        matches!(self.phase(), RaidPhase::Victory | RaidPhase::Loss)
    }

    /// Returns the three flags a raider's goals branch on.
    ///
    /// This is the only view of a raid that may be read from inside a mob's
    /// tick: it touches atomics alone, so it cannot contend with a raid tick.
    #[must_use]
    pub fn status(&self) -> RaidStatus {
        let phase = self.phase();
        RaidStatus {
            active: self.is_active(),
            loss: phase == RaidPhase::Loss,
            over: matches!(phase, RaidPhase::Victory | RaidPhase::Loss),
        }
    }

    /// Vanilla parity: `Raid.isStarted`.
    #[must_use]
    pub fn is_started(&self) -> bool {
        self.state.lock().started
    }

    /// Vanilla parity: `Raid.getGroupsSpawned`.
    #[must_use]
    pub fn groups_spawned(&self) -> i32 {
        self.state.lock().groups_spawned
    }

    /// Vanilla parity: `Raid.getTotalHealth`.
    #[must_use]
    pub fn total_health(&self) -> f32 {
        self.state.lock().total_health
    }

    /// Vanilla parity: `Raid.getRaidOmenLevel`.
    #[must_use]
    pub fn raid_omen_level(&self) -> i32 {
        self.state.lock().raid_omen_level
    }

    /// Vanilla parity: `Raid.setRaidOmenLevel`.
    pub fn set_raid_omen_level(&self, raid_omen_level: i32) {
        self.state.lock().raid_omen_level = raid_omen_level;
    }

    /// Vanilla parity: `Raid.getTotalRaidersAlive`.
    #[must_use]
    pub fn total_raiders_alive(&self) -> i32 {
        let state = self.state.lock();
        state
            .group_raiders
            .values()
            .map(|raiders| i32::try_from(raiders.len()).unwrap_or(i32::MAX))
            .sum()
    }

    /// Returns the entity ids of every raider still in this raid.
    ///
    /// Vanilla parity: `Raid.getAllRaiders`, which returns the mobs themselves.
    #[must_use]
    pub fn all_raider_ids(&self) -> Vec<i32> {
        let state = self.state.lock();
        state
            .group_raiders
            .values()
            .flat_map(|raiders| raiders.iter().copied())
            .collect()
    }

    /// Vanilla parity: `Raid.getLeader`.
    #[must_use]
    pub fn leader(&self, wave: i32) -> Option<i32> {
        self.state.lock().group_to_leader.get(&wave).copied()
    }

    /// Vanilla parity: `Raid.removeLeader`.
    pub fn remove_leader(&self, wave: i32) {
        self.state.lock().group_to_leader.remove(&wave);
    }

    /// Vanilla parity: `Raid.getEnchantOdds`, the chance a wave buff enchants.
    #[must_use]
    pub fn enchant_odds(&self) -> f32 {
        match self.raid_omen_level() {
            2 => 0.1,
            3 => 0.25,
            4 => 0.5,
            5 => 0.75,
            _ => 0.0,
        }
    }

    /// Vanilla parity: `Raid.addHeroOfTheVillage`.
    pub fn add_hero_of_the_village(&self, killer: Uuid) {
        self.state.lock().heroes_of_the_village.insert(killer);
    }

    /// Ends the raid and takes its bar off every screen.
    ///
    /// Vanilla parity: `Raid.stop`.
    pub fn stop(&self) {
        self.active.store(false, Ordering::Relaxed);
        self.boss_bar.remove_all_players();
        self.set_phase(RaidPhase::Stopped);
    }

    /// Raises the raid's omen level from the player's Raid Omen effect.
    ///
    /// Vanilla parity: `Raid.absorbRaidOmen`.
    pub fn absorb_raid_omen(&self, player: &Player) -> bool {
        let Some(effect) = LivingEntity::mob_effect(player, vanilla_mob_effects::RAID_OMEN) else {
            return false;
        };

        let max = DEFAULT_MAX_RAID_OMEN_LEVEL;
        {
            let mut state = self.state.lock();
            state.raid_omen_level = (state.raid_omen_level + effect.amplifier() + 1).clamp(0, max);
        }
        // Vanilla also awards the `raid_trigger` statistic and fires the
        // `RAID_OMEN` advancement trigger when this is the first wave. Steel
        // has neither system.
        true
    }

    /// Returns the summed health of every raider still in this raid.
    ///
    /// Vanilla parity: `Raid.getHealthOfLivingRaiders`. A raider whose entity
    /// no longer resolves contributes nothing rather than the health it had
    /// when it vanished; the next `update_raiders` pass drops it outright.
    #[must_use]
    pub fn health_of_living_raiders(&self, world: &World) -> f32 {
        self.all_raider_ids()
            .into_iter()
            .filter_map(|id| world.get_entity_by_id(id))
            .filter_map(|entity| entity.as_living_entity().map(LivingEntity::get_health))
            .sum()
    }

    /// Redraws the bar from the wave's remaining health.
    ///
    /// Vanilla parity: `Raid.updateBossbar`.
    pub fn update_boss_bar(&self, world: &World) {
        let total_health = self.total_health();
        // Vanilla divides unguarded, so a wave with no health yet -- a raid
        // reloaded before its first wave -- puts a NaN on the bar. Steel draws
        // it empty instead: a NaN progress never compares equal to itself, so
        // the bar would rebroadcast to every viewer on every call.
        let progress = if total_health > 0.0 {
            (self.health_of_living_raiders(world) / total_health).clamp(0.0, 1.0)
        } else {
            0.0
        };
        self.boss_bar.set_progress(progress);
    }

    /// Adds a raider to a wave.
    ///
    /// Vanilla parity: `Raid.addWaveMob`. Vanilla replaces an entry that shares
    /// the newcomer's UUID, which is how a reloaded mob takes its own place
    /// back; Steel keys on entity id, which a reload changes, so the stale id
    /// is left for `update_raiders` to drop.
    pub fn add_wave_mob(&self, world: &World, wave: i32, raider: &dyn Raider, update_health: bool) {
        {
            let mut state = self.state.lock();
            state
                .group_raiders
                .entry(wave)
                .or_default()
                .insert(raider.id());
        }
        if update_health {
            self.state.lock().total_health += LivingEntity::get_health(raider);
        }
        self.update_boss_bar(world);
    }

    /// Puts a raider into this raid, spawning it when it is not in the world yet.
    ///
    /// Vanilla parity: `Raid.joinRaid`.
    pub fn join_raid(
        self: &Arc<Self>,
        world: &Arc<World>,
        group_number: i32,
        raider: &dyn Raider,
        pos: Option<BlockPos>,
        exists: bool,
    ) {
        self.add_wave_mob(world, group_number, raider, true);
        raider.set_current_raid(Some(self.id));
        raider.set_wave(group_number);
        raider.set_can_join_raid(true);
        raider.set_ticks_outside_raid(0);
        if exists || pos.is_none() {
            return;
        }
        // Vanilla sets the position here; Steel builds the entity at the spawn
        // position instead, because `ENTITIES.create` takes it. What is left is
        // the finalize-and-add half.
        let _ = raider.finalize_spawn(world, EntitySpawnReason::Event, None);
        raider.apply_raid_buffs(group_number, false);
        raider.set_on_ground(true);
    }

    /// Drops a raider from this raid.
    ///
    /// Vanilla parity: `Raid.removeFromRaid`.
    pub fn remove_from_raid(&self, world: &World, raider_id: i32, remove_from_total_health: bool) {
        let wave = world
            .get_entity_by_id(raider_id)
            .and_then(|entity| entity.as_raider().map(Raider::wave));
        let removed = {
            let mut state = self.state.lock();
            match wave {
                Some(wave) => state
                    .group_raiders
                    .get_mut(&wave)
                    .is_some_and(|raiders| raiders.remove(&raider_id)),
                // The mob is gone, so the wave it belonged to cannot be asked
                // for. Vanilla never reaches this: it holds the mob itself.
                None => state
                    .group_raiders
                    .values_mut()
                    .any(|raiders| raiders.remove(&raider_id)),
            }
        };
        if !removed {
            return;
        }

        if remove_from_total_health {
            let health = world
                .get_entity_by_id(raider_id)
                .and_then(|entity| entity.as_living_entity().map(LivingEntity::get_health))
                .unwrap_or(0.0);
            self.state.lock().total_health -= health;
        }
        if let Some(entity) = world.get_entity_by_id(raider_id)
            && let Some(raider) = entity.as_raider()
        {
            raider.set_current_raid(None);
        }
        self.update_boss_bar(world);
    }

    /// Crowns a raider as the wave's captain.
    ///
    /// Vanilla parity: `Raid.setLeader`.
    pub fn set_leader(&self, wave: i32, raider: &dyn Raider) {
        self.state.lock().group_to_leader.insert(wave, raider.id());
        raider
            .living_base()
            .equipment()
            .lock()
            .set(EquipmentSlot::Head, ominous_banner());
        raider.set_drop_chance(EquipmentSlot::Head, OMINOUS_BANNER_DROP_CHANCE);
    }

    /// Runs one tick of the raid.
    ///
    /// Vanilla parity: `Raid.tick`.
    pub fn tick(self: &Arc<Self>, world: &Arc<World>) {
        if self.is_stopped() {
            return;
        }
        if self.is_over() {
            self.tick_celebration(world);
            return;
        }
        if self.phase() != RaidPhase::Ongoing {
            return;
        }

        let center = self.center();
        let was_active = self.is_active();
        let is_active = world.has_full_chunk(ChunkPos::new(
            SectionPos::block_to_section_coord(center.x()),
            SectionPos::block_to_section_coord(center.z()),
        ));
        self.active.store(is_active, Ordering::Relaxed);

        if world.difficulty() == Difficulty::Peaceful {
            self.stop();
            return;
        }
        if was_active != is_active {
            self.boss_bar.set_visible(is_active);
        }
        if !is_active {
            return;
        }

        if !world.is_village(center) {
            self.move_raid_center_to_nearby_village_section(world);
        }
        if !world.is_village(self.center()) {
            if self.groups_spawned() > 0 {
                self.set_phase(RaidPhase::Loss);
            } else {
                self.stop();
            }
        }

        let ticks_active = {
            let mut state = self.state.lock();
            state.ticks_active += 1;
            state.ticks_active
        };
        if ticks_active >= RAID_TIMEOUT_TICKS {
            self.stop();
            return;
        }

        let raiders_alive = self.total_raiders_alive();
        if raiders_alive == 0 && self.has_more_waves() && self.tick_cooldown(world) {
            return;
        }

        if ticks_active % 20 == 0 {
            self.update_players(world);
            self.update_raiders(world);
            self.boss_bar
                .set_name(raiders_remaining_name(raiders_alive));
        }

        self.spawn_pending_groups(world);
        self.tick_victory(world, raiders_alive);
    }

    /// Runs the countdown between waves.
    ///
    /// Vanilla parity: the `raidersAlive == 0 && hasMoreWaves()` branch of
    /// `Raid.tick`. Returns whether the tick should stop here, which is
    /// vanilla's early `return` after the cooldown is rearmed.
    fn tick_cooldown(self: &Arc<Self>, world: &Arc<World>) -> bool {
        let (cooldown, groups_spawned, cached_spawn_pos) = {
            let state = self.state.lock();
            (
                state.raid_cooldown_ticks,
                state.groups_spawned,
                state.wave_spawn_pos,
            )
        };

        if cooldown <= 0 {
            if cooldown == 0 && groups_spawned > 0 {
                self.state.lock().raid_cooldown_ticks = DEFAULT_PRE_RAID_TICKS;
                self.boss_bar.set_name(raid_name());
                return true;
            }
            return false;
        }

        let has_cached = cached_spawn_pos.is_some();
        let mut should_search = !has_cached && cooldown % 5 == 0;
        if let Some(cached) = cached_spawn_pos
            && !world.is_entity_ticking_chunk_loaded(cached)
        {
            should_search = true;
        }
        if should_search {
            let found = self.find_random_spawn_pos(world, 8);
            self.state.lock().wave_spawn_pos = found;
        }

        if cooldown == DEFAULT_PRE_RAID_TICKS || cooldown % 20 == 0 {
            self.update_players(world);
        }

        let remaining = {
            let mut state = self.state.lock();
            state.raid_cooldown_ticks -= 1;
            state.raid_cooldown_ticks
        };
        let elapsed = f64::from(DEFAULT_PRE_RAID_TICKS - remaining);
        let progress = (elapsed / f64::from(DEFAULT_PRE_RAID_TICKS)).clamp(0.0, 1.0) as f32;
        self.boss_bar.set_progress(progress);
        false
    }

    /// Spawns every wave the raid is due right now.
    ///
    /// Vanilla parity: the `while (this.shouldSpawnGroup())` loop of `Raid.tick`.
    fn spawn_pending_groups(self: &Arc<Self>, world: &Arc<World>) {
        let mut sound_played = false;
        let mut attempt = 0;

        while self.should_spawn_group() {
            let cached_spawn_pos = self.state.lock().wave_spawn_pos;
            let spawn_pos = match cached_spawn_pos {
                Some(pos) => Some(pos),
                None => self.find_random_spawn_pos(world, 20),
            };
            if let Some(spawn_pos) = spawn_pos {
                self.state.lock().started = true;
                self.spawn_group(world, spawn_pos);
                if !sound_played {
                    self.play_raid_horn(world, spawn_pos);
                    sound_played = true;
                }
            } else {
                attempt += 1;
            }

            if attempt > NUM_SPAWN_ATTEMPTS {
                self.stop();
                break;
            }
        }
    }

    /// Awards the raid once the last wave is dead.
    ///
    /// Vanilla parity: the closing `isStarted() && !hasMoreWaves()` branch of
    /// `Raid.tick`.
    fn tick_victory(&self, world: &Arc<World>, raiders_alive: i32) {
        if !self.is_started() || self.has_more_waves() || raiders_alive != 0 {
            return;
        }

        let post_raid_ticks = {
            let mut state = self.state.lock();
            if state.post_raid_ticks < POST_RAID_TICK_LIMIT {
                state.post_raid_ticks += 1;
                return;
            }
            state.post_raid_ticks
        };
        debug_assert!(post_raid_ticks >= POST_RAID_TICK_LIMIT);

        self.set_phase(RaidPhase::Victory);
        let (heroes, amplifier) = {
            let state = self.state.lock();
            (
                state
                    .heroes_of_the_village
                    .iter()
                    .copied()
                    .collect::<Vec<_>>(),
                state.raid_omen_level - 1,
            )
        };
        for hero_uuid in heroes {
            let Some(entity) = world.get_entity_by_uuid(&hero_uuid) else {
                continue;
            };
            if entity.is_spectator() {
                continue;
            }
            let Some(hero) = entity.as_living_entity() else {
                continue;
            };
            hero.add_mob_effect(
                MobEffectInstance::with_duration(
                    vanilla_mob_effects::HERO_OF_THE_VILLAGE,
                    HERO_OF_THE_VILLAGE_DURATION,
                    amplifier,
                )
                .with_ambient(false)
                .with_visible(false)
                .with_show_icon(true),
            );
            // Vanilla also awards the `raid_win` statistic and fires the
            // `RAID_WIN` advancement trigger for a player hero.
        }
    }

    /// Keeps the bar on screen for the twenty seconds after a raid ends.
    ///
    /// Vanilla parity: the `else if (this.isOver())` branch of `Raid.tick`.
    fn tick_celebration(&self, world: &Arc<World>) {
        let celebration_ticks = {
            let mut state = self.state.lock();
            state.celebration_ticks += 1;
            state.celebration_ticks
        };
        if celebration_ticks >= MAX_CELEBRATION_TICKS {
            self.stop();
            return;
        }
        if celebration_ticks % 20 != 0 {
            return;
        }

        self.update_players(world);
        self.boss_bar.set_visible(true);
        if self.is_victory() {
            self.boss_bar.set_progress(0.0);
            self.boss_bar
                .set_name(translated("event.minecraft.raid.victory.full"));
        } else {
            self.boss_bar
                .set_name(translated("event.minecraft.raid.defeat.full"));
        }
    }

    /// Vanilla parity: `Raid.hasMoreWaves`.
    fn has_more_waves(&self) -> bool {
        if self.has_bonus_wave() {
            !self.has_spawned_bonus_wave()
        } else {
            !self.is_final_wave()
        }
    }

    /// Vanilla parity: `Raid.isFinalWave`.
    fn is_final_wave(&self) -> bool {
        self.groups_spawned() == self.num_groups
    }

    /// Vanilla parity: `Raid.hasBonusWave`.
    fn has_bonus_wave(&self) -> bool {
        self.raid_omen_level() > 1
    }

    /// Vanilla parity: `Raid.hasSpawnedBonusWave`.
    fn has_spawned_bonus_wave(&self) -> bool {
        self.groups_spawned() > self.num_groups
    }

    /// Vanilla parity: `Raid.shouldSpawnBonusGroup`.
    fn should_spawn_bonus_group(&self) -> bool {
        self.is_final_wave() && self.total_raiders_alive() == 0 && self.has_bonus_wave()
    }

    /// Vanilla parity: `Raid.shouldSpawnGroup`.
    ///
    /// The cooldown is read into a local first: the rest of the condition locks
    /// the same state, and a guard living to the end of the expression would
    /// deadlock against it.
    fn should_spawn_group(&self) -> bool {
        let cooldown = self.state.lock().raid_cooldown_ticks;
        cooldown == 0
            && (self.groups_spawned() < self.num_groups || self.should_spawn_bonus_group())
            && self.total_raiders_alive() == 0
    }

    /// Moves the center onto the nearest village section it drifted off.
    ///
    /// Vanilla parity: `Raid.moveRaidCenterToNearbyVillageSection`.
    fn move_raid_center_to_nearby_village_section(&self, world: &World) {
        let center = self.center();
        let center_section = SectionPos::from_block_pos(center);
        let radius = SECTION_RADIUS_FOR_FINDING_NEW_VILLAGE_CENTER;

        let mut best: Option<(f64, BlockPos)> = None;
        for x in center_section.x() - radius..=center_section.x() + radius {
            for y in center_section.y() - radius..=center_section.y() + radius {
                for z in center_section.z() - radius..=center_section.z() + radius {
                    let section = SectionPos::new(x, y, z);
                    if !world.is_village_section(section) {
                        continue;
                    }
                    let candidate = section_center(section);
                    let distance = block_pos_dist_sqr(candidate, center);
                    if best.is_none_or(|(best_distance, _)| distance < best_distance) {
                        best = Some((distance, candidate));
                    }
                }
            }
        }

        if let Some((_, new_center)) = best {
            *self.center.lock() = new_center;
        }
    }

    /// Shows the bar to everyone standing in the raid and hides it from the rest.
    ///
    /// Vanilla parity: `Raid.updatePlayers`.
    fn update_players(&self, world: &Arc<World>) {
        let mut in_raid = Vec::new();
        world.players.iter_players(|_, player| {
            if LivingEntity::is_alive(player.as_ref())
                && world.raid_id_at(player.block_position()) == Some(self.id)
            {
                in_raid.push(Arc::clone(player));
            }
            true
        });

        let current = self.boss_bar.players();
        for player in &in_raid {
            if !current
                .iter()
                .any(|existing| existing.uuid() == player.uuid())
            {
                self.boss_bar.add_player(player);
            }
        }
        for player in current {
            if !in_raid.iter().any(|kept| kept.uuid() == player.uuid()) {
                self.boss_bar.remove_player(&player);
            }
        }
    }

    /// Drops raiders that wandered off, died out of sight or changed world.
    ///
    /// Vanilla parity: `Raid.updateRaiders`.
    fn update_raiders(&self, world: &Arc<World>) {
        let center = self.center();
        let mut to_remove = Vec::new();
        let mut leaders_to_clear = Vec::new();

        for raider_id in self.all_raider_ids() {
            let Some(entity) = world.get_entity_by_id(raider_id) else {
                to_remove.push((raider_id, None));
                continue;
            };
            let Some(raider) = entity.as_raider() else {
                to_remove.push((raider_id, None));
                continue;
            };

            let raider_pos = entity.block_position();
            if entity.is_removed()
                || block_pos_dist_sqr(center, raider_pos) >= RAID_REMOVAL_THRESHOLD_SQR
            {
                to_remove.push((raider_id, wave_if_leader(raider)));
                continue;
            }
            if entity.tick_count() <= RAIDER_SETTLE_TICKS {
                continue;
            }

            if !world.is_village(raider_pos) && raider.no_action_time() > MAX_NO_ACTION_TIME {
                raider.set_ticks_outside_raid(raider.ticks_outside_raid() + 1);
            }
            if raider.ticks_outside_raid() >= OUTSIDE_RAID_BOUNDS_TIMEOUT {
                to_remove.push((raider_id, wave_if_leader(raider)));
            }
        }

        for (raider_id, leader_wave) in to_remove {
            self.remove_from_raid(world, raider_id, true);
            if let Some(wave) = leader_wave {
                leaders_to_clear.push(wave);
            }
        }
        for wave in leaders_to_clear {
            self.remove_leader(wave);
        }
    }

    /// Sounds the raid horn towards the raid for everyone who can hear it.
    ///
    /// Vanilla parity: `Raid.playSound`. The sound is placed thirteen blocks
    /// from each listener along the line to the raid, which is what makes the
    /// horn come from the right direction however far away it really is.
    fn play_raid_horn(&self, world: &Arc<World>, sound_origin: BlockPos) {
        let in_raid = self.boss_bar.players();
        let seed = rand::random::<i64>();
        let (raid_x, _, raid_z) = sound_origin.get_center();

        world.players.iter_players(|_, player| {
            let player_pos = player.position();
            let dx = raid_x - player_pos.x;
            let dz = raid_z - player_pos.z;
            let distance = dx.hypot(dz);
            let hears = distance <= RAID_HORN_RANGE
                || in_raid
                    .iter()
                    .any(|listener| listener.uuid() == player.uuid());
            if !hears {
                return true;
            }
            // Vanilla divides by the distance unguarded, which is a NaN
            // coordinate for a player standing exactly on the raid center.
            // Steel plays it where they are instead rather than sending a NaN
            // through the protocol; the sound is at the listener either way.
            let (x, z) = if distance > 0.0 {
                (
                    player_pos.x + RAID_HORN_OFFSET / distance * dx,
                    player_pos.z + RAID_HORN_OFFSET / distance * dz,
                )
            } else {
                (player_pos.x, player_pos.z)
            };
            player.send_packet(CSound::new(
                &sound_events::EVENT_RAID_HORN,
                SoundSource::Neutral,
                DVec3::new(x, player_pos.y, z),
                RAID_HORN_VOLUME,
                1.0,
                seed,
            ));
            true
        });
    }

    /// Spawns one wave onto `pos`.
    ///
    /// Vanilla parity: `Raid.spawnGroup`.
    fn spawn_group(self: &Arc<Self>, world: &Arc<World>, pos: BlockPos) {
        let mut leader_set = false;
        let group_number = self.groups_spawned() + 1;
        self.state.lock().total_health = 0.0;
        let difficulty = world.difficulty();
        let is_bonus_group = self.should_spawn_bonus_group();
        let mut rng = rand::rng();
        let (spawn_x, _, spawn_z) = pos.get_center();
        let spawn_position = DVec3::new(spawn_x, f64::from(pos.y()) + 1.0, spawn_z);

        for raider_type in RaiderType::VALUES {
            let num_spawns =
                raider_type.default_num_spawns(group_number, self.num_groups, is_bonus_group)
                    + raider_type.potential_bonus_spawns(
                        group_number,
                        difficulty,
                        is_bonus_group,
                        &mut rng,
                    );
            let mut ravagers_spawned = 0;

            for _ in 0..num_spawns {
                let Some(entity) = ENTITIES.create(
                    raider_type.entity_type(),
                    next_entity_id(),
                    spawn_position,
                    Arc::downgrade(world),
                ) else {
                    break;
                };
                let Some(raider) = entity.as_raider() else {
                    break;
                };

                if !leader_set && raider.can_be_leader() {
                    raider.set_patrol_leader(true);
                    self.set_leader(group_number, raider);
                    leader_set = true;
                }

                self.join_raid(world, group_number, raider, Some(pos), false);
                if world.try_add_entity(Arc::clone(&entity)).is_err() {
                    self.remove_from_raid(world, entity.id(), true);
                    continue;
                }

                if raider_type != RaiderType::Ravager {
                    continue;
                }
                let rider_type = if group_number == num_groups(Difficulty::Normal) {
                    Some(&vanilla_entities::PILLAGER)
                } else if group_number >= num_groups(Difficulty::Hard) {
                    if ravagers_spawned == 0 {
                        Some(&vanilla_entities::EVOKER)
                    } else {
                        Some(&vanilla_entities::VINDICATOR)
                    }
                } else {
                    None
                };
                ravagers_spawned += 1;

                let Some(rider_type) = rider_type else {
                    continue;
                };
                let Some(rider_entity) = ENTITIES.create(
                    rider_type,
                    next_entity_id(),
                    spawn_position,
                    Arc::downgrade(world),
                ) else {
                    continue;
                };
                let Some(passenger) = rider_entity.as_raider() else {
                    continue;
                };
                self.join_raid(world, group_number, passenger, Some(pos), false);
                if world.try_add_entity(Arc::clone(&rider_entity)).is_err() {
                    self.remove_from_raid(world, rider_entity.id(), true);
                    continue;
                }
                rider_entity.start_riding(&entity);
            }
        }

        self.state.lock().wave_spawn_pos = None;
        self.state.lock().groups_spawned += 1;
        self.update_boss_bar(world);
    }

    /// Finds a spot on the ring outside the village to drop a wave on.
    ///
    /// Vanilla parity: `Raid.findRandomSpawnPos`. The ring grows as the
    /// countdown runs down -- `howFar` is negative for the first second, which
    /// is why an early search finds nothing -- and the last seven seconds allow
    /// a position inside the village itself, so a walled-in village still gets
    /// its raid.
    fn find_random_spawn_pos(&self, world: &Arc<World>, max_tries: i32) -> Option<BlockPos> {
        let center = self.center();
        let seconds_remaining = self.state.lock().raid_cooldown_ticks / 20;
        let how_far = 0.22_f32.mul_add(seconds_remaining as f32, -0.24);
        let start_angle = rand::random::<f32>() * TAU;

        for try_index in 0..max_tries {
            let angle = start_angle + PI * try_index as f32 / 8.0;
            let spawn_x = center.x()
                + (trig::cos(f64::from(angle)) * VILLAGE_SEARCH_RADIUS * how_far).floor() as i32
                + rand::random_range(0..3) * how_far.floor() as i32;
            let spawn_z = center.z()
                + (trig::sin(f64::from(angle)) * VILLAGE_SEARCH_RADIUS * how_far).floor() as i32
                + rand::random_range(0..3) * how_far.floor() as i32;
            let Some(spawn_y) = world.height_at(HeightmapType::WorldSurface, spawn_x, spawn_z)
            else {
                continue;
            };
            if (spawn_y - center.y()).abs() > VALID_RAID_RADIUS {
                continue;
            }

            let spawn_pos = BlockPos::new(spawn_x, spawn_y, spawn_z);
            if world.is_village(spawn_pos)
                && seconds_remaining > ALLOW_SPAWNING_WITHIN_VILLAGE_SECONDS_THRESHOLD
            {
                continue;
            }
            if !has_chunks_around(world, spawn_pos, SPAWN_POS_LOADED_MARGIN)
                || !world.is_entity_ticking_chunk_loaded(spawn_pos)
            {
                continue;
            }
            if spawn_placement_for(&vanilla_entities::RAVAGER).is_spawn_position_ok(
                world,
                spawn_pos,
                &vanilla_entities::RAVAGER,
            ) || world.is_snow_over_air(spawn_pos)
            {
                return Some(spawn_pos);
            }
        }

        None
    }

    pub(super) fn saved_fields(&self) -> SavedRaidFields {
        let state = self.state.lock();
        SavedRaidFields {
            center: *self.center.lock(),
            num_groups: self.num_groups,
            phase: self.phase(),
            active: self.is_active(),
            started: state.started,
            ticks_active: state.ticks_active,
            raid_omen_level: state.raid_omen_level,
            groups_spawned: state.groups_spawned,
            raid_cooldown_ticks: state.raid_cooldown_ticks,
            post_raid_ticks: state.post_raid_ticks,
            total_health: state.total_health,
            heroes_of_the_village: state.heroes_of_the_village.iter().copied().collect(),
        }
    }
}

/// Everything a raid persists, lifted out under one lock.
pub(super) struct SavedRaidFields {
    pub(super) center: BlockPos,
    pub(super) num_groups: i32,
    pub(super) phase: RaidPhase,
    pub(super) active: bool,
    pub(super) started: bool,
    pub(super) ticks_active: i64,
    pub(super) raid_omen_level: i32,
    pub(super) groups_spawned: i32,
    pub(super) raid_cooldown_ticks: i32,
    pub(super) post_raid_ticks: i32,
    pub(super) total_health: f32,
    pub(super) heroes_of_the_village: Vec<Uuid>,
}

/// Returns the wave a raider leads, if it leads one.
fn wave_if_leader(raider: &dyn Raider) -> Option<i32> {
    raider.is_patrol_leader().then(|| raider.wave())
}

/// Vanilla parity: `Level.hasChunksAt(x1, z1, x2, z2)`, over a square of blocks.
fn has_chunks_around(world: &World, pos: BlockPos, margin: i32) -> bool {
    let min_chunk_x = SectionPos::block_to_section_coord(pos.x() - margin);
    let max_chunk_x = SectionPos::block_to_section_coord(pos.x() + margin);
    let min_chunk_z = SectionPos::block_to_section_coord(pos.z() - margin);
    let max_chunk_z = SectionPos::block_to_section_coord(pos.z() + margin);
    for chunk_x in min_chunk_x..=max_chunk_x {
        for chunk_z in min_chunk_z..=max_chunk_z {
            if !world.has_full_chunk(ChunkPos::new(chunk_x, chunk_z)) {
                return false;
            }
        }
    }
    true
}

/// Vanilla parity: `SectionPos.center`.
const fn section_center(section: SectionPos) -> BlockPos {
    BlockPos::new(
        (section.x() << 4) + 8,
        (section.y() << 4) + 8,
        (section.z() << 4) + 8,
    )
}

/// Vanilla parity: `Vec3i.distSqr`, which is an integer-exact squared distance.
pub(super) fn block_pos_dist_sqr(a: BlockPos, b: BlockPos) -> f64 {
    let dx = f64::from(a.x() - b.x());
    let dy = f64::from(a.y() - b.y());
    let dz = f64::from(a.z() - b.z());
    dz.mul_add(dz, dx.mul_add(dx, dy * dy))
}

/// Vanilla parity: `Raid.RAID_NAME_COMPONENT`.
fn raid_name() -> TextComponent {
    translated("event.minecraft.raid")
}

/// Builds the bar title for a wave with `raiders_alive` mobs left.
///
/// Vanilla parity: the `ticksActive % 20` branch of `Raid.tick`, which appends
/// the count only once two raiders or fewer remain.
fn raiders_remaining_name(raiders_alive: i32) -> TextComponent {
    if raiders_alive <= 0 || raiders_alive > LOW_MOB_THRESHOLD {
        return raid_name();
    }
    raid_name()
        .add_child(TextComponent::plain(" - "))
        .add_child(
            TranslatedMessage {
                key: "event.minecraft.raid.raiders_remaining".into(),
                fallback: None,
                args: Some(vec![TextComponent::plain(raiders_alive.to_string())].into()),
            }
            .component(),
        )
}

fn translated(key: &'static str) -> TextComponent {
    TranslatedMessage {
        key: key.into(),
        fallback: None,
        args: None,
    }
    .component()
}
