//! Crafter block behavior.
//!
//! Vanilla parity: `CrafterBlock`. A crafting table that a redstone pulse
//! works: four ticks after the signal arrives it matches its nine slots
//! against the recipe book, takes one item out of each filled slot, and pushes
//! the result into whatever container it faces -- or throws it, if there is
//! none.
//!
//! Two things make it more than an automatic crafting table. Its slots can be
//! switched off, so a recipe with a hole in it keeps the hole while a hopper
//! pours items in. And it fires once per rising edge, not continuously, so a
//! held signal crafts exactly one item.

use std::sync::{Arc, Weak};

use glam::DVec3;
use steel_macros::block_behavior;
use steel_registry::blocks::BlockRef;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::blocks::properties::{
    BlockStateProperties, BoolProperty, Direction, EnumProperty, FrontAndTop,
};
use steel_registry::item_stack::ItemStack;
use steel_registry::{REGISTRY, level_events, vanilla_block_entity_types, vanilla_blocks};
use steel_utils::types::UpdateFlags;
use steel_utils::{BlockPos, BlockStateId, Downcast as _, translations};
use text_components::TextComponent;

use crate::behavior::InventoryAccess;
use crate::behavior::block::{BlockBehavior, BlockEntityCreation};
use crate::behavior::context::{
    BlockHitResult, BlockPlaceContext, InteractionResult, PlacementSource,
};
use crate::block_entity::entities::{CrafterBlockEntity, insert_into_containers_at};
use crate::block_entity::{BLOCK_ENTITIES, BlockEntityTicker};
use crate::inventory::menu::kinds::crafter;
use crate::player::Player;
use crate::world::{LevelReader, SignalGetter as _, World};
use steel_registry::block_entity_type::BlockEntityTypeRef;

/// Which way the block points, and which way is up from its own view.
const ORIENTATION: &EnumProperty<FrontAndTop> = &BlockStateProperties::ORIENTATION;

/// Whether a redstone pulse is already queued.
const TRIGGERED: &BoolProperty = &BlockStateProperties::TRIGGERED;

/// Whether the block is mid-craft, which is a pose the client draws.
const CRAFTING: &BoolProperty = &BlockStateProperties::CRAFTING;

/// Ticks between the pulse and the craft.
///
/// Vanilla parity: `CrafterBlock.CRAFTING_TICK_DELAY`.
const CRAFTING_TICK_DELAY: i32 = 4;

/// How long the block holds its crafting pose.
///
/// Vanilla parity: `CrafterBlock.MAX_CRAFTING_TICKS`.
const MAX_CRAFTING_TICKS: i32 = 6;

/// How far in front of the block a thrown result appears.
///
/// Vanilla parity: the `0.7` of `CrafterBlock.dispenseItem`.
const EJECT_OFFSET: f64 = 0.7;

/// Downward nudge applied to a thrown result.
///
/// Vanilla parity: the `0.15625` of `DefaultDispenseItemBehavior.spawnItem`
/// for a horizontal throw, and `0.125` for a vertical one.
const HORIZONTAL_SPAWN_DROP: f64 = 0.156_25;

/// Downward nudge applied to a result thrown up or down.
const VERTICAL_SPAWN_DROP: f64 = 0.125;

/// Spread of a thrown result, in vanilla's accuracy units.
///
/// Vanilla parity: the `6` `CrafterBlock.dispenseItem` passes to `spawnItem`.
const EJECT_ACCURACY: f64 = 6.0;

/// Deviation one accuracy unit is worth.
///
/// Vanilla parity: the `0.0172275` of `spawnItem`.
const ACCURACY_DEVIATION: f64 = 0.017_227_5;

/// Behavior for the crafter block.
#[block_behavior]
pub struct CrafterBlock {
    block: BlockRef,
}

impl CrafterBlock {
    /// Creates the crafter behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }

    /// Runs the craft.
    ///
    /// Vanilla parity: `CrafterBlock.dispenseFrom`.
    fn craft_from(state: BlockStateId, world: &Arc<World>, pos: BlockPos) {
        let Some(block_entity) = world.get_block_entity(pos) else {
            return;
        };
        let Some(crafter) = block_entity.downcast_ref::<CrafterBlockEntity>() else {
            return;
        };

        let input = crafter.as_craft_input();
        let result = REGISTRY
            .recipes
            .find_crafting_recipe(&input)
            .map(|recipe| (recipe.assemble(), recipe.get_remaining_items(&input)));

        let Some((result, remainders)) = result.filter(|(result, _)| !result.is_empty()) else {
            world.level_event(level_events::SOUND_CRAFTER_FAIL, pos, 0, None);
            return;
        };

        crafter.set_crafting_ticks_remaining(MAX_CRAFTING_TICKS);
        world.set_block(
            pos,
            state.set_value(CRAFTING, true),
            UpdateFlags::UPDATE_CLIENTS,
        );

        let facing = state.get_value(ORIENTATION).front();
        eject(world, pos, facing, result);
        for remainder in remainders {
            if !remainder.is_empty() {
                eject(world, pos, facing, remainder);
            }
        }

        crafter.consume_one_of_each();
    }
}

impl BlockBehavior for CrafterBlock {
    /// Vanilla parity: `CrafterBlock.getStateForPlacement`.
    ///
    /// Pointing at a wall gives a block that is top-up; pointing at the floor
    /// or ceiling has no "up" of its own, so the player's own facing supplies
    /// it -- which is why a crafter placed underfoot still lines up with the
    /// way you were looking.
    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        let front = context.get_nearest_looking_direction().opposite();
        let top = match front {
            Direction::Down => context.horizontal_direction().opposite(),
            Direction::Up => context.horizontal_direction(),
            _ => Direction::Up,
        };
        let orientation = FrontAndTop::from_front_and_top(front, top)?;

        let triggered = context.world.has_neighbor_signal(context.place_pos());
        Some(
            self.block
                .default_state()
                .set_value(ORIENTATION, orientation)
                .set_value(TRIGGERED, triggered),
        )
    }

    /// Vanilla parity: `CrafterBlock.setPlacedBy`, which crafts straight away
    /// when the block is placed into a signal that is already high -- there is
    /// no rising edge to wait for.
    fn set_placed_by(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        _source: &PlacementSource<'_>,
    ) {
        if state.get_value(TRIGGERED) {
            world.schedule_block_tick_default(pos, self.block, CRAFTING_TICK_DELAY);
        }
    }

    fn new_block_entity(
        &self,
        level: Weak<World>,
        pos: BlockPos,
        state: BlockStateId,
    ) -> BlockEntityCreation {
        BlockEntityCreation::from_registered_factory(BLOCK_ENTITIES.create(
            &vanilla_block_entity_types::CRAFTER,
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
        BlockEntityTicker::for_matching_entity_tick(
            block_entity_type,
            &vanilla_block_entity_types::CRAFTER,
        )
    }

    /// Vanilla parity: `CrafterBlock.neighborChanged`, which arms on the rising
    /// edge and disarms on the falling one. A signal that stays high crafts
    /// once, not once per tick.
    fn handle_neighbor_changed(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        _source_block: BlockRef,
        _moved_by_piston: bool,
    ) {
        let powered = world.has_neighbor_signal(pos);
        let triggered = state.get_value(TRIGGERED);
        if powered == triggered {
            return;
        }

        if powered {
            world.schedule_block_tick_default(pos, self.block, CRAFTING_TICK_DELAY);
            world.set_block(
                pos,
                state.set_value(TRIGGERED, true),
                UpdateFlags::UPDATE_CLIENTS,
            );
        } else {
            world.set_block(
                pos,
                state.set_value(TRIGGERED, false).set_value(CRAFTING, false),
                UpdateFlags::UPDATE_CLIENTS,
            );
        }
        set_block_entity_triggered(world, pos, powered);
    }

    fn tick(&self, state: BlockStateId, world: &Arc<World>, pos: BlockPos) {
        Self::craft_from(state, world, pos);
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
        let Some(crafter_entity) = block_entity.downcast_ref::<CrafterBlockEntity>() else {
            return InteractionResult::Pass;
        };
        let container = crafter_entity.container_ref();
        let data = crafter_entity.data();
        // Vanilla parity: `RandomizableContainerBlockEntity.createMenu`
        // unpacks with the opening player, whose luck the roll uses.
        container.unpack_loot_table(Some(player));

        let inventory = player.inventory.clone();
        player.open_menu(
            TextComponent::translated(translations::CONTAINER_CRAFTER.msg()),
            move |context| {
                crafter(
                    inventory,
                    context.container_id,
                    container.clone(),
                    Arc::clone(&data),
                )
            },
        );

        InteractionResult::Success
    }

    fn has_analog_output_signal(&self, _state: BlockStateId) -> bool {
        true
    }

    /// Vanilla parity: `CrafterBlock.getAnalogOutputSignal`, which counts
    /// switched-off slots as full.
    fn get_analog_output_signal(
        &self,
        _state: BlockStateId,
        world: &dyn LevelReader,
        pos: BlockPos,
        _direction: Direction,
    ) -> i32 {
        world
            .get_block_entity(pos)
            .and_then(|entity| {
                entity
                    .downcast_ref::<CrafterBlockEntity>()
                    .map(CrafterBlockEntity::redstone_signal)
            })
            .unwrap_or(0)
    }

    fn affect_neighbors_after_removal(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        _moved_by_piston: bool,
    ) {
        world.update_neighbor_for_output_signal(pos, state.get_block());
    }
}

/// Mirrors the block's `triggered` state onto the block entity, which is what
/// saves it across a reload.
fn set_block_entity_triggered(world: &Arc<World>, pos: BlockPos, triggered: bool) {
    if let Some(block_entity) = world.get_block_entity(pos)
        && let Some(crafter) = block_entity.downcast_ref::<CrafterBlockEntity>()
    {
        crafter.set_triggered(triggered);
    }
}

/// Hands `stack` to the container in front, or throws it.
///
/// Vanilla parity: `CrafterBlock.dispenseItem`. A crafter in front is fed one
/// item at a time so a chain of them spreads a stack across its grid instead of
/// piling it into the first slot; anything else takes the stack whole.
///
/// Not implemented: vanilla also feeds one at a time when the result is larger
/// than the target's stack limit for it, and it fires the crafter advancement
/// trigger. Steel has neither a per-target stack limit reachable from here nor
/// an advancement system.
fn eject(world: &Arc<World>, pos: BlockPos, facing: Direction, stack: ItemStack) {
    let target = pos.relative(facing);
    let back = facing.opposite();

    let leftover = if world.get_block_state(target).get_block() == &vanilla_blocks::CRAFTER {
        let mut remaining = stack;
        while !remaining.is_empty() {
            let one = remaining.copy_with_count(1);
            match insert_into_containers_at(world, target, one, back) {
                Some(rejected) if rejected.is_empty() => remaining.shrink(1),
                _ => break,
            }
        }
        remaining
    } else {
        insert_into_containers_at(world, target, stack.clone(), back).unwrap_or(stack)
    };

    if leftover.is_empty() {
        return;
    }

    throw(world, pos, facing, leftover);
    world.level_event(level_events::SOUND_CRAFTER_CRAFT, pos, 0, None);
    world.level_event(
        level_events::PARTICLES_SHOOT_WHITE_SMOKE,
        pos,
        facing.get_3d_data_value(),
        None,
    );
}

/// Throws an item out of the front face.
///
/// Vanilla parity: `DefaultDispenseItemBehavior.spawnItem`.
fn throw(world: &Arc<World>, pos: BlockPos, facing: Direction, stack: ItemStack) {
    let (step_x, step_y, step_z) = facing.offset();
    let center = DVec3::new(
        f64::from(pos.x()) + 0.5,
        f64::from(pos.y()) + 0.5,
        f64::from(pos.z()) + 0.5,
    );
    let normal = DVec3::new(f64::from(step_x), f64::from(step_y), f64::from(step_z));
    let mut position = center + normal * EJECT_OFFSET;
    position.y -= if step_y == 0 {
        HORIZONTAL_SPAWN_DROP
    } else {
        VERTICAL_SPAWN_DROP
    };

    let power = rand::random::<f64>().mul_add(0.1, 0.2);
    let deviation = ACCURACY_DEVIATION * EJECT_ACCURACY;
    let velocity = DVec3::new(
        triangle(f64::from(step_x) * power, deviation),
        triangle(0.2, deviation),
        triangle(f64::from(step_z) * power, deviation),
    );

    world.spawn_item_with_velocity(position, stack, velocity);
}

/// Vanilla parity: `RandomSource.triangle`.
fn triangle(mode: f64, deviation: f64) -> f64 {
    deviation.mul_add(rand::random::<f64>() - rand::random::<f64>(), mode)
}
