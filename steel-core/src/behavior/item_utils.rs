//! Helpers shared by item behavior implementations.

use steel_registry::blocks::block_state_ext::BlockStateExt;
use steel_registry::item_stack::ItemStack;
use steel_registry::vanilla_blocks;
use steel_utils::BlockPos;

use crate::behavior::UseItemContext;
use crate::inventory::lock::ContainerId;
use crate::player::player_inventory::PlayerInventory;
use crate::world::RaytraceAction;

/// Applies vanilla `Item.getPlayerPOVHitResult(level, player,
/// ClipContext.Fluid.SOURCE_ONLY)`.
///
/// Returns the clipped block position, or `None` for vanilla's
/// `HitResult.Type.MISS`. Flowing fluid is transparent to this clip; a source is
/// an immediate hit, which is also what makes a waterlogged block fill a bucket
/// or a bottle.
pub(crate) fn player_pov_hit_source_fluid(context: &UseItemContext<'_>) -> Option<BlockPos> {
    let (start, end) = context.player.get_ray_endpoints();
    let (hit_block, _) = context.world.raytrace(start, end, |pos, world| {
        let state = world.get_block_state(pos);
        if state.get_block() == &vanilla_blocks::AIR {
            return RaytraceAction::Pass;
        }

        let fluid_state = state.get_fluid_state();
        if fluid_state.is_source() {
            return RaytraceAction::ImmediateHit;
        }
        if !fluid_state.is_empty() {
            return RaytraceAction::Pass;
        }

        RaytraceAction::CheckShape
    });

    hit_block
}

/// Applies vanilla `ItemUtils.createFilledResult`.
pub(crate) fn create_filled_result(
    context: &UseItemContext,
    result_stack: ItemStack,
    limit_creative_stack_size: bool,
) {
    let player = context.player;
    let overflow = context.inv.with_guard(|guard| {
        let inv_id = ContainerId::from_arc(&player.inventory);
        let Some(inv) = guard.get_typed_mut::<PlayerInventory>(inv_id) else {
            return result_stack;
        };

        inv.apply_filled_result(
            context.hand,
            result_stack,
            player.has_infinite_materials(),
            limit_creative_stack_size,
        )
    });

    if !overflow.is_empty() {
        let _ = player.drop_item(overflow, false, false);
    }
}
