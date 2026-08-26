//! The level surface vanilla features and structure templates place into.

use std::sync::{Arc, Weak};

use simdnbt::owned::NbtCompound;
use steel_registry::block_entity_type::BlockEntityTypeRef;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_utils::random::RandomSource;
use steel_utils::{BlockPos, BlockStateId};

use crate::block_entity::BLOCK_ENTITIES;
use crate::chunk::light::LightLayer;
use crate::entity::SharedEntity;
use crate::world::{LevelAccessor, World};

/// Level access needed to place a feature or a structure template.
///
/// Vanilla parity: `WorldGenLevel`, the interface `ServerLevel` and `WorldGenRegion`
/// both implement. Steel mirrors it for the same reason vanilla has it: bone meal on
/// a moss block, the structure block in `LOAD` mode and jigsaw generation all run the
/// placement code the chunk generator runs, against a live world instead of a region.
///
/// Everything a placement reads or writes per block already lives on [`LevelAccessor`].
/// This trait only adds what placement needs *around* those writes: the world seed and
/// generation bounds, biome and light lookups, and the three writes a plain
/// `set_block_state` cannot express -- block entity data, entities, and the level's own
/// random source.
pub trait WorldGenLevel: LevelAccessor {
    /// Returns the world seed.
    ///
    /// Vanilla parity: `WorldGenLevel.getSeed`, which a live level answers through
    /// `ServerLevel.getSeed`.
    fn seed(&self) -> i64;

    /// Returns this dimension's sea level.
    fn sea_level(&self) -> i32;

    /// Returns the minimum Y coordinate vanilla `WorldGenerationContext` reports.
    fn generation_min_y(&self) -> i32;

    /// Returns the vertical depth vanilla `WorldGenerationContext` reports.
    fn generation_height(&self) -> i32;

    /// Returns the noise biome id at quart coordinates.
    ///
    /// Vanilla parity: `LevelReader.getNoiseBiome`. A region always has the column,
    /// because the generation step declared the chunk as a dependency. A live world
    /// answers `None` for an unloaded chunk rather than inventing a biome.
    fn noise_biome_id(&self, quart_x: i32, quart_y: i32, quart_z: i32) -> Option<u16>;

    /// Returns the block light level at a position.
    fn block_light_at(&self, pos: BlockPos) -> u8;

    /// Runs `apply` against the level's own random source.
    ///
    /// Vanilla parity: `LevelAccessor.getRandom`. A region hands out the positional
    /// stream seeded from its center chunk, so draws stay deterministic; a live level
    /// hands out `Level.random`, which vanilla creates unseeded.
    fn with_level_random<R>(&self, apply: impl FnOnce(&mut RandomSource) -> R) -> R;

    /// Attaches block entity data at a position that already holds `state`.
    ///
    /// Vanilla loads the tag into the block entity `setBlock` just created. Steel's
    /// region only records a pending marker while generating, so both surfaces build
    /// and register the entity here instead.
    fn set_block_entity_data(
        &self,
        pos: BlockPos,
        block_entity_type: BlockEntityTypeRef,
        state: BlockStateId,
        nbt: NbtCompound,
    ) -> bool;

    /// Removes the block entity at a position, if any.
    fn remove_block_entity(&self, pos: BlockPos) -> bool;

    /// Adds a freshly built entity to this level.
    ///
    /// Vanilla parity: `LevelWriter.addFreshEntity`.
    fn add_fresh_entity(&self, entity: SharedEntity) -> bool;

    /// Returns the world reference entities and block entities are built against.
    fn weak_world(&self) -> Weak<World>;
}

impl WorldGenLevel for Arc<World> {
    fn seed(&self) -> i64 {
        World::seed(self)
    }

    fn sea_level(&self) -> i32 {
        self.sea_level
    }

    fn generation_min_y(&self) -> i32 {
        self.chunk_map.world_gen_context.generation_min_y()
    }

    fn generation_height(&self) -> i32 {
        self.chunk_map.world_gen_context.generation_height()
    }

    fn noise_biome_id(&self, quart_x: i32, quart_y: i32, quart_z: i32) -> Option<u16> {
        World::noise_biome_id(self, quart_x, quart_y, quart_z)
    }

    fn block_light_at(&self, pos: BlockPos) -> u8 {
        self.light_value_at(LightLayer::Block, pos)
    }

    fn with_level_random<R>(&self, apply: impl FnOnce(&mut RandomSource) -> R) -> R {
        apply(&mut self.level_random.lock())
    }

    fn set_block_entity_data(
        &self,
        pos: BlockPos,
        block_entity_type: BlockEntityTypeRef,
        state: BlockStateId,
        nbt: NbtCompound,
    ) -> bool {
        if !block_entity_type.is_valid(state.get_block()) {
            log::warn!(
                "Block entity {} at {pos:?} does not accept block {}",
                block_entity_type.key,
                state.get_block().key,
            );
            return false;
        }

        let entity = BLOCK_ENTITIES.create_and_load_owned_or_raw(
            block_entity_type,
            Arc::downgrade(self),
            pos,
            state,
            nbt,
        );
        World::set_block_entity(self, entity)
    }

    fn remove_block_entity(&self, pos: BlockPos) -> bool {
        World::remove_block_entity(self, pos)
    }

    fn add_fresh_entity(&self, entity: SharedEntity) -> bool {
        self.try_add_entity(entity).is_ok()
    }

    fn weak_world(&self) -> Weak<World> {
        Arc::downgrade(self)
    }
}
