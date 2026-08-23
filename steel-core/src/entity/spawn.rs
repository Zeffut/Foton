use crate::entity::entities::{RabbitVariant, TropicalFishVariant};

/// Vanilla `EntitySpawnReason`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntitySpawnReason {
    Natural,
    ChunkGeneration,
    Spawner,
    Structure,
    Breeding,
    MobSummoned,
    Jockey,
    Event,
    Conversion,
    Reinforcement,
    Triggered,
    Bucket,
    SpawnItemUse,
    Command,
    Dispenser,
    Patrol,
    TrialSpawner,
    Load,
    DimensionTravel,
}

impl EntitySpawnReason {
    #[must_use]
    pub const fn is_spawner(self) -> bool {
        matches!(self, Self::Spawner | Self::TrialSpawner)
    }

    #[must_use]
    pub const fn ignores_light_requirements(self) -> bool {
        matches!(self, Self::TrialSpawner)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SpawnGroupData {
    AgeableMob(AgeableMobGroupData),
    /// Vanilla `Rabbit.RabbitGroupData`, which is an `AgeableMobGroupData` that
    /// also carries the variant every rabbit of the group is born with.
    Rabbit(RabbitGroupData),
    /// Vanilla `TropicalFish.TropicalFishGroupData`, which carries the variant
    /// a shoal shares. Vanilla derives it from
    /// `AbstractSchoolingFish.SchoolSpawnGroupData` and so also carries the
    /// school leader; Steel has no schooling fish, so that half is absent.
    TropicalFish(TropicalFishGroupData),
}

impl SpawnGroupData {
    /// Returns the ageable layer, for the kinds that extend one.
    #[must_use]
    pub const fn ageable(&self) -> Option<&AgeableMobGroupData> {
        match self {
            Self::AgeableMob(group_data) => Some(group_data),
            Self::Rabbit(group_data) => Some(&group_data.ageable),
            Self::TropicalFish(_) => None,
        }
    }

    /// Returns the ageable layer for mutation.
    #[must_use]
    pub const fn ageable_mut(&mut self) -> Option<&mut AgeableMobGroupData> {
        match self {
            Self::AgeableMob(group_data) => Some(group_data),
            Self::Rabbit(group_data) => Some(&mut group_data.ageable),
            Self::TropicalFish(_) => None,
        }
    }
}

/// Vanilla `TropicalFish.TropicalFishGroupData`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TropicalFishGroupData {
    variant: TropicalFishVariant,
}

impl TropicalFishGroupData {
    /// Creates group data for a shoal of tropical fish.
    #[must_use]
    pub const fn new(variant: TropicalFishVariant) -> Self {
        Self { variant }
    }

    /// Returns the variant the shoal shares.
    #[must_use]
    pub const fn variant(self) -> TropicalFishVariant {
        self.variant
    }
}

/// Vanilla `Rabbit.RabbitGroupData`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RabbitGroupData {
    ageable: AgeableMobGroupData,
    variant: RabbitVariant,
}

impl RabbitGroupData {
    /// Creates group data for a rabbit spawn group.
    ///
    /// Vanilla parity: `RabbitGroupData(variant)` calls `super(1.0F)`, so every
    /// rabbit after the first in a group rolls a guaranteed baby chance.
    #[must_use]
    pub const fn new(variant: RabbitVariant) -> Self {
        Self {
            ageable: AgeableMobGroupData::with_baby_spawn_chance(1.0),
            variant,
        }
    }

    /// Returns the variant shared by the group.
    #[must_use]
    pub const fn variant(self) -> RabbitVariant {
        self.variant
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AgeableMobGroupData {
    group_size: i32,
    should_spawn_baby: bool,
    baby_spawn_chance: f32,
}

impl AgeableMobGroupData {
    pub const DEFAULT_BABY_SPAWN_CHANCE: f32 = 0.05;

    #[must_use]
    pub const fn new(should_spawn_baby: bool, baby_spawn_chance: f32) -> Self {
        Self {
            group_size: 0,
            should_spawn_baby,
            baby_spawn_chance,
        }
    }

    #[must_use]
    pub const fn with_should_spawn_baby(should_spawn_baby: bool) -> Self {
        Self::new(should_spawn_baby, Self::DEFAULT_BABY_SPAWN_CHANCE)
    }

    #[must_use]
    pub const fn with_baby_spawn_chance(baby_spawn_chance: f32) -> Self {
        Self::new(true, baby_spawn_chance)
    }

    #[must_use]
    pub const fn group_size(self) -> i32 {
        self.group_size
    }

    #[must_use]
    pub const fn should_spawn_baby(self) -> bool {
        self.should_spawn_baby
    }

    #[must_use]
    pub const fn baby_spawn_chance(self) -> f32 {
        self.baby_spawn_chance
    }

    pub const fn increase_group_size_by_one(&mut self) {
        self.group_size += 1;
    }

    #[must_use]
    pub const fn needs_baby_spawn_roll(self) -> bool {
        self.should_spawn_baby && self.group_size > 0
    }

    pub fn finalize_ageable_spawn(&mut self, baby_roll: impl FnOnce() -> f32) -> bool {
        let spawn_baby = self.needs_baby_spawn_roll() && baby_roll() <= self.baby_spawn_chance;
        self.increase_group_size_by_one();
        spawn_baby
    }
}

#[cfg(test)]
mod tests {
    use super::AgeableMobGroupData;

    #[test]
    fn ageable_group_data_increments_before_later_baby_rolls_can_apply() {
        let mut group_data = AgeableMobGroupData::with_should_spawn_baby(true);

        assert!(!group_data.finalize_ageable_spawn(|| {
            panic!("first group member should not roll for baby spawn")
        }));
        assert_eq!(group_data.group_size(), 1);

        assert!(group_data.finalize_ageable_spawn(|| 0.05));
        assert_eq!(group_data.group_size(), 2);
    }

    #[test]
    fn ageable_group_data_can_disable_baby_spawns() {
        let mut group_data = AgeableMobGroupData::with_should_spawn_baby(false);

        assert!(
            !group_data
                .finalize_ageable_spawn(|| { panic!("disabled baby spawning should not roll") })
        );
        assert!(
            !group_data
                .finalize_ageable_spawn(|| { panic!("disabled baby spawning should not roll") })
        );
        assert_eq!(group_data.group_size(), 2);
    }
}
