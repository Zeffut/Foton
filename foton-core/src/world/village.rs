//! What counts as a village, and which raid is besieging it.
//!
//! Vanilla parity: the village and raid accessors of `ServerLevel` --
//! `isVillage`, `isCloseToVillage`, `sectionsToVillage`, `getRaids`,
//! `getRaidAt` and `isRaided`. They sit together because they are one question
//! asked two ways: the point-of-interest index answers where a village is, and
//! the raid manager answers what is happening to it.

use std::sync::Arc;

use foton_registry::blocks::block_state_ext::BlockStateExt as _;
use foton_registry::vanilla_blocks;
use foton_utils::{BlockPos, SectionPos};

use super::World;
use crate::poi::MAX_VILLAGE_DISTANCE;
use crate::raid::{Raid, Raids, VALID_RAID_RADIUS_SQR};

impl World {
    /// Returns the raids running in this loaded world.
    ///
    /// Vanilla parity: `ServerLevel.getRaids`.
    #[must_use]
    pub const fn raids(&self) -> &Raids {
        &self.raids
    }

    /// Returns whether `pos` is inside a village.
    ///
    /// Vanilla parity: `ServerLevel.isVillage(BlockPos)`.
    #[must_use]
    pub fn is_village(&self, pos: BlockPos) -> bool {
        self.is_close_to_village(pos, 1)
    }

    /// Returns whether the center of `section` is inside a village.
    ///
    /// Vanilla parity: `ServerLevel.isVillage(SectionPos)`.
    #[must_use]
    pub fn is_village_section(&self, section: SectionPos) -> bool {
        self.is_village(BlockPos::new(
            (section.x() << 4) + 8,
            (section.y() << 4) + 8,
            (section.z() << 4) + 8,
        ))
    }

    /// Returns whether a village center is within `section_distance` sections.
    ///
    /// Vanilla parity: `ServerLevel.isCloseToVillage`.
    #[must_use]
    pub fn is_close_to_village(&self, pos: BlockPos, section_distance: i32) -> bool {
        if section_distance > MAX_VILLAGE_DISTANCE {
            return false;
        }
        self.sections_to_village(SectionPos::from_block_pos(pos)) <= section_distance
    }

    /// Returns how many sections away the nearest village center is.
    ///
    /// Vanilla parity: `ServerLevel.sectionsToVillage`.
    #[must_use]
    pub fn sections_to_village(&self, section: SectionPos) -> i32 {
        self.poi_storage.lock().sections_to_village(section)
    }

    /// Returns the raid `pos` is inside, if any.
    ///
    /// Vanilla parity: `ServerLevel.getRaidAt`.
    #[must_use]
    pub fn get_raid_at(&self, pos: BlockPos) -> Option<Arc<Raid>> {
        self.raids.nearby_raid(pos, VALID_RAID_RADIUS_SQR)
    }

    /// Returns the id of the raid `pos` is inside, if any.
    ///
    /// The id rather than the raid, for callers comparing against a raid they
    /// already hold -- vanilla compares by object identity, which an `Arc`
    /// pulled out of the manager cannot answer without the manager's lock.
    #[must_use]
    pub fn raid_id_at(&self, pos: BlockPos) -> Option<i32> {
        self.get_raid_at(pos).map(|raid| raid.id())
    }

    /// Returns whether a raid is running over `pos`.
    ///
    /// Vanilla parity: `ServerLevel.isRaided`.
    #[must_use]
    pub fn is_raided(&self, pos: BlockPos) -> bool {
        self.get_raid_at(pos).is_some()
    }

    /// Returns whether `pos` is open air over a snow layer.
    ///
    /// Vanilla parity: the `getBlockState(pos.below()).is(Blocks.SNOW) &&
    /// getBlockState(pos).isAir()` half of `Raid.findRandomSpawnPos`, which is
    /// what lets a raid spawn on a snowy plain where the ravager's own spawn
    /// placement refuses the surface.
    #[must_use]
    pub fn is_snow_over_air(&self, pos: BlockPos) -> bool {
        self.get_block_state(pos.below()).get_block() == &vanilla_blocks::SNOW
            && self.get_block_state(pos).is_air()
    }
}
