//! The view an ore vein reads and writes through.

use foton_utils::types::UpdateFlags;
use foton_utils::{BlockPos, BlockStateId, PackedSectionBlockPos, SectionPos};

use crate::world::WorldGenLevel;

/// Ore-vein reads and writes, separated from the level surface underneath them.
///
/// Vanilla parity: `BulkSectionAccess`, which `OreFeature` builds over the level it
/// places into so a vein touches each chunk section once instead of once per block.
/// A generation region owns a section cache and can do exactly that. A live world
/// cannot hold sections open across a whole vein -- everything else in the tick shares
/// them -- so [`LevelOreAccess`] writes through the level one block at a time.
pub(crate) trait OreLevelAccess {
    /// Reads the block an ore vein is considering replacing.
    fn ore_target_block_state(&mut self, pos: BlockPos) -> BlockStateId;

    /// Reads a neighbor of a candidate position for the air-exposure check.
    fn ore_neighbor_block_state(&mut self, pos: BlockPos) -> BlockStateId;

    /// Replaces one ore target, deciding on the state read at the write point.
    ///
    /// Only suitable for veins that need no neighbor reads to decide.
    #[must_use]
    fn replace_ore_target_block_state(
        &mut self,
        pos: BlockPos,
        replacement: impl FnOnce(BlockStateId) -> Option<BlockStateId>,
    ) -> bool;

    /// Replaces every ore target collected for one chunk section, returning the count.
    fn replace_ore_target_block_states_in_section(
        &mut self,
        chunk_x: i32,
        chunk_z: i32,
        section_index: usize,
        positions: &[PackedSectionBlockPos],
        replacement: impl FnMut(BlockStateId) -> Option<BlockStateId>,
    ) -> u64;

    /// Writes one ore block.
    #[must_use]
    fn set_ore_block_state(&mut self, pos: BlockPos, state: BlockStateId) -> bool;

    /// Returns whether this surface accepts a write at the position.
    fn can_write_to_pos(&self, pos: BlockPos) -> bool;

    /// Counts one position the vein shape reached.
    fn record_ore_candidate_position(&mut self);

    /// Counts one position the vein had not tested yet.
    fn record_ore_unique_position(&mut self);

    /// Counts one position the write radius accepted.
    fn record_ore_write_allowed_position(&mut self);
}

/// Per-block ore access for a level with no section cache to batch through.
pub(crate) struct LevelOreAccess<'level, L: WorldGenLevel> {
    level: &'level L,
}

impl<'level, L: WorldGenLevel> LevelOreAccess<'level, L> {
    pub(crate) const fn new(level: &'level L) -> Self {
        Self { level }
    }

    fn replace(
        &self,
        pos: BlockPos,
        replacement: impl FnOnce(BlockStateId) -> Option<BlockStateId>,
    ) -> bool {
        let Some(state) = replacement(self.level.get_block_state(pos)) else {
            return false;
        };
        self.level
            .set_block_state(pos, state, UpdateFlags::UPDATE_CLIENTS)
    }
}

impl<L: WorldGenLevel> OreLevelAccess for LevelOreAccess<'_, L> {
    fn ore_target_block_state(&mut self, pos: BlockPos) -> BlockStateId {
        self.level.get_block_state(pos)
    }

    fn ore_neighbor_block_state(&mut self, pos: BlockPos) -> BlockStateId {
        self.level.get_block_state(pos)
    }

    fn replace_ore_target_block_state(
        &mut self,
        pos: BlockPos,
        replacement: impl FnOnce(BlockStateId) -> Option<BlockStateId>,
    ) -> bool {
        self.replace(pos, replacement)
    }

    fn replace_ore_target_block_states_in_section(
        &mut self,
        chunk_x: i32,
        chunk_z: i32,
        section_index: usize,
        positions: &[PackedSectionBlockPos],
        mut replacement: impl FnMut(BlockStateId) -> Option<BlockStateId>,
    ) -> u64 {
        let Ok(section_index) = i32::try_from(section_index) else {
            return 0;
        };
        let section = SectionPos::new(
            chunk_x,
            SectionPos::block_to_section_coord(self.level.min_y()) + section_index,
            chunk_z,
        );
        let mut placed = 0;
        for &local in positions {
            if self.replace(section.relative_to_block_pos(local), &mut replacement) {
                placed += 1;
            }
        }
        placed
    }

    fn set_ore_block_state(&mut self, pos: BlockPos, state: BlockStateId) -> bool {
        self.level
            .set_block_state(pos, state, UpdateFlags::UPDATE_CLIENTS)
    }

    fn can_write_to_pos(&self, pos: BlockPos) -> bool {
        self.level.can_write_to_chunk(
            SectionPos::block_to_section_coord(pos.x()),
            SectionPos::block_to_section_coord(pos.z()),
        )
    }

    /// A live level has no generation profile to count into.
    fn record_ore_candidate_position(&mut self) {}

    /// See [`Self::record_ore_candidate_position`].
    fn record_ore_unique_position(&mut self) {}

    /// See [`Self::record_ore_candidate_position`].
    fn record_ore_write_allowed_position(&mut self) {}
}
