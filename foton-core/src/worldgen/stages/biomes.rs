use std::sync::Arc;

use crate::chunk::{
    chunk_generation_task::StaticCache2D, chunk_holder::ChunkHolder, chunk_pyramid::ChunkStep,
    status::ChunkStatus,
};
use crate::worldgen::generator::ChunkGenerator;
use crate::worldgen::generator::context::WorldGenContext;

pub(crate) fn generate(
    context: Arc<WorldGenContext>,
    _step: &ChunkStep,
    _cache: &Arc<StaticCache2D<Arc<ChunkHolder>>>,
    holder: Arc<ChunkHolder>,
) {
    #[cfg_attr(
        not(test),
        expect(
            clippy::expect_used,
            reason = "the generation pyramid publishes this status before the stage that consumes it runs"
        )
    )]
    let chunk = holder
        .try_chunk(ChunkStatus::StructureReferences)
        .expect("Chunk not found at status StructureReferences");

    context.generator.create_biomes(chunk);
}
