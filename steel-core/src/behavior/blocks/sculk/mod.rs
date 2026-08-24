//! The sculk block family.
//!
//! `SculkVeinBlock` lives with the other multiface blocks in `vegetation`, because that is
//! where the multiface machinery it inherits from is. The charge-spreading algorithm the
//! catalyst and the deep dark share lives in `spreader`.

mod sculk_block;
mod sculk_catalyst_block;
mod sculk_sensor_block;
mod sculk_shrieker_block;
mod spreader;

pub use sculk_block::SculkBlock;
pub use sculk_catalyst_block::SculkCatalystBlock;
pub use sculk_sensor_block::{
    CalibratedSculkSensorBlock, SculkSensorBlock, can_activate_sculk_sensor,
    deactivate_sculk_sensor, sculk_sensor_phase, try_resonate_vibration,
};
pub use sculk_shrieker_block::SculkShriekerBlock;
pub use spreader::{ChargeCursor, SculkBehaviorKind, SculkSpreader, behavior_of};
