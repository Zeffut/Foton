//! What a village remembers about a player, and what it charges them for it.
//!
//! Vanilla parity: `net.minecraft.world.entity.ai.gossip.GossipContainer` and
//! `GossipType`. Every villager keeps its own container; villagers standing
//! near each other swap entries, which is how one player's reputation spreads
//! through a village without any village-wide object existing.
//!
//! The gameplay this drives is the cure loop: curing a zombie villager writes a
//! `MAJOR_POSITIVE` of twenty and a `MINOR_POSITIVE` of twenty-five, and
//! `MAJOR_POSITIVE` is the one type that never decays, so the discount outlives
//! everything else the villager remembers.

use foton_utils::UuidExt as _;
use rand::{Rng, RngExt as _};
use rustc_hash::FxHashMap;
use simdnbt::borrow::NbtList as BorrowedNbtList;
use simdnbt::owned::{NbtCompound, NbtList, NbtTag};
use uuid::Uuid;

/// A kind of thing a villager remembers, and how strongly.
///
/// Vanilla parity: `GossipType`. The five numbers are its constructor
/// arguments: how much one point is worth in reputation, the ceiling on a
/// single entry, how much a day takes off, and how much is lost when the entry
/// is told to another villager.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum GossipType {
    MajorNegative,
    MinorNegative,
    MinorPositive,
    MajorPositive,
    Trading,
}

impl GossipType {
    /// Every type, in the order vanilla declares them.
    pub const VALUES: [Self; 5] = [
        Self::MajorNegative,
        Self::MinorNegative,
        Self::MinorPositive,
        Self::MajorPositive,
        Self::Trading,
    ];

    /// Vanilla parity: `GossipType.REPUTATION_CHANGE_PER_EVENT`.
    pub const REPUTATION_CHANGE_PER_EVENT: i32 = 25;
    /// Vanilla parity: `GossipType.REPUTATION_CHANGE_PER_EVERLASTING_MEMORY`.
    pub const REPUTATION_CHANGE_PER_EVERLASTING_MEMORY: i32 = 20;
    /// Vanilla parity: `GossipType.REPUTATION_CHANGE_PER_TRADE`.
    pub const REPUTATION_CHANGE_PER_TRADE: i32 = 2;

    /// The serialized name, which is also the `Type` field of a saved entry.
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::MajorNegative => "major_negative",
            Self::MinorNegative => "minor_negative",
            Self::MinorPositive => "minor_positive",
            Self::MajorPositive => "major_positive",
            Self::Trading => "trading",
        }
    }

    /// Parses a serialized name back into a type.
    #[must_use]
    pub fn from_id(id: &str) -> Option<Self> {
        Self::VALUES.into_iter().find(|kind| kind.id() == id)
    }

    /// What one point of this type is worth in reputation.
    #[must_use]
    pub const fn weight(self) -> i32 {
        match self {
            Self::MajorNegative => -5,
            Self::MinorNegative => -1,
            Self::MinorPositive | Self::Trading => 1,
            Self::MajorPositive => 5,
        }
    }

    /// The ceiling on a single entry of this type.
    #[must_use]
    pub const fn max(self) -> i32 {
        match self {
            Self::MajorNegative => 100,
            Self::MinorNegative => 200,
            Self::MinorPositive | Self::Trading => 25,
            Self::MajorPositive => 20,
        }
    }

    /// How much a day's decay takes off an entry of this type.
    ///
    /// `MAJOR_POSITIVE` decays by zero: a cure is remembered forever.
    #[must_use]
    pub const fn decay_per_day(self) -> i32 {
        match self {
            Self::MajorNegative => 10,
            Self::MinorNegative => 20,
            Self::MinorPositive => 1,
            Self::MajorPositive => 0,
            Self::Trading => 2,
        }
    }

    /// How much is lost when this entry is retold to another villager.
    #[must_use]
    pub const fn decay_per_transfer(self) -> i32 {
        match self {
            Self::MajorNegative => 10,
            // Every other type loses the same twenty; they are listed apart
            // from each other because vanilla declares them that way and the
            // numbers are independent, not shared by design.
            Self::MinorNegative | Self::MinorPositive | Self::MajorPositive | Self::Trading => 20,
        }
    }
}

/// The point below which an entry stops existing at all.
///
/// Vanilla parity: `GossipContainer.DISCARD_THRESHOLD`.
const DISCARD_THRESHOLD: i32 = 2;

/// One villager's memory of one player, by type.
///
/// Vanilla parity: `GossipContainer.EntityGossips`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct EntityGossips {
    entries: FxHashMap<GossipType, i32>,
}

impl EntityGossips {
    /// Vanilla parity: `EntityGossips.weightedValue`.
    fn weighted_value(&self, accept: &impl Fn(GossipType) -> bool) -> i32 {
        self.entries
            .iter()
            .filter(|(kind, _)| accept(**kind))
            .map(|(kind, value)| value * kind.weight())
            .sum()
    }

    /// Vanilla parity: `EntityGossips.decay`, which drops an entry that falls
    /// under the discard threshold rather than letting it linger at one.
    fn decay(&mut self) {
        self.entries.retain(|kind, value| {
            *value -= kind.decay_per_day();
            *value >= DISCARD_THRESHOLD
        });
    }

    fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Vanilla parity: `EntityGossips.makeSureValueIsntTooLowOrTooHigh`.
    fn clamp_entry(&mut self, kind: GossipType) {
        let Some(value) = self.entries.get(&kind).copied() else {
            return;
        };
        if value > kind.max() {
            self.entries.insert(kind, kind.max());
        }
        if value < DISCARD_THRESHOLD {
            self.entries.remove(&kind);
        }
    }
}

/// One saved gossip entry.
///
/// Vanilla parity: the private `GossipContainer.GossipEntry` record, whose
/// codec names its fields `Target`, `Type` and `Value`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct GossipEntry {
    target: Uuid,
    kind: GossipType,
    value: i32,
}

impl GossipEntry {
    /// Vanilla parity: `GossipEntry.weightedValue`.
    const fn weighted_value(self) -> i32 {
        self.value * self.kind.weight()
    }
}

/// Everything one villager remembers about everyone it has met.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.gossip.GossipContainer`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GossipContainer {
    gossips: FxHashMap<Uuid, EntityGossips>,
}

impl GossipContainer {
    /// An empty container.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Ages every entry by one day's worth of decay.
    ///
    /// Vanilla parity: `GossipContainer.decay`, run once a day per villager.
    pub fn decay(&mut self) {
        self.gossips.retain(|_, gossips| {
            gossips.decay();
            !gossips.is_empty()
        });
    }

    /// What `entity` is worth to this villager, counting only accepted types.
    ///
    /// Vanilla parity: `GossipContainer.getReputation`. The villager's price
    /// adjustment reads this with a predicate that accepts everything, so a
    /// `MAJOR_NEGATIVE` really does cancel five `TRADING` points.
    #[must_use]
    pub fn reputation(&self, entity: Uuid, accept: impl Fn(GossipType) -> bool) -> i32 {
        self.gossips
            .get(&entity)
            .map_or(0, |gossips| gossips.weighted_value(&accept))
    }

    /// Adds `amount` points of `kind` about `target`.
    ///
    /// Vanilla parity: `GossipContainer.add`. Note the ceiling rule it uses:
    /// once an entry is at its maximum, adding more leaves it there rather than
    /// pushing it over, but an addition that *starts* above the maximum is kept
    /// at the old value, not clamped down.
    pub fn add(&mut self, target: Uuid, kind: GossipType, amount: i32) {
        let gossips = self.gossips.entry(target).or_default();
        let merged = match gossips.entries.get(&kind).copied() {
            None => amount,
            Some(existing) => {
                let sum = existing + amount;
                if sum > kind.max() {
                    kind.max().max(existing)
                } else {
                    sum
                }
            }
        };
        gossips.entries.insert(kind, merged);
        gossips.clamp_entry(kind);
        if gossips.is_empty() {
            self.gossips.remove(&target);
        }
    }

    /// Vanilla parity: `GossipContainer.remove(UUID, GossipType, int)`.
    pub fn remove_amount(&mut self, target: Uuid, kind: GossipType, amount: i32) {
        self.add(target, kind, -amount);
    }

    /// Vanilla parity: `GossipContainer.remove(UUID, GossipType)`.
    pub fn remove(&mut self, target: Uuid, kind: GossipType) {
        let Some(gossips) = self.gossips.get_mut(&target) else {
            return;
        };
        gossips.entries.remove(&kind);
        if gossips.is_empty() {
            self.gossips.remove(&target);
        }
    }

    /// Vanilla parity: `GossipContainer.remove(GossipType)`.
    pub fn remove_type(&mut self, kind: GossipType) {
        self.gossips.retain(|_, gossips| {
            gossips.entries.remove(&kind);
            !gossips.is_empty()
        });
    }

    /// Vanilla parity: `GossipContainer.clear`.
    pub fn clear(&mut self) {
        self.gossips.clear();
    }

    /// Vanilla parity: `GossipContainer.putAll`.
    pub fn put_all(&mut self, other: &Self) {
        for (target, gossips) in &other.gossips {
            let entry = self.gossips.entry(*target).or_default();
            for (kind, value) in &gossips.entries {
                entry.entries.insert(*kind, *value);
            }
        }
    }

    /// Vanilla parity: `GossipContainer.isEmpty`, by way of the map it wraps.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.gossips.is_empty()
    }

    /// Vanilla parity: the private `GossipContainer.unpack`.
    fn unpack(&self) -> Vec<GossipEntry> {
        let mut entries: Vec<GossipEntry> = self
            .gossips
            .iter()
            .flat_map(|(target, gossips)| {
                gossips
                    .entries
                    .iter()
                    .map(move |(kind, value)| GossipEntry {
                        target: *target,
                        kind: *kind,
                        value: *value,
                    })
            })
            .collect();
        // Vanilla iterates a `HashMap`, so its order is unspecified but stable
        // within a run. Sorting keeps a transfer's random draw reproducible and
        // keeps a save from reordering itself for no reason.
        entries.sort_by(|left, right| {
            (left.target, left.kind, left.value).cmp(&(right.target, right.kind, right.value))
        });
        entries
    }

    /// Picks entries to retell, weighted by how strongly they are felt.
    ///
    /// Vanilla parity: the private `GossipContainer.selectGossipsForTransfer`.
    /// It draws `max_count` times into one identity set, so a strongly-held
    /// entry is likelier to be picked *and* likelier to be picked twice --
    /// which means fewer than `max_count` distinct entries usually come out.
    fn select_gossips_for_transfer<R: Rng>(&self, rng: &mut R, max_count: i32) -> Vec<GossipEntry> {
        let entries = self.unpack();
        if entries.is_empty() {
            return Vec::new();
        }

        let mut ranges = Vec::with_capacity(entries.len());
        let mut ranges_end = 0;
        for entry in &entries {
            ranges_end += entry.weighted_value().abs();
            ranges.push(ranges_end - 1);
        }
        if ranges_end <= 0 {
            return Vec::new();
        }

        let mut chosen: Vec<GossipEntry> = Vec::new();
        for _ in 0..max_count {
            let choice = rng.random_range(0..ranges_end);
            // Vanilla's `Arrays.binarySearch` returns the insertion point
            // negated when the value is absent, which lands on the first range
            // whose end is at or above the choice.
            let index = ranges.partition_point(|end| *end < choice);
            let Some(entry) = entries.get(index) else {
                continue;
            };
            if !chosen.contains(entry) {
                chosen.push(*entry);
            }
        }
        chosen
    }

    /// Takes on what `source` has to say, minus the cost of the retelling.
    ///
    /// Vanilla parity: `GossipContainer.transferFrom`. An entry that would
    /// arrive below the discard threshold is not carried at all, and one that
    /// arrives over an entry already held keeps whichever is larger -- so
    /// gossip spreads without compounding.
    pub fn transfer_from<R: Rng>(&mut self, source: &Self, rng: &mut R, max_count: i32) {
        for entry in source.select_gossips_for_transfer(rng, max_count) {
            let decayed = entry.value - entry.kind.decay_per_transfer();
            if decayed < DISCARD_THRESHOLD {
                continue;
            }
            let gossips = self.gossips.entry(entry.target).or_default();
            gossips
                .entries
                .entry(entry.kind)
                .and_modify(|existing| *existing = (*existing).max(decayed))
                .or_insert(decayed);
        }
    }

    /// Vanilla parity: `GossipContainer.copy`.
    #[must_use]
    pub fn copy(&self) -> Self {
        let mut container = Self::new();
        container.put_all(self);
        container
    }

    /// Serializes to the list of `{Target, Type, Value}` compounds vanilla saves.
    ///
    /// Vanilla parity: `GossipContainer.CODEC`.
    #[must_use]
    pub fn save(&self) -> NbtTag {
        let compounds = self
            .unpack()
            .into_iter()
            .map(|entry| {
                let mut compound = NbtCompound::new();
                compound.insert(
                    "Target",
                    NbtTag::IntArray(entry.target.to_int_array().to_vec()),
                );
                compound.insert("Type", entry.kind.id());
                compound.insert("Value", entry.value);
                compound
            })
            .collect::<Vec<_>>();
        NbtTag::List(NbtList::from(compounds))
    }

    /// Reads back what [`Self::save`] wrote.
    ///
    /// Vanilla's `ExtraCodecs.POSITIVE_INT` rejects a non-positive `Value`, so
    /// an entry that somehow saved as zero is dropped rather than restored.
    pub fn load(&mut self, list: &BorrowedNbtList<'_, '_>) {
        let Some(compounds) = list.compounds() else {
            return;
        };
        for compound in compounds {
            let Some(target) = compound
                .int_array("Target")
                .and_then(|array| Uuid::from_int_array(&array))
            else {
                continue;
            };
            let Some(kind) = compound
                .string("Type")
                .and_then(|id| GossipType::from_id(id.to_str().as_ref()))
            else {
                continue;
            };
            let Some(value) = compound.int("Value").filter(|value| *value > 0) else {
                continue;
            };
            self.gossips
                .entry(target)
                .or_default()
                .entries
                .insert(kind, value);
        }
    }
}

/// Something that happened to a villager and changed how it feels about someone.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.village.ReputationEventType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReputationEventType {
    ZombieVillagerCured,
    GolemKilled,
    VillagerHurt,
    VillagerKilled,
    Trade,
}

#[cfg(test)]
mod tests;
