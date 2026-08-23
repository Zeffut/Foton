mod base_rail_block;
mod detector_rail_block;
mod powered_rail_block;
mod rail_block;
mod rail_state;

use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::blocks::properties::{BlockStateProperties, EnumProperty, RailShape};
use steel_utils::BlockStateId;

use base_rail_block::BaseRailBlock;

/// Which way a rail runs, or `None` if `state` is not a rail.
///
/// Vanilla parity: the `state.getValue(((BaseRailBlock)state.getBlock()).getShapeProperty())`
/// that every minecart starts from. A straight-only rail stores its shape in a
/// property with six values rather than ten, but both are called `shape` and
/// both hold a `RailShape`, so reading it by name works for either.
#[must_use]
pub fn rail_shape_at(state: BlockStateId) -> Option<RailShape> {
    const RAIL_SHAPE: &EnumProperty<RailShape> = &BlockStateProperties::RAIL_SHAPE;

    if !BaseRailBlock::is_rail_state(state) {
        return None;
    }
    Some(state.get_value(RAIL_SHAPE))
}

pub use detector_rail_block::DetectorRailBlock;
pub use powered_rail_block::PoweredRailBlock;
pub use rail_block::RailBlock;
