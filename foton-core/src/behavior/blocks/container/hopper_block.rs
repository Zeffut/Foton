//! Hopper block behavior.
//!
//! Vanilla parity: `HopperBlock`. The block owns the facing and the redstone
//! lock; the moving of items lives on [`HopperBlockEntity`].

use std::sync::{Arc, Weak};

use foton_macros::block_behavior;
use foton_registry::block_entity_type::BlockEntityTypeRef;
use foton_registry::blocks::BlockRef;
use foton_registry::blocks::block_state_ext::BlockStateExt;
use foton_registry::blocks::properties::{
    BlockStateProperties, BoolProperty, Direction, EnumProperty,
};
use foton_registry::vanilla_block_entity_types;
use foton_utils::types::UpdateFlags;
use foton_utils::{BlockPos, BlockStateId, Downcast as _, axis::Axis, translations};
use text_components::TextComponent;

use crate::behavior::InventoryAccess;
use crate::behavior::block::{BlockBehavior, BlockEntityCreation};
use crate::behavior::context::{BlockHitResult, BlockPlaceContext, InteractionResult};
use crate::block_entity::entities::HopperBlockEntity;
use crate::block_entity::{BLOCK_ENTITIES, BlockEntity, BlockEntityTicker};
use crate::entity::ai::path::PathComputationType;
use crate::inventory::container::calculate_redstone_signal_from_container;
use crate::inventory::lock::{ContainerLockGuard, ContainerRef};
use crate::inventory::menu::kinds::hopper;
use crate::player::Player;
use crate::world::{LevelReader, SignalGetter as _, World};

/// The direction the spout points at.
const FACING: &EnumProperty<Direction> = &BlockStateProperties::FACING_HOPPER;

/// Whether the hopper is free to move items.
const ENABLED: &BoolProperty = &BlockStateProperties::ENABLED;

/// Behavior for the hopper block.
#[block_behavior]
pub struct HopperBlock {
    block: BlockRef,
}

impl HopperBlock {
    /// Creates the behavior for the hopper block.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }

    /// Locks or frees the hopper to match the redstone signal around it.
    ///
    /// Vanilla parity: `HopperBlock.checkPoweredState`.
    fn check_powered_state(world: &Arc<World>, pos: BlockPos, state: BlockStateId) {
        let should_be_enabled = !world.has_neighbor_signal(pos);
        if should_be_enabled != state.get_value(ENABLED) {
            world.set_block(
                pos,
                state.set_value(ENABLED, should_be_enabled),
                UpdateFlags::UPDATE_CLIENTS,
            );
        }
    }
}

/// Runs one hopper tick for the live block state.
///
/// Vanilla parity: `HopperBlockEntity.pushItemsTick`, which vanilla reaches
/// through the ticker the block hands out. The state is read here so the block
/// entity does not have to duplicate the block's own properties.
fn push_items_tick(
    world: &Arc<World>,
    pos: BlockPos,
    state: BlockStateId,
    block_entity: &dyn BlockEntity,
) {
    let Some(hopper) = block_entity.downcast_ref::<HopperBlockEntity>() else {
        return;
    };
    hopper.push_items_tick(
        world,
        pos,
        state.get_value(FACING),
        state.get_value(ENABLED),
    );
}

impl BlockBehavior for HopperBlock {
    /// Vanilla parity: `HopperBlock.getStateForPlacement`. A hopper placed
    /// against a floor or ceiling points straight down rather than sideways.
    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        let against = context.clicked_face().opposite();
        let facing = if against.axis() == Axis::Y {
            Direction::Down
        } else {
            against
        };

        Some(
            self.block
                .default_state()
                .set_value(FACING, facing)
                .set_value(ENABLED, true),
        )
    }

    fn use_without_item(
        &self,
        _state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        player: &Player,
        _hit_result: &BlockHitResult,
        _inv: &mut InventoryAccess,
    ) -> InteractionResult {
        let Some(block_entity) = world.get_block_entity(pos) else {
            return InteractionResult::Pass;
        };
        let Some(container_ref) = ContainerRef::from_block_entity(block_entity.clone()) else {
            return InteractionResult::Pass;
        };

        // Vanilla parity: `RandomizableContainerBlockEntity.createMenu`
        // unpacks with the opening player, whose luck the roll uses.
        container_ref.unpack_loot_table(Some(player));

        let inventory = player.inventory.clone();
        player.open_menu(
            block_entity.display_name(TextComponent::translated(
                translations::CONTAINER_HOPPER.msg(),
            )),
            move |context| hopper(inventory, context.container_id, container_ref),
        );

        // TODO: Award stat INSPECT_HOPPER.

        InteractionResult::Success
    }

    fn new_block_entity(
        &self,
        level: Weak<World>,
        pos: BlockPos,
        state: BlockStateId,
    ) -> BlockEntityCreation {
        BlockEntityCreation::from_registered_factory(BLOCK_ENTITIES.create(
            &vanilla_block_entity_types::HOPPER,
            level,
            pos,
            state,
        ))
    }

    fn get_block_entity_ticker(
        &self,
        _world: &Arc<World>,
        _state: BlockStateId,
        block_entity_type: BlockEntityTypeRef,
    ) -> Option<BlockEntityTicker> {
        BlockEntityTicker::for_matching_tick(
            block_entity_type,
            &vanilla_block_entity_types::HOPPER,
            push_items_tick,
        )
    }

    /// Vanilla parity: `HopperBlock.onPlace`.
    fn on_place(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        old_state: BlockStateId,
        _moved_by_piston: bool,
    ) {
        if old_state.get_block() != state.get_block() {
            Self::check_powered_state(world, pos, state);
        }
    }

    /// Vanilla parity: `HopperBlock.neighborChanged`.
    fn handle_neighbor_changed(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        _source_block: BlockRef,
        _moved_by_piston: bool,
    ) {
        Self::check_powered_state(world, pos, state);
    }

    /// Vanilla parity: `HopperBlock.isPathfindable`, which keeps mobs from
    /// treating the bowl as walkable ground.
    fn is_pathfindable(&self, _state: BlockStateId, _path_type: PathComputationType) -> bool {
        false
    }

    fn has_analog_output_signal(&self, _state: BlockStateId) -> bool {
        true
    }

    fn get_analog_output_signal(
        &self,
        _state: BlockStateId,
        world: &dyn LevelReader,
        pos: BlockPos,
        _direction: Direction,
    ) -> i32 {
        let Some(container_ref) = world
            .get_block_entity(pos)
            .and_then(ContainerRef::from_block_entity)
        else {
            return 0;
        };
        let guard = ContainerLockGuard::lock_all(&[&container_ref]);
        guard
            .get(container_ref.container_id())
            .map_or(0, |container| {
                calculate_redstone_signal_from_container(container)
            })
    }
}
