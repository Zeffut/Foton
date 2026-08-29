//! What one trial spawner remembers between ticks.
//!
//! Vanilla parity:
//! `net.minecraft.world.level.block.entity.trialspawner.TrialSpawnerStateData`.
//! Vanilla names the persisted half `TrialSpawnerStateData.Packed`; here the
//! same struct is both, because there is no codec layer to pack for.

use foton_registry::blocks::properties::TrialSpawnerState;
use foton_registry::item_stack::ItemStack;
use foton_registry::loot_table::LootTableRef;
use foton_registry::spawn_data::SpawnData;
use foton_registry::trial_spawner_config::TrialSpawnerConfig;
use foton_registry::{REGISTRY, RegistryExt as _};
use foton_utils::random::weighted_list::WeightedList;
use foton_utils::uuid_ext::{load_uuid_list, save_uuid_list};
use foton_utils::{BlockPos, Identifier};
use simdnbt::borrow::NbtCompound as NbtCompoundView;
use simdnbt::owned::NbtCompound;
use uuid::Uuid;

/// Vanilla parity: `TrialSpawnerStateData.DELAY_BETWEEN_PLAYER_SCANS`.
pub const DELAY_BETWEEN_PLAYER_SCANS: i64 = 20;

/// Vanilla parity: `TrialSpawnerStateData.TRIAL_OMEN_PER_BAD_OMEN_LEVEL`.
pub const TRIAL_OMEN_PER_BAD_OMEN_LEVEL: i32 = 18_000;

/// The live state of one trial spawner.
///
/// Vanilla's `detectedPlayers` and `currentMobs` are hash sets. Foton keeps
/// them as insertion-ordered vectors so that the player an ejection pays out
/// first is the first one the spawner saw rather than whichever the hash
/// happened to put in front; one reward per player either way.
#[derive(Default)]
pub struct TrialSpawnerStateData {
    pub(super) detected_players: Vec<Uuid>,
    pub(super) current_mobs: Vec<Uuid>,
    pub(super) cooldown_ends_at: i64,
    pub(super) next_mob_spawns_at: i64,
    pub(super) total_mobs_spawned: i32,
    pub(super) next_spawn_data: Option<SpawnData>,
    pub(super) ejecting_loot_table: Option<LootTableRef>,
    /// Vanilla parity: the `dispensing` cache, rolled once per spawner.
    pub(super) dispensing: Option<WeightedList<ItemStack>>,
}

impl TrialSpawnerStateData {
    /// Vanilla parity: `TrialSpawnerStateData.reset`.
    pub fn reset(&mut self) {
        self.current_mobs.clear();
        self.next_spawn_data = None;
        self.reset_statistics();
    }

    /// Vanilla parity: `TrialSpawnerStateData.resetStatistics`.
    pub fn reset_statistics(&mut self) {
        self.detected_players.clear();
        self.total_mobs_spawned = 0;
        self.next_mob_spawns_at = 0;
        self.cooldown_ends_at = 0;
    }

    /// Vanilla parity: `TrialSpawnerStateData.hasFinishedSpawningAllMobs`.
    #[must_use]
    pub const fn has_finished_spawning_all_mobs(
        &self,
        config: &TrialSpawnerConfig,
        additional_players: i32,
    ) -> bool {
        self.total_mobs_spawned >= config.calculate_target_total_mobs(additional_players)
    }

    /// Vanilla parity: `TrialSpawnerStateData.haveAllCurrentMobsDied`.
    #[must_use]
    pub const fn have_all_current_mobs_died(&self) -> bool {
        self.current_mobs.is_empty()
    }

    /// Vanilla parity: `TrialSpawnerStateData.isReadyToSpawnNextMob`.
    #[must_use]
    pub const fn is_ready_to_spawn_next_mob(
        &self,
        game_time: i64,
        config: &TrialSpawnerConfig,
        additional_players: i32,
    ) -> bool {
        game_time >= self.next_mob_spawns_at
            && (self.current_mobs.len() as i32)
                < config.calculate_target_simultaneous_mobs(additional_players)
    }

    /// Vanilla parity: `TrialSpawnerStateData.countAdditionalPlayers`.
    #[must_use]
    pub fn count_additional_players(&self, pos: BlockPos) -> i32 {
        if self.detected_players.is_empty() {
            log::debug!("trial spawner at {pos:?} has no detected players");
        }
        (self.detected_players.len() as i32 - 1).max(0)
    }

    /// Vanilla parity: `TrialSpawnerStateData.isReadyToOpenShutter`.
    #[must_use]
    pub fn is_ready_to_open_shutter(
        &self,
        game_time: i64,
        delay_before_open: f32,
        target_cooldown_length: i32,
    ) -> bool {
        let cooldown_started_at = self.cooldown_ends_at - i64::from(target_cooldown_length);
        game_time as f32 >= cooldown_started_at as f32 + delay_before_open
    }

    /// Vanilla parity: `TrialSpawnerStateData.isReadyToEjectItems`, which lands
    /// on an exact multiple of the gap rather than counting down.
    #[must_use]
    pub fn is_ready_to_eject_items(
        &self,
        game_time: i64,
        time_between_ejections: f32,
        target_cooldown_length: i32,
    ) -> bool {
        let cooldown_started_at = self.cooldown_ends_at - i64::from(target_cooldown_length);
        ((game_time - cooldown_started_at) as f32) % time_between_ejections == 0.0
    }

    /// Vanilla parity: `TrialSpawnerStateData.isCooldownFinished`.
    #[must_use]
    pub const fn is_cooldown_finished(&self, game_time: i64) -> bool {
        game_time >= self.cooldown_ends_at
    }

    /// Adds every player not already known, reporting whether anything changed.
    ///
    /// Vanilla parity: the `detectedPlayers.addAll` of `tryDetectPlayers`.
    pub(super) fn add_detected_players(&mut self, found: &[Uuid]) -> bool {
        let mut changed = false;
        for uuid in found {
            if !self.detected_players.contains(uuid) {
                self.detected_players.push(*uuid);
                changed = true;
            }
        }
        changed
    }

    /// Returns the mob this spawner will make next, drawing one if it has none.
    ///
    /// Vanilla parity: `TrialSpawnerStateData.getOrCreateNextSpawnData`. Whether
    /// the draw changed anything is handed back so the caller can fire
    /// `markUpdated`, which vanilla does from inside.
    pub(super) fn get_or_create_next_spawn_data(&mut self, config: &TrialSpawnerConfig) -> bool {
        if self.next_spawn_data.is_some() {
            return false;
        }
        self.next_spawn_data = Some(
            config
                .spawn_potentials
                .get_random()
                .cloned()
                .unwrap_or_default(),
        );
        true
    }

    /// Vanilla parity: `TrialSpawnerStateData.getUpdateTag`.
    #[must_use]
    pub fn update_tag(&self, state: TrialSpawnerState) -> NbtCompound {
        let mut nbt = NbtCompound::new();
        if state == TrialSpawnerState::Active {
            nbt.insert("next_mob_spawns_at", self.next_mob_spawns_at);
        }
        if let Some(next) = &self.next_spawn_data {
            nbt.insert("spawn_data", next.save());
        }
        nbt
    }

    /// Vanilla parity: `TrialSpawnerStateData.Packed.MAP_CODEC` on the way in.
    pub fn load(&mut self, nbt: &NbtCompoundView<'_, '_>) {
        self.detected_players = load_uuid_list(nbt.list("registered_players").as_ref());
        self.current_mobs = load_uuid_list(nbt.list("current_mobs").as_ref());
        self.cooldown_ends_at = nbt.long("cooldown_ends_at").unwrap_or(0);
        self.next_mob_spawns_at = nbt.long("next_mob_spawns_at").unwrap_or(0);
        self.total_mobs_spawned = nbt.int("total_mobs_spawned").unwrap_or(0).max(0);
        self.next_spawn_data = nbt
            .compound("spawn_data")
            .map(|data| SpawnData::load(&data));
        self.ejecting_loot_table = nbt
            .string("ejecting_loot_table")
            .and_then(|key| key.to_str().parse::<Identifier>().ok())
            .and_then(|key| REGISTRY.loot_tables.by_key(&key));
    }

    /// Vanilla parity: `TrialSpawnerStateData.Packed.MAP_CODEC` on the way out.
    pub fn save(&self, nbt: &mut NbtCompound) {
        nbt.insert("registered_players", save_uuid_list(&self.detected_players));
        nbt.insert("current_mobs", save_uuid_list(&self.current_mobs));
        nbt.insert("cooldown_ends_at", self.cooldown_ends_at);
        nbt.insert("next_mob_spawns_at", self.next_mob_spawns_at);
        nbt.insert("total_mobs_spawned", self.total_mobs_spawned);
        if let Some(next) = &self.next_spawn_data {
            nbt.insert("spawn_data", next.save());
        }
        if let Some(loot_table) = self.ejecting_loot_table {
            nbt.insert("ejecting_loot_table", loot_table.key.to_string());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The ejection beat is a modulo on the game time rather than a countdown,
    /// so an ejection that lands one tick off never fires at all. Thirty ticks
    /// apart is what a player sees as one item per second and a half.
    #[test]
    fn items_eject_exactly_on_the_thirty_tick_beat() {
        // A cooldown that started at game time 1000.
        let data = TrialSpawnerStateData {
            cooldown_ends_at: 1000 + 36_000,
            ..TrialSpawnerStateData::default()
        };

        assert!(data.is_ready_to_eject_items(1000, 30.0, 36_000));
        assert!(!data.is_ready_to_eject_items(1015, 30.0, 36_000));
        assert!(data.is_ready_to_eject_items(1030, 30.0, 36_000));
    }

    /// The shutter opens forty ticks after the last mob dies, not immediately.
    /// Opening early would eject the reward into a fight that is still running.
    #[test]
    fn the_shutter_waits_forty_ticks_after_the_cooldown_starts() {
        let data = TrialSpawnerStateData {
            cooldown_ends_at: 1000 + 36_000,
            ..TrialSpawnerStateData::default()
        };

        assert!(!data.is_ready_to_open_shutter(1039, 40.0, 36_000));
        assert!(data.is_ready_to_open_shutter(1040, 40.0, 36_000));
    }

    /// One player is the baseline, so the extra-player count has to floor at
    /// zero -- a negative would scale the fight down below its own config.
    #[test]
    fn the_first_player_adds_nothing_and_none_never_goes_negative() {
        let mut data = TrialSpawnerStateData::default();
        let pos = BlockPos::new(8, 64, 8);
        assert_eq!(data.count_additional_players(pos), 0);

        data.detected_players.push(Uuid::from_u128(1));
        assert_eq!(data.count_additional_players(pos), 0);
        data.detected_players.push(Uuid::from_u128(2));
        assert_eq!(data.count_additional_players(pos), 1);
    }

    /// Detection runs every tick; re-adding a player who is already registered
    /// must not report a change, or the spawner would push its next spawn back
    /// by forty ticks forever and never start.
    #[test]
    fn re_detecting_the_same_player_reports_no_change() {
        let mut data = TrialSpawnerStateData::default();
        let player = Uuid::from_u128(7);

        assert!(data.add_detected_players(&[player]));
        assert!(!data.add_detected_players(&[player]));
        assert_eq!(data.detected_players.len(), 1);
    }
}
