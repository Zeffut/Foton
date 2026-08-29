//! Vanilla `ShelfBlock` behavior.
//!
//! A shelf displays three items. Clicking a slot swaps that one item with what
//! the player is holding. Powering the shelf instead makes it swap the whole
//! hotbar row at once, and lets up to three shelves in a line act as one nine
//! slot row -- that chain is what the `SIDE_CHAIN_PART` property records.

use std::sync::{Arc, Weak};

use foton_macros::block_behavior;
use foton_registry::blocks::BlockRef;
use foton_registry::blocks::block_state_ext::BlockStateExt as _;
use foton_registry::blocks::properties::{
    BlockStateProperties, BoolProperty, Direction, EnumProperty, SideChainPart,
};
use foton_registry::data_components::vanilla_components::USE_EFFECTS;
use foton_registry::game_events::GameEventRef;
use foton_registry::item_stack::ItemStack;
use foton_registry::sound_event::SoundEventRef;
use foton_registry::vanilla_block_tags::BlockTag;
use foton_registry::{sound_events, vanilla_block_entity_types, vanilla_game_events};
use foton_utils::types::{InteractionHand, UpdateFlags};
use foton_utils::{BlockPos, BlockStateId, Downcast as _};

use super::selectable_slot;
use crate::behavior::InventoryAccess;
use crate::behavior::block::{
    BlockBehavior, BlockEntityCreation, schedule_water_tick_if_waterlogged,
};
use crate::behavior::context::{BlockHitResult, BlockPlaceContext, InteractionResult};
use crate::block_entity::entities::{SHELF_SLOTS, ShelfBlockEntity};
use crate::block_entity::{BLOCK_ENTITIES, SharedBlockEntity};
use crate::entity::ai::path::PathComputationType;
use crate::fluid::is_water_fluid;
use crate::inventory::container::Container as _;
use crate::player::Player;
use crate::world::game_event::GameEventContext;
use crate::world::{LevelReader, ScheduledTickAccess, SignalGetter as _, World};

/// Behavior for the wooden shelves.
#[block_behavior]
pub struct ShelfBlock {
    block: BlockRef,
}

const FACING: &EnumProperty<Direction> = &BlockStateProperties::HORIZONTAL_FACING;
const POWERED: &BoolProperty = &BlockStateProperties::POWERED;
const SIDE_CHAIN_PART: &EnumProperty<SideChainPart> = &BlockStateProperties::SIDE_CHAIN_PART;
const WATERLOGGED: &BoolProperty = &BlockStateProperties::WATERLOGGED;

/// Vanilla `ShelfBlock.getRows`.
const ROWS: usize = 1;
/// Vanilla `ShelfBlock.getColumns`.
const COLUMNS: usize = 3;
/// Vanilla `ShelfBlock.getMaxChainLength`.
const MAX_CHAIN_LENGTH: usize = 3;
/// Hotbar slots a powered chain reaches into, matching vanilla's `9 - ...` math.
const HOTBAR_SLOTS: i32 = 9;

const SOUND_VOLUME: f32 = 1.0;
const SOUND_PITCH: f32 = 1.0;

/// Which way along the shelf's face a neighbour lies.
#[derive(Clone, Copy)]
enum Side {
    Left,
    Right,
}

/// One shelf next to another.
///
/// Vanilla parity: `SideChainPartBlock.Neighbor`, whose empty variant answers
/// "not connectable" and ignores every write. Vanilla caches these per
/// position; Foton reads fresh, which is equivalent because no path here reads
/// a neighbour again after writing it.
struct ShelfNeighbor {
    pos: BlockPos,
    /// `None` when the block there is not a connectable shelf facing the same way.
    part: Option<SideChainPart>,
}

impl ShelfNeighbor {
    const fn is_connectable(&self) -> bool {
        self.part.is_some()
    }

    fn is_unconnectable_or_chain_end(&self) -> bool {
        self.part.is_none_or(SideChainPart::is_chain_end)
    }

    fn connects_towards(&self, end_part: SideChainPart) -> bool {
        self.part
            .is_some_and(|part| part.is_connection_towards(end_part))
    }

    fn connect_to_the_right(&self, world: &Arc<World>) {
        self.repart(world, SideChainPart::when_connected_to_the_right);
    }

    fn connect_to_the_left(&self, world: &Arc<World>) {
        self.repart(world, SideChainPart::when_connected_to_the_left);
    }

    fn disconnect_from_right(&self, world: &Arc<World>) {
        self.repart(world, SideChainPart::when_disconnected_from_the_right);
    }

    fn disconnect_from_left(&self, world: &Arc<World>) {
        self.repart(world, SideChainPart::when_disconnected_from_the_left);
    }

    fn repart(&self, world: &Arc<World>, transition: fn(SideChainPart) -> SideChainPart) {
        let Some(part) = self.part else {
            return;
        };
        ShelfBlock::set_part(world, self.pos, transition(part));
    }
}

impl ShelfBlock {
    /// Creates a shelf behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }

    /// Vanilla parity: `ShelfBlock.isConnectable`.
    fn is_connectable(state: BlockStateId) -> bool {
        state.try_get_value(POWERED) == Some(true)
            && state.get_block().has_tag(&BlockTag::WOODEN_SHELVES)
    }

    /// Vanilla parity: `SideChainPartBlock.setPart`.
    fn set_part(world: &Arc<World>, pos: BlockPos, new_part: SideChainPart) {
        let state = world.get_block_state(pos);
        if state.try_get_value(SIDE_CHAIN_PART) == Some(new_part) {
            return;
        }
        world.set_block(
            pos,
            state.set_value(SIDE_CHAIN_PART, new_part),
            UpdateFlags::UPDATE_ALL,
        );
    }

    /// Vanilla parity: `SideChainPartBlock.Neighbors.left` and `.right`.
    fn neighbor(
        world: &Arc<World>,
        facing: Direction,
        center: BlockPos,
        side: Side,
        steps: i32,
    ) -> ShelfNeighbor {
        let direction = match side {
            Side::Left => facing.rotate_y_clockwise(),
            Side::Right => facing.rotate_y_counter_clockwise(),
        };
        let pos = center.relative_n(direction, steps);
        let state = world.get_block_state(pos);
        let part = (Self::is_connectable(state) && state.try_get_value(FACING) == Some(facing))
            .then(|| state.get_value(SIDE_CHAIN_PART));

        ShelfNeighbor { pos, part }
    }

    /// Vanilla parity: `SideChainPartBlock.getAllBlocksConnectedTo`.
    ///
    /// The result runs left to right along the shelf's face.
    fn all_blocks_connected_to(world: &Arc<World>, pos: BlockPos) -> Vec<BlockPos> {
        let state = world.get_block_state(pos);
        if !Self::is_connectable(state) {
            return Vec::new();
        }

        let facing = state.get_value(FACING);
        let mut left = Self::blocks_connecting_towards(world, facing, pos, Side::Left);
        left.reverse();
        left.push(pos);
        left.extend(Self::blocks_connecting_towards(
            world,
            facing,
            pos,
            Side::Right,
        ));
        left
    }

    /// Vanilla parity: `SideChainPartBlock.addBlocksConnectingTowards`.
    fn blocks_connecting_towards(
        world: &Arc<World>,
        facing: Direction,
        center: BlockPos,
        side: Side,
    ) -> Vec<BlockPos> {
        let end_part = match side {
            Side::Left => SideChainPart::Left,
            Side::Right => SideChainPart::Right,
        };

        let mut found = Vec::new();
        for steps in 1..MAX_CHAIN_LENGTH {
            let neighbor = Self::neighbor(world, facing, center, side, steps as i32);
            if neighbor.connects_towards(end_part) {
                found.push(neighbor.pos);
            }
            if neighbor.is_unconnectable_or_chain_end() {
                break;
            }
        }

        found
    }

    /// Vanilla parity: `SideChainPartBlock.updateNeighborsAfterPoweringDown`.
    fn update_neighbors_after_powering_down(
        world: &Arc<World>,
        pos: BlockPos,
        state: BlockStateId,
    ) {
        let Some(facing) = state.try_get_value(FACING) else {
            return;
        };
        Self::neighbor(world, facing, pos, Side::Left, 1).disconnect_from_right(world);
        Self::neighbor(world, facing, pos, Side::Right, 1).disconnect_from_left(world);
    }

    /// Vanilla parity: `SideChainPartBlock.canConnect`.
    const fn can_connect(new_blocks_to_connect_to: usize, current_chain_length: usize) -> bool {
        new_blocks_to_connect_to > 0
            && current_chain_length + new_blocks_to_connect_to <= MAX_CHAIN_LENGTH
    }

    /// Vanilla parity: `SideChainPartBlock.isBeingUpdatedByNeighbor`, the guard
    /// that stops the chain update from recursing through its own block writes.
    fn is_being_updated_by_neighbor(state: BlockStateId, old_state: BlockStateId) -> bool {
        let getting_connected = state
            .try_get_value(SIDE_CHAIN_PART)
            .is_some_and(SideChainPart::is_connected);
        let connected_before = Self::is_connectable(old_state)
            && old_state
                .try_get_value(SIDE_CHAIN_PART)
                .is_some_and(SideChainPart::is_connected);

        getting_connected || connected_before
    }

    /// Vanilla parity: `SideChainPartBlock.updateSelfAndNeighborsOnPoweringUp`.
    fn update_self_and_neighbors_on_powering_up(
        world: &Arc<World>,
        pos: BlockPos,
        state: BlockStateId,
        old_state: BlockStateId,
    ) {
        if !Self::is_connectable(state) || Self::is_being_updated_by_neighbor(state, old_state) {
            return;
        }

        let facing = state.get_value(FACING);
        let left = Self::neighbor(world, facing, pos, Side::Left, 1);
        let right = Self::neighbor(world, facing, pos, Side::Right, 1);

        let existing_on_the_left = if left.is_connectable() {
            Self::all_blocks_connected_to(world, left.pos).len()
        } else {
            0
        };
        let existing_on_the_right = if right.is_connectable() {
            Self::all_blocks_connected_to(world, right.pos).len()
        } else {
            0
        };

        let mut new_part = SideChainPart::Unconnected;
        let mut chain_length = 1;
        if Self::can_connect(existing_on_the_left, chain_length) {
            new_part = new_part.when_connected_to_the_left();
            left.connect_to_the_right(world);
            chain_length += existing_on_the_left;
        }
        if Self::can_connect(existing_on_the_right, chain_length) {
            new_part = new_part.when_connected_to_the_right();
            right.connect_to_the_left(world);
        }

        Self::set_part(world, pos, new_part);
    }

    fn play_sound(world: &Arc<World>, pos: BlockPos, sound: SoundEventRef) {
        world.play_block_sound(sound, pos, SOUND_VOLUME, SOUND_PITCH, None);
    }

    /// Returns the block entity at `pos` when it really is a shelf.
    fn shelf_entity_at(world: &dyn LevelReader, pos: BlockPos) -> Option<SharedBlockEntity> {
        let entity = world.get_block_entity(pos)?;
        entity.downcast_ref::<ShelfBlockEntity>()?;
        Some(entity)
    }

    /// Vanilla parity: the game event `ShelfBlock.swapSingleItem` chooses, which
    /// is silence for an item that asks not to vibrate on interaction.
    fn single_swap_event(new_inventory_item: &ItemStack) -> Option<GameEventRef> {
        let silent = new_inventory_item
            .get(USE_EFFECTS)
            .is_some_and(|effects| !effects.interact_vibrations);
        (!silent).then_some(&vanilla_game_events::ITEM_INTERACT_FINISH)
    }

    /// Vanilla parity: `ShelfBlock.swapSingleItem`.
    ///
    /// Returns the item that came off the shelf, and the item that was put on.
    fn swap_single_item(
        shelf: &ShelfBlockEntity,
        slot: usize,
        player: &Player,
        inv: &InventoryAccess,
    ) -> (ItemStack, ItemStack) {
        let infinite_materials = player.has_infinite_materials();

        inv.with_inventory(|inventory| {
            let selected = usize::from(inventory.get_selected_slot());
            let held = inventory.get_item(selected).clone();
            let removed = shelf.swap_item_no_update(slot, held.clone());
            let new_inventory_item = if infinite_materials && removed.is_empty() {
                held.clone()
            } else {
                removed.clone()
            };
            inventory.set_item(selected, new_inventory_item.clone());
            inventory.set_changed();
            shelf.set_changed_with_event(Self::single_swap_event(&new_inventory_item));
            (removed, held)
        })
    }

    /// Vanilla parity: `ShelfBlock.swapHotbar`.
    ///
    /// The chain is laid over the hotbar right-aligned, so a single shelf uses
    /// slots six to eight and a chain of three uses the whole row. Foton marks
    /// each part changed after the inventory lock is released rather than
    /// inside the loop; nothing between the two observes the difference.
    fn swap_hotbar(world: &Arc<World>, pos: BlockPos, inv: &InventoryAccess) -> bool {
        let connected = Self::all_blocks_connected_to(world, pos);
        if connected.is_empty() {
            return false;
        }

        let parts: Vec<(usize, SharedBlockEntity)> = connected
            .iter()
            .enumerate()
            .filter_map(|(index, part_pos)| {
                Some((index, Self::shelf_entity_at(world.as_ref(), *part_pos)?))
            })
            .collect();

        let chain_length = connected.len();
        let any_swapped = inv.with_inventory(|inventory| {
            let mut any_swapped = false;
            for (index, entity) in &parts {
                let Some(display) = entity.downcast_ref::<ShelfBlockEntity>() else {
                    continue;
                };
                for slot in 0..SHELF_SLOTS {
                    let inventory_slot = HOTBAR_SLOTS
                        - (chain_length - index) as i32 * SHELF_SLOTS as i32
                        + slot as i32;
                    let Ok(inventory_slot) = usize::try_from(inventory_slot) else {
                        continue;
                    };
                    if inventory_slot >= inventory.get_container_size() {
                        continue;
                    }

                    let placed = inventory.remove_item_no_update(inventory_slot);
                    let taken = display.swap_item_no_update(slot, placed.clone());
                    if !placed.is_empty() || !taken.is_empty() {
                        inventory.set_item(inventory_slot, taken);
                        any_swapped = true;
                    }
                }
            }
            inventory.set_changed();
            any_swapped
        });

        for (_, entity) in &parts {
            if let Some(display) = entity.downcast_ref::<ShelfBlockEntity>() {
                display.set_changed_with_event(Some(&vanilla_game_events::ENTITY_INTERACT));
            }
        }

        any_swapped
    }
}

impl BlockBehavior for ShelfBlock {
    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        Some(
            self.block
                .default_state()
                .set_value(FACING, context.horizontal_direction().opposite())
                .set_value(
                    POWERED,
                    context.world.has_neighbor_signal(context.place_pos()),
                )
                .set_value(WATERLOGGED, context.is_water_source()),
        )
    }

    fn update_shape(
        &self,
        state: BlockStateId,
        world: &dyn ScheduledTickAccess,
        pos: BlockPos,
        _direction: Direction,
        _neighbor_pos: BlockPos,
        _neighbor_state: BlockStateId,
    ) -> BlockStateId {
        schedule_water_tick_if_waterlogged(state, world, pos);

        state
    }

    fn on_place(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        old_state: BlockStateId,
        _moved_by_piston: bool,
    ) {
        if state.get_value(POWERED) {
            Self::update_self_and_neighbors_on_powering_up(world, pos, state, old_state);
        } else {
            Self::update_neighbors_after_powering_down(world, pos, state);
        }
    }

    fn handle_neighbor_changed(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        _source_block: BlockRef,
        _moved_by_piston: bool,
    ) {
        let signal = world.has_neighbor_signal(pos);
        if state.get_value(POWERED) == signal {
            return;
        }

        let mut new_state = state.set_value(POWERED, signal);
        if !signal {
            new_state = new_state.set_value(SIDE_CHAIN_PART, SideChainPart::Unconnected);
        }

        world.set_block(pos, new_state, UpdateFlags::UPDATE_ALL);
        Self::play_sound(
            world,
            pos,
            if signal {
                &sound_events::BLOCK_SHELF_ACTIVATE
            } else {
                &sound_events::BLOCK_SHELF_DEACTIVATE
            },
        );
        world.game_event(
            if signal {
                &vanilla_game_events::BLOCK_ACTIVATE
            } else {
                &vanilla_game_events::BLOCK_DEACTIVATE
            },
            pos,
            &GameEventContext::new(None, Some(new_state)),
        );
    }

    fn affect_neighbors_after_removal(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        _moved_by_piston: bool,
    ) {
        world.update_neighbor_for_output_signal(pos, state.get_block());
        Self::update_neighbors_after_powering_down(world, pos, state);
    }

    fn use_item_on(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        player: &Player,
        hand: InteractionHand,
        hit_result: &BlockHitResult,
        inv: &mut InventoryAccess,
    ) -> InteractionResult {
        if hand == InteractionHand::OffHand {
            return InteractionResult::Pass;
        }
        let Some(entity) = Self::shelf_entity_at(world.as_ref(), pos) else {
            return InteractionResult::Pass;
        };
        let Some(display) = entity.downcast_ref::<ShelfBlockEntity>() else {
            return InteractionResult::Pass;
        };
        let Some(slot) =
            selectable_slot::hit_slot(hit_result, state.get_value(FACING), ROWS, COLUMNS)
        else {
            return InteractionResult::Pass;
        };

        if state.get_value(POWERED) {
            if !Self::swap_hotbar(world, pos, inv) {
                return InteractionResult::Consume;
            }
            Self::play_sound(world, pos, &sound_events::BLOCK_SHELF_MULTI_SWAP);
            return InteractionResult::Success;
        }

        let (removed, placed) = Self::swap_single_item(display, slot, player, inv);
        if removed.is_empty() {
            if placed.is_empty() {
                return InteractionResult::Pass;
            }
            Self::play_sound(world, pos, &sound_events::BLOCK_SHELF_PLACE_ITEM);
        } else {
            Self::play_sound(
                world,
                pos,
                if placed.is_empty() {
                    &sound_events::BLOCK_SHELF_TAKE_ITEM
                } else {
                    &sound_events::BLOCK_SHELF_SINGLE_SWAP
                },
            );
        }

        InteractionResult::Success
    }

    fn new_block_entity(
        &self,
        level: Weak<World>,
        pos: BlockPos,
        state: BlockStateId,
    ) -> BlockEntityCreation {
        BlockEntityCreation::from_registered_factory(BLOCK_ENTITIES.create(
            &vanilla_block_entity_types::SHELF,
            level,
            pos,
            state,
        ))
    }

    fn is_pathfindable(&self, state: BlockStateId, computation_type: PathComputationType) -> bool {
        computation_type == PathComputationType::Water
            && is_water_fluid(state.get_fluid_state().fluid_id)
    }

    fn has_analog_output_signal(&self, _state: BlockStateId) -> bool {
        true
    }

    /// Vanilla parity: `ShelfBlock.getAnalogOutputSignal`, one bit per occupied
    /// slot and only for a comparator sitting behind the shelf.
    fn get_analog_output_signal(
        &self,
        state: BlockStateId,
        world: &dyn LevelReader,
        pos: BlockPos,
        direction: Direction,
    ) -> i32 {
        if direction != state.get_value(FACING).opposite() {
            return 0;
        }
        let Some(entity) = Self::shelf_entity_at(world, pos) else {
            return 0;
        };
        let Some(display) = entity.downcast_ref::<ShelfBlockEntity>() else {
            return 0;
        };

        (0..SHELF_SLOTS).fold(0, |signal, slot| {
            if display.item(slot).is_empty() {
                signal
            } else {
                signal | 1 << slot
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use foton_registry::{init_vanilla_registry, vanilla_blocks};
    use foton_utils::ChunkPos;

    use super::*;
    use crate::behavior::init_behaviors;
    use crate::test_support::{fresh_test_world, insert_ready_full_chunk};

    /// Places a shelf that really is powered: the block below it is a redstone
    /// block, so the neighbour update that follows placement agrees with the
    /// `POWERED` value rather than switching it straight back off.
    fn place_powered_shelf(world: &Arc<World>, pos: BlockPos, facing: Direction) {
        world.set_block(
            pos.below(),
            vanilla_blocks::REDSTONE_BLOCK.default_state(),
            UpdateFlags::UPDATE_NONE,
        );
        assert!(
            world.set_block(
                pos,
                vanilla_blocks::OAK_SHELF
                    .default_state()
                    .set_value(FACING, facing)
                    .set_value(POWERED, true),
                UpdateFlags::UPDATE_ALL,
            )
        );
    }

    #[test]
    fn powering_a_row_of_shelves_links_them_into_one_chain() {
        init_vanilla_registry();
        init_behaviors();

        let world = fresh_test_world("shelf_chain");
        let middle = BlockPos::new(8, 64, 8);
        insert_ready_full_chunk(&world, ChunkPos::from_block_pos(middle));

        // A north-facing shelf runs east to west, so its chain neighbours are
        // the blocks on either side of that axis.
        let facing = Direction::North;
        let left = middle.relative(facing.rotate_y_clockwise());
        let right = middle.relative(facing.rotate_y_counter_clockwise());

        for pos in [left, middle, right] {
            place_powered_shelf(&world, pos, facing);
        }

        let chain = ShelfBlock::all_blocks_connected_to(&world, middle);
        assert_eq!(chain, vec![left, middle, right]);
        assert_eq!(
            world.get_block_state(middle).get_value(SIDE_CHAIN_PART),
            SideChainPart::Center
        );
        assert_eq!(
            world.get_block_state(left).get_value(SIDE_CHAIN_PART),
            SideChainPart::Left
        );
        assert_eq!(
            world.get_block_state(right).get_value(SIDE_CHAIN_PART),
            SideChainPart::Right
        );
    }

    #[test]
    fn a_chain_never_grows_past_three_shelves() {
        init_vanilla_registry();
        init_behaviors();

        let world = fresh_test_world("shelf_chain_limit");
        let start = BlockPos::new(8, 64, 8);
        insert_ready_full_chunk(&world, ChunkPos::from_block_pos(start));

        let facing = Direction::North;
        let along = facing.rotate_y_counter_clockwise();
        let row: Vec<BlockPos> = (0..4).map(|step| start.relative_n(along, step)).collect();
        for pos in &row {
            place_powered_shelf(&world, *pos, facing);
        }

        assert_eq!(ShelfBlock::all_blocks_connected_to(&world, row[0]).len(), 3);
        assert_eq!(
            world.get_block_state(row[3]).get_value(SIDE_CHAIN_PART),
            SideChainPart::Unconnected,
            "the fourth shelf is left out of the full chain"
        );
    }

    #[test]
    fn an_unpowered_shelf_is_not_part_of_any_chain() {
        init_vanilla_registry();
        init_behaviors();

        let world = fresh_test_world("shelf_unpowered");
        let pos = BlockPos::new(8, 64, 8);
        insert_ready_full_chunk(&world, ChunkPos::from_block_pos(pos));

        assert!(world.set_block(
            pos,
            vanilla_blocks::OAK_SHELF.default_state(),
            UpdateFlags::UPDATE_ALL,
        ));
        assert!(ShelfBlock::all_blocks_connected_to(&world, pos).is_empty());
    }
}
