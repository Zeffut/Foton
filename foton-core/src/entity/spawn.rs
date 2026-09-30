use foton_registry::entity_variant::AxolotlVariant;

use crate::entity::LlamaVariant;
use crate::entity::entities::{HorseVariant, RabbitVariant, TropicalFishVariant};

/// Vanilla `EntitySpawnReason`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntitySpawnReason {
    /// The periodic mob spawning that fills loaded chunks.
    Natural,
    /// Placed while the chunk was being generated, before any player saw it.
    ChunkGeneration,
    /// A monster spawner block.
    Spawner,
    /// Placed as part of a structure, such as a pillager outpost's guards.
    Structure,
    /// Born from two parents.
    Breeding,
    /// Summoned by another mob -- a witch's cat, an evoker's vexes.
    MobSummoned,
    /// Spawned riding another mob, such as a chicken jockey.
    Jockey,
    /// Part of a scripted event, such as a raid wave.
    Event,
    /// The result of one mob turning into another. See
    /// [`crate::entity::conversion`].
    Conversion,
    /// Called in by a zombie that took damage.
    Reinforcement,
    /// Released by a trigger, such as a trial spawner's projectile.
    Triggered,
    /// Poured out of a bucket.
    Bucket,
    /// Placed by a spawn egg.
    SpawnItemUse,
    /// Created by `/summon` or another command.
    Command,
    /// Fired out of a dispenser.
    Dispenser,
    /// A pillager patrol.
    Patrol,
    /// A trial spawner.
    TrialSpawner,
    /// Read back from disk rather than newly created.
    Load,
    /// Re-created on the far side of a portal.
    DimensionTravel,
}

impl EntitySpawnReason {
    /// Whether a spawner block produced this mob.
    ///
    /// Vanilla parity: `EntitySpawnReason.isSpawner`. Both spawner kinds
    /// count, which is what exempts them from the spawn-position checks a
    /// natural spawn has to pass.
    #[must_use]
    pub const fn is_spawner(self) -> bool {
        matches!(self, Self::Spawner | Self::TrialSpawner)
    }

    /// Whether this reason lets a mob appear regardless of light level.
    ///
    /// Vanilla parity: `EntitySpawnReason.ignoresLightRequirements` -- only a
    /// trial spawner, which is why an ordinary spawner still respects light.
    #[must_use]
    pub const fn ignores_light_requirements(self) -> bool {
        matches!(self, Self::TrialSpawner)
    }
}

/// Paper 26.2 spawn provenance, independent of vanilla's gameplay reason.
/// `Custom` is reserved for an explicitly plugin-originated spawn request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PluginSpawnReason {
    /// Paper `NATURAL`.
    Natural,
    /// Paper `JOCKEY`.
    Jockey,
    /// Paper `CHUNK_GEN`.
    ChunkGen,
    /// Paper `SPAWNER`.
    Spawner,
    /// Paper `TRIAL_SPAWNER`.
    TrialSpawner,
    /// Paper `EGG`.
    Egg,
    /// Paper `SPAWNER_EGG`.
    SpawnerEgg,
    /// Paper `LIGHTNING`.
    Lightning,
    /// Paper `BUILD_SNOWMAN`.
    BuildSnowman,
    /// Paper `BUILD_IRONGOLEM`.
    BuildIronGolem,
    /// Paper `BUILD_COPPERGOLEM`.
    BuildCopperGolem,
    /// Paper `BUILD_WITHER`.
    BuildWither,
    /// Paper `VILLAGE_DEFENSE`.
    VillageDefense,
    /// Paper `VILLAGE_INVASION`.
    VillageInvasion,
    /// Paper `BREEDING`.
    Breeding,
    /// Paper `SLIME_SPLIT`.
    SlimeSplit,
    /// Paper `REINFORCEMENTS`.
    Reinforcements,
    /// Paper `NETHER_PORTAL`.
    NetherPortal,
    /// Paper `DISPENSE_EGG`.
    DispenseEgg,
    /// Paper `INFECTION`.
    Infection,
    /// Paper `CURED`.
    Cured,
    /// Paper `OCELOT_BABY`.
    OcelotBaby,
    /// Paper `SILVERFISH_BLOCK`.
    SilverfishBlock,
    /// Paper `MOUNT`.
    Mount,
    /// Paper `TRAP`.
    Trap,
    /// Paper `ENDER_PEARL`.
    EnderPearl,
    /// Paper `SHOULDER_ENTITY`.
    ShoulderEntity,
    /// Paper `DROWNED`.
    Drowned,
    /// Paper `SHEARED`.
    Sheared,
    /// Paper `EXPLOSION`.
    Explosion,
    /// Paper `RAID`.
    Raid,
    /// Paper `PATROL`.
    Patrol,
    /// Paper `BEEHIVE`.
    Beehive,
    /// Paper `PIGLIN_ZOMBIFIED`.
    PiglinZombified,
    /// Paper `SPELL`.
    Spell,
    /// Paper `FROZEN`.
    Frozen,
    /// Paper `METAMORPHOSIS`.
    Metamorphosis,
    /// Paper `DUPLICATION`.
    Duplication,
    /// Paper `COMMAND`.
    Command,
    /// Paper `ENCHANTMENT`.
    Enchantment,
    /// Paper `OMINOUS_ITEM_SPAWNER`.
    OminousItemSpawner,
    /// Paper `BUCKET`.
    Bucket,
    /// Paper `POTION_EFFECT`.
    PotionEffect,
    /// Paper `REANIMATE`.
    Reanimate,
    /// Paper `REHYDRATION`.
    Rehydration,
    /// Paper `CUSTOM`.
    Custom,
    #[default]
    /// Paper `DEFAULT`.
    Default,
}

impl PluginSpawnReason {
    /// The stable Paper name serialized by the JNI bridge.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Natural => "NATURAL",
            Self::Jockey => "JOCKEY",
            Self::ChunkGen => "CHUNK_GEN",
            Self::Spawner => "SPAWNER",
            Self::TrialSpawner => "TRIAL_SPAWNER",
            Self::Egg => "EGG",
            Self::SpawnerEgg => "SPAWNER_EGG",
            Self::Lightning => "LIGHTNING",
            Self::BuildSnowman => "BUILD_SNOWMAN",
            Self::BuildIronGolem => "BUILD_IRONGOLEM",
            Self::BuildCopperGolem => "BUILD_COPPERGOLEM",
            Self::BuildWither => "BUILD_WITHER",
            Self::VillageDefense => "VILLAGE_DEFENSE",
            Self::VillageInvasion => "VILLAGE_INVASION",
            Self::Breeding => "BREEDING",
            Self::SlimeSplit => "SLIME_SPLIT",
            Self::Reinforcements => "REINFORCEMENTS",
            Self::NetherPortal => "NETHER_PORTAL",
            Self::DispenseEgg => "DISPENSE_EGG",
            Self::Infection => "INFECTION",
            Self::Cured => "CURED",
            Self::OcelotBaby => "OCELOT_BABY",
            Self::SilverfishBlock => "SILVERFISH_BLOCK",
            Self::Mount => "MOUNT",
            Self::Trap => "TRAP",
            Self::EnderPearl => "ENDER_PEARL",
            Self::ShoulderEntity => "SHOULDER_ENTITY",
            Self::Drowned => "DROWNED",
            Self::Sheared => "SHEARED",
            Self::Explosion => "EXPLOSION",
            Self::Raid => "RAID",
            Self::Patrol => "PATROL",
            Self::Beehive => "BEEHIVE",
            Self::PiglinZombified => "PIGLIN_ZOMBIFIED",
            Self::Spell => "SPELL",
            Self::Frozen => "FROZEN",
            Self::Metamorphosis => "METAMORPHOSIS",
            Self::Duplication => "DUPLICATION",
            Self::Command => "COMMAND",
            Self::Enchantment => "ENCHANTMENT",
            Self::OminousItemSpawner => "OMINOUS_ITEM_SPAWNER",
            Self::Bucket => "BUCKET",
            Self::PotionEffect => "POTION_EFFECT",
            Self::Reanimate => "REANIMATE",
            Self::Rehydration => "REHYDRATION",
            Self::Custom => "CUSTOM",
            Self::Default => "DEFAULT",
        }
    }

    /// Restores a Paper-persisted spawn reason by its enum name.
    #[must_use]
    pub const fn from_paper_name(name: &str) -> Option<Self> {
        match name.as_bytes() {
            b"NATURAL" => Some(Self::Natural),
            b"JOCKEY" => Some(Self::Jockey),
            b"CHUNK_GEN" => Some(Self::ChunkGen),
            b"SPAWNER" => Some(Self::Spawner),
            b"TRIAL_SPAWNER" => Some(Self::TrialSpawner),
            b"EGG" => Some(Self::Egg),
            b"SPAWNER_EGG" => Some(Self::SpawnerEgg),
            b"LIGHTNING" => Some(Self::Lightning),
            b"BUILD_SNOWMAN" => Some(Self::BuildSnowman),
            b"BUILD_IRONGOLEM" => Some(Self::BuildIronGolem),
            b"BUILD_COPPERGOLEM" => Some(Self::BuildCopperGolem),
            b"BUILD_WITHER" => Some(Self::BuildWither),
            b"VILLAGE_DEFENSE" => Some(Self::VillageDefense),
            b"VILLAGE_INVASION" => Some(Self::VillageInvasion),
            b"BREEDING" => Some(Self::Breeding),
            b"SLIME_SPLIT" => Some(Self::SlimeSplit),
            b"REINFORCEMENTS" => Some(Self::Reinforcements),
            b"NETHER_PORTAL" => Some(Self::NetherPortal),
            b"DISPENSE_EGG" => Some(Self::DispenseEgg),
            b"INFECTION" => Some(Self::Infection),
            b"CURED" => Some(Self::Cured),
            b"OCELOT_BABY" => Some(Self::OcelotBaby),
            b"SILVERFISH_BLOCK" => Some(Self::SilverfishBlock),
            b"MOUNT" => Some(Self::Mount),
            b"TRAP" => Some(Self::Trap),
            b"ENDER_PEARL" => Some(Self::EnderPearl),
            b"SHOULDER_ENTITY" => Some(Self::ShoulderEntity),
            b"DROWNED" => Some(Self::Drowned),
            b"SHEARED" => Some(Self::Sheared),
            b"EXPLOSION" => Some(Self::Explosion),
            b"RAID" => Some(Self::Raid),
            b"PATROL" => Some(Self::Patrol),
            b"BEEHIVE" => Some(Self::Beehive),
            b"PIGLIN_ZOMBIFIED" => Some(Self::PiglinZombified),
            b"SPELL" => Some(Self::Spell),
            b"FROZEN" => Some(Self::Frozen),
            b"METAMORPHOSIS" => Some(Self::Metamorphosis),
            b"DUPLICATION" => Some(Self::Duplication),
            b"COMMAND" => Some(Self::Command),
            b"ENCHANTMENT" => Some(Self::Enchantment),
            b"OMINOUS_ITEM_SPAWNER" => Some(Self::OminousItemSpawner),
            b"BUCKET" => Some(Self::Bucket),
            b"POTION_EFFECT" => Some(Self::PotionEffect),
            b"REANIMATE" => Some(Self::Reanimate),
            b"REHYDRATION" => Some(Self::Rehydration),
            b"CUSTOM" => Some(Self::Custom),
            b"DEFAULT" => Some(Self::Default),
            _ => None,
        }
    }
}

impl From<EntitySpawnReason> for PluginSpawnReason {
    fn from(reason: EntitySpawnReason) -> Self {
        match reason {
            EntitySpawnReason::Natural => Self::Natural,
            EntitySpawnReason::ChunkGeneration => Self::ChunkGen,
            EntitySpawnReason::Spawner => Self::Spawner,
            EntitySpawnReason::Breeding => Self::Breeding,
            EntitySpawnReason::Jockey => Self::Jockey,
            EntitySpawnReason::Reinforcement => Self::Reinforcements,
            EntitySpawnReason::Bucket => Self::Bucket,
            EntitySpawnReason::SpawnItemUse => Self::SpawnerEgg,
            EntitySpawnReason::Command => Self::Command,
            EntitySpawnReason::Dispenser => Self::DispenseEgg,
            EntitySpawnReason::Patrol => Self::Patrol,
            EntitySpawnReason::TrialSpawner => Self::TrialSpawner,
            EntitySpawnReason::Structure
            | EntitySpawnReason::MobSummoned
            | EntitySpawnReason::Event
            | EntitySpawnReason::Conversion
            | EntitySpawnReason::Triggered
            | EntitySpawnReason::Load
            | EntitySpawnReason::DimensionTravel => Self::Default,
        }
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
    /// school leader; Foton has no schooling fish, so that half is absent.
    TropicalFish(TropicalFishGroupData),
    /// Vanilla `Horse.HorseGroupData`, which carries the coat every horse of a
    /// herd is born with.
    Horse(HorseGroupData),
    /// Vanilla `Llama.LlamaGroupData`, the same idea for a llama herd.
    Llama(LlamaGroupData),
    /// Vanilla `Axolotl.AxolotlGroupData`, which carries the two colors a
    /// cluster draws from rather than one shared color.
    Axolotl(AxolotlGroupData),
}

impl SpawnGroupData {
    /// Returns the ageable layer, for the kinds that extend one.
    #[must_use]
    pub const fn ageable(&self) -> Option<&AgeableMobGroupData> {
        match self {
            Self::AgeableMob(group_data) => Some(group_data),
            Self::Rabbit(group_data) => Some(&group_data.ageable),
            Self::Horse(group_data) => Some(&group_data.ageable),
            Self::Llama(group_data) => Some(&group_data.ageable),
            Self::Axolotl(group_data) => Some(&group_data.ageable),
            Self::TropicalFish(_) => None,
        }
    }

    /// Returns the ageable layer for mutation.
    #[must_use]
    pub const fn ageable_mut(&mut self) -> Option<&mut AgeableMobGroupData> {
        match self {
            Self::AgeableMob(group_data) => Some(group_data),
            Self::Rabbit(group_data) => Some(&mut group_data.ageable),
            Self::Horse(group_data) => Some(&mut group_data.ageable),
            Self::Llama(group_data) => Some(&mut group_data.ageable),
            Self::Axolotl(group_data) => Some(&mut group_data.ageable),
            Self::TropicalFish(_) => None,
        }
    }
}

/// Vanilla `Horse.HorseGroupData`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HorseGroupData {
    ageable: AgeableMobGroupData,
    variant: HorseVariant,
}

impl HorseGroupData {
    /// Creates group data for a herd of horses.
    ///
    /// Vanilla parity: `HorseGroupData(variant)` calls `super(true)`, so the
    /// herd keeps the default foal chance.
    #[must_use]
    pub const fn new(variant: HorseVariant) -> Self {
        Self {
            ageable: AgeableMobGroupData::with_should_spawn_baby(true),
            variant,
        }
    }

    /// Returns the coat shared by the herd.
    #[must_use]
    pub const fn variant(self) -> HorseVariant {
        self.variant
    }
}

/// Vanilla `Llama.LlamaGroupData`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LlamaGroupData {
    ageable: AgeableMobGroupData,
    variant: LlamaVariant,
}

impl LlamaGroupData {
    /// Creates group data for a herd of llamas.
    #[must_use]
    pub const fn new(variant: LlamaVariant) -> Self {
        Self {
            ageable: AgeableMobGroupData::with_should_spawn_baby(true),
            variant,
        }
    }

    /// Returns the coat shared by the herd.
    #[must_use]
    pub const fn variant(self) -> LlamaVariant {
        self.variant
    }
}

/// Vanilla `Axolotl.AxolotlGroupData`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AxolotlGroupData {
    ageable: AgeableMobGroupData,
    types: [AxolotlVariant; 2],
}

impl AxolotlGroupData {
    /// Creates group data for a cluster of axolotls.
    ///
    /// Vanilla parity: `AxolotlGroupData(Variant...)` calls `super(false)`, so
    /// the cluster never rolls the shared baby chance -- the axolotl decides
    /// that for itself from the group size instead.
    #[must_use]
    pub const fn new(types: [AxolotlVariant; 2]) -> Self {
        Self {
            ageable: AgeableMobGroupData::with_should_spawn_baby(false),
            types,
        }
    }

    /// How many axolotls of the cluster have spawned already.
    ///
    /// Vanilla parity: the inherited `AgeableMobGroupData.getGroupSize`, which
    /// the axolotl reads itself rather than leaving to the shared baby roll.
    #[must_use]
    pub const fn group_size(self) -> i32 {
        self.ageable.group_size()
    }

    /// Picks one of the two colors the cluster draws from.
    ///
    /// Vanilla parity: `AxolotlGroupData.getVariant`.
    #[must_use]
    pub fn variant(self, pick: impl FnOnce(usize) -> usize) -> AxolotlVariant {
        self.types[pick(self.types.len()) % self.types.len()]
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
    use foton_registry::vanilla_entities;
    use std::sync::Weak;

    #[test]
    fn plugin_spawn_provenance_maps_every_vanilla_reason_without_custom() {
        use super::EntitySpawnReason as V;
        use crate::entity::{Entity as _, entities::RawEntity};
        let entity = RawEntity::new(7, glam::DVec3::ZERO, Weak::new(), &vanilla_entities::ITEM);
        assert_eq!(entity.base().spawn_reason(), None);
        assert_eq!(entity.base().plugin_spawn_reason(), None);
        for (vanilla, expected) in [
            (V::Natural, "NATURAL"),
            (V::ChunkGeneration, "CHUNK_GEN"),
            (V::Spawner, "SPAWNER"),
            (V::Structure, "DEFAULT"),
            (V::Breeding, "BREEDING"),
            (V::MobSummoned, "DEFAULT"),
            (V::Jockey, "JOCKEY"),
            (V::Event, "DEFAULT"),
            (V::Conversion, "DEFAULT"),
            (V::Reinforcement, "REINFORCEMENTS"),
            (V::Triggered, "DEFAULT"),
            (V::Bucket, "BUCKET"),
            (V::SpawnItemUse, "SPAWNER_EGG"),
            (V::Command, "COMMAND"),
            (V::Dispenser, "DISPENSE_EGG"),
            (V::Patrol, "PATROL"),
            (V::TrialSpawner, "TRIAL_SPAWNER"),
            (V::Load, "DEFAULT"),
            (V::DimensionTravel, "DEFAULT"),
        ] {
            entity.base().set_spawn_reason(vanilla);
            assert_eq!(entity.base().spawn_reason(), Some(vanilla));
            assert_eq!(
                entity
                    .base()
                    .plugin_spawn_reason()
                    .map(super::PluginSpawnReason::as_str),
                Some(expected)
            );
        }
        entity.base().set_spawn_reason(V::Load);
        entity
            .base()
            .set_plugin_spawn_reason(super::PluginSpawnReason::Beehive);
        assert_eq!(entity.base().spawn_reason(), Some(V::Load));
        assert_eq!(
            entity.base().plugin_spawn_reason(),
            Some(super::PluginSpawnReason::Beehive)
        );
    }

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
