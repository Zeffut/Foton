//! How hard one trial spawner fights and what it pays out.
//!
//! Vanilla parity: `net.minecraft.world.level.block.entity.trialspawner.TrialSpawnerConfig`.
//! Vanilla holds these in the datapack registry `minecraft:trial_spawner`; the
//! entries are generated from the builtin datapack by
//! `steel-registry/build/trial_spawner_configs.rs` and looked up by key here.

use steel_utils::Identifier;
use steel_utils::random::weighted_list::{Weighted, WeightedList};

use crate::loot_table::LootTableRef;
use crate::spawn_data::SpawnData;
use crate::vanilla_loot_tables;
use crate::vanilla_trial_spawner_configs;

/// One trial spawner's tuning.
///
/// Vanilla parity: `TrialSpawnerConfig`.
#[derive(Clone, Debug)]
pub struct TrialSpawnerConfig {
    pub spawn_range: i32,
    pub total_mobs: f32,
    pub simultaneous_mobs: f32,
    pub total_mobs_added_per_player: f32,
    pub simultaneous_mobs_added_per_player: f32,
    pub ticks_between_spawn: i32,
    pub spawn_potentials: WeightedList<SpawnData>,
    pub loot_tables_to_eject: WeightedList<LootTableRef>,
    pub items_to_drop_when_ominous: LootTableRef,
}

/// Vanilla parity: `TrialSpawnerConfig.Builder.spawnRange`.
pub const DEFAULT_SPAWN_RANGE: i32 = 4;
/// Vanilla parity: `TrialSpawnerConfig.Builder.totalMobs`.
pub const DEFAULT_TOTAL_MOBS: f32 = 6.0;
/// Vanilla parity: `TrialSpawnerConfig.Builder.simultaneousMobs`.
pub const DEFAULT_SIMULTANEOUS_MOBS: f32 = 2.0;
/// Vanilla parity: `TrialSpawnerConfig.Builder.totalMobsAddedPerPlayer`.
pub const DEFAULT_TOTAL_MOBS_ADDED_PER_PLAYER: f32 = 2.0;
/// Vanilla parity: `TrialSpawnerConfig.Builder.simultaneousMobsAddedPerPlayer`.
pub const DEFAULT_SIMULTANEOUS_MOBS_ADDED_PER_PLAYER: f32 = 1.0;
/// Vanilla parity: `TrialSpawnerConfig.Builder.ticksBetweenSpawn`.
pub const DEFAULT_TICKS_BETWEEN_SPAWN: i32 = 40;

/// Ticks between two ominous item spawners.
///
/// Vanilla parity: `TrialSpawnerConfig.ticksBetweenItemSpawners`, which is a
/// constant rather than a configured field.
pub const TICKS_BETWEEN_ITEM_SPAWNERS: i64 = 160;

impl Default for TrialSpawnerConfig {
    fn default() -> Self {
        Self::vanilla_default()
    }
}

impl TrialSpawnerConfig {
    /// Returns the configuration a spawner has when nothing names one.
    ///
    /// Vanilla parity: `TrialSpawnerConfig.DEFAULT`, which is what
    /// `TrialSpawnerConfig.builder().build()` produces.
    #[must_use]
    pub fn vanilla_default() -> Self {
        Self {
            spawn_range: DEFAULT_SPAWN_RANGE,
            total_mobs: DEFAULT_TOTAL_MOBS,
            simultaneous_mobs: DEFAULT_SIMULTANEOUS_MOBS,
            total_mobs_added_per_player: DEFAULT_TOTAL_MOBS_ADDED_PER_PLAYER,
            simultaneous_mobs_added_per_player: DEFAULT_SIMULTANEOUS_MOBS_ADDED_PER_PLAYER,
            ticks_between_spawn: DEFAULT_TICKS_BETWEEN_SPAWN,
            spawn_potentials: WeightedList::empty(),
            loot_tables_to_eject: default_loot_tables_to_eject(),
            items_to_drop_when_ominous:
                &vanilla_loot_tables::SPAWNERS_TRIAL_CHAMBER_ITEMS_TO_DROP_WHEN_OMINOUS,
        }
    }

    /// Returns the registered configuration under `key`.
    ///
    /// Vanilla parity: a `Holder` resolved out of `Registries.TRIAL_SPAWNER_CONFIG`.
    #[must_use]
    pub fn by_key(key: &Identifier) -> Option<&'static Self> {
        vanilla_trial_spawner_configs::by_key(key)
    }

    /// Replaces the spawn potentials with a single entity type.
    ///
    /// Vanilla parity: `TrialSpawnerConfig.withSpawning`, used by the spawner
    /// minecart and by `/setblock`-style entity overrides.
    #[must_use]
    pub fn with_spawning(&self, entity_type_key: &Identifier) -> Self {
        Self {
            spawn_potentials: WeightedList::single(SpawnData::of_entity(entity_type_key)),
            ..self.clone()
        }
    }

    /// Returns how many mobs this spawner will produce in total.
    ///
    /// Vanilla parity: `TrialSpawnerConfig.calculateTargetTotalMobs`.
    #[must_use]
    pub fn calculate_target_total_mobs(&self, additional_players: i32) -> i32 {
        self.total_mobs_added_per_player
            .mul_add(additional_players as f32, self.total_mobs)
            .floor() as i32
    }

    /// Returns how many of its mobs may be alive at once.
    ///
    /// Vanilla parity: `TrialSpawnerConfig.calculateTargetSimultaneousMobs`.
    #[must_use]
    pub fn calculate_target_simultaneous_mobs(&self, additional_players: i32) -> i32 {
        self.simultaneous_mobs_added_per_player
            .mul_add(additional_players as f32, self.simultaneous_mobs)
            .floor() as i32
    }
}

/// The reward split a spawner ejects when its datapack entry names none.
///
/// Vanilla parity: the `lootTablesToEject` field initialiser of
/// `TrialSpawnerConfig.Builder`, in that order and both at weight one.
#[must_use]
pub fn default_loot_tables_to_eject() -> WeightedList<LootTableRef> {
    WeightedList::new(vec![
        Weighted {
            value: &vanilla_loot_tables::SPAWNERS_TRIAL_CHAMBER_CONSUMABLES as LootTableRef,
            weight: 1,
        },
        Weighted {
            value: &vanilla_loot_tables::SPAWNERS_TRIAL_CHAMBER_KEY as LootTableRef,
            weight: 1,
        },
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::init_vanilla_registry;

    /// The player scaling is the whole difficulty curve of a trial chamber, and
    /// vanilla floors rather than rounds -- three players on a default spawner
    /// get twelve mobs, not thirteen.
    #[test]
    fn player_scaling_floors_the_way_vanilla_does() {
        let config = TrialSpawnerConfig::vanilla_default();

        assert_eq!(config.calculate_target_total_mobs(0), 6);
        assert_eq!(config.calculate_target_total_mobs(3), 12);
        assert_eq!(config.calculate_target_simultaneous_mobs(0), 2);
        assert_eq!(config.calculate_target_simultaneous_mobs(3), 5);
    }

    /// A fractional per-player term is exactly where a rounding mistake hides:
    /// the breeze spawner adds half a simultaneous mob per extra player.
    #[test]
    fn a_half_mob_per_player_only_counts_on_the_second_player() {
        let config = TrialSpawnerConfig {
            simultaneous_mobs: 1.0,
            simultaneous_mobs_added_per_player: 0.5,
            ..TrialSpawnerConfig::vanilla_default()
        };

        assert_eq!(config.calculate_target_simultaneous_mobs(0), 1);
        assert_eq!(config.calculate_target_simultaneous_mobs(1), 1);
        assert_eq!(config.calculate_target_simultaneous_mobs(2), 2);
    }

    /// The generated registry is the only thing that resolves the config a
    /// trial chamber's spawner names, so a missing entry means every chamber
    /// spawner silently falls back to the default fight.
    #[test]
    fn the_generated_registry_carries_the_trial_chamber_entries() {
        init_vanilla_registry();
        let key = Identifier::vanilla_static("trial_chamber/melee/zombie/normal");
        let config = TrialSpawnerConfig::by_key(&key).expect("zombie melee config");

        assert_eq!(config.ticks_between_spawn, 20);
        assert_eq!(config.spawn_potentials.len(), 1);
        assert_eq!(
            config.spawn_potentials.entries()[0].value.entity_type_key(),
            Some(Identifier::vanilla_static("zombie"))
        );

        let ominous = TrialSpawnerConfig::by_key(&Identifier::vanilla_static(
            "trial_chamber/melee/zombie/ominous",
        ))
        .expect("zombie melee ominous config");
        assert_eq!(ominous.loot_tables_to_eject.total_weight(), 10);
    }
}
