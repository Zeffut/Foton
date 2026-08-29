//! The lead item.
//!
//! Vanilla parity: `LeadItem`. The lead has two halves: tying a mob to the
//! player, which lives on the mob interaction path (`Entity.interact`, ported
//! in `crate::entity::Entity::interact_entity`), and moving that end of the
//! rope from the player onto a fence, which is what this file does.

use std::sync::Arc;

use foton_macros::item_behavior;
use foton_registry::blocks::block_state_ext::BlockStateExt as _;
use foton_registry::vanilla_block_tags::BlockTag;
use foton_registry::vanilla_game_events;
use foton_utils::{BlockPos, Downcast as _};
use glam::DVec3;

use crate::behavior::{InteractionResult, ItemBehavior, UseOnContext};
use crate::entity::entities::LeashFenceKnotEntity;
use crate::entity::{Entity, RemovalReason, leashables_leashed_to_holder_at};
use crate::player::Player;
use crate::world::World;
use crate::world::game_event::GameEventContext;

/// Vanilla lead behavior.
#[item_behavior]
pub struct LeadItem;

impl ItemBehavior for LeadItem {
    /// Vanilla parity: `LeadItem.useOn`.
    ///
    /// In normal play the fence answers the click first: `FenceBlock`'s
    /// `use_without_item` calls the same [`bind_player_mobs`]. This path is the
    /// one a sneaking player takes, because sneaking with a full hand
    /// suppresses the block interaction and leaves only the item's `use_on`.
    fn use_on(&self, context: &mut UseOnContext) -> InteractionResult {
        let pos = context.hit_result.block_pos;
        if !context
            .world
            .get_block_state(pos)
            .get_block()
            .has_tag(&BlockTag::FENCES)
        {
            return InteractionResult::Pass;
        }

        bind_player_mobs(context.player, context.world, pos)
    }
}

/// Ties every mob the player is currently leading to the fence at `pos`.
///
/// Vanilla parity: `LeadItem.bindPlayerMobs`. Note that neither vanilla nor
/// this port consumes the stack: the lead item was already spent when the mob
/// was tied to the player, and this only moves the holder end of an existing
/// rope onto the knot.
pub fn bind_player_mobs(player: &Player, world: &Arc<World>, pos: BlockPos) -> InteractionResult {
    let entities_to_leash =
        leashables_leashed_to_holder_at(world.as_ref(), fence_center(pos), player.id());
    if entities_to_leash.is_empty() {
        return InteractionResult::Pass;
    }

    let existing_knot = LeashFenceKnotEntity::get_knot(world.as_ref(), pos);
    let reused_existing_knot = existing_knot.is_some();
    let Some(active_knot) = existing_knot.or_else(|| LeashFenceKnotEntity::create_knot(world, pos))
    else {
        // Deviation: vanilla's `createKnot` cannot fail. Foton's spawn refuses
        // when the fence sits in a chunk that holds no entities yet, and there
        // is nothing to tie the mobs to in that case.
        return InteractionResult::Pass;
    };

    let mut any_leashed = false;
    for leashable in entities_to_leash {
        let Some(mob) = leashable.as_mob() else {
            continue;
        };
        if mob.can_have_a_leash_attached_to(active_knot.as_ref()) {
            mob.set_leashed_to(&active_knot);
            any_leashed = true;
        }
    }

    if any_leashed {
        if let Some(knot) = active_knot.downcast_ref::<LeashFenceKnotEntity>() {
            knot.play_placement_sound();
        }
        world.game_event(
            &vanilla_game_events::BLOCK_ATTACH,
            pos,
            &GameEventContext::new(Some(player), None),
        );
        return InteractionResult::SuccessServer;
    }

    // Every candidate refused the knot -- they were all further away than their
    // snap distance. A knot spawned for nothing is taken back out again.
    if !reused_existing_knot {
        active_knot.set_removed(RemovalReason::Discarded);
    }

    InteractionResult::Pass
}

/// Vanilla parity: `Vec3.atCenterOf`, the scan origin `bindPlayerMobs` uses.
fn fence_center(pos: BlockPos) -> DVec3 {
    DVec3::new(
        f64::from(pos.x()) + 0.5,
        f64::from(pos.y()) + 0.5,
        f64::from(pos.z()) + 0.5,
    )
}

#[cfg(test)]
mod tests {
    use foton_registry::blocks::properties::Direction;
    use foton_registry::item_stack::ItemStack;
    use foton_registry::{vanilla_blocks, vanilla_entities, vanilla_items};
    use foton_utils::types::{InteractionHand, UpdateFlags};
    use foton_utils::{ChunkPos, WorldAabb};

    use crate::behavior::{BlockHitResult, init_behaviors};
    use crate::entity::entities::PigEntity;
    use crate::entity::{SharedEntity, next_entity_id};
    use crate::test_support::{TestPlayerBuilder, fresh_test_world, insert_ready_full_chunk};

    use super::*;

    const FENCE_POS: BlockPos = BlockPos::new(8, 64, 8);
    /// Far enough above the fence to be inside the 32 block leash scan but
    /// outside the 12 block snap distance.
    const HIGH_FENCE_POS: BlockPos = BlockPos::new(8, 100, 8);

    fn fence_world(key: &'static str, fence_pos: BlockPos) -> Arc<World> {
        let world = fresh_test_world(key);
        init_behaviors();
        insert_ready_full_chunk(&world, ChunkPos::from_block_pos(fence_pos));
        assert!(world.set_block(
            fence_pos,
            vanilla_blocks::OAK_FENCE.default_state(),
            UpdateFlags::UPDATE_ALL
        ));
        world
    }

    fn add_pig(world: &Arc<World>, position: DVec3) -> SharedEntity {
        let pig: SharedEntity = Arc::new(PigEntity::new(
            &vanilla_entities::PIG,
            next_entity_id(),
            position,
            Arc::downgrade(world),
        ));
        world
            .try_add_entity(Arc::clone(&pig))
            .expect("pig should attach to the loaded test chunk");
        pig
    }

    fn live_knots(world: &Arc<World>, pos: BlockPos) -> Vec<SharedEntity> {
        let search_box = WorldAabb::new(
            f64::from(pos.x()) - 1.0,
            f64::from(pos.y()) - 1.0,
            f64::from(pos.z()) - 1.0,
            f64::from(pos.x()) + 1.0,
            f64::from(pos.y()) + 1.0,
            f64::from(pos.z()) + 1.0,
        );
        world.get_entities_in_aabb_matching(&search_box, |entity| {
            entity.downcast_ref::<LeashFenceKnotEntity>().is_some() && !entity.is_removed()
        })
    }

    fn knot_holder_pos(pig: &SharedEntity) -> BlockPos {
        let mob = pig.as_mob().expect("pig should expose mob behavior");
        let holder = mob.leash_holder().expect("pig should stay leashed");
        holder
            .downcast_ref::<LeashFenceKnotEntity>()
            .expect("pig should now be held by a fence knot")
            .block_pos()
    }

    #[test]
    fn tying_a_led_pig_to_a_fence_moves_it_onto_a_freshly_spawned_knot() {
        let world = fence_world("lead_binds_to_new_knot", FENCE_POS);
        let player = TestPlayerBuilder::new(Arc::clone(&world), "Leader", next_entity_id()).build();
        let player_entity: SharedEntity = Arc::clone(&player) as SharedEntity;
        let pig = add_pig(&world, DVec3::new(8.0, 64.0, 7.0));
        pig.as_mob()
            .expect("pig should expose mob behavior")
            .set_leashed_to(&player_entity);

        let result = bind_player_mobs(player.as_ref(), &world, FENCE_POS);

        assert_eq!(result, InteractionResult::SuccessServer);
        assert_eq!(knot_holder_pos(&pig), FENCE_POS);
        assert_eq!(live_knots(&world, FENCE_POS).len(), 1);
    }

    #[test]
    fn tying_a_second_pig_to_the_same_fence_reuses_the_knot_already_there() {
        let world = fence_world("lead_reuses_existing_knot", FENCE_POS);
        let player = TestPlayerBuilder::new(Arc::clone(&world), "Leader", next_entity_id()).build();
        let player_entity: SharedEntity = Arc::clone(&player) as SharedEntity;
        let existing_knot = LeashFenceKnotEntity::create_knot(&world, FENCE_POS)
            .expect("test knot should spawn into the loaded chunk");
        let pig = add_pig(&world, DVec3::new(8.0, 64.0, 7.0));
        pig.as_mob()
            .expect("pig should expose mob behavior")
            .set_leashed_to(&player_entity);

        let result = bind_player_mobs(player.as_ref(), &world, FENCE_POS);

        assert_eq!(result, InteractionResult::SuccessServer);
        let mob = pig.as_mob().expect("pig should expose mob behavior");
        let holder = mob.leash_holder().expect("pig should stay leashed");
        assert_eq!(holder.id(), existing_knot.id());
        assert_eq!(live_knots(&world, FENCE_POS).len(), 1);
    }

    #[test]
    fn a_fence_click_with_nothing_in_tow_ties_nothing_and_spawns_no_knot() {
        let world = fence_world("lead_without_led_mobs", FENCE_POS);
        let player = TestPlayerBuilder::new(Arc::clone(&world), "Leader", next_entity_id()).build();
        // Leashed to nobody: the scan finds the pig but rejects it.
        add_pig(&world, DVec3::new(8.0, 64.0, 7.0));

        let result = bind_player_mobs(player.as_ref(), &world, FENCE_POS);

        assert_eq!(result, InteractionResult::Pass);
        assert!(live_knots(&world, FENCE_POS).is_empty());
    }

    #[test]
    fn a_pig_led_from_beyond_the_snap_distance_leaves_no_knot_behind() {
        let world = fence_world("lead_out_of_snap_range", HIGH_FENCE_POS);
        let player = TestPlayerBuilder::new(Arc::clone(&world), "Leader", next_entity_id()).build();
        let player_entity: SharedEntity = Arc::clone(&player) as SharedEntity;
        let pig = add_pig(&world, DVec3::new(8.0, 86.0, 8.0));
        let mob = pig.as_mob().expect("pig should expose mob behavior");
        mob.set_leashed_to(&player_entity);

        let result = bind_player_mobs(player.as_ref(), &world, HIGH_FENCE_POS);

        assert_eq!(result, InteractionResult::Pass);
        let holder = mob
            .leash_holder()
            .expect("pig should stay led by the player");
        assert_eq!(holder.id(), player.id());
        assert!(live_knots(&world, HIGH_FENCE_POS).is_empty());
    }

    #[test]
    fn using_the_lead_on_a_block_that_is_not_a_fence_does_nothing() {
        let world = fence_world("lead_on_non_fence", FENCE_POS);
        assert!(world.set_block(
            FENCE_POS,
            vanilla_blocks::STONE.default_state(),
            UpdateFlags::UPDATE_ALL
        ));
        let player = TestPlayerBuilder::new(Arc::clone(&world), "Leader", next_entity_id()).build();
        let player_entity: SharedEntity = Arc::clone(&player) as SharedEntity;
        let pig = add_pig(&world, DVec3::new(8.0, 64.0, 7.0));
        let mob = pig.as_mob().expect("pig should expose mob behavior");
        mob.set_leashed_to(&player_entity);
        player
            .inventory
            .lock()
            .set_selected_item(ItemStack::new(&vanilla_items::LEAD));

        let result = LeadItem.use_on(&mut use_on_context(&world, &player));

        assert_eq!(result, InteractionResult::Pass);
        let holder = mob
            .leash_holder()
            .expect("pig should stay led by the player");
        assert_eq!(holder.id(), player.id());
    }

    /// Vanilla parity: `LeadItem.useOn` never touches the stack. The lead was
    /// already spent when the mob was tied to the player.
    #[test]
    fn tying_mobs_to_a_fence_does_not_spend_the_lead_in_hand() {
        let world = fence_world("lead_is_not_consumed", FENCE_POS);
        let player = TestPlayerBuilder::new(Arc::clone(&world), "Leader", next_entity_id()).build();
        let player_entity: SharedEntity = Arc::clone(&player) as SharedEntity;
        let pig = add_pig(&world, DVec3::new(8.0, 64.0, 7.0));
        pig.as_mob()
            .expect("pig should expose mob behavior")
            .set_leashed_to(&player_entity);
        player
            .inventory
            .lock()
            .set_selected_item(ItemStack::new(&vanilla_items::LEAD));

        let result = LeadItem.use_on(&mut use_on_context(&world, &player));

        assert_eq!(result, InteractionResult::SuccessServer);
        assert_eq!(knot_holder_pos(&pig), FENCE_POS);
        assert_eq!(player.inventory.lock().get_selected_item_mut().count, 1);
    }

    fn use_on_context<'a>(world: &'a Arc<World>, player: &'a Arc<Player>) -> UseOnContext<'a> {
        UseOnContext::new(
            player.as_ref(),
            InteractionHand::MainHand,
            BlockHitResult {
                location: DVec3::new(8.5, 65.0, 8.5),
                direction: Direction::Up,
                block_pos: FENCE_POS,
                miss: false,
                inside: false,
                world_border_hit: false,
            },
            world,
            Arc::clone(&player.inventory),
        )
    }
}
