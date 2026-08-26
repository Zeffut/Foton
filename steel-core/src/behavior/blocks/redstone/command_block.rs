//! Command block behavior.
//!
//! Vanilla parity: `CommandBlock`, which backs all three of the command block,
//! the repeating command block and the chain command block. The `automatic`
//! flag is the only difference between them at the block level -- a chain block
//! is built with it set -- and everything else comes from which block is
//! standing there, through `CommandBlockEntity.getMode`.
//!
//! The three modes are three different shapes:
//!
//! * **redstone** runs once on a rising edge, scheduled a tick later,
//! * **auto** runs every tick and reschedules itself for as long as it is
//!   powered or set to always-active,
//! * **sequence** never schedules at all -- it is pulled by the block in front
//!   of it, along the chain `execute_chain` walks.

use std::sync::{Arc, Weak};

use steel_macros::block_behavior;
use steel_protocol::packets::game::CBlockEntityData;
use steel_registry::RegistryEntry as _;
use steel_registry::blocks::BlockRef;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::blocks::properties::{BlockStateProperties, Direction};
use steel_registry::vanilla_game_rules::{COMMAND_BLOCKS_WORK, MAX_COMMAND_SEQUENCE_LENGTH};
use steel_utils::serial::OptionalNbt;
use steel_utils::{BlockPos, BlockStateId, Downcast as _};
use text_components::TextComponent;

use crate::behavior::block::{BlockBehavior, BlockEntityCreation};
use crate::behavior::context::{BlockHitResult, BlockPlaceContext, InteractionResult};
use crate::behavior::{InventoryAccess, PlacementSource};
use crate::block_entity::entities::{CommandBlockEntity, CommandBlockMode};
use crate::block_entity::{BLOCK_ENTITIES, BlockEntity};
use crate::command::execution::CommandSource;
use crate::player::Player;
use crate::world::{LevelReader as _, SignalGetter as _, World};

/// The face the command block points at.
const FACING: &steel_registry::blocks::properties::EnumProperty<Direction> =
    &BlockStateProperties::FACING;

/// Behavior for the three command blocks.
#[block_behavior]
pub struct CommandBlock {
    block: BlockRef,
    /// Whether this block is "always active" the moment it is placed.
    ///
    /// Vanilla parity: the `automatic` codec field, true only for the chain
    /// command block.
    #[json_arg(value, json = "automatic")]
    automatic: bool,
}

impl CommandBlock {
    /// Creates the behavior for one of the three command blocks.
    #[must_use]
    pub const fn new(block: BlockRef, automatic: bool) -> Self {
        Self { block, automatic }
    }

    /// Arms or disarms the block to match the redstone around it.
    ///
    /// Vanilla parity: `CommandBlock.setPoweredAndUpdate`. Only a rising edge
    /// on a block that is neither automatic nor a chain block schedules a run,
    /// which is what makes a plain command block a one-shot.
    fn set_powered_and_update(
        &self,
        world: &Arc<World>,
        pos: BlockPos,
        block_entity: &CommandBlockEntity,
        is_powered: bool,
    ) {
        if is_powered == block_entity.is_powered() {
            return;
        }

        block_entity.set_powered(is_powered);
        if !is_powered {
            return;
        }
        if block_entity.is_automatic() || block_entity.mode() == CommandBlockMode::Sequence {
            return;
        }

        block_entity.mark_condition_met();
        world.schedule_block_tick_default(pos, self.block, 1);
    }

    /// Runs the stored command, or clears the count when there is none.
    ///
    /// Vanilla parity: `CommandBlock.execute`.
    fn execute(&self, state: BlockStateId, world: &Arc<World>, pos: BlockPos, has_command: bool) {
        if let Some(shared) = world.get_block_entity(pos)
            && let Some(block_entity) = shared.downcast_ref::<CommandBlockEntity>()
        {
            if has_command {
                perform_command(world, block_entity);
            } else {
                block_entity.command_block().set_success_count(0);
            }
        }

        let facing = state.try_get_value(FACING).unwrap_or(Direction::North);
        execute_chain(world, pos, facing);
    }
}

impl BlockBehavior for CommandBlock {
    /// Vanilla parity: `CommandBlock.getStateForPlacement`.
    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        Some(
            self.block
                .default_state()
                .set_value(FACING, context.get_nearest_looking_direction().opposite()),
        )
    }

    fn new_block_entity(
        &self,
        level: Weak<World>,
        pos: BlockPos,
        state: BlockStateId,
    ) -> BlockEntityCreation {
        let created = BLOCK_ENTITIES.create(
            &steel_registry::vanilla_block_entity_types::COMMAND_BLOCK,
            level,
            pos,
            state,
        );
        if let Some(entity) = &created
            && let Some(block) = entity.downcast_ref::<CommandBlockEntity>()
        {
            // Vanilla parity: `newBlockEntity` seeds `setAutomatic` from the
            // block, which is what makes a freshly placed chain block active.
            block.set_automatic(self.automatic);
        }
        BlockEntityCreation::from_registered_factory(created)
    }

    /// Vanilla parity: `CommandBlock.setPlacedBy`.
    ///
    /// Vanilla also seeds `setTrackOutput` from `sendCommandFeedback` and skips
    /// both when the placed item carried block-entity data. Steel's placement
    /// path has no `BLOCK_ENTITY_DATA` component on block items yet, so a
    /// placed block is always freshly seeded.
    fn set_placed_by(
        &self,
        _state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        _source: &PlacementSource<'_>,
    ) {
        let Some(shared) = world.get_block_entity(pos) else {
            return;
        };
        let Some(block_entity) = shared.downcast_ref::<CommandBlockEntity>() else {
            return;
        };
        block_entity.set_automatic(self.automatic);
        let powered = world.has_neighbor_signal(pos);
        self.set_powered_and_update(world, pos, block_entity, powered);
    }

    /// Vanilla parity: `CommandBlock.neighborChanged`.
    fn handle_neighbor_changed(
        &self,
        _state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        _source_block: BlockRef,
        _moved_by_piston: bool,
    ) {
        let Some(shared) = world.get_block_entity(pos) else {
            return;
        };
        let Some(block_entity) = shared.downcast_ref::<CommandBlockEntity>() else {
            return;
        };
        let powered = world.has_neighbor_signal(pos);
        self.set_powered_and_update(world, pos, block_entity, powered);
    }

    /// Vanilla parity: `CommandBlock.tick`, the whole mode state machine.
    fn tick(&self, state: BlockStateId, world: &Arc<World>, pos: BlockPos) {
        let Some(shared) = world.get_block_entity(pos) else {
            return;
        };
        let Some(block_entity) = shared.downcast_ref::<CommandBlockEntity>() else {
            return;
        };

        let has_command = !block_entity.command_block().command().is_empty();
        let mode = block_entity.mode();
        let was_condition_met = block_entity.was_condition_met();

        match mode {
            CommandBlockMode::Auto => {
                block_entity.mark_condition_met();
                if was_condition_met {
                    self.execute(state, world, pos, has_command);
                } else if block_entity.is_conditional() {
                    block_entity.command_block().set_success_count(0);
                }

                if block_entity.is_powered() || block_entity.is_automatic() {
                    world.schedule_block_tick_default(pos, self.block, 1);
                }
            }
            CommandBlockMode::Redstone => {
                if was_condition_met {
                    self.execute(state, world, pos, has_command);
                } else if block_entity.is_conditional() {
                    block_entity.command_block().set_success_count(0);
                }
            }
            // Vanilla parity: a chain block is never scheduled, so `tick` has
            // no arm for it -- `executeChain` drives it instead.
            CommandBlockMode::Sequence => {}
        }

        world.update_neighbor_for_output_signal(pos, self.block);
    }

    /// Opens the editor for a gamemaster.
    ///
    /// Vanilla parity: `CommandBlock.useWithoutItem`, whose `openCommandBlock`
    /// sends the block entity's data to that one player.
    fn use_without_item(
        &self,
        _state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        player: &Player,
        _hit_result: &BlockHitResult,
        _inv: &mut InventoryAccess,
    ) -> InteractionResult {
        if !player.can_use_game_master_blocks() {
            return InteractionResult::Pass;
        }
        let Some(shared) = world.get_block_entity(pos) else {
            return InteractionResult::Pass;
        };
        let Some(block_entity) = shared.downcast_ref::<CommandBlockEntity>() else {
            return InteractionResult::Pass;
        };
        let Some(nbt) = BlockEntity::get_update_tag(block_entity) else {
            return InteractionResult::Pass;
        };
        player.send_packet(CBlockEntityData {
            pos,
            block_entity_type: BlockEntity::get_type(block_entity).id() as i32,
            nbt: OptionalNbt(Some(nbt)),
        });
        InteractionResult::Success
    }

    /// Vanilla parity: `CommandBlock.hasAnalogOutputSignal`.
    fn has_analog_output_signal(&self, _state: BlockStateId) -> bool {
        true
    }

    /// Vanilla parity: `CommandBlock.getAnalogOutputSignal`, which reports how
    /// many targets the last run succeeded against.
    fn get_analog_output_signal(
        &self,
        _state: BlockStateId,
        world: &dyn crate::world::LevelReader,
        pos: BlockPos,
        _direction: Direction,
    ) -> i32 {
        world
            .get_block_entity(pos)
            .and_then(|entity| {
                entity
                    .downcast_ref::<CommandBlockEntity>()
                    .map(|block| block.command_block().success_count())
            })
            .unwrap_or(0)
    }
}

/// Runs one command block's command.
///
/// Vanilla parity: `BaseCommandBlock.performCommand`. The `Searge` easter egg
/// is kept because a map can and does test for it.
///
/// Returns whether the block ran at all -- false only when it already ran on
/// this game tick, which is what stops a chain looping back into itself.
fn perform_command(world: &Arc<World>, block_entity: &CommandBlockEntity) -> bool {
    let command_block = block_entity.command_block();
    let game_time = world.game_time();
    if command_block.already_ran_at(game_time) {
        return false;
    }

    let command = command_block.command();
    if command.eq_ignore_ascii_case("Searge") {
        command_block.set_last_output(Some(TextComponent::plain("#itzlipofutzli")));
        command_block.set_success_count(1);
        return true;
    }

    command_block.set_success_count(0);
    if world.get_game_rule(&COMMAND_BLOCKS_WORK) && !command.is_empty() {
        command_block.set_last_output(None);
        run_command_block_command(world, block_entity, &command);
        // Vanilla resends the block from inside the command source, once per
        // output line; the end state is the same and this is one packet.
        block_entity.broadcast_update(world);
    }

    command_block.mark_ran_at(game_time);
    true
}

/// Builds the command source and runs the command inside this tick.
fn run_command_block_command(world: &Arc<World>, block_entity: &CommandBlockEntity, command: &str) {
    let Some(server) = world.server() else {
        // A world with no server attached is a test fixture; nothing to run on.
        return;
    };

    let source = CommandSource::for_command_block(
        Arc::clone(block_entity.command_block()),
        Arc::clone(&server),
        Arc::clone(world),
        block_entity.command_source_position(),
        block_entity.command_source_rotation(),
    );
    let successes = server.run_command_now(source, command);
    block_entity.command_block().set_success_count(successes);
}

/// Pulls the chain of chain command blocks in front of `pos`.
///
/// Vanilla parity: `CommandBlock.executeChain`. The walk stops at the first
/// block that is not an active chain block, and each step turns to follow that
/// block's own facing, which is what lets a chain bend.
fn execute_chain(world: &Arc<World>, start: BlockPos, mut direction: Direction) {
    let limit = world.get_game_rule(&MAX_COMMAND_SEQUENCE_LENGTH).max(0);
    let mut pos = start;
    let mut remaining = limit;

    while remaining > 0 {
        remaining -= 1;
        pos = pos.relative(direction);
        let state = world.get_block_state(pos);
        if state.get_block() != &steel_registry::vanilla_blocks::CHAIN_COMMAND_BLOCK {
            return;
        }
        let Some(shared) = world.get_block_entity(pos) else {
            return;
        };
        let Some(block_entity) = shared.downcast_ref::<CommandBlockEntity>() else {
            return;
        };
        if block_entity.mode() != CommandBlockMode::Sequence {
            return;
        }

        if block_entity.is_powered() || block_entity.is_automatic() {
            if block_entity.mark_condition_met() {
                if !perform_command(world, block_entity) {
                    return;
                }
                world.update_neighbor_for_output_signal(pos, state.get_block());
            } else if block_entity.is_conditional() {
                block_entity.command_block().set_success_count(0);
            }
        }

        direction = state.try_get_value(FACING).unwrap_or(direction);
    }

    log::warn!("Command Block chain tried to execute more than {limit} steps!");
}

#[cfg(test)]
mod tests {
    use steel_registry::{init_vanilla_registry, vanilla_block_entity_types, vanilla_blocks};
    use steel_utils::ChunkPos;
    use steel_utils::types::UpdateFlags;

    use super::*;
    use crate::behavior::init_behaviors;
    use crate::block_entity::init_block_entities;
    use crate::test_support::{fresh_test_world, insert_ready_full_chunk};

    fn behavior_for(block: steel_registry::blocks::BlockRef, automatic: bool) -> CommandBlock {
        CommandBlock::new(block, automatic)
    }

    /// The chain command block is the one that is built "always active", which
    /// is what lets a chain run without any redstone touching it.
    #[test]
    fn only_the_chain_block_is_born_always_active() {
        init_vanilla_registry();
        init_behaviors();
        init_block_entities();
        let world = fresh_test_world("command_block_automatic_seed");

        for (block, automatic) in [
            (&vanilla_blocks::COMMAND_BLOCK, false),
            (&vanilla_blocks::REPEATING_COMMAND_BLOCK, false),
            (&vanilla_blocks::CHAIN_COMMAND_BLOCK, true),
        ] {
            let behavior = behavior_for(block, automatic);
            let created = behavior
                .new_block_entity(
                    Arc::downgrade(&world),
                    BlockPos::new(8, 64, 8),
                    block.default_state(),
                )
                .into_created()
                .expect("a command block always has a block entity");
            assert_eq!(
                created.get_type(),
                &vanilla_block_entity_types::COMMAND_BLOCK
            );
            let entity = created
                .downcast_ref::<CommandBlockEntity>()
                .expect("a command block's entity is a CommandBlockEntity");
            assert_eq!(entity.is_automatic(), automatic, "block {:?}", block.key);
        }
    }

    /// A comparator beside a command block reads how many targets the last run
    /// succeeded against. This is the only output a command block has that is
    /// not a chat message, and maps lean on it heavily.
    #[test]
    fn a_comparator_reads_the_stored_success_count() {
        init_vanilla_registry();
        init_behaviors();
        init_block_entities();
        let world = fresh_test_world("command_block_comparator_output");
        insert_ready_full_chunk(&world, ChunkPos::new(0, 0));

        let pos = BlockPos::new(8, 64, 8);
        let state = vanilla_blocks::COMMAND_BLOCK.default_state();
        let behavior = behavior_for(&vanilla_blocks::COMMAND_BLOCK, false);
        assert!(behavior.has_analog_output_signal(state));

        world.set_block(pos, state, UpdateFlags::UPDATE_CLIENTS);
        let Some(shared) = world.get_block_entity(pos) else {
            panic!("placing a command block must create its block entity");
        };
        let entity = shared
            .downcast_ref::<CommandBlockEntity>()
            .expect("a command block's entity is a CommandBlockEntity");

        assert_eq!(
            behavior.get_analog_output_signal(state, world.as_ref(), pos, Direction::Up),
            0
        );
        entity.command_block().set_success_count(4);
        assert_eq!(
            behavior.get_analog_output_signal(state, world.as_ref(), pos, Direction::Up),
            4
        );
    }

    /// Redstone arming is edge-triggered on the block entity, not on the block
    /// state, so an unpowered neighbour update must leave it disarmed and a
    /// powered one must arm it.
    #[test]
    fn a_neighbour_change_arms_the_block_from_the_redstone_around_it() {
        init_vanilla_registry();
        init_behaviors();
        init_block_entities();
        let world = fresh_test_world("command_block_redstone_arming");
        insert_ready_full_chunk(&world, ChunkPos::new(0, 0));

        let pos = BlockPos::new(8, 64, 8);
        let state = vanilla_blocks::COMMAND_BLOCK.default_state();
        let behavior = behavior_for(&vanilla_blocks::COMMAND_BLOCK, false);
        world.set_block(pos, state, UpdateFlags::UPDATE_CLIENTS);

        behavior.handle_neighbor_changed(
            state,
            &world,
            pos,
            &vanilla_blocks::REDSTONE_BLOCK,
            false,
        );
        let Some(shared) = world.get_block_entity(pos) else {
            panic!("placing a command block must create its block entity");
        };
        let entity = shared
            .downcast_ref::<CommandBlockEntity>()
            .expect("a command block's entity is a CommandBlockEntity");
        assert!(
            !entity.is_powered(),
            "nothing is powering it, so it must stay disarmed"
        );

        world.set_block(
            pos.above(),
            vanilla_blocks::REDSTONE_BLOCK.default_state(),
            UpdateFlags::UPDATE_CLIENTS,
        );
        behavior.handle_neighbor_changed(
            state,
            &world,
            pos,
            &vanilla_blocks::REDSTONE_BLOCK,
            false,
        );
        assert!(
            entity.is_powered(),
            "a redstone block on top must arm the command block"
        );
    }
}
