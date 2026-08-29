//! Vanilla `SelectableSlotContainer`: the grid a player clicks individual
//! slots on, shared by the chiseled bookshelf and the shelf.

use foton_registry::blocks::properties::Direction;
use glam::DVec3;

use crate::behavior::context::BlockHitResult;

/// One block edge in pixels, the unit vanilla divides a face into sections with.
pub(super) const PIXELS_PER_BLOCK_EDGE: f32 = 16.0;

/// Vanilla parity: `SelectableSlotContainer.getHitSlot`.
///
/// Returns the slot a click landed on, reading the face the block presents as a
/// `rows` by `columns` grid. A click on any other face selects nothing.
#[must_use]
pub(super) fn hit_slot(
    hit_result: &BlockHitResult,
    block_facing: Direction,
    rows: usize,
    columns: usize,
) -> Option<usize> {
    let (x, y) = relative_hit_coordinates_for_block_face(hit_result, block_facing)?;
    let row = section(1.0 - y, rows);
    let column = section(x, columns);
    Some(column + row * columns)
}

/// Vanilla parity: `SelectableSlotContainer.getRelativeHitCoordinatesForBlockFace`.
fn relative_hit_coordinates_for_block_face(
    hit_result: &BlockHitResult,
    block_facing: Direction,
) -> Option<(f32, f32)> {
    let hit_direction = hit_result.direction;
    if block_facing != hit_direction {
        return None;
    }

    let hit_face_origin = hit_direction.relative(hit_result.block_pos);
    let relative_hit = hit_result.location
        - DVec3::new(
            f64::from(hit_face_origin.x()),
            f64::from(hit_face_origin.y()),
            f64::from(hit_face_origin.z()),
        );

    let x = match hit_direction {
        Direction::North => 1.0 - relative_hit.x,
        Direction::South => relative_hit.x,
        Direction::West => relative_hit.z,
        Direction::East => 1.0 - relative_hit.z,
        Direction::Down | Direction::Up => return None,
    };

    Some((x as f32, relative_hit.y as f32))
}

/// Vanilla parity: `SelectableSlotContainer.getSection`.
fn section(relative_coordinate: f32, max_sections: usize) -> usize {
    let targeted_pixel = relative_coordinate * PIXELS_PER_BLOCK_EDGE;
    let section_size = PIXELS_PER_BLOCK_EDGE / max_sections as f32;
    let index = (targeted_pixel / section_size).floor() as i32;
    index.clamp(0, max_sections as i32 - 1) as usize
}
