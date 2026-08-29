//! The item that puts an armor stand in the world.
//!
//! Vanilla parity: `ArmorStandItem`. The stand entity has been in the tree with
//! no way for a player to reach it -- without this item one can only be
//! summoned by a command, which is not something a player has.

use std::sync::Arc;

use foton_macros::item_behavior;
use foton_protocol::packets::game::SoundSource;
use foton_registry::blocks::properties::Direction;
use foton_registry::{sound_events, vanilla_entities, vanilla_game_events};
use foton_utils::angle::wrap_degrees;
use foton_utils::axis::Axis;
use foton_utils::{BlockPos, WorldAabb};
use glam::DVec3;

use crate::behavior::{InteractionResult, ItemBehavior, UseOnContext};
use crate::entity::entities::ArmorStandEntity;
use crate::entity::{Entity as _, next_entity_id};
use crate::physics::collision::CollisionWorld as _;
use crate::physics::{WorldCollisionProvider, collide, has_collision};
use crate::world::World;
use crate::world::game_event::GameEventContext;

/// Vanilla parity: the `0.75F` of the `playSound` in `useOn`.
const PLACE_VOLUME: f32 = 0.75;

/// Vanilla parity: the `0.8F` of the same call.
const PLACE_PITCH: f32 = 0.8;

/// The angle between the eight facings a placed stand can take.
///
/// Vanilla parity: the `45.0F` of `useOn`.
const FACING_STEP: f32 = 45.0;

/// How far the stand is allowed to drop onto its support.
///
/// Vanilla parity: the `-2.0` `EntityType.getYOffset` passes to `Shapes.collide`
/// when the spawn was moved up.
const MAX_DROP: f64 = 2.0;

/// Behavior for the armor stand item.
#[item_behavior]
pub struct ArmorStandItem;

impl ItemBehavior for ArmorStandItem {
    /// Vanilla parity: `ArmorStandItem.useOn`.
    ///
    /// A stand is nearly two blocks tall, so the whole of the placement check
    /// is whether that column is empty: vanilla asks for no block collision and
    /// no entity at all in the box the stand would occupy, and refuses rather
    /// than nudging the stand somewhere it does fit.
    ///
    /// Note that only a click on the underside is refused outright, not every
    /// click that is not on a top face -- a stand placed against a wall lands
    /// in the block beside it, which is what `BlockPlaceContext` resolves.
    fn use_on(&self, context: &mut UseOnContext) -> InteractionResult {
        if context.hit_result.direction == Direction::Down {
            return InteractionResult::Fail;
        }

        // Vanilla parity: the `new BlockPlaceContext(context)` of `useOn`. The
        // stand goes where a block from the same click would have gone, so
        // clicking tall grass replaces it rather than standing on top of it.
        let (place_pos, player_yaw) = {
            let placement = context.build_place_context();
            (placement.place_pos(), placement.rotation())
        };

        let dimensions = vanilla_entities::ARMOR_STAND.dimensions;
        let (center_x, floor_y, center_z) = place_pos.get_bottom_center();
        let stand_box = WorldAabb::entity_box(
            center_x,
            floor_y,
            center_z,
            f64::from(dimensions.half_width()),
            f64::from(dimensions.height),
        );

        // Vanilla parity: the `level.noCollision(null, box)` of `useOn`. Passing
        // no source entity is what makes vanilla skip the world border here, and
        // Foton's provider skips it for the same reason -- it only reports border
        // shapes for a moving entity.
        if has_collision(&WorldCollisionProvider::new(context.world), stand_box) {
            return InteractionResult::Fail;
        }

        // Vanilla parity: the `level.getEntities(null, box).isEmpty()` of
        // `useOn`, which goes through `EntitySelector.NO_SPECTATORS`. This is a
        // stricter test than the collision one above: a stand refuses to share
        // its square with a dropped item or another stand, neither of which
        // collides with anything.
        if context
            .world
            .has_entity_in_aabb_matching(&stand_box, |entity| !entity.is_spectator())
        {
            return InteractionResult::Fail;
        }

        let raised_box = stand_box.translate(DVec3::new(0.0, 1.0, 0.0));
        let stand_y = floor_y + resting_y_offset(context.world, place_pos, raised_box);
        let stand = Arc::new(ArmorStandEntity::new(
            &vanilla_entities::ARMOR_STAND,
            next_entity_id(),
            DVec3::new(center_x, stand_y, center_z),
            Arc::downgrade(context.world),
        ));

        // Vanilla parity: the `snapTo` of `useOn`, which overwrites the random
        // yaw `EntityType.create` rolled a moment earlier -- a placed stand is
        // never randomly turned, it faces the player.
        stand.set_rotation((placement_yaw(player_yaw), 0.0));

        // Not implemented: vanilla's `EntityType.createDefaultStackConfig` also
        // copies the stack's components and its `ENTITY_DATA` tag onto the new
        // stand. Foton has no `applyComponentsFromItemStack` for entities, so a
        // stand item that carries a custom name or a command-written pose
        // places a plain stand.
        let position = stand.position();
        if context.world.try_add_entity(stand).is_err() {
            return InteractionResult::Fail;
        }

        context.world.play_sound_at(
            &sound_events::ENTITY_ARMOR_STAND_PLACE,
            SoundSource::Blocks,
            position,
            PLACE_VOLUME,
            PLACE_PITCH,
            None,
        );
        context.world.game_event_at(
            &vanilla_game_events::ENTITY_PLACE,
            position,
            &GameEventContext::new(Some(context.player), None),
        );

        // Deviation: vanilla shrinks unconditionally and lets the creative
        // inventory put the stack back. Foton has no such restore, so the
        // creative case is skipped here, as it is for the item frame.
        if !context.player.has_infinite_materials() {
            context.inv.with_item(|item| item.shrink(1));
        }

        InteractionResult::Success
    }
}

/// Snaps a player yaw to the eight facings a placed stand can take.
///
/// Vanilla parity: the
/// `Mth.floor((Mth.wrapDegrees(context.getRotation() - 180.0F) + 22.5F) / 45.0F) * 45.0F`
/// of `ArmorStandItem.useOn`. The half turn is what makes the stand look back
/// at whoever placed it.
fn placement_yaw(player_yaw: f32) -> f32 {
    let steps = (wrap_degrees(player_yaw - 180.0) + FACING_STEP / 2.0) / FACING_STEP;
    steps.floor() * FACING_STEP
}

/// How far above the placement block's floor the stand comes to rest.
///
/// Vanilla parity: `EntityType.getYOffset`, reached through the
/// `EntityType.create(..., true, true)` of `useOn`. The stand is lifted a block
/// and dropped back onto whatever is under the placement position, which is
/// what leaves one placed on a slab standing on the slab rather than half a
/// block above it. With nothing at all below, the drop runs out at `MAX_DROP`
/// and the stand starts a block low -- vanilla does the same and lets gravity
/// finish.
fn resting_y_offset(world: &Arc<World>, place_pos: BlockPos, raised_box: WorldAabb) -> f64 {
    let x = f64::from(place_pos.x());
    let y = f64::from(place_pos.y());
    let z = f64::from(place_pos.z());
    // Vanilla parity: the `new AABB(spawnPos).expandTowards(0.0, -1.0, 0.0)`
    // that bounds the search, so only the placement block and the one under it
    // can catch the stand.
    let search = WorldAabb::new(x, y - 1.0, z, x + 1.0, y + 1.0, z + 1.0);

    let provider = WorldCollisionProvider::new(world);
    let mut shapes = provider.get_block_collisions(&search);
    shapes.extend(provider.get_entity_collisions(&search));

    1.0 + collide(Axis::Y, &raised_box, &shapes, -MAX_DROP)
}

#[cfg(test)]
mod tests {
    use foton_registry::item_stack::ItemStack;
    use foton_registry::items::item::BlockHitResult;
    use foton_registry::{init_vanilla_registry, vanilla_blocks, vanilla_items};
    use foton_utils::ChunkPos;
    use foton_utils::types::{InteractionHand, UpdateFlags};

    use super::*;
    use crate::bootstrap::init_globals_once;
    use crate::player::Player;
    use crate::test_support::{TestPlayerBuilder, fresh_test_world, insert_ready_full_chunk};

    /// The block whose top face every test clicks.
    const GROUND: BlockPos = BlockPos::new(8, 64, 8);

    /// Where a stand placed on `GROUND` belongs.
    const STAND_FLOOR: f64 = 65.0;

    const TEST_PLAYER_ENTITY_ID: i32 = 1;

    /// A world with one solid block to click on and the chunk around it loaded.
    fn ground_world(key: &'static str) -> Arc<World> {
        init_vanilla_registry();
        init_globals_once();
        let world = fresh_test_world(key);
        insert_ready_full_chunk(&world, ChunkPos::new(0, 0));
        assert!(world.set_block(
            GROUND,
            vanilla_blocks::STONE.default_state(),
            UpdateFlags::UPDATE_NONE,
        ));
        world
    }

    /// A survival player holding a stack of two stands, so a shrink shows.
    fn stand_holder(world: &Arc<World>) -> Arc<Player> {
        let player =
            TestPlayerBuilder::new(Arc::clone(world), "StandTester", TEST_PLAYER_ENTITY_ID).build();
        let mut stack = ItemStack::new(&vanilla_items::ARMOR_STAND);
        stack.count = 2;
        player
            .inventory
            .lock()
            .set_item_in_hand(InteractionHand::MainHand, stack);
        player
    }

    fn hit(pos: BlockPos, face: Direction) -> BlockHitResult {
        let (step_x, step_y, step_z) = face.offset();
        BlockHitResult {
            location: DVec3::new(
                f64::from(pos.x()) + 0.5 + f64::from(step_x) * 0.5,
                f64::from(pos.y()) + 0.5 + f64::from(step_y) * 0.5,
                f64::from(pos.z()) + 0.5 + f64::from(step_z) * 0.5,
            ),
            direction: face,
            block_pos: pos,
            miss: false,
            inside: false,
            world_border_hit: false,
        }
    }

    /// Every stand the world holds, wherever the click put it.
    fn stands(world: &Arc<World>) -> Vec<DVec3> {
        let everywhere = WorldAabb::new(0.0, 0.0, 0.0, 16.0, 128.0, 16.0);
        world
            .get_entities_in_aabb_matching(&everywhere, |entity| {
                entity.entity_type().key == vanilla_entities::ARMOR_STAND.key
            })
            .iter()
            .map(|entity| entity.position())
            .collect()
    }

    fn click(
        world: &Arc<World>,
        player: &Arc<Player>,
        hit_result: BlockHitResult,
    ) -> (InteractionResult, i32) {
        let mut context = UseOnContext::new(
            player,
            InteractionHand::MainHand,
            hit_result,
            world,
            player.inventory.clone(),
        );
        let result = ArmorStandItem.use_on(&mut context);
        let left = context.inv.with_item(|item| item.count);
        (result, left)
    }

    /// The whole point of the item: a click on something solid leaves a stand
    /// standing on it, one block up from the block that was clicked, and takes
    /// one stand out of the stack.
    #[test]
    fn a_click_on_a_block_top_leaves_a_stand_on_the_block() {
        let world = ground_world("armor_stand_item_place");
        let player = stand_holder(&world);

        let (result, left) = click(&world, &player, hit(GROUND, Direction::Up));

        assert_eq!(result, InteractionResult::Success);
        assert_eq!(left, 1, "placing a stand should take one from the stack");

        let placed = stands(&world);
        assert_eq!(placed.len(), 1, "exactly one stand should have been placed");
        assert!(
            (placed[0].y - STAND_FLOOR).abs() < 1.0e-9,
            "the stand should rest on the clicked block, not float: {}",
            placed[0].y
        );
        assert!(
            (placed[0].x - 8.5).abs() < 1.0e-9 && (placed[0].z - 8.5).abs() < 1.0e-9,
            "the stand should stand in the middle of its block: {placed:?}"
        );
    }

    /// Vanilla parity: the `clickedFace == Direction.DOWN` guard. A stand has
    /// no way to hang, so a click on a ceiling places nothing at all.
    #[test]
    fn a_click_on_the_underside_of_a_block_places_nothing() {
        let world = ground_world("armor_stand_item_ceiling");
        let player = stand_holder(&world);

        let (result, left) = click(&world, &player, hit(GROUND, Direction::Down));

        assert_eq!(result, InteractionResult::Fail);
        assert_eq!(left, 2, "a refused placement should cost nothing");
        assert!(stands(&world).is_empty());
    }

    /// A stand is nearly two blocks tall, so one block of headroom is not
    /// enough. This is the check that catches a stand placed under a low
    /// ceiling, which vanilla refuses.
    #[test]
    fn a_stand_needs_two_blocks_of_room_above_the_block() {
        let world = ground_world("armor_stand_item_no_headroom");
        let player = stand_holder(&world);
        assert!(world.set_block(
            BlockPos::new(GROUND.x(), GROUND.y() + 2, GROUND.z()),
            vanilla_blocks::STONE.default_state(),
            UpdateFlags::UPDATE_NONE,
        ));

        let (result, left) = click(&world, &player, hit(GROUND, Direction::Up));

        assert_eq!(result, InteractionResult::Fail);
        assert_eq!(left, 2, "a refused placement should cost nothing");
        assert!(stands(&world).is_empty());
    }

    /// The block the stand would stand in being taken is the simpler half of
    /// the same rule.
    #[test]
    fn a_blocked_square_takes_no_stand() {
        let world = ground_world("armor_stand_item_blocked");
        let player = stand_holder(&world);
        assert!(world.set_block(
            BlockPos::new(GROUND.x(), GROUND.y() + 1, GROUND.z()),
            vanilla_blocks::STONE.default_state(),
            UpdateFlags::UPDATE_NONE,
        ));

        let (result, left) = click(&world, &player, hit(GROUND, Direction::Up));

        assert_eq!(result, InteractionResult::Fail);
        assert_eq!(left, 2, "a refused placement should cost nothing");
        assert!(stands(&world).is_empty());
    }

    /// Compares two yaws as directions: vanilla's arithmetic can land on -180
    /// or 180 for the same facing, and either is right.
    fn assert_facing(actual: f32, expected: f32) {
        let difference = (actual - expected).rem_euclid(360.0);
        assert!(
            difference < 1.0e-4 || (360.0 - difference) < 1.0e-4,
            "expected the stand to face {expected}, got {actual}"
        );
    }

    /// Vanilla parity: the yaw arithmetic of `useOn`. Eight facings, each one
    /// turned around from the player, so a stand placed by somebody looking
    /// north looks south at them.
    #[test]
    fn the_stand_faces_back_at_the_player_in_eight_steps() {
        assert_facing(placement_yaw(0.0), 180.0);
        assert_facing(placement_yaw(180.0), 0.0);
        assert_facing(placement_yaw(90.0), 270.0);
        assert_facing(placement_yaw(270.0), 90.0);

        // A yaw a little either side of a facing still snaps onto it.
        assert_facing(placement_yaw(20.0), 180.0);
        assert_facing(placement_yaw(-20.0), 180.0);

        // And every yaw lands on one of the eight, never between them.
        for degrees in -360_i16..=360 {
            let yaw = placement_yaw(f32::from(degrees));
            assert!(
                (yaw / FACING_STEP).fract().abs() < 1.0e-4,
                "a yaw of {degrees} gave {yaw}, which is not one of the eight facings"
            );
        }
    }
}
