//! Flower pot behavior.
//!
//! Vanilla parity: `FlowerPotBlock`. Thirty-nine blocks: one empty pot and
//! thirty-eight already holding something. Putting a plant in swaps the empty
//! pot for the matching full one, and taking it out swaps back -- there is no
//! block entity anywhere, which is why the potted variants exist as separate
//! blocks at all.
//!
//! Which plant each pot holds comes from the extracted `potted`, and the
//! reverse -- plant to pot -- is built once from the same data through
//! [`BlockBehavior::potted_content`], the way vanilla's constructors fill
//! `POTTED_BY_CONTENT`.

use std::sync::{Arc, LazyLock};

use rustc_hash::FxHashMap;
use steel_macros::block_behavior;
use steel_registry::blocks::BlockRef;
use steel_registry::item_stack::ItemStack;
use steel_registry::{REGISTRY, RegistryExt as _, vanilla_blocks, vanilla_game_events};
use steel_utils::types::InteractionHand;
use steel_utils::types::UpdateFlags;
use steel_utils::{BlockPos, BlockStateId, Identifier};

use crate::behavior::block::BlockBehavior;
use crate::behavior::context::{BlockHitResult, BlockPlaceContext, InteractionResult};
use crate::behavior::{BLOCK_BEHAVIORS, InventoryAccess};
use crate::player::Player;
use crate::world::World;
use crate::world::game_event::GameEventContext;

/// Which pot holds which plant.
///
/// Vanilla builds this in `FlowerPotBlock`'s constructor; Steel builds it once
/// from the behaviors, which carry the same extracted data.
static POTTED_BY_CONTENT: LazyLock<FxHashMap<Identifier, BlockRef>> = LazyLock::new(|| {
    REGISTRY
        .blocks
        .iter()
        .filter_map(|(_, block)| {
            let potted = BLOCK_BEHAVIORS.get_behavior(block).potted_content()?;
            Some((potted.key.clone(), block))
        })
        .collect()
});

/// Behavior for a flower pot, empty or full.
#[block_behavior]
pub struct FlowerPotBlock {
    block: BlockRef,
    /// What this pot holds, or air for the empty one.
    #[json_arg(vanilla_blocks, json = "potted")]
    potted: BlockRef,
}

impl FlowerPotBlock {
    /// Creates a flower pot behavior.
    #[must_use]
    pub const fn new(block: BlockRef, potted: BlockRef) -> Self {
        Self { block, potted }
    }

    /// Returns whether this is the empty pot.
    #[must_use]
    fn is_empty(&self) -> bool {
        self.potted == &vanilla_blocks::AIR
    }
}

impl BlockBehavior for FlowerPotBlock {
    fn get_state_for_placement(&self, _context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        Some(self.block.default_state())
    }

    /// Vanilla parity: `FlowerPotBlock.getPotted`, which is what lets the
    /// reverse map be built.
    fn potted_content(&self) -> Option<BlockRef> {
        (!self.is_empty()).then_some(self.potted)
    }

    /// Puts a plant in the pot.
    ///
    /// Vanilla parity: `FlowerPotBlock.useItemOn`. A pot that already holds
    /// something consumes the click rather than swapping, which is why you
    /// cannot replace a plant without taking the first one out.
    fn use_item_on(
        &self,
        _state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        player: &Player,
        _hand: InteractionHand,
        _hit_result: &BlockHitResult,
        inv: &mut InventoryAccess,
    ) -> InteractionResult {
        let held_key = inv.with_item(|item| item.item().key.clone());
        let Some(full_pot) = POTTED_BY_CONTENT.get(&held_key) else {
            return InteractionResult::TryEmptyHandInteraction;
        };

        if !self.is_empty() {
            return InteractionResult::Consume;
        }

        let planted = full_pot.default_state();
        world.set_block(pos, planted, UpdateFlags::UPDATE_ALL);
        world.game_event(
            &vanilla_game_events::BLOCK_CHANGE,
            pos,
            &GameEventContext::new(Some(player), Some(planted)),
        );

        if !player.has_infinite_materials() {
            inv.with_item(|item| item.shrink(1));
        }

        InteractionResult::Success
    }

    /// Takes the plant back out.
    ///
    /// Vanilla parity: `FlowerPotBlock.useWithoutItem`.
    fn use_without_item(
        &self,
        _state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        player: &Player,
        _hit_result: &BlockHitResult,
        _inv: &mut InventoryAccess,
    ) -> InteractionResult {
        if self.is_empty() {
            return InteractionResult::Consume;
        }

        let Some(plant_item) = REGISTRY.items.by_key(&self.potted.key) else {
            return InteractionResult::Consume;
        };
        player.add_item_or_drop(ItemStack::new(plant_item));

        let empty = vanilla_blocks::FLOWER_POT.default_state();
        world.set_block(pos, empty, UpdateFlags::UPDATE_ALL);
        world.game_event(
            &vanilla_game_events::BLOCK_CHANGE,
            pos,
            &GameEventContext::new(Some(player), Some(empty)),
        );

        InteractionResult::Success
    }

    /// Vanilla parity: `FlowerPotBlock.getCloneItemStack`, which picks the
    /// plant rather than the pot when the pot has something in it.
    fn get_clone_item_stack(
        &self,
        block: BlockRef,
        _state: BlockStateId,
        _include_data: bool,
    ) -> Option<ItemStack> {
        if self.is_empty() {
            return REGISTRY.items.by_key(&block.key).map(ItemStack::new);
        }
        REGISTRY.items.by_key(&self.potted.key).map(ItemStack::new)
    }
}

#[cfg(test)]
mod tests {
    use steel_registry::init_vanilla_registry;

    use super::*;
    use crate::behavior::init_behaviors;

    /// Every full pot knows its plant, and the empty one knows it holds none.
    #[test]
    fn a_pot_knows_what_it_holds() {
        init_vanilla_registry();

        let empty = FlowerPotBlock::new(&vanilla_blocks::FLOWER_POT, &vanilla_blocks::AIR);
        assert!(empty.is_empty());
        assert!(empty.potted_content().is_none());

        let cactus = FlowerPotBlock::new(&vanilla_blocks::POTTED_CACTUS, &vanilla_blocks::CACTUS);
        assert!(!cactus.is_empty());
        assert_eq!(
            cactus.potted_content().map(|block| &block.key),
            Some(&vanilla_blocks::CACTUS.key)
        );
    }

    /// The reverse map really covers the potted blocks.
    ///
    /// It is built from the behaviors rather than written out, so this is the
    /// check that the extracted `potted` is reaching them -- a pot that named
    /// nothing would simply refuse every plant, silently.
    #[test]
    fn the_plant_to_pot_map_is_built_from_the_behaviors() {
        init_vanilla_registry();
        init_behaviors();

        for (plant, pot) in [
            (&vanilla_blocks::CACTUS, &vanilla_blocks::POTTED_CACTUS),
            (
                &vanilla_blocks::OAK_SAPLING,
                &vanilla_blocks::POTTED_OAK_SAPLING,
            ),
            (
                &vanilla_blocks::DANDELION,
                &vanilla_blocks::POTTED_DANDELION,
            ),
        ] {
            let found = POTTED_BY_CONTENT
                .get(&plant.key)
                .unwrap_or_else(|| panic!("no pot holds {}", plant.key));
            assert_eq!(found.key, pot.key);
        }

        assert!(
            POTTED_BY_CONTENT.len() > 30,
            "only {} pots were found; the extracted `potted` is not reaching them",
            POTTED_BY_CONTENT.len()
        );
        assert!(
            !POTTED_BY_CONTENT.contains_key(&vanilla_blocks::AIR.key),
            "the empty pot must not claim to hold air"
        );
    }
}
