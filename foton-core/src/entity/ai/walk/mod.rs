//! Vanilla walk path-type classification.

mod collision;
mod evaluator;
mod fly_evaluator;
pub mod node_evaluator;
mod path_evaluator;
mod settings;
mod swim_evaluator;

pub use collision::WalkNodeCollision;
pub use evaluator::NodeEvaluator;
pub use fly_evaluator::FlyNodeEvaluator;
use fly_evaluator::fly_path_type;
pub use node_evaluator::WalkNodeEvaluator;
pub use path_evaluator::WalkPathEvaluator;
use path_evaluator::does_block_have_partial_collision;
pub use settings::MobPathSettings;
pub use swim_evaluator::SwimNodeEvaluator;

use foton_math::fast_floor;
use foton_registry::blocks::block_state_ext::BlockStateExt as _;
use foton_registry::blocks::properties::BlockStateProperties;
use foton_registry::fluid::FluidState;
use foton_registry::vanilla_block_tags::BlockTag;
use foton_registry::vanilla_blocks;
use foton_utils::{BlockPos, Direction, WorldAabb, axis::Axis};

use crate::behavior::{BLOCK_BEHAVIORS, BlockCollisionContext, BlockStateBehaviorExt as _};
use crate::entity::Mob;
use crate::entity::ai::node::{Node, NodeStore};
use crate::entity::ai::path::{
    PathComputationType, PathType, PathTypeSet, PathfindingContext, PathfindingMalus,
};
use crate::fluid::FluidStateExt as _;
use crate::world::LevelReader;

#[cfg(test)]
mod tests;
