use std::sync::LazyLock;

use rustc_hash::FxHashMap;
use steel_utils::Identifier;

use crate::biome::BiomeRef;
use crate::{RegistryTags, vanilla_biomes, vanilla_villager_types};

#[derive(Debug)]
pub struct VillagerType {
    pub key: Identifier,
}

pub type VillagerTypeRef = &'static VillagerType;

/// The variant a villager born or spawned in each biome wears.
///
/// Vanilla parity: `VillagerType.BY_BIOME`. It is a hardcoded Java map -- no
/// datapack writes it, nothing carries it over the wire, and `SteelExtractor`
/// emits no asset for it -- so it is transcribed here beside the registry it
/// answers with, the way `crate::fuel` transcribes the furnace burn times.
///
/// Every biome it does not name falls through to `plains`, which is why the
/// list reads as a set of exceptions rather than a full mapping: forests,
/// oceans, caves and the whole Nether all produce plains villagers.
static BY_BIOME: LazyLock<FxHashMap<&'static Identifier, VillagerTypeRef>> = LazyLock::new(|| {
    let mut by_biome: FxHashMap<&'static Identifier, VillagerTypeRef> = FxHashMap::default();
    let mut put = |biomes: &[&'static Identifier], villager_type: VillagerTypeRef| {
        for biome in biomes {
            by_biome.insert(biome, villager_type);
        }
    };
    put(
        &[
            &vanilla_biomes::BADLANDS.key,
            &vanilla_biomes::DESERT.key,
            &vanilla_biomes::ERODED_BADLANDS.key,
            &vanilla_biomes::WOODED_BADLANDS.key,
        ],
        &vanilla_villager_types::DESERT,
    );
    put(
        &[
            &vanilla_biomes::BAMBOO_JUNGLE.key,
            &vanilla_biomes::JUNGLE.key,
            &vanilla_biomes::SPARSE_JUNGLE.key,
        ],
        &vanilla_villager_types::JUNGLE,
    );
    put(
        &[
            &vanilla_biomes::SAVANNA_PLATEAU.key,
            &vanilla_biomes::SAVANNA.key,
            &vanilla_biomes::WINDSWEPT_SAVANNA.key,
        ],
        &vanilla_villager_types::SAVANNA,
    );
    put(
        &[
            &vanilla_biomes::DEEP_FROZEN_OCEAN.key,
            &vanilla_biomes::FROZEN_OCEAN.key,
            &vanilla_biomes::FROZEN_RIVER.key,
            &vanilla_biomes::ICE_SPIKES.key,
            &vanilla_biomes::SNOWY_BEACH.key,
            &vanilla_biomes::SNOWY_TAIGA.key,
            &vanilla_biomes::SNOWY_PLAINS.key,
            &vanilla_biomes::GROVE.key,
            &vanilla_biomes::SNOWY_SLOPES.key,
            &vanilla_biomes::FROZEN_PEAKS.key,
            &vanilla_biomes::JAGGED_PEAKS.key,
        ],
        &vanilla_villager_types::SNOW,
    );
    put(
        &[
            &vanilla_biomes::SWAMP.key,
            &vanilla_biomes::MANGROVE_SWAMP.key,
        ],
        &vanilla_villager_types::SWAMP,
    );
    put(
        &[
            &vanilla_biomes::OLD_GROWTH_SPRUCE_TAIGA.key,
            &vanilla_biomes::OLD_GROWTH_PINE_TAIGA.key,
            &vanilla_biomes::WINDSWEPT_GRAVELLY_HILLS.key,
            &vanilla_biomes::WINDSWEPT_HILLS.key,
            &vanilla_biomes::TAIGA.key,
            &vanilla_biomes::WINDSWEPT_FOREST.key,
        ],
        &vanilla_villager_types::TAIGA,
    );
    by_biome
});

impl VillagerType {
    /// The variant a villager appearing in `biome` wears.
    ///
    /// Vanilla parity: `VillagerType.byBiome`, whose fallback is
    /// `VillagerData.DEFAULT_TYPE`.
    #[must_use]
    pub fn by_biome(biome: BiomeRef) -> VillagerTypeRef {
        BY_BIOME
            .get(&biome.key)
            .copied()
            .unwrap_or(&vanilla_villager_types::PLAINS)
    }
}

pub struct VillagerTypeRegistry {
    villager_types_by_id: Vec<VillagerTypeRef>,
    villager_types_by_key: FxHashMap<Identifier, usize>,
    tags: RegistryTags,
    allows_registering: bool,
}

impl VillagerTypeRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self {
            villager_types_by_id: Vec::new(),
            villager_types_by_key: FxHashMap::default(),
            tags: RegistryTags::default(),
            allows_registering: true,
        }
    }
}

crate::impl_standard_methods!(
    VillagerTypeRegistry,
    VillagerTypeRef,
    villager_types_by_id,
    villager_types_by_key,
    allows_registering
);

crate::impl_registry!(
    VillagerTypeRegistry,
    VillagerType,
    villager_types_by_id,
    villager_types_by_key,
    villager_types
);
crate::impl_tagged_registry!(VillagerTypeRegistry, villager_types_by_key, "villager type");
