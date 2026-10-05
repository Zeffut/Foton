//! Real RAM-only world fixture, matching foton-core/tests/support construction.

use std::sync::{Arc, OnceLock};
use tokio::runtime as bridge_runtime;
use toml::map as bridge_toml_map;

use foton_core::{
    level_data::WorldGenerationSettings,
    world::{World, WorldConfig, WorldStorageConfig},
    worldgen::{ChunkGeneratorType, EmptyChunkGenerator},
};
use foton_registry::{init_vanilla_registry, vanilla_dimension_types};
use foton_utils::{
    Identifier,
    types::{Difficulty, GameType},
};

pub(super) fn world() -> &'static Arc<World> {
    static WORLD: OnceLock<Arc<World>> = OnceLock::new();
    WORLD.get_or_init(|| {
        init_vanilla_registry();
        let runtime = Arc::new(
            bridge_runtime::Builder::new_multi_thread()
                .worker_threads(1)
                .enable_all()
                .build()
                .expect("item fixture runtime"),
        );
        let pool = Arc::new(
            rayon::ThreadPoolBuilder::new()
                .num_threads(1)
                .build()
                .expect("item fixture pool"),
        );
        let dimension = &vanilla_dimension_types::OVERWORLD;
        let generator = Arc::new(ChunkGeneratorType::Empty(EmptyChunkGenerator::new()));
        let settings = WorldGenerationSettings::from_generator_config(
            Identifier::vanilla_static("empty"),
            &toml::Value::Table(bridge_toml_map::Map::new()),
            dimension.key.clone(),
            dimension.min_y,
            dimension.height,
        );
        runtime
            .block_on(World::new_with_config(
                Arc::clone(&runtime),
                Identifier::new_static("foton", "item_bridge_test"),
                dimension,
                0,
                WorldConfig {
                    storage: WorldStorageConfig::RamOnly,
                    level_data_path: None,
                    generator,
                    generation_settings: settings,
                    view_distance: 2,
                    simulation_distance: 2,
                    max_chained_neighbor_updates: 1_000_000,
                    compression: None,
                    is_flat: false,
                    sea_level: 63,
                    default_gamemode: GameType::Survival,
                    difficulty: Difficulty::Normal,
                    bonus_chest: false,
                },
                pool,
            ))
            .expect("real item fixture world")
    })
}
