//! Dispenser and dropper block behavior.
//!
//! Vanilla parity: `DispenserBlock` and `DropperBlock`. A redstone pulse makes
//! either of them act on one random slot four ticks later; the difference is
//! what "act" means. A dropper hands the item to the container it faces, or
//! throws it if there is none. A dispenser looks the item up in a behavior
//! registry and does whatever that says.

use std::sync::{Arc, Weak};

use glam::DVec3;
use steel_macros::block_behavior;
use steel_registry::block_entity_type::BlockEntityType;
use steel_registry::blocks::BlockRef;
use steel_registry::blocks::block_state_ext::BlockStateExt;
use steel_registry::blocks::properties::{
    BlockStateProperties, BoolProperty, Direction, EnumProperty,
};
use steel_registry::item_stack::ItemStack;
use steel_registry::{level_events, vanilla_block_entity_types};
use steel_utils::types::UpdateFlags;
use steel_utils::{BlockPos, BlockStateId, Downcast as _, translations};
use text_components::TextComponent;
use text_components::translation::TranslatedMessage;

use super::dispense_behavior::{DispenseOutcome, DispenseSource, dispense_behavior_for};
use crate::behavior::InventoryAccess;
use crate::behavior::block::{BlockBehavior, BlockEntityCreation};
use crate::behavior::context::{BlockHitResult, BlockPlaceContext, InteractionResult};
use crate::behavior::item_utils::spawn_item_toward;
use crate::block_entity::BLOCK_ENTITIES;
use crate::block_entity::entities::{DispenserBlockEntity, insert_into_containers_at};
use crate::inventory::container::calculate_redstone_signal_from_container;
use crate::inventory::lock::{ContainerLockGuard, ContainerRef};
use crate::inventory::menu::kinds::dispenser;
use crate::player::Player;
use crate::world::{LevelReader, SignalGetter as _, World};

/// The face the dispenser points at.
const FACING: &EnumProperty<Direction> = &BlockStateProperties::FACING;

/// Whether a redstone pulse is already queued.
const TRIGGERED: &BoolProperty = &BlockStateProperties::TRIGGERED;

/// Ticks between the pulse and the item leaving.
///
/// Vanilla parity: `DispenserBlock.TRIGGER_DURATION`.
const TRIGGER_DURATION: i32 = 4;

/// How far in front of the block an item appears.
///
/// Vanilla parity: the `0.7` of `DispenserBlock.getDispensePosition`.
const DISPENSE_OFFSET: f64 = 0.7;

/// Spread of a dispensed item, in vanilla's accuracy units.
///
/// Vanilla parity: `DefaultDispenseItemBehavior.DEFAULT_ACCURACY`.
const DEFAULT_ACCURACY: i32 = 6;

/// Behavior shared by the dispenser and the dropper.
struct DispenserBase {
    block: BlockRef,
    block_entity_type: &'static BlockEntityType,
    title: TranslatedMessage,
}

impl DispenserBase {
    const fn new(
        block: BlockRef,
        block_entity_type: &'static BlockEntityType,
        title: TranslatedMessage,
    ) -> Self {
        Self {
            block,
            block_entity_type,
            title,
        }
    }

    /// Vanilla parity: `DispenserBlock.getStateForPlacement`.
    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> BlockStateId {
        self.block
            .default_state()
            .set_value(FACING, context.get_nearest_looking_direction().opposite())
            .set_value(TRIGGERED, false)
    }

    fn use_without_item(
        &self,
        world: &Arc<World>,
        pos: BlockPos,
        player: &Player,
    ) -> InteractionResult {
        let Some(container_ref) = world
            .get_block_entity(pos)
            .and_then(ContainerRef::from_block_entity)
        else {
            return InteractionResult::Pass;
        };

        let inventory = player.inventory.clone();
        let title = self.title.clone();
        player.open_menu(TextComponent::translated(title), move |context| {
            dispenser(inventory, context.container_id, container_ref)
        });

        // TODO: Award stat INSPECT_DISPENSER or INSPECT_DROPPER.

        InteractionResult::Success
    }

    fn new_block_entity(
        &self,
        level: Weak<World>,
        pos: BlockPos,
        state: BlockStateId,
    ) -> BlockEntityCreation {
        BlockEntityCreation::from_registered_factory(BLOCK_ENTITIES.create(
            self.block_entity_type,
            level,
            pos,
            state,
        ))
    }

    /// Arms or disarms the block to match the redstone around it.
    ///
    /// Vanilla parity: `DispenserBlock.neighborChanged`. The signal above counts
    /// too, which is what lets a redstone block sit on top of a dispenser.
    fn handle_neighbor_changed(&self, state: BlockStateId, world: &Arc<World>, pos: BlockPos) {
        let powered = world.has_neighbor_signal(pos) || world.has_neighbor_signal(pos.above());
        let triggered = state.get_value(TRIGGERED);

        if powered && !triggered {
            world.schedule_block_tick_default(pos, self.block, TRIGGER_DURATION);
            world.set_block(
                pos,
                state.set_value(TRIGGERED, true),
                UpdateFlags::UPDATE_CLIENTS,
            );
        } else if !powered && triggered {
            world.set_block(
                pos,
                state.set_value(TRIGGERED, false),
                UpdateFlags::UPDATE_CLIENTS,
            );
        }
    }
}

/// Returns the point in front of the block where items appear.
///
/// Vanilla parity: `DispenserBlock.getDispensePosition`.
fn dispense_position(pos: BlockPos, facing: Direction) -> DVec3 {
    let center = DVec3::new(
        f64::from(pos.x()) + 0.5,
        f64::from(pos.y()) + 0.5,
        f64::from(pos.z()) + 0.5,
    );
    let (step_x, step_y, step_z) = facing.offset();
    let normal = DVec3::new(f64::from(step_x), f64::from(step_y), f64::from(step_z));
    center + normal * DISPENSE_OFFSET
}

/// Throws one item out of the block.
///
/// Vanilla parity: `DefaultDispenseItemBehavior.spawnItem` at the dispenser's
/// own accuracy.
fn spawn_dispensed_item(world: &Arc<World>, pos: BlockPos, facing: Direction, stack: ItemStack) {
    spawn_item_toward(
        world,
        dispense_position(pos, facing),
        facing,
        DEFAULT_ACCURACY,
        stack,
    );
}

/// Plays the click and the puff of smoke.
///
/// Vanilla parity: the two `levelEvent` calls of
/// `DefaultDispenseItemBehavior.dispense`.
fn play_dispense_effects(world: &Arc<World>, pos: BlockPos, facing: Direction) {
    world.level_event(level_events::SOUND_DISPENSER_DISPENSE, pos, 0, None);
    world.level_event(
        level_events::PARTICLES_SHOOT_SMOKE,
        pos,
        facing.get_3d_data_value(),
        None,
    );
}

/// Behavior for the dispenser block.
#[block_behavior]
pub struct DispenserBlock {
    inner: DispenserBase,
}

/// Behavior for the dropper block.
#[block_behavior]
pub struct DropperBlock {
    inner: DispenserBase,
}

impl DispenserBlock {
    /// Creates the behavior for the dispenser block.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self {
            inner: DispenserBase::new(
                block,
                &vanilla_block_entity_types::DISPENSER,
                translations::CONTAINER_DISPENSER.msg(),
            ),
        }
    }
}

impl DropperBlock {
    /// Creates the behavior for the dropper block.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self {
            inner: DispenserBase::new(
                block,
                &vanilla_block_entity_types::DROPPER,
                translations::CONTAINER_DROPPER.msg(),
            ),
        }
    }
}

/// Takes one item out of the block and throws it.
///
/// Vanilla parity: `DispenserBlock.dispenseFrom` with
/// `DefaultDispenseItemBehavior`.
///
/// An item with no registered behavior is thrown, which is what vanilla does
/// too.
///
/// TODO: Steel registers arrows and TNT so far. Vanilla also places water and
/// lava, equips armor on the mob in front, shears sheep, spreads bone meal,
/// hatches spawn eggs and launches boats and minecarts; each needs a system
/// Steel does not have yet.
fn dispense_from(world: &Arc<World>, state: BlockStateId, pos: BlockPos) {
    let Some(block_entity) = world.get_block_entity(pos) else {
        return;
    };
    let Some(dispenser) = block_entity.downcast_ref::<DispenserBlockEntity>() else {
        return;
    };

    let Some(slot) = dispenser.get_random_slot() else {
        world.level_event(level_events::SOUND_DISPENSER_FAIL, pos, 0, None);
        return;
    };

    let mut stack = dispenser.get_item(slot);
    if stack.is_empty() {
        return;
    }

    let facing = state.get_value(FACING);
    let source = DispenseSource { world, pos, facing };

    let Some(behavior) = dispense_behavior_for(stack.item()) else {
        let thrown = stack.split(1);
        spawn_dispensed_item(world, pos, facing, thrown);
        dispenser.set_item(slot, stack);
        play_dispense_effects(world, pos, facing);
        return;
    };

    match behavior.execute(&source, stack) {
        DispenseOutcome::Acted {
            remainder,
            sound_override,
        } => {
            dispenser.set_item(slot, remainder);
            match sound_override {
                Some(event) => {
                    world.level_event(event, pos, 0, None);
                    world.level_event(
                        level_events::PARTICLES_SHOOT_SMOKE,
                        pos,
                        facing.get_3d_data_value(),
                        None,
                    );
                }
                None => play_dispense_effects(world, pos, facing),
            }
        }
        DispenseOutcome::Failed(unchanged) => {
            dispenser.set_item(slot, unchanged);
            world.level_event(level_events::SOUND_DISPENSER_FAIL, pos, 0, None);
        }
    }
}

/// Hands one item to the container in front, or throws it when there is none.
///
/// Vanilla parity: `DropperBlock.dispenseFrom`.
fn drop_from(world: &Arc<World>, state: BlockStateId, pos: BlockPos) {
    let Some(block_entity) = world.get_block_entity(pos) else {
        return;
    };
    let Some(dropper) = block_entity.downcast_ref::<DispenserBlockEntity>() else {
        return;
    };

    let Some(slot) = dropper.get_random_slot() else {
        world.level_event(level_events::SOUND_DISPENSER_FAIL, pos, 0, None);
        return;
    };

    let mut stack = dropper.get_item(slot);
    if stack.is_empty() {
        return;
    }

    let facing = state.get_value(FACING);
    let target = pos.relative(facing);
    let one = stack.copy_with_count(1);

    // Vanilla hands the item in through the face opposite the way the dropper
    // points, the same face a hopper would use.
    match insert_into_containers_at(world, target, one, facing.opposite()) {
        // Nothing to hand it to, so throw it the way a dispenser would.
        None => {
            let thrown = stack.split(1);
            spawn_dispensed_item(world, pos, facing, thrown);
            dropper.set_item(slot, stack);
            play_dispense_effects(world, pos, facing);
        }
        Some(leftover) if leftover.is_empty() => {
            stack.shrink(1);
            dropper.set_item(slot, stack);
        }
        // A container that is there but full: vanilla leaves the slot alone and
        // stays silent, which is why a blocked dropper makes no sound.
        Some(_) => {}
    }
}

/// Reads the comparator output of a dispenser or dropper.
fn analog_output_signal(world: &dyn LevelReader, pos: BlockPos) -> i32 {
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

/// Declares one of the two variants and forwards the shared behavior.
macro_rules! dispenser_variant {
    ($name:ident, $dispense:ident) => {
        impl BlockBehavior for $name {
            fn get_state_for_placement(
                &self,
                context: &BlockPlaceContext<'_>,
            ) -> Option<BlockStateId> {
                Some(self.inner.get_state_for_placement(context))
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
                self.inner.use_without_item(world, pos, player)
            }

            fn new_block_entity(
                &self,
                level: Weak<World>,
                pos: BlockPos,
                state: BlockStateId,
            ) -> BlockEntityCreation {
                self.inner.new_block_entity(level, pos, state)
            }

            fn handle_neighbor_changed(
                &self,
                state: BlockStateId,
                world: &Arc<World>,
                pos: BlockPos,
                _source_block: BlockRef,
                _moved_by_piston: bool,
            ) {
                self.inner.handle_neighbor_changed(state, world, pos);
            }

            /// Vanilla parity: `DispenserBlock.tick`, the scheduled tick the
            /// redstone pulse queued four ticks earlier.
            fn tick(&self, state: BlockStateId, world: &Arc<World>, pos: BlockPos) {
                $dispense(world, state, pos);
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
                analog_output_signal(world, pos)
            }
        }
    };
}

dispenser_variant!(DispenserBlock, dispense_from);
dispenser_variant!(DropperBlock, drop_from);
