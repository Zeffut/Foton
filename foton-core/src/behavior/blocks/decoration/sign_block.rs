//! Sign block behavior implementation.
//!
//! Handles sign placement and block entity creation for all sign types.

use std::cmp::Ordering;
use std::f64::consts::PI;
use std::sync::{Arc, Weak};

use foton_macros::block_behavior;
use foton_registry::REGISTRY;
use foton_registry::block_entity_type::BlockEntityTypeRef;
use foton_registry::blocks::BlockRef;
use foton_registry::blocks::block_state_ext::BlockStateExt;
use foton_registry::blocks::properties::{
    BlockStateProperties, BoolProperty, Direction, EnumProperty, IntProperty,
};
use foton_registry::blocks::shapes::SupportType;
use foton_registry::{vanilla_block_entity_types, vanilla_blocks, vanilla_game_events};
use foton_utils::types::InteractionHand;
use foton_utils::{BlockPos, BlockStateId, Downcast as _};

use crate::behavior::block::{
    BlockBehavior, BlockEntityCreation, schedule_water_tick_if_waterlogged,
};
use crate::behavior::context::{BlockHitResult, BlockPlaceContext, InteractionResult};
use crate::behavior::{ITEM_BEHAVIORS, InventoryAccess};
use crate::block_entity::{BlockEntity as _, BlockEntityTicker, entities::SignBlockEntity};
use crate::entity::Entity;
use crate::player::Player;
use crate::world::game_event::GameEventContext;
use crate::world::{LevelReader, ScheduledTickAccess, World};

/// Converts a rotation in degrees to a 16-segment rotation value (0-15).
///
/// This is equivalent to vanilla's `RotationSegment.convertToSegment(float)`.
/// Each segment is 22.5 degrees, and rotation is measured clockwise from south.
fn convert_to_rotation_segment(degrees: f32) -> u8 {
    // Normalize to 0-360
    let normalized = degrees.rem_euclid(360.0);
    // Convert to segment (each segment is 22.5 degrees)
    // Round to nearest segment
    (((normalized / 22.5) + 0.5) as u8) & 15
}

/// Gets the nearest looking directions from the player's rotation.
///
/// Returns horizontal directions in order of how closely they match the player's look direction.
fn get_nearest_looking_directions(rotation: f32, clicked_face: Direction) -> Vec<Direction> {
    // Build list of directions in order of preference
    // Start with the opposite of the clicked face (most natural for wall signs)
    // Then add directions based on player facing
    let mut directions = Vec::with_capacity(4);

    // Add horizontal directions in order of how closely they match player's look
    let all_horizontal = [
        Direction::North,
        Direction::East,
        Direction::South,
        Direction::West,
    ];

    // Calculate angle for each direction and sort by distance to player's rotation
    let mut scored: Vec<(Direction, f32)> = all_horizontal
        .iter()
        .map(|&dir| {
            let dir_angle = dir.to_yaw();
            let diff = (rotation - dir_angle + 180.0).rem_euclid(360.0) - 180.0;
            (dir, diff.abs())
        })
        .collect();

    scored.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(Ordering::Equal));

    for (dir, _) in scored {
        directions.push(dir);
    }

    // If clicked face is horizontal, prefer placing on the opposite side
    if clicked_face.is_horizontal() {
        let opposite = clicked_face.opposite();
        if let Some(pos) = directions.iter().position(|&d| d == opposite) {
            directions.remove(pos);
            directions.insert(0, opposite);
        }
    }

    directions
}

/// Calculates whether the player is facing the front of a sign.
///
/// Uses the sign's rotation (from block state) and the player's position
/// relative to the sign to determine which side they're looking at.
fn is_facing_front_text(state: BlockStateId, pos: BlockPos, player: &Player) -> bool {
    // Get the sign's Y rotation in degrees from the block state
    let sign_y_rot = get_sign_rotation_degrees(state);

    // Calculate player's angle relative to the sign center
    let player_pos = player.position();
    let dx = player_pos.x - (f64::from(pos.0.x) + 0.5);
    let dz = player_pos.z - (f64::from(pos.0.z) + 0.5);

    // Calculate angle from sign to player (in degrees, -90 to account for Minecraft's coordinate system)
    let player_angle = (dz.atan2(dx) * 180.0 / PI) as f32 - 90.0;

    // Front text if the angle difference is <= 90 degrees
    let diff = (sign_y_rot - player_angle + 180.0).rem_euclid(360.0) - 180.0;
    diff.abs() <= 90.0
}

/// Gets the Y rotation of a sign in degrees from its block state.
fn get_sign_rotation_degrees(state: BlockStateId) -> f32 {
    // Standing signs use "rotation" property (0-15, each step is 22.5 degrees)
    if let Some(rotation) = state.try_get_value(ROTATION_16) {
        return f32::from(rotation) * 22.5;
    }

    // Wall signs use "facing" property
    if let Some(facing) = state.try_get_value(HORIZONTAL_FACING) {
        return facing.to_yaw();
    }

    0.0
}

/// Checks if a block state can support a standing sign.
///
/// Vanilla uses `isSolid()` which checks if the collision shape is a full cube.
/// This means signs cannot be placed on other signs, fences, walls, etc.
fn can_support_standing_sign(world: &dyn LevelReader, pos: BlockPos) -> bool {
    let below_pos = BlockPos::new(pos.x(), pos.y() - 1, pos.z());
    let below_state = world.get_block_state(below_pos);
    below_state.is_solid()
}

/// Checks if a wall sign can survive at the given position with the given facing.
///
/// Vanilla uses `isSolid()` which allows wall signs to be placed on other signs
/// (since signs have `forceSolidOn`).
fn can_wall_sign_survive(world: &dyn LevelReader, pos: BlockPos, facing: Direction) -> bool {
    // Wall sign needs a solid block behind it
    let behind_pos = facing.opposite().relative(pos);
    let behind_state = world.get_block_state(behind_pos);
    behind_state.is_solid()
}

/// Checks if a ceiling hanging sign can survive at the given position.
fn can_ceiling_hanging_sign_survive(world: &dyn LevelReader, pos: BlockPos) -> bool {
    let above_pos = BlockPos::new(pos.x(), pos.y() + 1, pos.z());
    let above_state = world.get_block_state(above_pos);
    world.is_face_sturdy_for(above_state, above_pos, Direction::Down, SupportType::Center)
}

/// Checks if a wall hanging sign can attach to a neighboring block.
///
/// Vanilla's `WallHangingSignBlock.canAttachTo` checks:
/// 1. If the neighbor is a wall hanging sign on the same axis, allow attachment
/// 2. Otherwise, check if the face is sturdy with FULL support type
fn can_attach_to(
    world: &dyn LevelReader,
    sign_facing: Direction,
    attach_pos: BlockPos,
    attach_face: Direction,
) -> bool {
    let attach_state = world.get_block_state(attach_pos);
    let attach_block = REGISTRY.blocks.by_state_id(attach_state);

    // Check if it's another wall hanging sign (vanilla uses BlockTags.WALL_HANGING_SIGNS)
    if let Some(block) = attach_block
        && block.key.path.contains("wall_hanging_sign")
    {
        // Wall hanging signs can chain if they're on the same axis
        if let Some(neighbor_facing) = attach_state.try_get_value(HORIZONTAL_FACING) {
            return neighbor_facing.axis() == sign_facing.axis();
        }
    }

    // Otherwise, check for sturdy face with FULL support
    world.is_face_sturdy_for(attach_state, attach_pos, attach_face, SupportType::Full)
}

/// Checks if a wall hanging sign can survive at the given position.
///
/// Wall hanging signs need support on at least one side perpendicular to facing.
/// This matches vanilla's `WallHangingSignBlock.canPlace`.
fn can_wall_hanging_sign_survive(
    world: &dyn LevelReader,
    pos: BlockPos,
    facing: Direction,
) -> bool {
    let clockwise = facing.rotate_y_clockwise();
    let counter_clockwise = facing.rotate_y_counter_clockwise();

    let can_attach_clockwise = {
        let attach_pos = clockwise.relative(pos);
        can_attach_to(world, facing, attach_pos, counter_clockwise)
    };

    let can_attach_counter = {
        let attach_pos = counter_clockwise.relative(pos);
        can_attach_to(world, facing, attach_pos, clockwise)
    };

    can_attach_clockwise || can_attach_counter
}

/// Lets the held item change the sign, when the item is a sign applicator.
///
/// Vanilla parity: `SignBlock.useItemOn`. The item itself decides what changes
/// and plays its own sound; this is the half that guards the sign -- waxed, in
/// use by someone else, blank -- and pays for the change.
///
/// Deviations, each a system Foton does not have yet:
/// - vanilla calls `SignBlockEntity.executeClickCommandsIfPresent` on success.
///   Foton's sign text carries no click events, so there is nothing to run.
/// - vanilla awards `Stats.ITEM_USED`. Foton has no statistics registry.
/// - vanilla's client-side arm has no counterpart; Foton is server only.
fn try_apply_sign_applicator(
    state: BlockStateId,
    world: &Arc<World>,
    pos: BlockPos,
    player: &Player,
    inv: &mut InventoryAccess,
) -> InteractionResult {
    let Some(block_entity) = world.get_block_entity(pos) else {
        return InteractionResult::Pass;
    };
    let Some(sign) = block_entity.downcast_ref::<SignBlockEntity>() else {
        return InteractionResult::Pass;
    };

    // Cloned rather than borrowed: the applicator only reads the stack, and
    // holding the inventory lock across it would outlive its purpose.
    let stack = inv.with_item(|item| item.clone());
    let behavior = ITEM_BEHAVIORS.get_behavior(stack.item());
    let Some(applicator) = behavior.as_sign_applicator() else {
        return InteractionResult::TryEmptyHandInteraction;
    };

    let may_build = player.abilities.lock().may_build;
    if !may_build || sign.is_waxed() || sign.is_other_player_editing(player.gameprofile.id) {
        return InteractionResult::TryEmptyHandInteraction;
    }

    let is_front_text = is_facing_front_text(state, pos, player);
    if !applicator.can_apply_to_sign(&sign.get_text(is_front_text), &stack, player)
        || !applicator.try_apply_to_sign(world, sign, is_front_text, &stack, player)
    {
        return InteractionResult::TryEmptyHandInteraction;
    }

    world.game_event(
        &vanilla_game_events::BLOCK_CHANGE,
        pos,
        &GameEventContext::new(Some(player), Some(sign.get_block_state())),
    );
    // Vanilla parity: `ItemStack.consume(1, player)`, which spares creative.
    if !player.has_infinite_materials() {
        inv.with_item(|item| item.shrink(1));
    }

    InteractionResult::Success
}

/// Attempts to open the sign editor for a player.
///
/// Checks all conditions required by vanilla:
/// 1. Block entity exists and is a sign
/// 2. Sign is not waxed
/// 3. No other player is currently editing
/// 4. Player has build permission (`may_build`)
///
/// Returns `Success` if the editor was opened, `Pass` otherwise.
fn try_open_sign_editor(
    state: BlockStateId,
    world: &Arc<World>,
    pos: BlockPos,
    player: &Player,
) -> InteractionResult {
    // Get the block entity
    let Some(block_entity) = world.get_block_entity(pos) else {
        return InteractionResult::Pass;
    };

    let Some(sign) = block_entity.downcast_ref::<SignBlockEntity>() else {
        return InteractionResult::Pass;
    };

    // Check 1: Is the sign waxed?
    if sign.is_waxed() {
        // TODO: Play waxed sign interaction fail sound
        return InteractionResult::Success; // Vanilla returns SUCCESS even when waxed
    }

    // Check 2: Is another player editing?
    if sign.is_other_player_editing(player.gameprofile.id) {
        return InteractionResult::Pass;
    }

    // Check 3: Player must have build permission.
    //
    // Vanilla parity: the `player.mayBuild()` in `SignBlock.useWithoutItem`.
    // This function's own doc list has always named the check; it just was not
    // performed, so an adventure-mode player could open the editor on any sign.
    // The waxed path two functions up already reads the ability the same way.
    if !player.abilities.lock().may_build {
        return InteractionResult::Pass;
    }

    // Determine which side the player is facing
    let is_front_text = is_facing_front_text(state, pos, player);

    // Set the editing player lock
    sign.set_player_who_may_edit(Some(player.gameprofile.id));

    // Open the editor
    player.open_sign_editor(pos, is_front_text);
    InteractionResult::Success
}

/// Behavior for standing sign blocks (placed on ground).
#[block_behavior]
pub struct StandingSignBlock {
    block: BlockRef,
}

const ATTACHED: &BoolProperty = &BlockStateProperties::ATTACHED;
const HORIZONTAL_FACING: &EnumProperty<Direction> = &BlockStateProperties::HORIZONTAL_FACING;
const ROTATION_16: &IntProperty = &BlockStateProperties::ROTATION_16;

impl StandingSignBlock {
    /// Creates a new standing sign block behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }
}

impl BlockBehavior for StandingSignBlock {
    fn is_possible_to_respawn_in_this(&self, _state: BlockStateId) -> bool {
        true
    }

    fn update_shape(
        &self,
        state: BlockStateId,
        world: &dyn ScheduledTickAccess,
        pos: BlockPos,
        direction: Direction,
        _neighbor_pos: BlockPos,
        _neighbor_state: BlockStateId,
    ) -> BlockStateId {
        // Standing signs break when the block below is removed
        if direction == Direction::Down && !can_support_standing_sign(world, pos) {
            return REGISTRY.blocks.get_default_state_id(&vanilla_blocks::AIR);
        }
        schedule_water_tick_if_waterlogged(state, world, pos);
        state
    }

    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        // Check if we can place on the block below
        if !can_support_standing_sign(context.world, context.place_pos()) {
            return None;
        }

        // Calculate rotation from player's yaw
        // Vanilla: RotationSegment.convertToSegment(context.getRotation() + 180.0F)
        let rotation = convert_to_rotation_segment(context.rotation() + 180.0);

        Some(self.block.default_state().set_value(ROTATION_16, rotation))
    }

    fn new_block_entity(
        &self,
        level: Weak<World>,
        pos: BlockPos,
        state: BlockStateId,
    ) -> BlockEntityCreation {
        BlockEntityCreation::Created(Arc::new(SignBlockEntity::new(level, pos, state)))
    }

    fn get_block_entity_ticker(
        &self,
        _world: &Arc<World>,
        _state: BlockStateId,
        block_entity_type: BlockEntityTypeRef,
    ) -> Option<BlockEntityTicker> {
        BlockEntityTicker::for_matching_entity_tick(
            block_entity_type,
            &vanilla_block_entity_types::SIGN,
        )
    }

    fn use_item_on(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        player: &Player,
        _hand: InteractionHand,
        _hit_result: &BlockHitResult,
        inv: &mut InventoryAccess,
    ) -> InteractionResult {
        try_apply_sign_applicator(state, world, pos, player, inv)
    }

    fn use_without_item(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        player: &Player,
        _hit_result: &BlockHitResult,
        _inv: &mut InventoryAccess,
    ) -> InteractionResult {
        try_open_sign_editor(state, world, pos, player)
    }
}

/// Behavior for wall sign blocks (attached to walls).
#[block_behavior]
pub struct WallSignBlock {
    block: BlockRef,
}

impl WallSignBlock {
    /// Creates a new wall sign block behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }
}

impl BlockBehavior for WallSignBlock {
    fn is_possible_to_respawn_in_this(&self, _state: BlockStateId) -> bool {
        true
    }

    fn update_shape(
        &self,
        state: BlockStateId,
        world: &dyn ScheduledTickAccess,
        pos: BlockPos,
        direction: Direction,
        _neighbor_pos: BlockPos,
        _neighbor_state: BlockStateId,
    ) -> BlockStateId {
        // Wall signs break when the block they're attached to is removed
        // The sign is attached to the block opposite of its facing direction
        if let Some(facing) = state.try_get_value(HORIZONTAL_FACING)
            && direction.opposite() == facing
            && !can_wall_sign_survive(world, pos, facing)
        {
            return REGISTRY.blocks.get_default_state_id(&vanilla_blocks::AIR);
        }
        schedule_water_tick_if_waterlogged(state, world, pos);
        state
    }

    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        // Try each horizontal direction based on player's look direction
        let directions = get_nearest_looking_directions(context.rotation(), context.clicked_face());

        for direction in directions {
            // The sign faces the opposite direction of where it's attached
            let facing = direction.opposite();

            // Check if sign can survive with this facing
            if can_wall_sign_survive(context.world, context.place_pos(), facing) {
                return Some(
                    self.block
                        .default_state()
                        .set_value(HORIZONTAL_FACING, facing),
                );
            }
        }

        // No valid placement found
        None
    }

    fn new_block_entity(
        &self,
        level: Weak<World>,
        pos: BlockPos,
        state: BlockStateId,
    ) -> BlockEntityCreation {
        BlockEntityCreation::Created(Arc::new(SignBlockEntity::new(level, pos, state)))
    }

    fn get_block_entity_ticker(
        &self,
        _world: &Arc<World>,
        _state: BlockStateId,
        block_entity_type: BlockEntityTypeRef,
    ) -> Option<BlockEntityTicker> {
        BlockEntityTicker::for_matching_entity_tick(
            block_entity_type,
            &vanilla_block_entity_types::SIGN,
        )
    }

    fn use_item_on(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        player: &Player,
        _hand: InteractionHand,
        _hit_result: &BlockHitResult,
        inv: &mut InventoryAccess,
    ) -> InteractionResult {
        try_apply_sign_applicator(state, world, pos, player, inv)
    }

    fn use_without_item(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        player: &Player,
        _hit_result: &BlockHitResult,
        _inv: &mut InventoryAccess,
    ) -> InteractionResult {
        try_open_sign_editor(state, world, pos, player)
    }
}

/// Behavior for ceiling hanging sign blocks.
#[block_behavior]
pub struct CeilingHangingSignBlock {
    block: BlockRef,
}

impl CeilingHangingSignBlock {
    /// Creates a new ceiling hanging sign block behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }
}

impl BlockBehavior for CeilingHangingSignBlock {
    fn is_possible_to_respawn_in_this(&self, _state: BlockStateId) -> bool {
        true
    }

    fn update_shape(
        &self,
        state: BlockStateId,
        world: &dyn ScheduledTickAccess,
        pos: BlockPos,
        direction: Direction,
        _neighbor_pos: BlockPos,
        _neighbor_state: BlockStateId,
    ) -> BlockStateId {
        // Ceiling hanging signs break when the block above is removed
        if direction == Direction::Up && !can_ceiling_hanging_sign_survive(world, pos) {
            return REGISTRY.blocks.get_default_state_id(&vanilla_blocks::AIR);
        }
        schedule_water_tick_if_waterlogged(state, world, pos);
        state
    }

    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        // Check if we can hang from the block above
        if !can_ceiling_hanging_sign_survive(context.world, context.place_pos()) {
            return None;
        }

        let above_pos = BlockPos::new(
            context.place_pos().x(),
            context.place_pos().y() + 1,
            context.place_pos().z(),
        );
        let above_state = context.world.get_block_state(above_pos);

        // Determine if we should attach to the middle or not based on block above
        let direction = Direction::from_yaw(context.rotation());
        let is_above_full = context.world.is_face_sturdy_for(
            above_state,
            above_pos,
            Direction::Down,
            SupportType::Full,
        );

        // Check if block above is also a hanging sign
        let above_block = REGISTRY.blocks.by_state_id(above_state);
        let is_below_hanging_sign =
            above_block.is_some_and(|b| b.key.path.contains("hanging_sign"));

        // Determine if attached to middle based on vanilla logic
        let attached_to_middle = if is_below_hanging_sign {
            // When below another hanging sign, check if we can chain
            if let Some(above_facing) = above_state.try_get_value(HORIZONTAL_FACING) {
                // Wall hanging sign above - check axis alignment
                above_facing.axis() != direction.axis()
            } else if let Some(above_rotation) = above_state.try_get_value(ROTATION_16) {
                // Ceiling hanging sign above - check if we can align
                let above_direction = rotation_to_direction(above_rotation);
                above_direction.is_none_or(|d| d.axis() != direction.axis())
            } else {
                !is_above_full
            }
        } else {
            !is_above_full
        };

        // Calculate rotation
        let rotation = if attached_to_middle {
            // Attached to middle - use player rotation
            convert_to_rotation_segment(context.rotation() + 180.0)
        } else {
            // Attached to chains - align with direction
            convert_to_rotation_segment(direction.opposite().to_yaw())
        };

        Some(
            self.block
                .default_state()
                .set_value(ROTATION_16, rotation)
                .set_value(ATTACHED, attached_to_middle),
        )
    }

    fn new_block_entity(
        &self,
        level: Weak<World>,
        pos: BlockPos,
        state: BlockStateId,
    ) -> BlockEntityCreation {
        BlockEntityCreation::Created(Arc::new(SignBlockEntity::new_hanging(level, pos, state)))
    }

    fn get_block_entity_ticker(
        &self,
        _world: &Arc<World>,
        _state: BlockStateId,
        block_entity_type: BlockEntityTypeRef,
    ) -> Option<BlockEntityTicker> {
        BlockEntityTicker::for_matching_entity_tick(
            block_entity_type,
            &vanilla_block_entity_types::HANGING_SIGN,
        )
    }

    fn use_item_on(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        player: &Player,
        _hand: InteractionHand,
        _hit_result: &BlockHitResult,
        inv: &mut InventoryAccess,
    ) -> InteractionResult {
        try_apply_sign_applicator(state, world, pos, player, inv)
    }

    fn use_without_item(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        player: &Player,
        _hit_result: &BlockHitResult,
        _inv: &mut InventoryAccess,
    ) -> InteractionResult {
        try_open_sign_editor(state, world, pos, player)
    }
}

/// Converts a rotation segment (0-15) to a cardinal direction, if applicable.
const fn rotation_to_direction(rotation: u8) -> Option<Direction> {
    match rotation {
        0 => Some(Direction::South),
        4 => Some(Direction::West),
        8 => Some(Direction::North),
        12 => Some(Direction::East),
        _ => None,
    }
}

/// Behavior for wall hanging sign blocks.
#[block_behavior]
pub struct WallHangingSignBlock {
    block: BlockRef,
}

impl WallHangingSignBlock {
    /// Creates a new wall hanging sign block behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }
}

impl BlockBehavior for WallHangingSignBlock {
    fn is_possible_to_respawn_in_this(&self, _state: BlockStateId) -> bool {
        true
    }

    fn update_shape(
        &self,
        state: BlockStateId,
        world: &dyn ScheduledTickAccess,
        pos: BlockPos,
        direction: Direction,
        _neighbor_pos: BlockPos,
        _neighbor_state: BlockStateId,
    ) -> BlockStateId {
        // Wall hanging signs break when blocks on the perpendicular axis are removed
        // and they can no longer survive
        if let Some(facing) = state.try_get_value(HORIZONTAL_FACING) {
            // Check if the change is on the perpendicular axis (clockwise/counterclockwise)
            if direction.axis() == facing.rotate_y_clockwise().axis()
                && !can_wall_hanging_sign_survive(world, pos, facing)
            {
                return REGISTRY.blocks.get_default_state_id(&vanilla_blocks::AIR);
            }
        }
        schedule_water_tick_if_waterlogged(state, world, pos);
        state
    }

    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        // Try each horizontal direction based on player's look direction
        let directions = get_nearest_looking_directions(context.rotation(), context.clicked_face());

        for direction in directions {
            // Wall hanging signs face perpendicular to the wall they're attached to
            // Skip if the clicked face is on the same axis
            if direction.axis() == context.clicked_face().axis() {
                continue;
            }

            let facing = direction.opposite();

            // Check if sign can survive with this facing
            if can_wall_hanging_sign_survive(context.world, context.place_pos(), facing) {
                return Some(
                    self.block
                        .default_state()
                        .set_value(HORIZONTAL_FACING, facing),
                );
            }
        }

        // No valid placement found
        None
    }

    fn new_block_entity(
        &self,
        level: Weak<World>,
        pos: BlockPos,
        state: BlockStateId,
    ) -> BlockEntityCreation {
        BlockEntityCreation::Created(Arc::new(SignBlockEntity::new_hanging(level, pos, state)))
    }

    fn get_block_entity_ticker(
        &self,
        _world: &Arc<World>,
        _state: BlockStateId,
        block_entity_type: BlockEntityTypeRef,
    ) -> Option<BlockEntityTicker> {
        BlockEntityTicker::for_matching_entity_tick(
            block_entity_type,
            &vanilla_block_entity_types::HANGING_SIGN,
        )
    }

    fn use_item_on(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        player: &Player,
        _hand: InteractionHand,
        _hit_result: &BlockHitResult,
        inv: &mut InventoryAccess,
    ) -> InteractionResult {
        try_apply_sign_applicator(state, world, pos, player, inv)
    }

    fn use_without_item(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        player: &Player,
        _hit_result: &BlockHitResult,
        _inv: &mut InventoryAccess,
    ) -> InteractionResult {
        try_open_sign_editor(state, world, pos, player)
    }
}

#[cfg(test)]
mod tests {
    use foton_registry::init_vanilla_registry;
    use foton_registry::item_stack::ItemStack;
    use foton_registry::{DyeColor, vanilla_items};
    use foton_utils::ChunkPos;
    use foton_utils::types::UpdateFlags;
    use glam::DVec3;
    use text_components::TextComponent;

    use super::*;
    use crate::block_entity::entities::SignText;
    use crate::bootstrap::init_globals_once;
    use crate::test_support::{
        TestLevel, TestPlayerBuilder, fresh_test_world, insert_ready_full_chunk,
    };

    const WATERLOGGED: &BoolProperty = &BlockStateProperties::WATERLOGGED;

    const SIGN_POS: BlockPos = BlockPos::new(8, 64, 8);
    const SIGN_PLAYER_ENTITY_ID: i32 = 4242;

    #[test]
    fn standing_sign_only_schedules_water_when_support_survives() {
        init_vanilla_registry();
        let pos = BlockPos::new(0, 64, 0);
        let sign = StandingSignBlock::new(&vanilla_blocks::OAK_SIGN);
        let state = vanilla_blocks::OAK_SIGN
            .default_state()
            .set_value(WATERLOGGED, true);
        let supported =
            TestLevel::default().with_block(pos.below(), vanilla_blocks::STONE.default_state());

        assert_eq!(
            sign.update_shape(
                state,
                &supported,
                pos,
                Direction::North,
                pos.north(),
                vanilla_blocks::AIR.default_state(),
            ),
            state
        );
        assert!(supported.scheduled_water_tick());

        let unsupported = TestLevel::default();
        assert!(
            sign.update_shape(
                state,
                &unsupported,
                pos,
                Direction::Down,
                pos.below(),
                vanilla_blocks::AIR.default_state(),
            )
            .is_air()
        );
        assert!(!unsupported.scheduled_water_tick());
    }

    #[test]
    fn sign_variants_select_their_matching_vanilla_tickers() {
        init_vanilla_registry();
        let world = fresh_test_world("sign_ticker_selection");

        let standing = StandingSignBlock::new(&vanilla_blocks::OAK_SIGN);
        assert!(
            standing
                .get_block_entity_ticker(
                    &world,
                    vanilla_blocks::OAK_SIGN.default_state(),
                    &vanilla_block_entity_types::SIGN,
                )
                .is_some()
        );

        let wall = WallSignBlock::new(&vanilla_blocks::OAK_WALL_SIGN);
        assert!(
            wall.get_block_entity_ticker(
                &world,
                vanilla_blocks::OAK_WALL_SIGN.default_state(),
                &vanilla_block_entity_types::SIGN,
            )
            .is_some()
        );

        let ceiling_hanging = CeilingHangingSignBlock::new(&vanilla_blocks::OAK_HANGING_SIGN);
        assert!(
            ceiling_hanging
                .get_block_entity_ticker(
                    &world,
                    vanilla_blocks::OAK_HANGING_SIGN.default_state(),
                    &vanilla_block_entity_types::HANGING_SIGN,
                )
                .is_some()
        );

        let wall_hanging = WallHangingSignBlock::new(&vanilla_blocks::OAK_WALL_HANGING_SIGN);
        assert!(
            wall_hanging
                .get_block_entity_ticker(
                    &world,
                    vanilla_blocks::OAK_WALL_HANGING_SIGN.default_state(),
                    &vanilla_block_entity_types::HANGING_SIGN,
                )
                .is_some()
        );
        assert!(
            wall_hanging
                .get_block_entity_ticker(
                    &world,
                    vanilla_blocks::OAK_WALL_HANGING_SIGN.default_state(),
                    &vanilla_block_entity_types::SIGN,
                )
                .is_none()
        );
    }

    /// An oak sign standing in a loaded chunk, with a player next to it.
    fn placed_sign(key: &'static str) -> (Arc<World>, Arc<Player>, BlockStateId) {
        init_globals_once();
        let world = fresh_test_world(key);
        insert_ready_full_chunk(&world, ChunkPos::from_block_pos(SIGN_POS));
        let state = vanilla_blocks::OAK_SIGN.default_state();
        assert!(world.set_block(SIGN_POS, state, UpdateFlags::UPDATE_ALL));

        let player =
            TestPlayerBuilder::new(Arc::clone(&world), "SignTester", SIGN_PLAYER_ENTITY_ID).build();

        (world, player, state)
    }

    /// The same sign with a line written on it.
    ///
    /// Both sides get one, because the default `canApplyToSign` refuses a blank
    /// side and the player could be facing either one.
    fn written_sign(key: &'static str) -> (Arc<World>, Arc<Player>, BlockStateId) {
        let (world, player, state) = placed_sign(key);
        let sign = world
            .get_block_entity(SIGN_POS)
            .expect("placing an oak sign creates its block entity");
        let sign = sign
            .downcast_ref::<SignBlockEntity>()
            .expect("an oak sign carries a sign block entity");
        for front in [true, false] {
            let mut text = SignText::new();
            text.set_message(0, TextComponent::plain("hello"));
            sign.set_text(text, front);
        }

        (world, player, state)
    }

    /// Reads back whether the sign at [`SIGN_POS`] is waxed.
    fn sign_is_waxed(world: &Arc<World>) -> bool {
        let sign = world.get_block_entity(SIGN_POS).expect("sign block entity");
        sign.downcast_ref::<SignBlockEntity>()
            .expect("a sign")
            .is_waxed()
    }

    /// Runs the four-argument `use_item_on` with `item` in the player's hand.
    fn click_sign_holding(
        world: &Arc<World>,
        player: &Player,
        state: BlockStateId,
        item: ItemStack,
    ) -> InteractionResult {
        player.inventory.lock().set_selected_item(item);
        let mut inv =
            InventoryAccess::new(Arc::clone(&player.inventory), InteractionHand::MainHand);
        let (x, y, z) = SIGN_POS.get_center();
        StandingSignBlock::new(&vanilla_blocks::OAK_SIGN).use_item_on(
            state,
            world,
            SIGN_POS,
            player,
            InteractionHand::MainHand,
            &BlockHitResult {
                location: DVec3::new(x, y, z),
                direction: Direction::Up,
                block_pos: SIGN_POS,
                miss: false,
                inside: false,
                world_border_hit: false,
            },
            &mut inv,
        )
    }

    /// Reads back the side of the sign the player is standing in front of.
    fn faced_text(world: &Arc<World>, player: &Player, state: BlockStateId) -> SignText {
        let sign = world.get_block_entity(SIGN_POS).expect("sign block entity");
        let sign = sign.downcast_ref::<SignBlockEntity>().expect("a sign");
        sign.get_text(is_facing_front_text(state, SIGN_POS, player))
    }

    #[test]
    fn a_glow_ink_sac_makes_the_sign_text_glow_and_is_spent() {
        let (world, player, state) = written_sign("sign_glow_ink_applies");
        assert!(!faced_text(&world, &player, state).has_glowing_text);

        let held = ItemStack::with_count(&vanilla_items::GLOW_INK_SAC, 2);
        assert_eq!(
            click_sign_holding(&world, &player, state, held),
            InteractionResult::Success
        );
        assert!(faced_text(&world, &player, state).has_glowing_text);
        assert_eq!(player.inventory.lock().get_selected_item().count(), 1);
    }

    #[test]
    fn a_second_glow_ink_sac_on_glowing_text_changes_nothing_and_is_kept() {
        let (world, player, state) = written_sign("sign_glow_ink_is_idempotent");
        let held = ItemStack::with_count(&vanilla_items::GLOW_INK_SAC, 2);
        assert_eq!(
            click_sign_holding(&world, &player, state, held.clone()),
            InteractionResult::Success
        );

        assert_eq!(
            click_sign_holding(&world, &player, state, held),
            InteractionResult::TryEmptyHandInteraction
        );
        assert_eq!(player.inventory.lock().get_selected_item().count(), 2);
    }

    #[test]
    fn an_ink_sac_takes_the_glow_back_off() {
        let (world, player, state) = written_sign("sign_ink_removes_glow");
        assert_eq!(
            click_sign_holding(
                &world,
                &player,
                state,
                ItemStack::new(&vanilla_items::GLOW_INK_SAC),
            ),
            InteractionResult::Success
        );
        assert!(faced_text(&world, &player, state).has_glowing_text);

        assert_eq!(
            click_sign_holding(
                &world,
                &player,
                state,
                ItemStack::new(&vanilla_items::INK_SAC),
            ),
            InteractionResult::Success
        );
        assert!(!faced_text(&world, &player, state).has_glowing_text);
    }

    #[test]
    fn a_waxed_sign_refuses_both_ink_sacs() {
        let (world, player, state) = written_sign("sign_waxed_refuses_ink");
        {
            let sign = world.get_block_entity(SIGN_POS).expect("sign block entity");
            let sign = sign.downcast_ref::<SignBlockEntity>().expect("a sign");
            assert!(sign.wax());
        }

        for item in [&vanilla_items::GLOW_INK_SAC, &vanilla_items::INK_SAC] {
            let held = ItemStack::with_count(item, 3);
            assert_eq!(
                click_sign_holding(&world, &player, state, held),
                InteractionResult::TryEmptyHandInteraction
            );
            assert!(!faced_text(&world, &player, state).has_glowing_text);
            assert_eq!(player.inventory.lock().get_selected_item().count(), 3);
        }
    }

    #[test]
    fn a_dye_recolors_the_sign_text() {
        let (world, player, state) = written_sign("sign_dye_recolors");
        assert_eq!(faced_text(&world, &player, state).color, DyeColor::Black);

        assert_eq!(
            click_sign_holding(
                &world,
                &player,
                state,
                ItemStack::new(&vanilla_items::RED_DYE),
            ),
            InteractionResult::Success
        );
        assert_eq!(faced_text(&world, &player, state).color, DyeColor::Red);
    }

    #[test]
    fn an_item_that_is_no_sign_applicator_falls_through_to_the_editor() {
        let (world, player, state) = written_sign("sign_plain_item_falls_through");
        assert_eq!(
            click_sign_holding(
                &world,
                &player,
                state,
                ItemStack::new(&vanilla_items::STONE)
            ),
            InteractionResult::TryEmptyHandInteraction
        );
    }

    /// A blank sign has nothing to make glow, but it can still be sealed.
    ///
    /// This is the split between the default `canApplyToSign` and honeycomb's
    /// override of it.
    #[test]
    fn a_blank_sign_refuses_ink_but_accepts_honeycomb() {
        let (world, player, state) = placed_sign("sign_blank_refuses_ink");
        assert_eq!(
            click_sign_holding(
                &world,
                &player,
                state,
                ItemStack::new(&vanilla_items::GLOW_INK_SAC),
            ),
            InteractionResult::TryEmptyHandInteraction
        );
        assert!(!faced_text(&world, &player, state).has_glowing_text);

        assert_eq!(
            click_sign_holding(
                &world,
                &player,
                state,
                ItemStack::new(&vanilla_items::HONEYCOMB),
            ),
            InteractionResult::Success
        );
        assert!(sign_is_waxed(&world));
    }

    /// Honeycomb waxes signs through the applicator path, not through its own
    /// `use_on`: once the sign block claims `use_item_on`, the item's `use_on`
    /// is never reached for a sign.
    #[test]
    fn honeycomb_waxes_a_sign_through_the_applicator_path() {
        let (world, player, state) = written_sign("sign_honeycomb_waxes");
        assert!(!sign_is_waxed(&world));
        assert_eq!(
            click_sign_holding(
                &world,
                &player,
                state,
                ItemStack::new(&vanilla_items::HONEYCOMB),
            ),
            InteractionResult::Success
        );
        assert!(sign_is_waxed(&world));
    }
}
