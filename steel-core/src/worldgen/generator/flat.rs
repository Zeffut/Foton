use std::sync::Arc;

use glam::IVec3;
use rustc_hash::FxHashMap;
use steel_registry::biome::BiomeRef;
use steel_registry::blocks::block_state_ext::BlockStateExt;
use steel_registry::feature::{
    ConfiguredFeatureKind, ConfiguredFeatureRef, LayerConfiguration, PlacedFeatureData,
};
use steel_registry::template_pool::{TemplateData, TemplatePoolData};
use steel_registry::{REGISTRY, RegistryExt, vanilla_biomes, vanilla_blocks};
use steel_utils::random::RandomSource;
use steel_utils::{BlockStateId, ChunkPos, Identifier};
use steel_worldgen::biomes::obfuscate_biome_seed;

use crate::chunk::Chunk;
use crate::worldgen::feature::{
    BiomeFeatures, DECORATION_STEP_COUNT, FeatureDecorationRunner, FeatureEntry,
};
use crate::worldgen::generator::{
    CarversPhase, ChunkGenerator, FirstFreeHeightBody, GenerationChunk, NoiseBiomeBody, NoisePhase,
    SurfacePhase, xoroshiro_worldgen_region_random,
};
use crate::worldgen::region::WorldGenRegion;
use crate::worldgen::structure::{StructureGenerator, create_structures};
use steel_worldgen::noise::Beardifier;
use steel_worldgen::structure::{ColumnBlock, StructureGenerationContext};

/// Vanilla's `GenerationStep.Decoration` ordinals `adjustGenerationSettings`
/// singles out.
const LAKES_STEP: usize = 1;
const UNDERGROUND_STRUCTURES_STEP: usize = 3;
const SURFACE_STRUCTURES_STEP: usize = 4;
const TOP_LAYER_MODIFICATION_STEP: usize = 10;

/// The two lakes `lakes = true` adds.
///
/// Vanilla parity: `FlatLevelGeneratorSettings.createLakesList`.
const LAKE_FEATURES: [&str; 2] = ["lake_lava_underground", "lake_lava_surface"];

/// What a flat preset asks the decoration pass for.
///
/// Vanilla parity: the `lakes` and `features` fields of
/// `FlatLevelGeneratorSettings`.
#[derive(Clone, Copy, Debug, Default)]
pub struct FlatDecoration {
    /// Whether the biome's own features are placed.
    pub features: bool,
    /// Whether the two lava lakes are placed.
    pub lakes: bool,
}

/// A chunk generator that generates a flat world.
///
/// Uses a fixed biome (plains) for all positions, matching vanilla's
/// `FlatLevelSource` with `FixedBiomeSource`.
pub struct FlatChunkGenerator {
    /// Block layers from world bottom upwards.
    ///
    /// A layer that does not block motion has already been taken out of this
    /// list and moved into a `FILL_LAYER` feature; see
    /// [`FlatChunkGenerator::adjust_generation_settings`].
    pub layers: Vec<BlockStateId>,
    /// The biome ID for plains (cached at construction).
    biome_id: u16,
    /// World seed for structure placement.
    seed: i64,
    /// Seed vanilla's `BiomeManager` fuzzes block-level biome lookups with.
    biome_zoom_seed: i64,
    /// Sea level for this flat generator's dimension type.
    sea_level: i32,
    /// Optional structure engine from flat structure overrides.
    structure_generator: Option<StructureGenerator>,
    /// Decoration order for this preset's adjusted generation settings.
    feature_runner: FeatureDecorationRunner,
}

impl FlatChunkGenerator {
    /// Creates a new `FlatChunkGenerator`.
    #[must_use]
    pub fn new(bedrock: BlockStateId, dirt: BlockStateId, grass: BlockStateId) -> Self {
        Self::new_layers(vec![bedrock, dirt, dirt, grass])
    }

    /// Creates a new flat generator with explicit block layers from bottom upwards.
    #[must_use]
    pub fn new_layers(layers: Vec<BlockStateId>) -> Self {
        Self::new_layers_with_structures(layers, 0, 63, None, FlatDecoration::default())
    }

    /// Creates a flat generator with optional structure generation.
    #[must_use]
    pub(crate) fn new_layers_with_structures(
        layers: Vec<BlockStateId>,
        seed: i64,
        sea_level: i32,
        structure_generator: Option<StructureGenerator>,
        decoration: FlatDecoration,
    ) -> Self {
        let biome_id = REGISTRY
            .biomes
            .id_from_key(&Identifier::vanilla("plains".to_string()))
            .unwrap_or(0) as u16;
        let (layers, feature_runner) =
            Self::adjust_generation_settings(layers, &vanilla_biomes::PLAINS, decoration);

        Self {
            layers,
            biome_id,
            seed,
            biome_zoom_seed: obfuscate_biome_seed(seed),
            sea_level,
            structure_generator,
            feature_runner,
        }
    }

    /// Rewrites the flat biome's feature list and takes the layers that do not
    /// block motion out of the stack.
    ///
    /// Vanilla parity: `FlatLevelGeneratorSettings.adjustGenerationSettings`.
    /// The two structure steps are dropped because a flat world places its
    /// structures from its own overrides, and a layer that does not block
    /// motion is filled back in at `TOP_LAYER_MODIFICATION` -- only where
    /// decoration left air -- instead of during the noise fill. Vanilla writes
    /// `null` into its layer list for those; Steel writes air, which the layer
    /// reads and the noise fill both treat the same way.
    fn adjust_generation_settings(
        layers: Vec<BlockStateId>,
        biome: BiomeRef,
        decoration: FlatDecoration,
    ) -> (Vec<BlockStateId>, FeatureDecorationRunner) {
        let (layers, steps) = Self::adjusted_features(layers, biome, decoration);
        let runner = FeatureDecorationRunner::with_features(vec![BiomeFeatures { biome, steps }]);
        (layers, runner)
    }

    /// The layer stack and per-step feature list `adjust_generation_settings`
    /// produces, before they are handed to the decoration runner.
    fn adjusted_features(
        mut layers: Vec<BlockStateId>,
        biome: BiomeRef,
        decoration: FlatDecoration,
    ) -> (Vec<BlockStateId>, Vec<Vec<FeatureEntry>>) {
        let biome_features = FeatureDecorationRunner::registered_features(biome, &REGISTRY);
        let mut steps: Vec<Vec<FeatureEntry>> =
            vec![Vec::new(); DECORATION_STEP_COUNT.max(biome_features.len())];

        if decoration.lakes {
            for name in LAKE_FEATURES {
                let key = Identifier::vanilla_static(name);
                let Some(feature) = REGISTRY.placed_features.by_key(&key) else {
                    panic!("minecraft:flat lakes reference unknown placed feature {key}");
                };
                steps[LAKES_STEP].push(FeatureEntry::Registered(feature));
            }
        }

        let void_gen = layers
            .iter()
            .all(|state| state.get_block() == &vanilla_blocks::AIR);
        if (!void_gen || biome.key == vanilla_biomes::THE_VOID.key) && decoration.features {
            for (step, stage) in biome_features.into_iter().enumerate() {
                if step == UNDERGROUND_STRUCTURES_STEP
                    || step == SURFACE_STRUCTURES_STEP
                    || (decoration.lakes && step == LAKES_STEP)
                {
                    continue;
                }
                steps[step].extend(stage);
            }
        }

        let air = vanilla_blocks::AIR.default_state();
        for (index, layer) in layers.iter_mut().enumerate() {
            let state = *layer;
            if state.blocks_motion() || state.has_fluid() {
                continue;
            }
            *layer = air;
            let Ok(height) = i32::try_from(index) else {
                panic!("minecraft:flat layer index {index} exceeds i32 range");
            };
            steps[TOP_LAYER_MODIFICATION_STEP].push(FeatureEntry::Inline(
                index,
                Arc::new(PlacedFeatureData {
                    feature: ConfiguredFeatureRef::Inline(Box::new(
                        ConfiguredFeatureKind::FillLayer(LayerConfiguration { height, state }),
                    )),
                    // Vanilla parity: `PlacementUtils.inlinePlaced` with no modifiers.
                    placement: Vec::new(),
                }),
            ));
        }

        (layers, steps)
    }
}

impl FlatChunkGenerator {
    /// The layer at an absolute Y, if the stack reaches that high.
    fn state_at_y(&self, min_y: i32, y: i32) -> Option<BlockStateId> {
        let relative_y = y.checked_sub(min_y)? as usize;
        self.layers.get(relative_y).copied()
    }

    /// Whether the layer at `y` counts as terrain for the heightmap.
    ///
    /// `ocean_floor=false` is `WORLD_SURFACE_WG`; `true` is `OCEAN_FLOOR_WG`.
    fn is_opaque_at_y(&self, min_y: i32, y: i32, ocean_floor: bool) -> bool {
        let Some(state) = self.state_at_y(min_y, y) else {
            return false;
        };
        if ocean_floor {
            state.is_solid()
        } else {
            state.is_solid() || state.has_fluid()
        }
    }

    /// Vanilla's `FlatLevelSource.getBaseHeight`: one above the topmost layer
    /// that counts, or the floor when none does.
    fn base_height(&self, min_y: i32, height: i32, ocean_floor: bool) -> i32 {
        for y in (min_y..min_y + height).rev() {
            if self.is_opaque_at_y(min_y, y, ocean_floor) {
                return y + 1;
            }
        }
        min_y
    }
}

struct FlatGenerationContext<'a> {
    seed: i64,
    chunk_x: i32,
    chunk_z: i32,
    chunk_min_x: i32,
    chunk_min_z: i32,
    center_block_x: i32,
    center_block_z: i32,
    sea_level: i32,
    min_y: i32,
    height: i32,
    generator: &'a FlatChunkGenerator,
    biome: BiomeRef,
    template_pools: &'a FxHashMap<Identifier, TemplatePoolData>,
    templates: &'a FxHashMap<Identifier, TemplateData>,
    surface_y_cache: Option<i32>,
}

impl FlatGenerationContext<'_> {
    fn state_at_y(&self, y: i32) -> Option<BlockStateId> {
        self.generator.state_at_y(self.min_y, y)
    }

    fn is_opaque_at_y(&self, y: i32, ocean_floor: bool) -> bool {
        self.generator.is_opaque_at_y(self.min_y, y, ocean_floor)
    }

    fn base_height_flat(&self, ocean_floor: bool) -> i32 {
        self.generator
            .base_height(self.min_y, self.height, ocean_floor)
    }
}

impl StructureGenerationContext for FlatGenerationContext<'_> {
    fn seed(&self) -> i64 {
        self.seed
    }

    fn chunk_x(&self) -> i32 {
        self.chunk_x
    }

    fn chunk_z(&self) -> i32 {
        self.chunk_z
    }

    fn chunk_min_x(&self) -> i32 {
        self.chunk_min_x
    }

    fn chunk_min_z(&self) -> i32 {
        self.chunk_min_z
    }

    fn center_block_x(&self) -> i32 {
        self.center_block_x
    }

    fn center_block_z(&self) -> i32 {
        self.center_block_z
    }

    fn sea_level(&self) -> i32 {
        self.sea_level
    }

    fn min_y(&self) -> i32 {
        self.min_y
    }

    fn height(&self) -> i32 {
        self.height
    }

    fn template_pools(&self) -> &FxHashMap<Identifier, TemplatePoolData> {
        self.template_pools
    }

    fn templates(&self) -> &FxHashMap<Identifier, TemplateData> {
        self.templates
    }

    fn base_height(&mut self, _x: i32, _z: i32, ocean_floor: bool) -> i32 {
        self.base_height_flat(ocean_floor)
    }

    fn base_height_full(&mut self, _x: i32, _z: i32, ocean_floor: bool) -> i32 {
        self.base_height_flat(ocean_floor)
    }

    fn biome_at(&mut self, _block_x: i32, _block_y: i32, _block_z: i32) -> BiomeRef {
        self.biome
    }

    fn column_state(&mut self, _x: i32, y: i32, _z: i32) -> ColumnBlock {
        let Some(state) = self.state_at_y(y) else {
            return ColumnBlock::Air;
        };
        if state.is_solid() {
            ColumnBlock::Solid
        } else if state.has_fluid() {
            ColumnBlock::Fluid
        } else {
            ColumnBlock::Air
        }
    }

    fn surface_y(&mut self) -> i32 {
        if let Some(y) = self.surface_y_cache {
            return y;
        }
        let y = self.base_height_flat(false) - 1;
        self.surface_y_cache = Some(y);
        y
    }

    fn terrain_surface_height(&self, _x: i32, _z: i32, ocean_floor: bool) -> i32 {
        self.base_height_flat(ocean_floor)
    }

    fn terrain_is_opaque(&self, _x: i32, y: i32, _z: i32, ocean_floor: bool) -> bool {
        self.is_opaque_at_y(y, ocean_floor)
    }
}

impl ChunkGenerator for FlatChunkGenerator {
    fn min_y(&self) -> i32 {
        0
    }

    fn gen_depth(&self) -> i32 {
        384
    }

    fn noise_biome(&self, _quart_x: i32, _quart_y: i32, _quart_z: i32) -> BiomeRef {
        &vanilla_biomes::PLAINS
    }

    fn spawn_height(&self, min_y: i32, height: i32) -> i32 {
        min_y + height.min(self.layers.len() as i32)
    }

    fn structure_generator(&self) -> Option<&StructureGenerator> {
        self.structure_generator.as_ref()
    }

    fn create_structures(&self, chunk: &Chunk) {
        let Some(structure_generator) = &self.structure_generator else {
            return;
        };

        let pos = chunk.pos();
        let chunk_x = pos.0.x;
        let chunk_z = pos.0.y;
        let chunk_min_x = chunk_x * 16;
        let chunk_min_z = chunk_z * 16;
        let mut ctx = FlatGenerationContext {
            seed: self.seed,
            chunk_x,
            chunk_z,
            chunk_min_x,
            chunk_min_z,
            center_block_x: chunk_min_x + 8,
            center_block_z: chunk_min_z + 8,
            sea_level: self.sea_level,
            min_y: chunk.min_y(),
            height: (chunk.sections().sections.len() * 16) as i32,
            generator: self,
            biome: &vanilla_biomes::PLAINS,
            template_pools: structure_generator.template_pools(),
            templates: structure_generator.templates(),
            surface_y_cache: None,
        };
        create_structures(structure_generator, chunk, &mut ctx);
    }

    fn create_biomes(&self, chunk: &Chunk) {
        let section_count = chunk.sections().sections.len();

        for section_index in 0..section_count {
            let section = &chunk.sections().sections[section_index];
            let mut section_guard = section.write();

            for local_quart_x in 0..4usize {
                for local_quart_y in 0..4usize {
                    for local_quart_z in 0..4usize {
                        section_guard.biomes.set(
                            local_quart_x,
                            local_quart_y,
                            local_quart_z,
                            self.biome_id,
                        );
                    }
                }
            }
            drop(section_guard);
        }

        chunk.mark_dirty();
    }

    fn fill_from_noise(
        &self,
        chunk: GenerationChunk<'_, NoisePhase>,
        _beardifier: Option<&Beardifier>,
    ) {
        let max_relative_y = chunk.section_count() * 16;

        for x in 0..16 {
            for z in 0..16 {
                for (relative_y, block) in self.layers.iter().enumerate().take(max_relative_y) {
                    chunk.set_relative_block(x, relative_y, z, *block);
                }
            }
        }
    }

    fn build_surface(
        &self,
        _chunk: GenerationChunk<'_, SurfacePhase>,
        _neighbor_biomes: &dyn Fn(IVec3) -> u16,
    ) {
    }

    fn apply_carvers(&self, _chunk: GenerationChunk<'_, CarversPhase>) {}

    fn create_worldgen_region_random(&self, world_seed: i64, center: ChunkPos) -> RandomSource {
        xoroshiro_worldgen_region_random(world_seed, center)
    }

    /// Vanilla parity: `ChunkGenerator.applyBiomeDecoration`, which a flat level
    /// runs like any other -- it is what writes the pieces of the structures the
    /// preset's overrides placed, as well as any features the preset asked for.
    fn apply_biome_decorations(&self, region: &WorldGenRegion<'_>) {
        self.feature_runner
            .decorate(region, &REGISTRY, self.seed, self.biome_zoom_seed);
    }

    fn possible_biomes(&self) -> Vec<BiomeRef> {
        // Vanilla parity: `FlatLevelSource` uses a `FixedBiomeSource`.
        vec![&vanilla_biomes::PLAINS]
    }

    fn with_noise_biomes(&self, body: &mut NoiseBiomeBody<'_>) {
        body(&mut |_, _, _| &vanilla_biomes::PLAINS);
    }

    fn with_first_free_height(&self, min_y: i32, body: &mut FirstFreeHeightBody<'_>) {
        // Vanilla parity: `FlatLevelSource.getBaseHeight` reads the layer list,
        // which is the same for every column.
        let height = self.base_height(min_y, self.layers.len() as i32, false);
        body(&mut |_, _| height);
    }
}

#[cfg(test)]
mod tests {
    use steel_registry::feature::ConfiguredFeatureKind;
    use steel_registry::init_vanilla_registry;

    use super::*;

    fn state(name: &'static str) -> BlockStateId {
        let Some(block) = REGISTRY.blocks.by_key(&Identifier::vanilla_static(name)) else {
            panic!("{name} should be a vanilla block");
        };
        REGISTRY.blocks.get_default_state_id(block)
    }

    fn classic_layers() -> Vec<BlockStateId> {
        vec![
            state("bedrock"),
            state("dirt"),
            state("dirt"),
            state("grass_block"),
        ]
    }

    fn registered_keys(steps: &[Vec<FeatureEntry>], step: usize) -> Vec<Identifier> {
        steps[step]
            .iter()
            .filter_map(|entry| match entry {
                FeatureEntry::Registered(feature) => Some(feature.key.clone()),
                FeatureEntry::Inline(..) => None,
            })
            .collect()
    }

    /// A preset that asks for nothing gets nothing: the flags are the only thing
    /// that puts a feature in a flat world, and turning them off has to leave
    /// the decoration pass with an empty list rather than the biome's own.
    #[test]
    fn a_plain_preset_decorates_with_nothing() {
        init_vanilla_registry();
        let (layers, steps) = FlatChunkGenerator::adjusted_features(
            classic_layers(),
            &vanilla_biomes::PLAINS,
            FlatDecoration::default(),
        );

        assert_eq!(layers, classic_layers());
        assert!(steps.iter().all(Vec::is_empty));
    }

    /// `lakes = true` adds exactly the two lava lakes vanilla lists, at the
    /// lakes step, and `features = true` then has to leave that step alone
    /// rather than adding the biome's own lakes on top.
    #[test]
    fn lakes_replace_the_biome_lake_step() {
        init_vanilla_registry();
        let expected = vec![
            Identifier::vanilla_static("lake_lava_underground"),
            Identifier::vanilla_static("lake_lava_surface"),
        ];

        let (_, lakes_only) = FlatChunkGenerator::adjusted_features(
            classic_layers(),
            &vanilla_biomes::PLAINS,
            FlatDecoration {
                features: false,
                lakes: true,
            },
        );
        assert_eq!(registered_keys(&lakes_only, LAKES_STEP), expected);

        let (_, both) = FlatChunkGenerator::adjusted_features(
            classic_layers(),
            &vanilla_biomes::PLAINS,
            FlatDecoration {
                features: true,
                lakes: true,
            },
        );
        assert_eq!(registered_keys(&both, LAKES_STEP), expected);
    }

    /// `features = true` copies the biome's list except for the two structure
    /// steps. A flat world places its structures from its own overrides, so
    /// leaving those in would generate them twice.
    #[test]
    fn features_take_the_biome_list_without_its_structure_steps() {
        init_vanilla_registry();
        let biome = &vanilla_biomes::PLAINS;
        let biome_features = FeatureDecorationRunner::registered_features(biome, &REGISTRY);
        let (_, steps) = FlatChunkGenerator::adjusted_features(
            classic_layers(),
            biome,
            FlatDecoration {
                features: true,
                lakes: false,
            },
        );

        assert!(steps[UNDERGROUND_STRUCTURES_STEP].is_empty());
        assert!(steps[SURFACE_STRUCTURES_STEP].is_empty());
        // The plains list is not empty in these two steps, so the assertions
        // above are about dropping them rather than about there being nothing.
        assert!(
            !biome_features[UNDERGROUND_STRUCTURES_STEP].is_empty()
                || !biome_features[SURFACE_STRUCTURES_STEP].is_empty()
        );
        for (step, stage) in biome_features.iter().enumerate() {
            if step == UNDERGROUND_STRUCTURES_STEP || step == SURFACE_STRUCTURES_STEP {
                continue;
            }
            assert_eq!(steps[step].len(), stage.len(), "step {step} lost features");
        }
    }

    /// A layer that does not block motion leaves the stack and comes back as a
    /// `FILL_LAYER` feature at the last decoration step, so a tree or a piece
    /// of a village that wants the space keeps it. Everything solid stays put.
    #[test]
    fn a_non_opaque_layer_moves_out_of_the_stack_into_fill_layer() {
        init_vanilla_registry();
        let grass = state("short_grass");
        let (layers, steps) = FlatChunkGenerator::adjusted_features(
            vec![state("bedrock"), state("dirt"), state("grass_block"), grass],
            &vanilla_biomes::PLAINS,
            FlatDecoration::default(),
        );

        let air = vanilla_blocks::AIR.default_state();
        assert_eq!(
            layers,
            vec![state("bedrock"), state("dirt"), state("grass_block"), air]
        );

        let [FeatureEntry::Inline(_, placed)] = steps[TOP_LAYER_MODIFICATION_STEP].as_slice()
        else {
            panic!("the short grass layer should have become one inline feature");
        };
        assert!(placed.placement.is_empty());
        let ConfiguredFeatureRef::Inline(kind) = &placed.feature else {
            panic!("the fill layer feature is built, not referenced");
        };
        let ConfiguredFeatureKind::FillLayer(config) = kind.as_ref() else {
            panic!("the inline feature should be a fill layer");
        };
        assert_eq!(config.height, 3);
        assert_eq!(config.state, grass);
    }
}
