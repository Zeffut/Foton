//! What a spawner is told to spawn.
//!
//! Vanilla parity: `net.minecraft.world.level.SpawnData` and
//! `net.minecraft.world.entity.EquipmentTable`. Both live here rather than in
//! `foton-core` because the trial-spawner config registry is generated data and
//! has to name them.
//!
//! The light check `SpawnData.CustomSpawnRules.isValidPosition` needs a level,
//! so it stays in `foton-core`; what is here is the range pair it reads.

use foton_utils::Identifier;
use foton_utils::random::weighted_list::{Weighted, WeightedList};
use foton_utils::types::InclusiveRange;
use rustc_hash::FxHashMap;
use simdnbt::borrow::NbtCompound as NbtCompoundView;
use simdnbt::owned::{NbtCompound, NbtList};

use crate::equipment::EquipmentSlot;
use crate::loot_table::LootTableRef;
use crate::{REGISTRY, RegistryExt as _};

/// The light band a custom spawn rule accepts.
///
/// Vanilla parity: `SpawnData.CustomSpawnRules.LIGHT_RANGE`.
pub const LIGHT_RANGE: InclusiveRange<i32> = InclusiveRange::new(0, 15);

/// Extra placement limits a spawner may impose on its mob.
///
/// Vanilla parity: `SpawnData.CustomSpawnRules`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CustomSpawnRules {
    pub block_light_limit: InclusiveRange<i32>,
    pub sky_light_limit: InclusiveRange<i32>,
}

impl Default for CustomSpawnRules {
    fn default() -> Self {
        Self {
            block_light_limit: LIGHT_RANGE,
            sky_light_limit: LIGHT_RANGE,
        }
    }
}

impl CustomSpawnRules {
    fn load(nbt: &NbtCompoundView<'_, '_>) -> Self {
        Self {
            block_light_limit: load_light_limit(nbt, "block_light_limit"),
            sky_light_limit: load_light_limit(nbt, "sky_light_limit"),
        }
    }

    fn save(self) -> NbtCompound {
        let mut nbt = NbtCompound::new();
        nbt.insert(
            "block_light_limit",
            save_light_limit(self.block_light_limit),
        );
        nbt.insert("sky_light_limit", save_light_limit(self.sky_light_limit));
        nbt
    }
}

/// Reads one light range, clamped into vanilla's zero-to-fifteen window.
///
/// Vanilla parity: `SpawnData.CustomSpawnRules.lightLimit`, whose codec
/// validates the bounds. A saved spawner that is out of range is clamped here
/// rather than refused, because refusing would drop the whole block entity.
fn load_light_limit(nbt: &NbtCompoundView<'_, '_>, name: &str) -> InclusiveRange<i32> {
    let Some(range) = nbt.compound(name) else {
        return LIGHT_RANGE;
    };
    let min = range.int("min_inclusive").unwrap_or(0).clamp(0, 15);
    let max = range.int("max_inclusive").unwrap_or(15).clamp(min, 15);
    InclusiveRange::new(min, max)
}

fn save_light_limit(range: InclusiveRange<i32>) -> NbtCompound {
    let mut nbt = NbtCompound::new();
    nbt.insert("min_inclusive", range.min_inclusive);
    nbt.insert("max_inclusive", range.max_inclusive);
    nbt
}

/// A loot table that dresses a spawned mob, and how likely each slot is to drop.
///
/// Vanilla parity: `EquipmentTable`.
#[derive(Clone, Debug, PartialEq)]
pub struct EquipmentTable {
    pub loot_table: LootTableRef,
    pub slot_drop_chances: FxHashMap<EquipmentSlot, f32>,
}

impl EquipmentTable {
    /// Builds a table that gives every slot the same drop chance.
    ///
    /// Vanilla parity: the `(lootTable, float)` `EquipmentTable` constructor,
    /// which fans one chance out over `EquipmentSlot.values()`.
    #[must_use]
    pub fn with_uniform_drop_chance(loot_table: LootTableRef, drop_chance: f32) -> Self {
        Self {
            loot_table,
            slot_drop_chances: EquipmentSlot::ALL
                .iter()
                .map(|slot| (*slot, drop_chance))
                .collect(),
        }
    }

    fn load(nbt: &NbtCompoundView<'_, '_>) -> Option<Self> {
        let key: Identifier = nbt.string("loot_table")?.to_str().parse().ok()?;
        let loot_table = REGISTRY.loot_tables.by_key(&key)?;

        // Vanilla's DROP_CHANCES_CODEC accepts either one float for every slot
        // or a slot-keyed map, so both shapes have to be read back.
        if let Some(uniform) = nbt
            .float("slot_drop_chances")
            .or_else(|| nbt.double("slot_drop_chances").map(|value| value as f32))
        {
            return Some(Self::with_uniform_drop_chance(loot_table, uniform));
        }

        let mut slot_drop_chances = FxHashMap::default();
        if let Some(map) = nbt.compound("slot_drop_chances") {
            for (name, tag) in map.iter() {
                let Some(slot) = EquipmentSlot::by_name(name.to_str().as_ref()) else {
                    continue;
                };
                if let Some(chance) = tag.float() {
                    slot_drop_chances.insert(slot, chance);
                }
            }
        }
        Some(Self {
            loot_table,
            slot_drop_chances,
        })
    }

    fn save(&self) -> NbtCompound {
        let mut nbt = NbtCompound::new();
        nbt.insert("loot_table", self.loot_table.key.to_string());

        let mut chances = NbtCompound::new();
        for (slot, chance) in &self.slot_drop_chances {
            chances.insert(slot.name(), *chance);
        }
        nbt.insert("slot_drop_chances", chances);
        nbt
    }
}

/// One entry of a spawner's spawn potentials.
///
/// Vanilla parity: `SpawnData`. The entity is kept as the raw compound vanilla
/// keeps, because a spawner is allowed to name any field of any entity.
#[derive(Clone, Debug)]
pub struct SpawnData {
    entity_to_spawn: NbtCompound,
    custom_spawn_rules: Option<CustomSpawnRules>,
    equipment: Option<EquipmentTable>,
}

impl Default for SpawnData {
    fn default() -> Self {
        Self::empty()
    }
}

impl SpawnData {
    /// Returns the spawn data that names no entity.
    ///
    /// Vanilla parity: the no-argument `SpawnData` constructor, which is what a
    /// freshly placed spawner holds until something sets its entity.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            entity_to_spawn: NbtCompound::new(),
            custom_spawn_rules: None,
            equipment: None,
        }
    }

    /// Returns spawn data over an entity tag, with vanilla's `id` canonicalisation.
    ///
    /// Vanilla parity: the compact constructor of `SpawnData`, which re-stores a
    /// parseable `id` and removes one it cannot read.
    #[must_use]
    pub fn new(
        mut entity_to_spawn: NbtCompound,
        custom_spawn_rules: Option<CustomSpawnRules>,
        equipment: Option<EquipmentTable>,
    ) -> Self {
        let canonical_id = entity_to_spawn
            .string("id")
            .and_then(|id| id.to_str().parse::<Identifier>().ok());
        // `simdnbt`'s insert appends rather than replaces, so the old key has to
        // go first -- and a compound loaded from disk may already hold two.
        while entity_to_spawn.remove("id").is_some() {}
        if let Some(id) = canonical_id {
            entity_to_spawn.insert("id", id.to_string());
        }
        Self {
            entity_to_spawn,
            custom_spawn_rules,
            equipment,
        }
    }

    /// Returns spawn data that names one entity type and nothing else.
    #[must_use]
    pub fn of_entity(entity_type_key: &Identifier) -> Self {
        let mut entity = NbtCompound::new();
        entity.insert("id", entity_type_key.to_string());
        Self::new(entity, None, None)
    }

    /// Returns the tag the entity is built from.
    ///
    /// Vanilla parity: `SpawnData.getEntityToSpawn`.
    #[must_use]
    pub const fn entity_to_spawn(&self) -> &NbtCompound {
        &self.entity_to_spawn
    }

    /// Returns a mutable handle on the entity tag.
    ///
    /// Vanilla hands out the live `CompoundTag` from `getEntityToSpawn` and lets
    /// `BaseSpawner.setEntityId` write straight into it.
    pub const fn entity_to_spawn_mut(&mut self) -> &mut NbtCompound {
        &mut self.entity_to_spawn
    }

    /// Returns the entity type key the tag names, if it names one.
    #[must_use]
    pub fn entity_type_key(&self) -> Option<Identifier> {
        self.entity_to_spawn
            .string("id")
            .and_then(|id| id.to_str().parse().ok())
    }

    /// Returns the extra placement limits, if any.
    ///
    /// Vanilla parity: `SpawnData.getCustomSpawnRules`.
    #[must_use]
    pub const fn custom_spawn_rules(&self) -> Option<&CustomSpawnRules> {
        self.custom_spawn_rules.as_ref()
    }

    /// Returns the equipment table, if any.
    ///
    /// Vanilla parity: `SpawnData.getEquipment`.
    #[must_use]
    pub const fn equipment(&self) -> Option<&EquipmentTable> {
        self.equipment.as_ref()
    }

    /// Returns whether the tag holds nothing but an entity id.
    ///
    /// Vanilla parity: the `hasNoConfiguration` local of `BaseSpawner.serverTick`,
    /// which decides whether the mob is finalized like a natural spawn.
    #[must_use]
    pub fn has_no_configuration(&self) -> bool {
        self.entity_to_spawn.len() == 1 && self.entity_to_spawn.contains("id")
    }

    /// Reads one spawn data entry.
    ///
    /// Vanilla parity: `SpawnData.CODEC`.
    #[must_use]
    pub fn load(nbt: &NbtCompoundView<'_, '_>) -> Self {
        let entity_to_spawn = nbt
            .compound("entity")
            .map(|entity| entity.to_owned())
            .unwrap_or_default();
        let custom_spawn_rules = nbt
            .compound("custom_spawn_rules")
            .map(|rules| CustomSpawnRules::load(&rules));
        let equipment = nbt
            .compound("equipment")
            .and_then(|equipment| EquipmentTable::load(&equipment));
        Self::new(entity_to_spawn, custom_spawn_rules, equipment)
    }

    /// Writes one spawn data entry.
    #[must_use]
    pub fn save(&self) -> NbtCompound {
        let mut nbt = NbtCompound::new();
        nbt.insert("entity", self.entity_to_spawn.clone());
        if let Some(rules) = self.custom_spawn_rules {
            nbt.insert("custom_spawn_rules", rules.save());
        }
        if let Some(equipment) = &self.equipment {
            nbt.insert("equipment", equipment.save());
        }
        nbt
    }

    /// Reads a weighted list of spawn data.
    ///
    /// Vanilla parity: `SpawnData.LIST_CODEC`.
    #[must_use]
    pub fn load_list(list: Option<&simdnbt::borrow::NbtList<'_, '_>>) -> WeightedList<Self> {
        let Some(compounds) = list.and_then(simdnbt::borrow::NbtList::compounds) else {
            return WeightedList::empty();
        };
        let entries = compounds
            .into_iter()
            .filter_map(|entry| {
                let data = entry.compound("data")?;
                Some(Weighted {
                    value: Self::load(&data),
                    weight: entry.int("weight").unwrap_or(1),
                })
            })
            .collect();
        WeightedList::new(entries)
    }

    /// Writes a weighted list of spawn data.
    #[must_use]
    pub fn save_list(list: &WeightedList<Self>) -> NbtList {
        NbtList::Compound(
            list.entries()
                .iter()
                .map(|entry| {
                    let mut nbt = NbtCompound::new();
                    nbt.insert("data", entry.value.save());
                    nbt.insert("weight", entry.weight);
                    nbt
                })
                .collect(),
        )
    }
}

/// Reads a weighted list of loot-table keys.
///
/// Vanilla parity: `WeightedList.codec(LootTable.KEY_CODEC)`. A key naming a
/// table Foton does not have is dropped rather than defaulted, so an unknown
/// reward cannot silently become a known one.
#[must_use]
pub fn load_loot_table_list(
    list: Option<&simdnbt::borrow::NbtList<'_, '_>>,
) -> WeightedList<LootTableRef> {
    let Some(compounds) = list.and_then(simdnbt::borrow::NbtList::compounds) else {
        return WeightedList::empty();
    };
    let entries = compounds
        .into_iter()
        .filter_map(|entry| {
            let key: Identifier = entry.string("data")?.to_str().parse().ok()?;
            Some(Weighted {
                value: REGISTRY.loot_tables.by_key(&key)?,
                weight: entry.int("weight").unwrap_or(1),
            })
        })
        .collect();
    WeightedList::new(entries)
}

/// Writes a weighted list of loot-table keys.
#[must_use]
pub fn save_loot_table_list(list: &WeightedList<LootTableRef>) -> NbtList {
    NbtList::Compound(
        list.entries()
            .iter()
            .map(|entry| {
                let mut nbt = NbtCompound::new();
                nbt.insert("data", entry.value.key.to_string());
                nbt.insert("weight", entry.weight);
                nbt
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;
    use crate::init_vanilla_registry;

    fn reparse(nbt: &NbtCompound) -> Vec<u8> {
        let mut bytes = Vec::new();
        nbt.write(&mut bytes);
        bytes
    }

    /// A spawner's `SpawnData` is written to disk every save, so a field that
    /// does not survive the round trip is a spawner that quietly changes what
    /// it spawns.
    #[test]
    fn spawn_data_survives_the_nbt_round_trip() {
        init_vanilla_registry();
        let mut entity = NbtCompound::new();
        entity.insert("id", "minecraft:zombie");
        entity.insert("IsBaby", 1i8);
        let original = SpawnData::new(
            entity,
            Some(CustomSpawnRules {
                block_light_limit: InclusiveRange::new(0, 7),
                sky_light_limit: InclusiveRange::new(2, 15),
            }),
            None,
        );

        let bytes = reparse(&original.save());
        let borrowed = simdnbt::borrow::read_compound(&mut Cursor::new(&bytes))
            .expect("saved spawn data must parse");
        let view: NbtCompoundView<'_, '_> = (&borrowed).into();
        let loaded = SpawnData::load(&view);

        assert_eq!(
            loaded.entity_type_key(),
            Some(Identifier::vanilla_static("zombie"))
        );
        assert_eq!(loaded.entity_to_spawn().byte("IsBaby"), Some(1));
        let rules = loaded.custom_spawn_rules().copied().expect("rules kept");
        assert_eq!(rules.block_light_limit, InclusiveRange::new(0, 7));
        assert_eq!(rules.sky_light_limit, InclusiveRange::new(2, 15));
    }

    /// Vanilla's compact constructor drops an `id` it cannot parse. A spawner
    /// that kept a malformed id would try to spawn it on every attempt.
    #[test]
    fn an_unparsable_entity_id_is_dropped() {
        let mut entity = NbtCompound::new();
        entity.insert("id", "not a valid id");
        let data = SpawnData::new(entity, None, None);

        assert!(!data.entity_to_spawn().contains("id"));
        assert_eq!(data.entity_type_key(), None);
    }

    /// `hasNoConfiguration` decides whether a spawned mob is finalized the way a
    /// natural spawn is, so an extra field has to turn it off.
    #[test]
    fn only_a_bare_id_counts_as_unconfigured() {
        let bare = SpawnData::of_entity(&Identifier::vanilla_static("zombie"));
        assert!(bare.has_no_configuration());

        let mut entity = NbtCompound::new();
        entity.insert("id", "minecraft:zombie");
        entity.insert("IsBaby", 1i8);
        assert!(!SpawnData::new(entity, None, None).has_no_configuration());
    }
}
