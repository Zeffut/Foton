//! Shared item-frame entity behavior.

use foton_utils::Direction;

use super::Entity;

/// Vanilla `ItemFrame` behavior used by comparators.
pub trait ItemFrame: Entity {
    /// Returns the direction from the backing block toward the frame.
    fn direction(&self) -> Direction;

    /// Returns vanilla `ItemFrame.getAnalogOutput`.
    fn analog_output(&self) -> i32;
}
