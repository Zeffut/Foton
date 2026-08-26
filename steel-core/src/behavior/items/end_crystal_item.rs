//! The end crystal item.

use std::sync::Arc;

use glam::DVec3;
use steel_macros::item_behavior;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::{vanilla_blocks, vanilla_entities, vanilla_game_events};
use steel_utils::BlockPos;

use crate::behavior::{InteractionResult, ItemBehavior, UseOnContext};
use crate::entity::entities::EndCrystalEntity;
use crate::entity::next_entity_id;
use crate::world::game_event::GameEventContext;
use crate::world::{LevelReader as _, World};
use steel_utils::WorldAabb;

/// Places an end crystal on obsidian or bedrock.
///
/// Vanilla parity: `EndCrystalItem`. Placing one in the End also asks the
/// [fight](crate::dimension::end::EnderDragonFight) whether the four crystals
/// of the respawn ritual are now standing, which is the ritual's only trigger.
#[item_behavior]
pub struct EndCrystalItem;

impl ItemBehavior for EndCrystalItem {
    /// Vanilla parity: `EndCrystalItem.useOn`.
    fn use_on(&self, context: &mut UseOnContext) -> InteractionResult {
        let pos = context.hit_result.block_pos;
        let state = context.world.get_block_state(pos);
        let block = state.get_block();
        if block != &vanilla_blocks::OBSIDIAN && block != &vanilla_blocks::BEDROCK {
            return InteractionResult::Fail;
        }

        let above = pos.above();
        if !context.world.get_block_state(above).is_air() {
            return InteractionResult::Fail;
        }

        if !crystal_space_is_free(context.world, above) {
            return InteractionResult::Fail;
        }

        let position = DVec3::new(
            f64::from(above.x()) + 0.5,
            f64::from(above.y()),
            f64::from(above.z()) + 0.5,
        );
        let crystal = Arc::new(EndCrystalEntity::new(
            &vanilla_entities::END_CRYSTAL,
            next_entity_id(),
            position,
            Arc::downgrade(context.world),
        ));
        crystal.set_show_bottom(false);
        if let Err(error) = context.world.try_add_entity(crystal) {
            log::warn!("Failed to place end crystal: {error}");
            return InteractionResult::Fail;
        }

        context.world.game_event(
            &vanilla_game_events::ENTITY_PLACE,
            above,
            &GameEventContext::new(Some(context.player), None),
        );
        if let Some(fight) = context.world.dragon_fight() {
            fight.try_respawn(context.world);
        }
        context.inv.with_item(|item| item.shrink(1));

        InteractionResult::Success
    }
}

/// Vanilla parity: the `level.getEntities(null, new AABB(x, y, z, x+1, y+2, z+1))`
/// emptiness test -- an end crystal needs two clear blocks.
fn crystal_space_is_free(world: &Arc<World>, above: BlockPos) -> bool {
    let x = f64::from(above.x());
    let y = f64::from(above.y());
    let z = f64::from(above.z());
    let bounds = WorldAabb::new(x, y, z, x + 1.0, y + 2.0, z + 1.0);
    world.get_entities_in_aabb(&bounds).is_empty()
}
