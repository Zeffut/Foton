//! Helpers shared by item behavior implementations.

use std::sync::Arc;

use glam::DVec3;
use steel_registry::blocks::block_state_ext::BlockStateExt;
use steel_registry::blocks::properties::{Axis, Direction};
use steel_registry::item_stack::ItemStack;
use steel_registry::vanilla_blocks;
use steel_utils::BlockPos;

use crate::behavior::UseItemContext;
use crate::entity::Entity;
use crate::entity::entities::ItemEntity;
use crate::inventory::lock::ContainerId;
use crate::player::player_inventory::PlayerInventory;
use crate::world::{RaytraceAction, World};

/// Spills a destroyed container item's contents where it died.
///
/// Vanilla parity: `ItemUtils.onContainerDestroyed`.
pub(crate) fn on_container_destroyed(
    entity: &ItemEntity,
    contents: impl IntoIterator<Item = ItemStack>,
) {
    let Some(world) = entity.level() else {
        return;
    };
    let position = entity.position();
    for stack in contents {
        world.spawn_item(position, stack);
    }
}

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

/// Vanilla parity: the `0.0172275` of `DefaultDispenseItemBehavior.spawnItem`.
const ACCURACY_DEVIATION: f64 = 0.017_227_5;

/// Vanilla parity: the `-= 0.125` a vertical throw drops by.
const VERTICAL_SPAWN_DROP: f64 = 0.125;

/// Vanilla parity: the `-= 0.15625` a horizontal throw drops by.
const HORIZONTAL_SPAWN_DROP: f64 = 0.156_25;

/// Vanilla parity: `RandomSource.triangle`.
fn triangle(mode: f64, deviation: f64) -> f64 {
    deviation.mul_add(rand::random::<f64>() - rand::random::<f64>(), mode)
}

/// Throws one item out of a block face.
///
/// Vanilla parity: `DefaultDispenseItemBehavior.spawnItem`. A dispenser throws
/// with accuracy six; a trial spawner and a vault both eject with accuracy two,
/// which is what makes their rewards land in a tidy pile.
pub(crate) fn spawn_item_toward(
    world: &Arc<World>,
    position: DVec3,
    direction: Direction,
    accuracy: i32,
    stack: ItemStack,
) {
    let mut position = position;
    position.y -= if direction.get_axis() == Axis::Y {
        VERTICAL_SPAWN_DROP
    } else {
        HORIZONTAL_SPAWN_DROP
    };

    let (step_x, _, step_z) = direction.offset();
    let power = rand::random::<f64>().mul_add(0.1, 0.2);
    let deviation = ACCURACY_DEVIATION * f64::from(accuracy);
    let velocity = DVec3::new(
        triangle(f64::from(step_x) * power, deviation),
        triangle(0.2, deviation),
        triangle(f64::from(step_z) * power, deviation),
    );

    world.spawn_item_with_velocity(position, stack, velocity);
}
