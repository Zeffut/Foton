//! The statistics a player accumulates.
//!
//! Vanilla parity: `net.minecraft.stats`. A statistic is a pair -- a stat type
//! and one value out of the registry that type ranges over -- and that pair is
//! exactly what travels in `ClientboundAwardStatsPacket`, as two registry ids.
//!
//! The counters themselves are per player and live in `foton-core`; this is the
//! registry half, generated from the extracted `stat_type` and `custom_stat`
//! registries.

use foton_utils::Identifier;
use rustc_hash::FxHashMap;

use crate::RegistryEntry;

/// Which registry a stat type's values come from.
///
/// Vanilla parity: the `Registry<T>` a `StatType<T>` is built with. Vanilla
/// carries the registry itself; Foton names it, because the four vanilla stat
/// types cover four different registries and the id is all the wire needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatValueRegistry {
    /// `minecraft:block`, for `minecraft:mined`.
    Block,
    /// `minecraft:item`, for `crafted`, `used`, `broken`, `picked_up`, `dropped`.
    Item,
    /// `minecraft:entity_type`, for `killed` and `killed_by`.
    EntityType,
    /// `minecraft:custom_stat`, for `minecraft:custom`.
    CustomStat,
}

/// One family of statistics.
///
/// Vanilla parity: `StatType<T>`.
#[derive(Debug)]
pub struct StatType {
    /// The stat type's registry key.
    pub key: Identifier,
    /// The registry its values are drawn from.
    pub value_registry: StatValueRegistry,
}

/// A borrowed reference to a generated stat type.
pub type StatTypeRef = &'static StatType;

/// One value of the `minecraft:custom` stat type.
///
/// Vanilla parity: an entry of `BuiltInRegistries.CUSTOM_STAT`, which is a
/// registry of bare identifiers.
#[derive(Debug)]
pub struct CustomStat {
    /// The custom stat's registry key.
    pub key: Identifier,
}

/// A borrowed reference to a generated custom stat.
pub type CustomStatRef = &'static CustomStat;

/// One statistic, as a stat type and a value from that type's registry.
///
/// Vanilla parity: `Stat<T>`. Both halves are registry ids rather than
/// references so that one type covers all four value registries -- which is
/// what the wire form does as well, and what makes a stat usable as a map key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Stat {
    /// The stat type's id in the `minecraft:stat_type` registry.
    pub stat_type: usize,
    /// The value's id in the registry that stat type names.
    pub value: usize,
}

impl Stat {
    /// The statistic for one value of `stat_type`.
    ///
    /// # Panics
    /// If either side is not registered. Both come from the same generated
    /// data, so that is a build inconsistency rather than a runtime condition.
    #[must_use]
    pub fn new(stat_type: StatTypeRef, value: &impl RegistryEntry) -> Self {
        Self {
            stat_type: id_of(stat_type),
            value: id_of(value),
        }
    }

    /// The statistic for one `minecraft:custom` value.
    ///
    /// Vanilla parity: `Stats.CUSTOM.get(id)`, which is how every counter that
    /// is not keyed by a block, an item or an entity type is addressed.
    #[must_use]
    pub fn custom(stat: CustomStatRef) -> Self {
        Self::new(&crate::vanilla_stat_types::CUSTOM, stat)
    }
}

fn id_of(entry: &impl RegistryEntry) -> usize {
    entry
        .try_id()
        .expect("a generated registry entry is always registered")
}

/// Every stat type the game defines.
pub struct StatTypeRegistry {
    stat_types_by_id: Vec<StatTypeRef>,
    stat_types_by_key: FxHashMap<Identifier, usize>,
    allows_registering: bool,
}

impl StatTypeRegistry {
    /// An empty registry, open for registration.
    #[must_use]
    pub fn new() -> Self {
        Self {
            stat_types_by_id: Vec::new(),
            stat_types_by_key: FxHashMap::default(),
            allows_registering: true,
        }
    }
}

crate::impl_standard_methods!(
    StatTypeRegistry,
    StatTypeRef,
    stat_types_by_id,
    stat_types_by_key,
    allows_registering
);

crate::impl_registry!(
    StatTypeRegistry,
    StatType,
    stat_types_by_id,
    stat_types_by_key,
    stat_types
);

/// Every value of the `minecraft:custom` stat type.
pub struct CustomStatRegistry {
    custom_stats_by_id: Vec<CustomStatRef>,
    custom_stats_by_key: FxHashMap<Identifier, usize>,
    allows_registering: bool,
}

impl CustomStatRegistry {
    /// An empty registry, open for registration.
    #[must_use]
    pub fn new() -> Self {
        Self {
            custom_stats_by_id: Vec::new(),
            custom_stats_by_key: FxHashMap::default(),
            allows_registering: true,
        }
    }
}

crate::impl_standard_methods!(
    CustomStatRegistry,
    CustomStatRef,
    custom_stats_by_id,
    custom_stats_by_key,
    allows_registering
);

crate::impl_registry!(
    CustomStatRegistry,
    CustomStat,
    custom_stats_by_id,
    custom_stats_by_key,
    custom_stats
);

#[cfg(test)]
mod tests;
