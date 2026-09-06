use std::sync::Arc;

use crate::chunk::{
    chunk_generation_task::StaticCache2D, chunk_holder::ChunkHolder, chunk_pyramid::ChunkStep,
    status::ChunkStatus,
};
use crate::worldgen::generator::ChunkGenerator;
use crate::worldgen::generator::context::WorldGenContext;
use crate::worldgen::region::WorldGenRegion;

#[cfg_attr(
    not(test),
    expect(
        clippy::panic,
        reason = "this comes from extracted vanilla data or from a count taken over it, so an unresolved reference means the extraction is broken rather than anything at runtime"
    )
)]
pub(crate) fn generate(
    context: Arc<WorldGenContext>,
    step: &ChunkStep,
    cache: &Arc<StaticCache2D<Arc<ChunkHolder>>>,
    holder: Arc<ChunkHolder>,
) {
    let center = holder.get_pos();

    let Some(chunk) = holder.try_chunk(ChunkStatus::Carvers) else {
        panic!("Chunk not found at status Carvers");
    };
    chunk.prime_final_heightmaps();

    let world_seed = context.world().seed();
    let region_random = context
        .generator
        .create_worldgen_region_random(world_seed, center);
    let region = WorldGenRegion::new(context.as_ref(), step, cache, center, region_random);
    context.generator.apply_biome_decorations(&region);
}
