//! Vanilla `BottleItem` -- the empty glass bottle.
//!
//! Two of vanilla's three ways to fill a bottle are unreachable in Steel today:
//!
//! - The dragon-breath branch of `BottleItem.use` collects area effect clouds
//!   whose `getOwner()` is an `EnderDragon`. Steel has an area effect cloud
//!   entity but no ender dragon (see `dev/parity-gaps.txt`) and no owner on the
//!   cloud, so the filter can never match and the branch is omitted rather than
//!   approximated.
//! - Filling from a water cauldron is not this class's job in vanilla either: it
//!   lives in the `CauldronInteraction.WATER` map, which Steel has not ported
//!   (`blocks/building/cauldron_block.rs` says as much). Nothing here can stand
//!   in for it.
//!
//! What is implemented is the branch that is reachable: clipping against a water
//! source and turning the bottle into a water bottle.

use steel_macros::item_behavior;
use steel_protocol::packets::game::SoundSource;
use steel_registry::blocks::block_state_ext::BlockStateExt;
use steel_registry::data_components::PotionContents;
use steel_registry::data_components::vanilla_components::POTION_CONTENTS;
use steel_registry::fluid::FluidStateExt;
use steel_registry::item_stack::ItemStack;
use steel_registry::{
    RegistryReference, sound_events, vanilla_game_events, vanilla_items, vanilla_potions,
};

use crate::behavior::item_utils::{create_filled_result, player_pov_hit_source_fluid};
use crate::behavior::{InteractionResult, ItemBehavior, UseItemContext};
use crate::entity::Entity;
use crate::world::game_event::GameEventContext;

/// Behavior for the empty glass bottle.
#[item_behavior]
pub struct BottleItem;

impl ItemBehavior for BottleItem {
    fn use_item(&self, context: &mut UseItemContext) -> InteractionResult {
        let Some(pos) = player_pov_hit_source_fluid(context) else {
            return InteractionResult::Pass;
        };

        if !context.world.may_interact(context.player, pos) {
            return InteractionResult::Pass;
        }

        if !context
            .world
            .get_block_state(pos)
            .get_fluid_state()
            .is_water()
        {
            return InteractionResult::Pass;
        }

        // Vanilla `level.playSound(player, ..)` excludes the acting player, who
        // already played the sound locally.
        context.world.play_sound_at(
            &sound_events::ITEM_BOTTLE_FILL,
            SoundSource::Neutral,
            context.player.position(),
            1.0,
            1.0,
            Some(context.player.id()),
        );
        context.world.game_event(
            &vanilla_game_events::FLUID_PICKUP,
            pos,
            &GameEventContext::new(Some(context.player), None),
        );

        // Vanilla `BottleItem.turnBottleIntoItem`, which is
        // `ItemUtils.createFilledResult` plus a statistic.
        // TODO: Award Stats.ITEM_USED once Steel has a statistics foundation.
        create_filled_result(context, water_bottle(), true);
        InteractionResult::Success
    }
}

/// Builds vanilla `PotionContents.createItemStack(Items.POTION, Potions.WATER)`.
fn water_bottle() -> ItemStack {
    let mut stack = ItemStack::new(&vanilla_items::POTION);
    stack.set(
        POTION_CONTENTS,
        PotionContents::new(
            Some(RegistryReference::new(&vanilla_potions::WATER)),
            None,
            Vec::new(),
            None,
        ),
    );
    stack
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use glam::DVec3;
    use steel_registry::blocks::BlockRef;
    use steel_registry::vanilla_blocks;
    use steel_utils::types::{InteractionHand, UpdateFlags};
    use steel_utils::{BlockPos, ChunkPos};

    use super::*;
    use crate::bootstrap::init_globals_once;
    use crate::player::Player;
    use crate::test_support::{TestPlayerBuilder, fresh_test_world, insert_ready_full_chunk};

    const FLUID_POS: BlockPos = BlockPos::new(8, 64, 8);

    /// Stands a bottle-holding player two blocks above `fluid` and points them
    /// straight down at it, so `use_item` clips the way the client would.
    fn use_bottle_over(key: &'static str, fluid: BlockRef) -> Arc<Player> {
        init_globals_once();
        let world = fresh_test_world(key);
        insert_ready_full_chunk(&world, ChunkPos::from_block_pos(FLUID_POS));
        world.set_block(FLUID_POS, fluid.default_state(), UpdateFlags::UPDATE_NONE);

        let player = TestPlayerBuilder::new(Arc::clone(&world), "BottleDipper", 1).build();
        player.base().set_position_local(DVec3::new(8.5, 66.0, 8.5));
        player.set_rotation((0.0, 90.0));
        player
            .inventory
            .lock()
            .set_selected_item(ItemStack::new(&vanilla_items::GLASS_BOTTLE));

        let mut context = UseItemContext::new(
            &player,
            InteractionHand::MainHand,
            &world,
            player.inventory.clone(),
        );
        let expected = if fluid == &vanilla_blocks::WATER {
            InteractionResult::Success
        } else {
            InteractionResult::Pass
        };
        assert_eq!(BottleItem.use_item(&mut context), expected);

        drop(context);
        player
    }

    #[test]
    fn dipping_a_glass_bottle_in_water_fills_it_with_a_water_potion() {
        let player = use_bottle_over("bottle_fills_from_water", &vanilla_blocks::WATER);

        let inventory = player.inventory.lock();
        let filled = inventory.get_selected_item();
        assert!(filled.is(&vanilla_items::POTION));
        assert!(
            filled
                .get(POTION_CONTENTS)
                .is_some_and(|contents| contents.is(&vanilla_potions::WATER)),
            "a bottle filled from water holds the water potion, not empty contents"
        );
    }

    #[test]
    fn a_glass_bottle_cannot_be_filled_from_lava() {
        let player = use_bottle_over("bottle_ignores_lava", &vanilla_blocks::LAVA);

        assert!(
            player
                .inventory
                .lock()
                .get_selected_item()
                .is(&vanilla_items::GLASS_BOTTLE)
        );
    }
}
