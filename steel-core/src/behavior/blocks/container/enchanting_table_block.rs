//! Enchanting table block behavior.
//!
//! Vanilla parity: `EnchantingTableBlock`.

use std::sync::Arc;

use steel_macros::block_behavior;
use steel_registry::blocks::BlockRef;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::vanilla_block_tags::BlockTag;
use steel_utils::{BlockPos, BlockStateId, translations};
use text_components::TextComponent;

use crate::behavior::InventoryAccess;
use crate::behavior::block::BlockBehavior;
use crate::behavior::context::{BlockHitResult, BlockPlaceContext, InteractionResult};
use crate::inventory::menu::kinds::enchantment;
use crate::player::Player;
use crate::world::{LevelReader as _, World};

/// Where a table looks for the shelves that power it.
///
/// Vanilla parity: `EnchantingTableBlock.BOOKSHELF_OFFSETS`, which is every
/// position in a five-by-five ring, two blocks out and one or two blocks up.
/// The ring is what a player builds; the corners count too, which is why
/// fifteen shelves fit around one table.
static BOOKSHELF_OFFSETS: [(i32, i32, i32); 16] = [
    (-2, 0, -2),
    (-2, 0, -1),
    (-2, 0, 0),
    (-2, 0, 1),
    (-2, 0, 2),
    (-1, 0, -2),
    (-1, 0, 2),
    (0, 0, -2),
    (0, 0, 2),
    (1, 0, -2),
    (1, 0, 2),
    (2, 0, -2),
    (2, 0, -1),
    (2, 0, 0),
    (2, 0, 1),
    (2, 0, 2),
];

/// Returns how much enchanting power surrounds the table at `pos`.
///
/// Vanilla parity: the `BOOKSHELF_OFFSETS` walk of `EnchantmentMenu.slotsChanged`
/// over `EnchantingTableBlock.isValidBookShelf`. A shelf only counts when the
/// block halfway between it and the table is clear, which is why walling a
/// table in with the shelves outside stops them counting.
#[must_use]
pub fn count_enchanting_power(world: &Arc<World>, pos: BlockPos) -> i32 {
    let mut power = 0;
    for &(dx, dy, dz) in &BOOKSHELF_OFFSETS {
        for level in [dy, dy + 1] {
            if is_valid_bookshelf(world, pos, (dx, level, dz)) {
                power += 1;
            }
        }
    }
    power
}

/// Returns whether the shelf at this offset reaches the table.
///
/// Vanilla parity: `EnchantingTableBlock.isValidBookShelf`.
fn is_valid_bookshelf(world: &Arc<World>, pos: BlockPos, offset: (i32, i32, i32)) -> bool {
    let (dx, dy, dz) = offset;
    let shelf = pos.offset(dx, dy, dz);
    if !world
        .get_block_state(shelf)
        .get_block()
        .has_tag(&BlockTag::ENCHANTMENT_POWER_PROVIDER)
    {
        return false;
    }

    // Vanilla halves the horizontal offset to find the block between, and keeps
    // the vertical one, so a shelf two out is blocked by whatever sits one out.
    let between = pos.offset(dx / 2, dy, dz / 2);
    world
        .get_block_state(between)
        .get_block()
        .has_tag(&BlockTag::ENCHANTMENT_POWER_TRANSMITTER)
}

/// Behavior for the enchanting table block.
#[block_behavior]
pub struct EnchantingTableBlock {
    _block: BlockRef,
}

impl EnchantingTableBlock {
    /// Creates the behavior for this block.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { _block: block }
    }
}

impl BlockBehavior for EnchantingTableBlock {
    /// An enchanting table has no placement state to choose.
    fn get_state_for_placement(&self, _context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        None
    }

    /// Vanilla parity: `EnchantingTableBlock.useWithoutItem`.
    fn use_without_item(
        &self,
        _state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        player: &Player,
        _hit_result: &BlockHitResult,
        _inv: &mut InventoryAccess,
    ) -> InteractionResult {
        let inventory = player.inventory.clone();
        let world = Arc::clone(world);
        player.open_menu(
            TextComponent::translated(translations::CONTAINER_ENCHANT.msg()),
            move |context| enchantment(inventory, context.container_id, pos, &world),
        );
        InteractionResult::Success
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ring_has_the_positions_vanilla_walks() {
        // Vanilla filters a five-by-five-by-two box down to the positions whose
        // x or z is exactly two out; that is sixteen columns, each counted at
        // two heights, for the thirty-two checks that yield at most fifteen
        // shelves in practice.
        assert_eq!(BOOKSHELF_OFFSETS.len(), 16);
        for &(dx, _, dz) in &BOOKSHELF_OFFSETS {
            assert!(
                dx.abs() == 2 || dz.abs() == 2,
                "({dx}, {dz}) is not on the ring"
            );
            assert!(dx.abs() <= 2 && dz.abs() <= 2);
        }
    }

    #[test]
    fn no_offset_is_listed_twice() {
        let mut seen: Vec<(i32, i32, i32)> = BOOKSHELF_OFFSETS.to_vec();
        let before = seen.len();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(before, seen.len());
    }
}
