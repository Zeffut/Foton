pub(super) use std::sync::LazyLock;

pub(super) use foton_math::{fast_floor, lerp};
pub(super) use foton_registry::biome::{BiomeRef, TemperatureModifier};
pub(super) use foton_registry::blocks::{
    BlockRef, block_state_ext::BlockStateExt as _, properties::BambooLeaves,
    properties::BlockStateProperties, properties::CreakingHeartState, properties::DoubleBlockHalf,
    properties::SpeleothemThickness, properties::WallSide, shapes,
};
pub(super) use foton_registry::feature::{
    AttachedToLeavesDecorator, AttachedToLogsDecorator, BambooConfiguration,
    BasaltColumnsConfiguration, BendingTrunkPlacer, BlobFoliagePlacer, BlockBlobConfiguration,
    BlockColumnConfiguration, BlockHolderSet, BlockPileConfiguration, BlockPredicate,
    BlockStateData, BlockStateProvider, CherryFoliagePlacer, CherryTrunkPlacer,
    ConfiguredFeatureKind, ConfiguredFeatureRef, DeltaFeatureConfiguration, DiskConfiguration,
    DripstoneClusterConfiguration, DualNoiseProvider, EndGatewayConfiguration, EndSpike,
    EndSpikeConfiguration, FallenTreeConfiguration, FeatureHeightmap, FeatureNoiseParameters,
    FeatureSize, FluidStateData, FoliagePlacer, FossilConfiguration, GeodeBlockSettings,
    GeodeConfiguration, HugeFungusConfiguration, HugeMushroomConfiguration, LakeConfiguration,
    LargeDripstoneConfiguration, MangroveRootPlacement, MangroveRootPlacer,
    MultifaceGrowthConfiguration, NetherForestVegetationConfiguration,
    NetherrackReplaceBlobsConfiguration, NoiseProvider, NoiseThresholdProvider, OreConfiguration,
    PlaceOnGroundDecorator, PlacedFeatureData, PlacedFeatureEntryRef, PlacedFeatureRef,
    PlacementModifier, PointedDripstoneConfiguration, RandomSpreadFoliagePlacer, RootPlacer,
    RootSystemConfiguration, RuleTest, SculkPatchConfiguration, SeaPickleConfiguration,
    SeagrassConfiguration, SimpleBlockConfiguration, SpeleothemClusterConfiguration,
    SpeleothemConfiguration, SpikeConfiguration, SpringConfiguration, TreeConfiguration,
    TreeDecorator, TrunkPlacer, TwistingVinesConfiguration, UnderwaterMagmaConfiguration,
    UpwardsBranchingTrunkPlacer, VegetationPatchConfiguration, VerticalSurface,
};
pub(super) use foton_registry::fluid::{FluidRef, FluidState, FluidStateExt as _};
pub(super) use foton_registry::{
    REGISTRY, Registry, RegistryEntry as _, RegistryExt as _, TaggedRegistryExt as _,
    vanilla_blocks, vanilla_fluids,
};
pub(super) use foton_utils::axis::Axis;
pub(super) use foton_utils::random::{
    Random as _, RandomSource, legacy_random::LegacyRandom, worldgen_random::WorldgenRandom,
};
pub(super) use foton_utils::types::UpdateFlags;
pub(super) use foton_utils::value_providers::IntProvider;
pub(super) use foton_utils::{BlockPos, BlockStateId, Direction, Identifier, Rotation, SectionPos};
pub(super) use foton_worldgen::noise::{NormalNoise, PerlinSimplexNoise};
pub(super) use rustc_hash::FxHashSet;

pub(super) use crate::behavior::BLOCK_BEHAVIORS;
pub(super) use crate::chunk::heightmap::HeightmapType;
pub(super) use crate::chunk::status::ChunkStatus;
pub(super) use crate::fluid::state::get_fluid_state_from_block;
pub(super) use crate::world::{LevelAccessor, LevelReader, WorldGenLevel};
pub(super) use crate::worldgen::generator::vanilla::fuzzed_biome_at_block;
pub(super) use crate::worldgen::region::WorldGenRegion;

pub(crate) const DECORATION_STEP_COUNT: usize = 11;
