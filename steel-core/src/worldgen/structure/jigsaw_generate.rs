//! A jigsaw block assembling a structure into a live world.
//!
//! Vanilla parity: `JigsawPlacement.generateJigsaw`. It runs the same assembly a
//! jigsaw structure runs, but from the jigsaw block's own position instead of a
//! chunk corner, with no start-height sample, no pool aliases, no expansion
//! hack, and no heightmap projection.

use std::sync::{Arc, OnceLock};

use glam::IVec3;
use rustc_hash::FxHashMap;
use steel_registry::REGISTRY;
use steel_registry::structure::{DimensionPadding, LiquidSettingsData};
use steel_registry::template_pool::{TemplateData, TemplatePoolData};
use steel_registry::vanilla_template_pools::{vanilla_template_pools, vanilla_templates};
use steel_utils::random::legacy_random::LegacyRandom;
use steel_utils::random::worldgen_random::WorldgenRandom;
use steel_utils::{BlockPos, BoundingBox, Identifier};
use steel_worldgen::structure::jigsaw::{JigsawPlacement, MaxDistance, assemble};

use crate::world::World;
use crate::worldgen::ChunkGenerator as _;
use crate::worldgen::structure::piece_placer::StructurePiecePlacer;

/// Vanilla's `JigsawStructure.MaxDistance(128)`, the limit `generateJigsaw` uses.
const GENERATE_MAX_DISTANCE: i32 = 128;

/// The template pools and templates a jigsaw block assembles from.
///
/// A jigsaw block can sit in a world whose generator owns no `StructureGenerator`
/// -- a superflat with no structure overrides -- and vanilla still reads the
/// pools straight out of the registry, so they live here instead of being taken
/// from the generator. Nothing is built until a generate button is pressed.
struct JigsawAssets {
    pools: FxHashMap<Identifier, TemplatePoolData>,
    templates: FxHashMap<Identifier, TemplateData>,
}

static JIGSAW_ASSETS: OnceLock<JigsawAssets> = OnceLock::new();

fn jigsaw_assets() -> &'static JigsawAssets {
    JIGSAW_ASSETS.get_or_init(|| JigsawAssets {
        pools: vanilla_template_pools()
            .into_iter()
            .map(|pool| (pool.key.clone(), pool))
            .collect(),
        templates: vanilla_templates().into_iter().collect(),
    })
}

/// Assembles `pool` from `position` and writes the pieces into the world.
///
/// Vanilla parity: `JigsawPlacement.generateJigsaw`. Returns whether an assembly
/// was found; a missing pool, an empty one, or a start piece with no jigsaw
/// named `target` all return `false` without touching a block.
#[must_use]
pub(crate) fn generate_jigsaw(
    world: &Arc<World>,
    pool: &Identifier,
    target: &Identifier,
    max_depth: i32,
    position: BlockPos,
    keep_jigsaws: bool,
) -> bool {
    let assets = jigsaw_assets();
    let min_y = world.get_min_y();
    // `StructureGenerationContext::max_y` is one past the top build layer.
    let max_y = world.get_max_y() + 1;

    let placement = JigsawPlacement {
        start_pool: pool,
        start_jigsaw_name: Some(target),
        max_depth,
        position: IVec3::new(position.x(), position.y(), position.z()),
        use_expansion_hack: false,
        project_start_to_heightmap: false,
        max_distance: MaxDistance::new(GENERATE_MAX_DISTANCE),
        // Vanilla parity: `JigsawStructure.DEFAULT_DIMENSION_PADDING` is zero.
        dimension_padding: DimensionPadding { bottom: 0, top: 0 },
        // Vanilla parity: `JigsawStructure.DEFAULT_LIQUID_SETTINGS`.
        liquid_settings: LiquidSettingsData::ApplyWaterlogging,
    };

    // Vanilla parity: `Structure.GenerationContext` seeds its random from the
    // chunk holding the start position, and `generateJigsaw` samples no start
    // height, so the first draw is the start piece's rotation.
    let mut rng = LegacyRandom::from_seed(0);
    rng.set_large_feature_seed(world.seed(), position.x() >> 4, position.z() >> 4);

    // Vanilla parity: `PoolAliasLookup.EMPTY`.
    let alias_map = FxHashMap::default();

    let mut assembly = None;
    world
        .chunk_map
        .world_gen_context
        .generator
        .with_first_free_height(min_y, &mut |get_height| {
            assembly = assemble(
                &placement,
                &mut rng,
                &assets.pools,
                &assets.templates,
                &alias_map,
                get_height,
                min_y,
                max_y,
            );
        });
    let Some(assembly) = assembly else {
        return false;
    };

    // Vanilla passes `BoundingBox.infinite()`; Steel's placement always clips to
    // a box, so this is the whole buildable column.
    let clip = BoundingBox::new(
        IVec3::new(i32::MIN, min_y, i32::MIN),
        IVec3::new(i32::MAX, world.get_max_y(), i32::MAX),
    );
    // Vanilla passes `level.getRandom()`, which is seeded from the wall clock;
    // nothing about this placement is meant to be repeatable.
    let mut random = WorldgenRandom::from_seed(rand::random());
    let biome_zoom_seed = world.biome_zoom_seed();

    for piece in assembly.pieces {
        StructurePiecePlacer::place_pool_element(
            world,
            &REGISTRY,
            &piece.element,
            BlockPos::new(piece.position.x, piece.position.y, piece.position.z),
            position,
            piece.rotation,
            clip,
            &mut random,
            placement.liquid_settings,
            biome_zoom_seed,
            keep_jigsaws,
        );
    }

    true
}
