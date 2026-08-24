//! How hard one trial spawner fights and what it pays out.
//!
//! Vanilla parity: `net.minecraft.world.level.block.entity.trialspawner.TrialSpawnerConfig`.
//! Vanilla holds these in the datapack registry `minecraft:trial_spawner`; the
//! entries are generated from the builtin datapack by
//! `steel-registry/build/trial_spawner_configs.rs` and looked up by key here.

use std::sync::Arc;

use simdnbt::borrow::{NbtCompound as NbtCompoundView, NbtTag as BorrowedNbtTag};
use simdnbt::owned::{NbtCompound, NbtTag};
use steel_utils::Identifier;
use steel_utils::nbt::NbtNumeric as _;
use steel_utils::random::weighted_list::{Weighted, WeightedList};

use crate::loot_table::LootTableRef;
use crate::spawn_data::{SpawnData, load_loot_table_list, save_loot_table_list};
use crate::{REGISTRY, RegistryExt as _, vanilla_loot_tables, vanilla_trial_spawner_configs};

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
    pub const fn calculate_target_total_mobs(&self, additional_players: i32) -> i32 {
        self.total_mobs_added_per_player
            .mul_add(additional_players as f32, self.total_mobs)
            .floor() as i32
    }

    /// Returns how many of its mobs may be alive at once.
    ///
    /// Vanilla parity: `TrialSpawnerConfig.calculateTargetSimultaneousMobs`.
    #[must_use]
    pub const fn calculate_target_simultaneous_mobs(&self, additional_players: i32) -> i32 {
        self.simultaneous_mobs_added_per_player
            .mul_add(additional_players as f32, self.simultaneous_mobs)
            .floor() as i32
    }
}

impl TrialSpawnerConfig {
    /// Reads one inline configuration.
    ///
    /// Vanilla parity: `TrialSpawnerConfig.DIRECT_CODEC`.
    #[must_use]
    pub fn load(nbt: &NbtCompoundView<'_, '_>) -> Self {
        let default = Self::vanilla_default();
        let items_to_drop_when_ominous = nbt
            .string("items_to_drop_when_ominous")
            .and_then(|key| key.to_str().parse::<Identifier>().ok())
            .and_then(|key| REGISTRY.loot_tables.by_key(&key))
            .unwrap_or(default.items_to_drop_when_ominous);

        Self {
            spawn_range: numeric_or(nbt, "spawn_range", default.spawn_range),
            total_mobs: float_or(nbt, "total_mobs", default.total_mobs),
            simultaneous_mobs: float_or(nbt, "simultaneous_mobs", default.simultaneous_mobs),
            total_mobs_added_per_player: float_or(
                nbt,
                "total_mobs_added_per_player",
                default.total_mobs_added_per_player,
            ),
            simultaneous_mobs_added_per_player: float_or(
                nbt,
                "simultaneous_mobs_added_per_player",
                default.simultaneous_mobs_added_per_player,
            ),
            ticks_between_spawn: numeric_or(
                nbt,
                "ticks_between_spawn",
                default.ticks_between_spawn,
            ),
            spawn_potentials: SpawnData::load_list(nbt.list("spawn_potentials").as_ref()),
            loot_tables_to_eject: match nbt.list("loot_tables_to_eject") {
                Some(list) => load_loot_table_list(Some(&list)),
                None => default.loot_tables_to_eject,
            },
            items_to_drop_when_ominous,
        }
    }

    /// Writes one inline configuration.
    #[must_use]
    pub fn save(&self) -> NbtCompound {
        let mut nbt = NbtCompound::new();
        nbt.insert("spawn_range", self.spawn_range);
        nbt.insert("total_mobs", self.total_mobs);
        nbt.insert("simultaneous_mobs", self.simultaneous_mobs);
        nbt.insert(
            "total_mobs_added_per_player",
            self.total_mobs_added_per_player,
        );
        nbt.insert(
            "simultaneous_mobs_added_per_player",
            self.simultaneous_mobs_added_per_player,
        );
        nbt.insert("ticks_between_spawn", self.ticks_between_spawn);
        nbt.insert(
            "spawn_potentials",
            SpawnData::save_list(&self.spawn_potentials),
        );
        nbt.insert(
            "loot_tables_to_eject",
            save_loot_table_list(&self.loot_tables_to_eject),
        );
        nbt.insert(
            "items_to_drop_when_ominous",
            self.items_to_drop_when_ominous.key.to_string(),
        );
        nbt
    }
}

/// Vanilla parity: `ValueInput.getIntOr`, which accepts any numeric tag.
fn numeric_or(nbt: &NbtCompoundView<'_, '_>, name: &str, fallback: i32) -> i32 {
    nbt.get(name)
        .and_then(|tag| tag.codec_i32())
        .unwrap_or(fallback)
}

/// Vanilla parity: `ValueInput.getFloatOr`.
fn float_or(nbt: &NbtCompoundView<'_, '_>, name: &str, fallback: f32) -> f32 {
    nbt.get(name)
        .and_then(|tag| tag.codec_f32())
        .unwrap_or(fallback)
}

/// A configuration a trial spawner points at, by key or inline.
///
/// Vanilla parity: `Holder<TrialSpawnerConfig>` under `RegistryFileCodec`,
/// which writes a bare key for a registered value and the whole object for a
/// direct one. Keeping the two apart is what lets a saved spawner come back
/// pointing at the same thing it was pointing at.
#[derive(Clone, Debug)]
pub enum TrialSpawnerConfigHolder {
    /// A value out of the `minecraft:trial_spawner` registry.
    Registry {
        /// The key it was named by, which is what gets written back.
        key: Identifier,
        /// The registered value.
        value: &'static TrialSpawnerConfig,
    },
    /// A configuration written out in full.
    Direct(Arc<TrialSpawnerConfig>),
}

impl TrialSpawnerConfigHolder {
    /// Returns the configuration this holder points at.
    #[must_use]
    pub fn value(&self) -> &TrialSpawnerConfig {
        match self {
            Self::Registry { value, .. } => value,
            Self::Direct(value) => value,
        }
    }

    /// Returns a direct holder over `config`.
    #[must_use]
    pub fn direct(config: TrialSpawnerConfig) -> Self {
        Self::Direct(Arc::new(config))
    }

    /// Reads a holder from a key string or an inline object.
    ///
    /// An unknown registry key falls back to a direct default and says so:
    /// silently swapping in the default fight would make a datapack spawner
    /// look like it worked.
    #[must_use]
    pub fn load(tag: BorrowedNbtTag<'_, '_>) -> Option<Self> {
        if let Some(key) = tag.string() {
            let key: Identifier = key.to_str().parse().ok()?;
            let Some(value) = vanilla_trial_spawner_configs::by_key(&key) else {
                log::warn!("unknown trial spawner config {key}; using the default");
                return Some(Self::direct(TrialSpawnerConfig::vanilla_default()));
            };
            return Some(Self::Registry { key, value });
        }
        let compound = tag.compound()?;
        Some(Self::direct(TrialSpawnerConfig::load(&compound)))
    }

    /// Writes this holder the way vanilla's `RegistryFileCodec` would.
    #[must_use]
    pub fn save(&self) -> NbtTag {
        match self {
            Self::Registry { key, .. } => NbtTag::String(key.to_string().into()),
            Self::Direct(value) => NbtTag::Compound(value.save()),
        }
    }
}

impl Default for TrialSpawnerConfigHolder {
    fn default() -> Self {
        Self::direct(TrialSpawnerConfig::vanilla_default())
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
