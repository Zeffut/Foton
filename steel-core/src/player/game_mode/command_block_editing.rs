//! Saving a command block or a command block minecart from its editor.
//!
//! Vanilla parity: `ServerGamePacketListenerImpl.handleSetCommandBlock` and
//! `handleSetCommandMinecart`. Both refuse anyone without
//! `Player.canUseGameMasterBlocks` with the same `advMode.notAllowed` message,
//! and both report back whether the command will actually run -- a block saved
//! while `commandBlocksWork` is off still stores its command, and vanilla says
//! so rather than pretending it worked.

use std::sync::Arc;

use steel_protocol::packets::game::{CBlockEntityData, SSetCommandBlock, SSetCommandMinecart};
use steel_registry::RegistryEntry as _;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::blocks::properties::BlockStateProperties;
use steel_registry::vanilla_game_rules::COMMAND_BLOCKS_WORK;
use steel_utils::serial::OptionalNbt;
use steel_utils::types::UpdateFlags;
use steel_utils::{Downcast as _, translations};
use text_components::TextComponent;

use crate::block_entity::entities::{CommandBlockEntity, CommandBlockMode};
use crate::block_entity::{BlockEntity, BlockEntityLifecycleExt as _};
use crate::command::base_command_block::BaseCommandBlock;
use crate::entity::entities::MinecartCommandBlockEntity;
use crate::player::Player;
use crate::world::{LevelReader as _, World};

impl Player {
    /// Stores what a player typed into a command block's editor.
    ///
    /// Vanilla parity: `handleSetCommandBlock`.
    pub fn handle_set_command_block(self: &Arc<Self>, packet: SSetCommandBlock) {
        if !self.can_use_game_master_blocks() {
            self.send_message(&TextComponent::translated(
                translations::ADV_MODE_NOT_ALLOWED.msg(),
            ));
            return;
        }

        let Some(mode) = CommandBlockMode::from_wire(packet.mode) else {
            log::debug!("command block editor sent unknown mode {}", packet.mode);
            return;
        };

        let world = self.get_world();
        let pos = packet.pos;
        let Some(shared) = world.get_block_entity(pos) else {
            return;
        };
        let Some(block_entity) = shared.downcast_ref::<CommandBlockEntity>() else {
            return;
        };

        let old_mode = block_entity.mode();
        let current_state = world.get_block_state(pos);
        let Some(facing) = current_state.try_get_value(&BlockStateProperties::FACING) else {
            return;
        };

        // Vanilla parity: the mode is stored as *which block* is there, so
        // switching mode swaps the block and keeps the same block entity.
        let new_state = mode
            .block()
            .default_state()
            .set_value(&BlockStateProperties::FACING, facing)
            .set_value(&BlockStateProperties::CONDITIONAL, packet.conditional);
        if new_state != current_state {
            world.set_block(pos, new_state, UpdateFlags::UPDATE_CLIENTS);
            block_entity.set_block_state(new_state);
        }

        let command_block = block_entity.command_block();
        command_block.set_command(packet.command.clone());
        command_block.set_track_output(packet.track_output);
        if !packet.track_output {
            command_block.set_last_output(None);
        }

        block_entity.set_automatic(packet.automatic);
        if old_mode != mode {
            block_entity.on_mode_switch();
        }

        let enabled = world.get_game_rule(&COMMAND_BLOCKS_WORK);
        if enabled {
            self.send_block_entity_update(&world, pos, block_entity);
        }
        self.report_stored_command(&packet.command, enabled);
    }

    /// Stores what a player typed into a command block minecart's editor.
    ///
    /// Vanilla parity: `handleSetCommandMinecart`.
    pub fn handle_set_command_minecart(self: &Arc<Self>, packet: SSetCommandMinecart) {
        if !self.can_use_game_master_blocks() {
            self.send_message(&TextComponent::translated(
                translations::ADV_MODE_NOT_ALLOWED.msg(),
            ));
            return;
        }

        let world = self.get_world();
        let Some(entity) = world.get_entity_by_id(packet.entity) else {
            return;
        };
        let Some(minecart) = entity.downcast_ref::<MinecartCommandBlockEntity>() else {
            return;
        };

        let command_block: &Arc<BaseCommandBlock> = minecart.command_block();
        command_block.set_command(packet.command.clone());
        command_block.set_track_output(packet.track_output);
        if !packet.track_output {
            command_block.set_last_output(None);
        }

        let enabled = world.get_game_rule(&COMMAND_BLOCKS_WORK);
        if enabled {
            minecart.publish_command_to_clients();
        }
        self.report_stored_command(&packet.command, enabled);
    }

    /// Sends one block entity's editor data back to this player.
    fn send_block_entity_update(
        &self,
        world: &Arc<World>,
        pos: steel_utils::BlockPos,
        block_entity: &CommandBlockEntity,
    ) {
        let Some(nbt) = BlockEntity::get_update_tag(block_entity) else {
            return;
        };
        // Vanilla resends the block to everyone tracking it, not just the
        // editing player, so a second open editor sees the change too.
        world.broadcast_block_entity_update(pos, BlockEntity::get_type(block_entity), nbt.clone());
        self.send_packet(CBlockEntityData {
            pos,
            block_entity_type: BlockEntity::get_type(block_entity).id() as i32,
            nbt: OptionalNbt(Some(nbt)),
        });
    }

    /// Tells the player whether the command they stored will run.
    ///
    /// Vanilla parity: the `advMode.setCommand.success` /
    /// `advMode.setCommand.disabled` pair, which is only sent for a non-empty
    /// command -- clearing a block says nothing.
    fn report_stored_command(&self, command: &str, enabled: bool) {
        if command.is_empty() {
            return;
        }
        let message = if enabled {
            translations::ADV_MODE_SET_COMMAND_SUCCESS
                .message([TextComponent::plain(command.to_owned())])
        } else {
            translations::ADV_MODE_SET_COMMAND_DISABLED
                .message([TextComponent::plain(command.to_owned())])
        };
        self.send_message(&TextComponent::translated(message));
    }
}
