use std::sync::Arc;

use foton_macros::item_behavior;
use foton_registry::item_stack::ItemStack;
use foton_registry::{
    REGISTRY, blocks::block_state_ext::BlockStateExt, level_events, vanilla_game_events,
};
use foton_utils::types::UpdateFlags;

use crate::{
    behavior::{
        InteractionResult, ItemBehavior, SignApplicator, UseOnContext,
        waxables::get_waxed_from_normal_variant,
    },
    block_entity::{
        BlockEntity as _,
        entities::{SignBlockEntity, SignText},
    },
    entity::Entity,
    player::Player,
    world::{World, game_event::GameEventContext},
};

use super::copper_chest_events::emit_connected_chest_block_change;

/// Behavior for the honeycomb item. Waxes copper blocks and signs.
#[item_behavior]
pub struct HoneycombItem;

impl ItemBehavior for HoneycombItem {
    fn use_on(&self, context: &mut UseOnContext) -> InteractionResult {
        let pos = context.hit_result.block_pos;

        let old_block_state = context.world.get_block_state(pos);
        let Some(waxed_block) = get_waxed_from_normal_variant(old_block_state.get_block()) else {
            // Vanilla parity: `HoneycombItem.useOn` passes on anything that is
            // not waxable. Signs are waxed through the `SignApplicator` path,
            // which `SignBlock.useItemOn` reaches before this ever runs.
            return InteractionResult::Pass;
        };

        context.inv.with_item(|item| item.shrink(1));
        // TODO: trigger CriteriaTriggers.ITEM_USED_ON_BLOCK advancement
        let waxed_state = REGISTRY
            .blocks
            .copy_matching_properties(old_block_state, waxed_block);
        context
            .world
            .set_block(pos, waxed_state, UpdateFlags::UPDATE_ALL);
        context.world.game_event(
            &vanilla_game_events::BLOCK_CHANGE,
            pos,
            &GameEventContext::new(Some(context.player), Some(waxed_state)),
        );
        context.world.level_event(
            level_events::PARTICLES_AND_SOUND_WAX_ON,
            pos,
            0,
            Some(context.player.id()),
        );
        emit_connected_chest_block_change(
            context.world,
            pos,
            old_block_state,
            context.player,
            Some(level_events::PARTICLES_AND_SOUND_WAX_ON),
        );
        InteractionResult::Success
    }

    fn as_sign_applicator(&self) -> Option<&dyn SignApplicator> {
        Some(self)
    }
}

impl SignApplicator for HoneycombItem {
    /// Vanilla parity: `HoneycombItem.tryApplyToSign`. The level event excludes
    /// nobody, unlike the copper path above, because vanilla passes a null
    /// player here.
    fn try_apply_to_sign(
        &self,
        world: &Arc<World>,
        sign: &SignBlockEntity,
        _is_front_text: bool,
        _stack: &ItemStack,
        _player: &Player,
    ) -> bool {
        if !sign.wax() {
            return false;
        }
        world.level_event(
            level_events::PARTICLES_AND_SOUND_WAX_ON,
            sign.get_block_pos(),
            0,
            None,
        );
        true
    }

    /// Vanilla parity: `HoneycombItem.canApplyToSign`, which overrides the
    /// default so a blank sign can be waxed too.
    fn can_apply_to_sign(&self, _text: &SignText, _stack: &ItemStack, _player: &Player) -> bool {
        true
    }
}
