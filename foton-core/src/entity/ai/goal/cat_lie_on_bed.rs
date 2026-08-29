use foton_registry::blocks::block_state_ext::BlockStateExt as _;
use foton_registry::vanilla_block_tags::BlockTag;
use foton_utils::Downcast as _;

use super::move_to_block::MoveToBlockGoal;
use super::selector::{Goal, GoalControls};
use crate::entity::entities::CatEntity;
use crate::entity::{PathfinderMob, TamableAnimal};

/// How far a cat looks for a bed.
///
/// Vanilla parity: the `8` `Cat.registerGoals` passes.
const SEARCH_RANGE: i32 = 8;

/// How far up a cat looks for a bed.
///
/// Vanilla parity: the `6` of the `CatLieOnBedGoal` super call.
const VERTICAL_SEARCH_RANGE: i32 = 6;

/// How far down a cat starts looking.
///
/// Vanilla parity: `this.verticalSearchStart = -2`.
const VERTICAL_SEARCH_START: i32 = -2;

/// Ticks between two bed searches.
///
/// Vanilla parity: the `nextStartTick` override, a flat 40 rather than the
/// randomized two hundred the base goal uses.
const START_INTERVAL_TICKS: i32 = 40;

/// Sends a tamed cat to sleep on the nearest bed.
///
/// Vanilla parity: `CatLieOnBedGoal`.
pub struct CatLieOnBedGoal {
    move_to_block: MoveToBlockGoal,
}

impl CatLieOnBedGoal {
    #[must_use]
    pub(crate) fn new(speed_modifier: f64) -> Self {
        Self {
            move_to_block: MoveToBlockGoal::with_vertical_search_range(
                speed_modifier,
                SEARCH_RANGE,
                VERTICAL_SEARCH_RANGE,
                |level, pos| {
                    level.get_block_state(pos.above()).is_air()
                        && level
                            .get_block_state(pos)
                            .get_block()
                            .has_tag(&BlockTag::BEDS)
                },
            )
            .with_vertical_search_start(VERTICAL_SEARCH_START)
            .with_fixed_start_interval(START_INTERVAL_TICKS),
        }
    }
}

impl Goal for CatLieOnBedGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::JUMP | GoalControls::MOVE
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        let Some(cat) = mob.downcast_ref::<CatEntity>() else {
            return false;
        };
        cat.is_tame()
            && !cat.is_ordered_to_sit()
            && !cat.is_lying()
            && self.move_to_block.can_use(mob)
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
            cat.set_lying(false);
        }
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        self.move_to_block.tick(mob);
        let Some(cat) = mob.downcast_ref::<CatEntity>() else {
            return;
        };

        cat.set_in_sitting_pose(false);
        cat.set_lying(self.move_to_block.is_reached_target());
    }
}
