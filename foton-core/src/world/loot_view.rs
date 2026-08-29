//! Lets a loot roll read the world it happens in.
//!
//! Vanilla parity: the `ServerLevel` a `LootContext` carries. `foton-registry`
//! defines what loot needs of a world and `World` answers it here, because the
//! registry crate cannot see this one.

use foton_registry::biome::BiomeRef;
use foton_registry::loot_table::LootWorldView;
use foton_utils::{BlockPos, BlockStateId, ChunkPos};

use crate::world::World;

impl LootWorldView for World {
    fn loaded_block_state(&self, x: i32, y: i32, z: i32) -> Option<BlockStateId> {
        let pos = BlockPos::new(x, y, z);
        // Vanilla `BlockPredicate.matches` opens on `level.isLoaded(pos)`, and
        // `World::get_block_state` answers air for an unloaded chunk -- which
        // would read as a real block that simply is not the one asked for.
        self.has_full_chunk(ChunkPos::from_block_pos(pos))
            .then(|| self.get_block_state(pos))
    }

    fn loaded_biome(&self, x: i32, y: i32, z: i32) -> Option<BiomeRef> {
        Self::biome_at(self, BlockPos::new(x, y, z))
    }
}
