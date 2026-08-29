//! One trial spawner: its configuration, its state machine, and its rewards.
//!
//! Vanilla parity:
//! `net.minecraft.world.level.block.entity.trialspawner.TrialSpawner`.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use foton_protocol::packets::game::SoundSource;
use foton_registry::blocks::block_state_ext::BlockStateExt as _;
use foton_registry::blocks::properties::{BlockStateProperties, TrialSpawnerState};
use foton_registry::item_stack::ItemStack;
use foton_registry::loot_table::{LootContext, LootTableRef};
use foton_registry::spawn_data::SpawnData;
use foton_registry::trial_spawner_config::{
    TICKS_BETWEEN_ITEM_SPAWNERS, TrialSpawnerConfig, TrialSpawnerConfigHolder,
};
use foton_registry::vanilla_game_rules::{SPAWN_MOBS, SPAWNER_BLOCKS_WORK};
use foton_registry::{
    REGISTRY, RegistryExt as _, level_events, sound_events, vanilla_game_events,
    vanilla_mob_effects,
};
use foton_utils::locks::SyncMutex;
use foton_utils::nbt::NbtNumeric as _;
use foton_utils::random::weighted_list::{Weighted, WeightedList};
use foton_utils::types::{Difficulty, UpdateFlags};
use foton_utils::{BlockPos, Direction, Identifier};
use glam::DVec3;
use rand::SeedableRng as _;
use rand::rngs::StdRng;
use simdnbt::borrow::NbtCompound as NbtCompoundView;
use simdnbt::owned::NbtCompound;
use uuid::Uuid;

use super::player_detector::{PlayerDetector, center_of, in_line_of_sight};
use super::state::{DELAY_BEFORE_EJECT_AFTER_KILLING_LAST_MOB, TIME_BETWEEN_EACH_EJECTION};
use super::state_data::{DELAY_BETWEEN_PLAYER_SCANS, TrialSpawnerStateData};
use crate::behavior::item_utils::spawn_item_toward;
use crate::entity::entities::OminousItemSpawnerEntity;
use crate::entity::{
    Entity, EntitySpawnReason, LivingEntity as _, MobEffectInstance, RemovalReason,
};
use crate::physics::{WorldCollisionProvider, has_collision};
use crate::player::Player;
use crate::world::base_spawner::{custom_spawn_rules_allow, load_spawner_entity};
use crate::world::game_event::GameEventContext;
use crate::world::{ClipBlockShape, ClipFluid, World};

/// Vanilla parity: `TrialSpawner.DETECT_PLAYER_SPAWN_BUFFER`.
const DETECT_PLAYER_SPAWN_BUFFER: i64 = 40;
/// Vanilla parity: `TrialSpawner.DEFAULT_TARGET_COOLDOWN_LENGTH`.
const DEFAULT_TARGET_COOLDOWN_LENGTH: i32 = 36_000;
/// Vanilla parity: `TrialSpawner.DEFAULT_PLAYER_SCAN_RANGE`.
const DEFAULT_PLAYER_SCAN_RANGE: i32 = 14;
/// Vanilla parity: `TrialSpawner.MAX_MOB_TRACKING_DISTANCE_SQR`.
const MAX_MOB_TRACKING_DISTANCE_SQR: i64 = 47 * 47;
/// Vanilla parity: `TrialSpawner.SPAWNING_AMBIENT_SOUND_CHANCE` -- client-side,
/// kept only so the omission is visible.
const _SPAWNING_AMBIENT_SOUND_CHANCE: f32 = 0.02;
/// Vanilla parity: the `relative(Direction.UP, 1.2)` of `ejectReward`.
const EJECT_HEIGHT: f64 = 1.2;
/// Vanilla parity: the `accuracy` argument of the eject's `spawnItem`.
const EJECT_ACCURACY: i32 = 2;
/// Vanilla parity: the `+ 2.0F + random.nextInt(4)` of `calculatePositionAbove`.
const ITEM_SPAWNER_HEIGHT_BASE: f64 = 2.0;
/// And the width of the roll on top of it.
const ITEM_SPAWNER_HEIGHT_ROLL: i32 = 4;

/// Which flame a trial spawner shows.
///
/// Vanilla parity: `TrialSpawner.FlameParticle`, encoded into the level event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlameParticle {
    /// Vanilla parity: `FlameParticle.NORMAL`.
    Normal,
    /// Vanilla parity: `FlameParticle.OMINOUS`.
    Ominous,
}

impl FlameParticle {
    /// Vanilla parity: `FlameParticle.encode`, which is the ordinal.
    #[must_use]
    pub const fn encode(self) -> i32 {
        match self {
            Self::Normal => 0,
            Self::Ominous => 1,
        }
    }
}

/// The two configurations a spawner switches between, and its two ranges.
///
/// Vanilla parity: `TrialSpawner.FullConfig`.
#[derive(Clone, Debug)]
pub struct FullConfig {
    /// What the spawner fights with while it is not ominous.
    pub normal: TrialSpawnerConfigHolder,
    /// And what it fights with once a player brings an omen in.
    pub ominous: TrialSpawnerConfigHolder,
    /// How long the spawner sleeps after a trial is beaten.
    pub target_cooldown_length: i32,
    /// How far away a player still counts as watching.
    pub required_player_range: i32,
}

impl Default for FullConfig {
    fn default() -> Self {
        Self {
            normal: TrialSpawnerConfigHolder::default(),
            ominous: TrialSpawnerConfigHolder::default(),
            target_cooldown_length: DEFAULT_TARGET_COOLDOWN_LENGTH,
            required_player_range: DEFAULT_PLAYER_SCAN_RANGE,
        }
    }
}

impl FullConfig {
    /// Vanilla parity: `TrialSpawner.FullConfig.overrideEntity`.
    #[must_use]
    fn override_entity(&self, entity_type_key: &Identifier) -> Self {
        Self {
            normal: TrialSpawnerConfigHolder::direct(
                self.normal.value().with_spawning(entity_type_key),
            ),
            ominous: TrialSpawnerConfigHolder::direct(
                self.ominous.value().with_spawning(entity_type_key),
            ),
            target_cooldown_length: self.target_cooldown_length,
            required_player_range: self.required_player_range,
        }
    }

    fn load(nbt: &NbtCompoundView<'_, '_>) -> Self {
        let default = Self::default();
        Self {
            normal: nbt
                .get("normal_config")
                .and_then(TrialSpawnerConfigHolder::load)
                .unwrap_or(default.normal),
            ominous: nbt
                .get("ominous_config")
                .and_then(TrialSpawnerConfigHolder::load)
                .unwrap_or(default.ominous),
            target_cooldown_length: nbt
                .get("target_cooldown_length")
                .and_then(|tag| tag.codec_i32())
                .unwrap_or(default.target_cooldown_length)
                .max(0),
            required_player_range: nbt
                .get("required_player_range")
                .and_then(|tag| tag.codec_i32())
                .unwrap_or(default.required_player_range)
                .clamp(1, 128),
        }
    }

    fn save(&self, nbt: &mut NbtCompound) {
        nbt.insert("normal_config", self.normal.save());
        nbt.insert("ominous_config", self.ominous.save());
        nbt.insert("target_cooldown_length", self.target_cooldown_length);
        nbt.insert("required_player_range", self.required_player_range);
    }
}

/// The block state a trial spawner reads and writes.
///
/// Vanilla parity: `TrialSpawner.StateAccessor`, implemented by the block
/// entity because the state lives on the block, not on the spawner.
pub trait TrialSpawnerStateAccessor {
    /// Vanilla parity: `StateAccessor.getState`.
    fn trial_spawner_state(&self) -> TrialSpawnerState;

    /// Vanilla parity: `StateAccessor.setState`.
    fn set_trial_spawner_state(&self, world: &Arc<World>, state: TrialSpawnerState);

    /// Vanilla parity: `StateAccessor.markUpdated`.
    fn mark_trial_spawner_updated(&self);
}

/// One trial spawner.
///
/// Vanilla parity: `TrialSpawner`. The client half (`tickClient`, the spin, the
/// display entity, the ambient sound roll) is absent: Foton is a server, and
/// every one of those is written only from `tickClient`.
pub struct TrialSpawner {
    data: SyncMutex<TrialSpawnerStateData>,
    config: SyncMutex<FullConfig>,
    player_detector: PlayerDetector,
    is_ominous: AtomicBool,
}

impl Default for TrialSpawner {
    fn default() -> Self {
        Self::new()
    }
}

impl TrialSpawner {
    /// Creates a spawner with the default configuration.
    ///
    /// Vanilla parity: `TrialSpawnerBlockEntity.createDefaultSpawner`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            data: SyncMutex::new(TrialSpawnerStateData::default()),
            config: SyncMutex::new(FullConfig::default()),
            player_detector: PlayerDetector::NoCreativePlayers,
            is_ominous: AtomicBool::new(false),
        }
    }

    /// Vanilla parity: `TrialSpawner.isOminous`.
    #[must_use]
    pub fn is_ominous(&self) -> bool {
        self.is_ominous.load(Ordering::Relaxed)
    }

    /// Vanilla parity: `TrialSpawner.activeConfig`.
    #[must_use]
    pub fn active_config(&self) -> TrialSpawnerConfig {
        let config = self.config.lock();
        if self.is_ominous() {
            config.ominous.value().clone()
        } else {
            config.normal.value().clone()
        }
    }

    /// Vanilla parity: `TrialSpawner.ominousConfig`.
    #[must_use]
    pub fn ominous_config(&self) -> TrialSpawnerConfig {
        self.config.lock().ominous.value().clone()
    }

    /// Vanilla parity: `TrialSpawner.getTargetCooldownLength`.
    #[must_use]
    pub fn target_cooldown_length(&self) -> i32 {
        self.config.lock().target_cooldown_length
    }

    /// Vanilla parity: `TrialSpawner.getRequiredPlayerRange`.
    #[must_use]
    pub fn required_player_range(&self) -> i32 {
        self.config.lock().required_player_range
    }

    /// Runs the whole state data behind its lock.
    ///
    /// Every caller that touches the spawner's live state goes through here so
    /// the lock is never held across a call back into the spawner.
    pub fn with_data<R>(&self, action: impl FnOnce(&mut TrialSpawnerStateData) -> R) -> R {
        action(&mut self.data.lock())
    }

    /// Vanilla parity: `TrialSpawner.canSpawnInLevel`.
    #[must_use]
    pub fn can_spawn_in_level(world: &Arc<World>) -> bool {
        if !world.get_game_rule(&SPAWNER_BLOCKS_WORK) {
            return false;
        }
        if world.difficulty() == Difficulty::Peaceful {
            return false;
        }
        world.get_game_rule(&SPAWN_MOBS)
    }

    /// Vanilla parity: `TrialSpawner.applyOminous`.
    pub fn apply_ominous(
        &self,
        accessor: &dyn TrialSpawnerStateAccessor,
        world: &Arc<World>,
        pos: BlockPos,
    ) {
        let state = world.get_block_state(pos);
        world.set_block(
            pos,
            state.set_value(&BlockStateProperties::OMINOUS, true),
            UpdateFlags::UPDATE_ALL,
        );
        world.level_event(
            level_events::PARTICLES_TRIAL_SPAWNER_BECOME_OMINOUS,
            pos,
            1,
            None,
        );
        self.is_ominous.store(true, Ordering::Relaxed);
        self.reset_after_becoming_ominous(accessor, world);
    }

    /// Vanilla parity: `TrialSpawner.removeOminous`.
    pub fn remove_ominous(&self, world: &Arc<World>, pos: BlockPos) {
        let state = world.get_block_state(pos);
        world.set_block(
            pos,
            state.set_value(&BlockStateProperties::OMINOUS, false),
            UpdateFlags::UPDATE_ALL,
        );
        self.is_ominous.store(false, Ordering::Relaxed);
    }

    /// Vanilla parity: `TrialSpawnerStateData.resetAfterBecomingOminous`, which
    /// lives on the data in vanilla but needs the spawner's config here.
    fn reset_after_becoming_ominous(
        &self,
        accessor: &dyn TrialSpawnerStateAccessor,
        world: &Arc<World>,
    ) {
        let ominous = self.ominous_config();
        let tracked: Vec<Uuid> = self.with_data(|data| data.current_mobs.clone());
        for uuid in tracked {
            let Some(entity) = world.get_entity_by_uuid(&uuid) else {
                continue;
            };
            world.level_event(
                level_events::PARTICLES_TRIAL_SPAWNER_SPAWN_MOB_AT,
                entity.block_position(),
                FlameParticle::Normal.encode(),
                None,
            );
            if let Some(mob) = entity.as_mob() {
                mob.drop_preserved_equipment(world);
            }
            entity.set_removed(RemovalReason::Discarded);
        }

        let game_time = world.game_time();
        self.with_data(|data| {
            if !ominous.spawn_potentials.is_empty() {
                data.next_spawn_data = None;
            }
            data.total_mobs_spawned = 0;
            data.current_mobs.clear();
            data.next_mob_spawns_at = game_time + i64::from(ominous.ticks_between_spawn);
            data.cooldown_ends_at = game_time + TICKS_BETWEEN_ITEM_SPAWNERS;
        });
        accessor.mark_trial_spawner_updated();
    }

    /// Vanilla parity: `TrialSpawner.overrideEntityToSpawn`.
    pub fn override_entity_to_spawn(
        &self,
        accessor: &dyn TrialSpawnerStateAccessor,
        world: &Arc<World>,
        entity_type_key: &Identifier,
    ) {
        self.with_data(TrialSpawnerStateData::reset);
        {
            let mut config = self.config.lock();
            *config = config.override_entity(entity_type_key);
        }
        accessor.set_trial_spawner_state(world, TrialSpawnerState::Inactive);
    }

    /// Vanilla parity: `TrialSpawner.load`.
    pub fn load(&self, nbt: &NbtCompoundView<'_, '_>) {
        self.with_data(|data| data.load(nbt));
        *self.config.lock() = FullConfig::load(nbt);
    }

    /// Vanilla parity: `TrialSpawner.store`.
    pub fn save(&self, nbt: &mut NbtCompound) {
        self.with_data(|data| data.save(nbt));
        self.config.lock().save(nbt);
    }

    /// Vanilla parity: `TrialSpawner.tickServer`.
    pub fn tick_server(
        &self,
        accessor: &dyn TrialSpawnerStateAccessor,
        world: &Arc<World>,
        pos: BlockPos,
        is_ominous: bool,
    ) {
        self.is_ominous.store(is_ominous, Ordering::Relaxed);
        let current = accessor.trial_spawner_state();
        let current_for_compare = current.clone();

        // Vanilla parity: the `currentMobs.removeIf` of `tickServer`, which
        // pushes the next spawn back whenever a tracked mob is dropped -- so a
        // player who kills one does not immediately get its replacement.
        let ticks_between_spawn = self.active_config().ticks_between_spawn;
        let game_time = world.game_time();
        self.with_data(|data| {
            let before = data.current_mobs.len();
            data.current_mobs
                .retain(|uuid| !should_mob_be_untracked(world, pos, *uuid));
            if data.current_mobs.len() != before {
                data.next_mob_spawns_at = game_time + i64::from(ticks_between_spawn);
            }
        });

        let next = self.tick_and_get_next(current, accessor, world, pos);
        if next != current_for_compare {
            accessor.set_trial_spawner_state(world, next);
        }
    }

    /// Vanilla parity: `TrialSpawnerState.tickAndGetNext`.
    fn tick_and_get_next(
        &self,
        current: TrialSpawnerState,
        accessor: &dyn TrialSpawnerStateAccessor,
        world: &Arc<World>,
        pos: BlockPos,
    ) -> TrialSpawnerState {
        let config = self.active_config();
        match current {
            TrialSpawnerState::Inactive => self.tick_inactive(accessor, &config),
            TrialSpawnerState::WaitingForPlayers => {
                self.tick_waiting_for_players(accessor, world, pos, &config)
            }
            TrialSpawnerState::Active => self.tick_active(accessor, world, pos, &config),
            TrialSpawnerState::WaitingForRewardEjection => {
                self.tick_waiting_for_reward_ejection(world, pos)
            }
            TrialSpawnerState::EjectingReward => self.tick_ejecting_reward(world, pos, &config),
            TrialSpawnerState::Cooldown => self.tick_cooldown(accessor, world, pos),
        }
    }

    /// Vanilla parity: the `INACTIVE` arm, which waits for a mob to display.
    ///
    /// Vanilla asks `getOrCreateDisplayEntity`, whose only server-visible job is
    /// to answer whether the next spawn data names an entity at all.
    fn tick_inactive(
        &self,
        accessor: &dyn TrialSpawnerStateAccessor,
        config: &TrialSpawnerConfig,
    ) -> TrialSpawnerState {
        let named = self.with_data(|data| {
            let drawn = data.get_or_create_next_spawn_data(config);
            let named = data
                .next_spawn_data
                .as_ref()
                .and_then(SpawnData::entity_type_key)
                .is_some();
            (drawn, named)
        });
        if named.0 {
            accessor.mark_trial_spawner_updated();
        }
        if named.1 {
            TrialSpawnerState::WaitingForPlayers
        } else {
            TrialSpawnerState::Inactive
        }
    }

    /// Vanilla parity: the `WAITING_FOR_PLAYERS` arm.
    fn tick_waiting_for_players(
        &self,
        accessor: &dyn TrialSpawnerStateAccessor,
        world: &Arc<World>,
        pos: BlockPos,
        config: &TrialSpawnerConfig,
    ) -> TrialSpawnerState {
        if !Self::can_spawn_in_level(world) {
            self.with_data(TrialSpawnerStateData::reset_statistics);
            return TrialSpawnerState::WaitingForPlayers;
        }
        if !self.has_mob_to_spawn(accessor, config) {
            return TrialSpawnerState::Inactive;
        }
        self.try_detect_players(accessor, world, pos);
        if self.with_data(|data| data.detected_players.is_empty()) {
            TrialSpawnerState::WaitingForPlayers
        } else {
            TrialSpawnerState::Active
        }
    }

    /// Vanilla parity: the `ACTIVE` arm.
    fn tick_active(
        &self,
        accessor: &dyn TrialSpawnerStateAccessor,
        world: &Arc<World>,
        pos: BlockPos,
        config: &TrialSpawnerConfig,
    ) -> TrialSpawnerState {
        if !Self::can_spawn_in_level(world) {
            self.with_data(TrialSpawnerStateData::reset_statistics);
            return TrialSpawnerState::WaitingForPlayers;
        }
        if !self.has_mob_to_spawn(accessor, config) {
            return TrialSpawnerState::Inactive;
        }

        let additional_players = self.with_data(|data| data.count_additional_players(pos));
        self.try_detect_players(accessor, world, pos);
        if self.is_ominous() {
            self.spawn_ominous_item_spawner(world, pos, config);
        }

        let game_time = world.game_time();
        let finished =
            self.with_data(|data| data.has_finished_spawning_all_mobs(config, additional_players));
        if finished {
            if self.with_data(|data| data.have_all_current_mobs_died()) {
                let cooldown = i64::from(self.target_cooldown_length());
                self.with_data(|data| {
                    data.cooldown_ends_at = game_time + cooldown;
                    data.total_mobs_spawned = 0;
                    data.next_mob_spawns_at = 0;
                });
                return TrialSpawnerState::WaitingForRewardEjection;
            }
            return TrialSpawnerState::Active;
        }

        let ready = self.with_data(|data| {
            data.is_ready_to_spawn_next_mob(game_time, config, additional_players)
        });
        if ready && let Some(spawned) = self.spawn_mob(world, pos, config) {
            let redrawn = self.with_data(|data| {
                data.current_mobs.push(spawned);
                data.total_mobs_spawned += 1;
                data.next_mob_spawns_at = game_time + i64::from(config.ticks_between_spawn);
                match config.spawn_potentials.get_random() {
                    Some(next) => {
                        data.next_spawn_data = Some(next.clone());
                        true
                    }
                    None => false,
                }
            });
            if redrawn {
                accessor.mark_trial_spawner_updated();
            }
        }

        TrialSpawnerState::Active
    }

    /// Vanilla parity: the `WAITING_FOR_REWARD_EJECTION` arm.
    fn tick_waiting_for_reward_ejection(
        &self,
        world: &Arc<World>,
        pos: BlockPos,
    ) -> TrialSpawnerState {
        let cooldown = self.target_cooldown_length();
        let ready = self.with_data(|data| {
            data.is_ready_to_open_shutter(
                world.game_time(),
                DELAY_BEFORE_EJECT_AFTER_KILLING_LAST_MOB,
                cooldown,
            )
        });
        if !ready {
            return TrialSpawnerState::WaitingForRewardEjection;
        }
        world.play_sound(
            &sound_events::BLOCK_TRIAL_SPAWNER_OPEN_SHUTTER,
            SoundSource::Blocks,
            pos,
            1.0,
            1.0,
            None,
        );
        TrialSpawnerState::EjectingReward
    }

    /// Vanilla parity: the `EJECTING_REWARD` arm.
    fn tick_ejecting_reward(
        &self,
        world: &Arc<World>,
        pos: BlockPos,
        config: &TrialSpawnerConfig,
    ) -> TrialSpawnerState {
        let cooldown = self.target_cooldown_length();
        let ready = self.with_data(|data| {
            data.is_ready_to_eject_items(world.game_time(), TIME_BETWEEN_EACH_EJECTION, cooldown)
        });
        if !ready {
            return TrialSpawnerState::EjectingReward;
        }

        if self.with_data(|data| data.detected_players.is_empty()) {
            world.play_sound(
                &sound_events::BLOCK_TRIAL_SPAWNER_CLOSE_SHUTTER,
                SoundSource::Blocks,
                pos,
                1.0,
                1.0,
                None,
            );
            self.with_data(|data| data.ejecting_loot_table = None);
            return TrialSpawnerState::Cooldown;
        }

        let loot_table = self.with_data(|data| {
            if data.ejecting_loot_table.is_none() {
                data.ejecting_loot_table = config.loot_tables_to_eject.get_random().copied();
            }
            data.ejecting_loot_table
        });
        if let Some(loot_table) = loot_table {
            eject_reward(world, pos, loot_table);
        }
        self.with_data(|data| {
            if !data.detected_players.is_empty() {
                data.detected_players.remove(0);
            }
        });
        TrialSpawnerState::EjectingReward
    }

    /// Vanilla parity: the `COOLDOWN` arm.
    fn tick_cooldown(
        &self,
        accessor: &dyn TrialSpawnerStateAccessor,
        world: &Arc<World>,
        pos: BlockPos,
    ) -> TrialSpawnerState {
        self.try_detect_players(accessor, world, pos);
        if !self.with_data(|data| data.detected_players.is_empty()) {
            self.with_data(|data| {
                data.total_mobs_spawned = 0;
                data.next_mob_spawns_at = 0;
            });
            return TrialSpawnerState::Active;
        }
        if self.with_data(|data| data.is_cooldown_finished(world.game_time())) {
            self.remove_ominous(world, pos);
            self.with_data(TrialSpawnerStateData::reset);
            return TrialSpawnerState::WaitingForPlayers;
        }
        TrialSpawnerState::Cooldown
    }

    /// Vanilla parity: `TrialSpawnerStateData.hasMobToSpawn`.
    fn has_mob_to_spawn(
        &self,
        accessor: &dyn TrialSpawnerStateAccessor,
        config: &TrialSpawnerConfig,
    ) -> bool {
        let (drawn, named) = self.with_data(|data| {
            let drawn = data.get_or_create_next_spawn_data(config);
            let named = data
                .next_spawn_data
                .as_ref()
                .and_then(SpawnData::entity_type_key)
                .is_some();
            (drawn, named)
        });
        if drawn {
            accessor.mark_trial_spawner_updated();
        }
        named || !config.spawn_potentials.is_empty()
    }

    /// Vanilla parity: `TrialSpawnerStateData.tryDetectPlayers`.
    fn try_detect_players(
        &self,
        accessor: &dyn TrialSpawnerStateAccessor,
        world: &Arc<World>,
        pos: BlockPos,
    ) {
        // Vanilla staggers the scan by position so that a chamber full of
        // spawners does not scan on the same tick.
        let throttled =
            (packed_pos(pos).wrapping_add(world.game_time())) % DELAY_BETWEEN_PLAYER_SCANS != 0;
        if throttled {
            return;
        }

        let state = accessor.trial_spawner_state();
        if state == TrialSpawnerState::Cooldown && self.is_ominous() {
            return;
        }

        let range = f64::from(self.required_player_range());
        let in_line_of_sight = self.player_detector.detect(world, pos, range, true);

        let mut became_ominous = false;
        if !self.is_ominous()
            && !in_line_of_sight.is_empty()
            && let Some((player_uuid, effect_is_bad_omen)) =
                find_player_with_ominous_effect(world, &in_line_of_sight)
        {
            {
                if let Some(player) = world.players.get_by_uuid(&player_uuid) {
                    if effect_is_bad_omen {
                        transform_bad_omen_into_trial_omen(&player);
                    }
                    let eye = BlockPos::new(
                        player.position().x.floor() as i32,
                        player.get_eye_y().floor() as i32,
                        player.position().z.floor() as i32,
                    );
                    world.level_event(
                        level_events::PARTICLES_TRIAL_SPAWNER_BECOME_OMINOUS,
                        eye,
                        0,
                        None,
                    );
                }
                self.apply_ominous(accessor, world, pos);
                became_ominous = true;
            }
        }

        if state == TrialSpawnerState::Cooldown && !became_ominous {
            return;
        }

        let searching_for_first = self.with_data(|data| data.detected_players.is_empty());
        let found = if searching_for_first {
            in_line_of_sight
        } else {
            self.player_detector.detect(world, pos, range, false)
        };

        let game_time = world.game_time();
        let (changed, count) = self.with_data(|data| {
            let changed = data.add_detected_players(&found);
            if changed {
                data.next_mob_spawns_at = data
                    .next_mob_spawns_at
                    .max(game_time + DETECT_PLAYER_SPAWN_BUFFER);
            }
            (changed, data.detected_players.len() as i32)
        });

        if changed && !became_ominous {
            let event = if self.is_ominous() {
                level_events::PARTICLES_TRIAL_SPAWNER_DETECT_PLAYER_OMINOUS
            } else {
                level_events::PARTICLES_TRIAL_SPAWNER_DETECT_PLAYER
            };
            world.level_event(event, pos, count, None);
        }
    }

    /// Vanilla parity: `TrialSpawner.spawnMob`.
    fn spawn_mob(
        &self,
        world: &Arc<World>,
        pos: BlockPos,
        config: &TrialSpawnerConfig,
    ) -> Option<Uuid> {
        let spawn_data = self.with_data(|data| {
            data.get_or_create_next_spawn_data(config);
            data.next_spawn_data.clone()
        })?;
        let entity_type_key = spawn_data.entity_type_key()?;
        let entity_type = REGISTRY.entity_types.by_key(&entity_type_key)?;

        let range = f64::from(config.spawn_range);
        let spawn_pos = DVec3::new(
            (rand::random::<f64>() - rand::random::<f64>())
                .mul_add(range, f64::from(pos.x()) + 0.5),
            f64::from(pos.y() + rand::random_range(0..3) - 1),
            (rand::random::<f64>() - rand::random::<f64>())
                .mul_add(range, f64::from(pos.z()) + 0.5),
        );

        let spawn_aabb = foton_utils::WorldAabb::entity_box(
            spawn_pos.x,
            spawn_pos.y,
            spawn_pos.z,
            f64::from(entity_type.dimensions.half_width()),
            f64::from(entity_type.dimensions.height),
        );
        if has_collision(&WorldCollisionProvider::new(world), spawn_aabb) {
            return None;
        }
        if !in_line_of_sight(world, center_of(pos), spawn_pos) {
            return None;
        }

        let spawn_block_pos = BlockPos::new(
            spawn_pos.x.floor() as i32,
            spawn_pos.y.floor() as i32,
            spawn_pos.z.floor() as i32,
        );

        let entity = load_spawner_entity(
            world,
            entity_type,
            spawn_pos,
            &spawn_data,
            EntitySpawnReason::TrialSpawner,
        )?;
        entity.set_rotation((rand::random::<f32>() * 360.0, 0.0));

        if let Some(mob) = entity.as_mob() {
            // Vanilla checks `SpawnPlacements.checkSpawnRules` before creating
            // the mob; Foton has no static predicate, so the instance is asked
            // here, next to the obstruction check vanilla does in the same
            // place. `EntitySpawnReason::TrialSpawner` already waives the light
            // requirement, which is what lets a chamber spawn in torchlight.
            if !mob.check_spawn_rules(world, EntitySpawnReason::TrialSpawner, spawn_block_pos) {
                return None;
            }
            if let Some(rules) = spawn_data.custom_spawn_rules()
                && !custom_spawn_rules_allow(rules, world, spawn_block_pos)
            {
                return None;
            }
            if !mob.is_free(DVec3::ZERO) {
                return None;
            }
            if spawn_data.has_no_configuration() {
                let _ = mob.finalize_spawn(world, EntitySpawnReason::TrialSpawner, None);
            }
            mob.set_persistence_required();
            if let Some(equipment) = spawn_data.equipment() {
                mob.equip_from_table(world, equipment);
            }
        }

        let uuid = entity.uuid();
        if world.try_add_entity(Arc::clone(&entity)).is_err() {
            return None;
        }

        let flame = if self.is_ominous() {
            FlameParticle::Ominous
        } else {
            FlameParticle::Normal
        };
        world.level_event(
            level_events::PARTICLES_TRIAL_SPAWNER_SPAWN,
            pos,
            flame.encode(),
            None,
        );
        world.level_event(
            level_events::PARTICLES_TRIAL_SPAWNER_SPAWN_MOB_AT,
            spawn_block_pos,
            flame.encode(),
            None,
        );
        world.game_event(
            &vanilla_game_events::ENTITY_PLACE,
            spawn_block_pos,
            &GameEventContext::new(Some(entity.as_ref()), None),
        );
        Some(uuid)
    }

    /// Vanilla parity: `TrialSpawnerState.spawnOminousOminousItemSpawner`.
    fn spawn_ominous_item_spawner(
        &self,
        world: &Arc<World>,
        pos: BlockPos,
        config: &TrialSpawnerConfig,
    ) {
        let item = self
            .dispensing_items(world, pos, config)
            .get_random()
            .cloned()
            .unwrap_or_else(ItemStack::empty);
        if item.is_empty() {
            return;
        }
        if world.game_time() < self.with_data(|data| data.cooldown_ends_at) {
            return;
        }
        let Some(spawn_pos) = self.position_to_spawn_item_spawner(world, pos) else {
            return;
        };

        let spawner = OminousItemSpawnerEntity::create(world, spawn_pos, item);
        let spawn_block = BlockPos::new(
            spawn_pos.x.floor() as i32,
            spawn_pos.y.floor() as i32,
            spawn_pos.z.floor() as i32,
        );
        if world.try_add_entity(spawner).is_err() {
            return;
        }

        let pitch = (rand::random::<f32>() - rand::random::<f32>()).mul_add(0.2, 1.0);
        world.play_sound(
            &sound_events::BLOCK_TRIAL_SPAWNER_SPAWN_ITEM_BEGIN,
            SoundSource::Blocks,
            spawn_block,
            1.0,
            pitch,
            None,
        );
        let cooldown = world.game_time() + TICKS_BETWEEN_ITEM_SPAWNERS;
        self.with_data(|data| data.cooldown_ends_at = cooldown);
    }

    /// Vanilla parity: `TrialSpawnerState.calculatePositionToSpawnSpawner`, and
    /// the two helpers it calls.
    fn position_to_spawn_item_spawner(&self, world: &Arc<World>, pos: BlockPos) -> Option<DVec3> {
        let center = center_of(pos);
        let range_sqr = f64::from(self.required_player_range()).powi(2);

        let nearby_players: Vec<DVec3> = self.with_data(|data| {
            data.detected_players
                .iter()
                .filter_map(|uuid| world.players.get_by_uuid(uuid))
                .filter(|player| {
                    !player.has_infinite_materials()
                        && !player.is_spectator()
                        && Entity::is_alive(player.as_ref())
                        && player.position().distance_squared(center) <= range_sqr
                })
                .map(|player| player.position())
                .collect()
        });
        if nearby_players.is_empty() {
            return None;
        }

        let nearby_mobs: Vec<DVec3> = self.with_data(|data| {
            data.current_mobs
                .iter()
                .filter_map(|uuid| world.get_entity_by_uuid(uuid))
                .filter(|entity| {
                    !entity.is_removed() && entity.position().distance_squared(center) <= range_sqr
                })
                .map(|entity| entity.position())
                .collect()
        });

        // Vanilla flips a coin between the tracked mobs and the players, and
        // spawns above whichever list it drew from.
        let candidates = if rand::random::<bool>() {
            nearby_mobs
        } else {
            nearby_players
        };
        let target = *candidates.get(rand::random_range(0..candidates.len().max(1)))?;

        position_above(world, target)
    }

    /// Vanilla parity: `TrialSpawnerStateData.getDispensingItems`.
    fn dispensing_items(
        &self,
        world: &Arc<World>,
        pos: BlockPos,
        config: &TrialSpawnerConfig,
    ) -> WeightedList<ItemStack> {
        if let Some(cached) = self.with_data(|data| data.dispensing.clone()) {
            return cached;
        }

        // Vanilla seeds this roll from a low-resolution position so that every
        // spawner in one chamber dispenses the same items.
        let low_resolution = BlockPos::new(
            (f64::from(pos.x()) / 30.0).floor() as i32,
            (f64::from(pos.y()) / 20.0).floor() as i32,
            (f64::from(pos.z()) / 30.0).floor() as i32,
        );
        let seed = world.seed().wrapping_add(packed_pos(low_resolution));
        let mut rng = StdRng::seed_from_u64(seed as u64);
        let mut context = LootContext::new(&mut rng).with_game_time(world.game_time());
        let drops = config
            .items_to_drop_when_ominous
            .get_random_items(&mut context);
        if drops.is_empty() {
            return WeightedList::empty();
        }

        let dispensing = WeightedList::new(
            drops
                .into_iter()
                .map(|drop| Weighted {
                    weight: drop.count(),
                    value: drop.copy_with_count(1),
                })
                .collect(),
        );
        self.with_data(|data| data.dispensing = Some(dispensing.clone()));
        dispensing
    }
}

/// Vanilla parity: `BlockPos.asLong`, the packed form vanilla stirs into its
/// scan throttle and its dispensing seed.
fn packed_pos(pos: BlockPos) -> i64 {
    foton_utils::PackedBlockPos::from(pos).as_raw()
}

/// Vanilla parity: `TrialSpawner.shouldMobBeUntracked`.
fn should_mob_be_untracked(world: &Arc<World>, spawner_pos: BlockPos, uuid: Uuid) -> bool {
    let Some(entity) = world.get_entity_by_uuid(&uuid) else {
        return true;
    };
    if entity.is_removed() {
        return true;
    }
    let mob_pos = entity.block_position();
    let dx = i64::from(mob_pos.x() - spawner_pos.x());
    let dy = i64::from(mob_pos.y() - spawner_pos.y());
    let dz = i64::from(mob_pos.z() - spawner_pos.z());
    dz * dz + dx * dx + dy * dy > MAX_MOB_TRACKING_DISTANCE_SQR
}

/// Vanilla parity: `TrialSpawner.ejectReward`.
fn eject_reward(world: &Arc<World>, pos: BlockPos, loot_table: LootTableRef) {
    let mut rng = rand::rng();
    let mut context = LootContext::new(&mut rng).with_game_time(world.game_time());
    let drops = loot_table.get_random_items(&mut context);
    if drops.is_empty() {
        return;
    }
    let origin = DVec3::new(
        f64::from(pos.x()) + 0.5,
        f64::from(pos.y()) + EJECT_HEIGHT,
        f64::from(pos.z()) + 0.5,
    );
    for drop in drops {
        spawn_item_toward(world, origin, Direction::Up, EJECT_ACCURACY, drop);
    }
    world.level_event(
        level_events::ANIMATION_TRIAL_SPAWNER_EJECT_ITEM,
        pos,
        0,
        None,
    );
}

/// Vanilla parity: `TrialSpawnerStateData.findPlayerWithOminousEffect`.
///
/// Returns the player and whether the effect found was bad omen rather than
/// trial omen -- trial omen wins outright, bad omen is the fallback.
fn find_player_with_ominous_effect(
    world: &Arc<World>,
    candidates: &[Uuid],
) -> Option<(Uuid, bool)> {
    let mut with_bad_omen = None;
    for uuid in candidates {
        let Some(player) = world.players.get_by_uuid(uuid) else {
            continue;
        };
        if player.has_mob_effect(vanilla_mob_effects::TRIAL_OMEN) {
            return Some((*uuid, false));
        }
        if with_bad_omen.is_none() && player.has_mob_effect(vanilla_mob_effects::BAD_OMEN) {
            with_bad_omen = Some(*uuid);
        }
    }
    with_bad_omen.map(|uuid| (uuid, true))
}

/// Vanilla parity: `TrialSpawnerStateData.transformBadOmenIntoTrialOmen`.
fn transform_bad_omen_into_trial_omen(player: &Arc<Player>) {
    let Some(bad_omen) = player
        .living_base()
        .mob_effect(vanilla_mob_effects::BAD_OMEN)
    else {
        return;
    };
    let amplifier = bad_omen.amplifier() + 1;
    let duration = super::state_data::TRIAL_OMEN_PER_BAD_OMEN_LEVEL * amplifier;
    player.remove_mob_effect(vanilla_mob_effects::BAD_OMEN);
    player.add_mob_effect(MobEffectInstance::with_duration(
        vanilla_mob_effects::TRIAL_OMEN,
        duration,
        0,
    ));
}

/// Vanilla parity: `TrialSpawnerState.calculatePositionAbove`.
fn position_above(world: &Arc<World>, entity_pos: DVec3) -> Option<DVec3> {
    let height =
        ITEM_SPAWNER_HEIGHT_BASE + f64::from(rand::random_range(0..ITEM_SPAWNER_HEIGHT_ROLL));
    let try_pos = entity_pos + DVec3::new(0.0, height, 0.0);
    let hit = world.clip(entity_pos, try_pos, ClipBlockShape::Visual, ClipFluid::None);
    let down = center_of(hit.block_pos) - DVec3::new(0.0, 1.0, 0.0);
    let block_down = BlockPos::new(
        down.x.floor() as i32,
        down.y.floor() as i32,
        down.z.floor() as i32,
    );
    let state = world.get_block_state(block_down);
    state
        .get_collision_shape_at(block_down)
        .is_empty()
        .then_some(down)
}
