use foton_registry::blocks::block_state_ext::BlockStateExt as _;
use foton_registry::blocks::properties::{BedPart, BlockStateProperties};
use foton_registry::vanilla_block_tags::BlockTag;
use foton_registry::vanilla_blocks;
use foton_utils::{BlockPos, BlockStateId, Downcast as _};

use super::move_to_block::MoveToBlockGoal;
use super::selector::{Goal, GoalControls};
use crate::entity::entities::CatEntity;
use crate::entity::{PathfinderMob, TamableAnimal};
use crate::world::LevelReader;

/// How far a cat looks for something warm to sit on.
///
/// Vanilla parity: the `8` of `CatSitOnBlockGoal`.
const SEARCH_RANGE: i32 = 8;

/// Sends a tamed cat to sit on a chest, a lit furnace or the foot of a bed.
///
/// Vanilla parity: `CatSitOnBlockGoal`.
pub struct CatSitOnBlockGoal {
    move_to_block: MoveToBlockGoal,
}

impl CatSitOnBlockGoal {
    #[must_use]
    pub(crate) fn new(speed_modifier: f64) -> Self {
        Self {
            move_to_block: MoveToBlockGoal::new(speed_modifier, SEARCH_RANGE, is_valid_seat),
        }
    }
}

/// Vanilla parity: `CatSitOnBlockGoal.isValidTarget`.
fn is_valid_seat(level: &dyn LevelReader, pos: BlockPos) -> bool {
    if !level.get_block_state(pos.above()).is_air() {
        return false;
    }

    let state = level.get_block_state(pos);
    let block = state.get_block();
    if block == &vanilla_blocks::CHEST {
        // Vanilla parity: `ChestBlockEntity.getOpenCount(level, pos) < 1`. A cat
        // will not sit on a chest somebody has open.
        return level
            .get_block_entity(pos)
            .is_none_or(|block_entity| block_entity.base().opener_count() < 1);
    }

    if block == &vanilla_blocks::FURNACE {
        return state.get_value(&BlockStateProperties::LIT);
    }

    block.has_tag(&BlockTag::BEDS) && !is_bed_head(state)
}

fn is_bed_head(state: BlockStateId) -> bool {
    state
        .try_get_value(&BlockStateProperties::BED_PART)
        .is_some_and(|part| part == BedPart::Head)
}

impl Goal for CatSitOnBlockGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::MOVE | GoalControls::JUMP
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        let Some(cat) = mob.downcast_ref::<CatEntity>() else {
            return false;
        };
        cat.is_tame() && !cat.is_ordered_to_sit() && self.move_to_block.can_use(mob)
    }

    fn can_continue_to_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        self.move_to_block.can_continue_to_use(mob)
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        self.move_to_block.start(mob);
        if let Some(cat) = mob.downcast_ref::<CatEntity>() {
            cat.set_in_sitting_pose(false);
        }
    }

    fn stop(&mut self, mob: &dyn PathfinderMob) {
        self.move_to_block.stop(mob);
        if let Some(cat) = mob.downcast_ref::<CatEntity>() {
            cat.set_in_sitting_pose(false);
        }
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        self.move_to_block.tick(mob);
        if let Some(cat) = mob.downcast_ref::<CatEntity>() {
            cat.set_in_sitting_pose(self.move_to_block.is_reached_target());
        }
    }
}
