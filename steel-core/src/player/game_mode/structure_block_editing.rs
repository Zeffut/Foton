//! Saving a structure block or a jigsaw block from its editor.
//!
//! Vanilla parity: `ServerGamePacketListenerImpl.handleSetStructureBlock` and
//! `handleSetJigsawBlock`. Both are gated on `Player.canUseGameMasterBlocks`
//! and both resend the block afterwards so an open editor agrees with the
//! server.
//!
//! The structure block's editor has four buttons, and two of them Steel cannot
//! honor yet: saving needs `StructureTemplate.fillFromWorld` and a template
//! manager to write the file, and loading needs `place_in_world` against a live
//! world. Those two report vanilla's own failure messages --
//! `structure_block.save_failure` and `structure_block.load_not_found` -- which
//! is what a player sees when the operation genuinely does not work, rather
//! than a success message for something that did not happen.

use std::sync::Arc;

use steel_protocol::packets::game::{
    CBlockEntityData, SSetJigsawBlock, SSetStructureBlock, StructureUpdateType,
};
use steel_registry::RegistryEntry as _;
use steel_utils::{Downcast as _, Identifier, translations};
use text_components::TextComponent;

use crate::block_entity::BlockEntity;
use crate::block_entity::entities::{
    JigsawBlockEntity, JigsawJointType, StructureBlockEntity, StructureMirror, StructureRotation,
    structure_mode_from_ordinal,
};
use crate::player::Player;
use crate::world::{LevelReader as _, World};

impl Player {
    /// Stores what a player typed into a structure block's editor.
    ///
    /// Vanilla parity: `handleSetStructureBlock`.
    pub fn handle_set_structure_block(self: &Arc<Self>, packet: SSetStructureBlock) {
        if !self.can_use_game_master_blocks() {
            return;
        }
        let Some(update_type) = StructureUpdateType::from_wire(packet.update_type) else {
            return;
        };
        let Some(mode) = structure_mode_from_ordinal(packet.mode) else {
            return;
        };

        let world = self.get_world();
        let pos = packet.pos;
        let Some(shared) = world.get_block_entity(pos) else {
            return;
        };
        let Some(block_entity) = shared.downcast_ref::<StructureBlockEntity>() else {
            return;
        };

        block_entity.set_mode(mode);
        block_entity.set_structure_name(&packet.name);
        block_entity.set_offset((
            i32::from(packet.offset.0),
            i32::from(packet.offset.1),
            i32::from(packet.offset.2),
        ));
        block_entity.set_size((
            i32::from(packet.size.0),
            i32::from(packet.size.1),
            i32::from(packet.size.2),
        ));
        block_entity.set_mirror(StructureMirror::from_ordinal(packet.mirror).unwrap_or_default());
        block_entity
            .set_rotation(StructureRotation::from_ordinal(packet.rotation).unwrap_or_default());
        block_entity.set_metadata(packet.data);
        block_entity.set_ignore_entities(packet.ignore_entities);
        block_entity.set_strict(packet.strict);
        block_entity.set_show_air(packet.show_air);
        block_entity.set_show_bounding_box(packet.show_bounding_box);
        block_entity.set_integrity(packet.integrity);
        block_entity.set_seed(packet.seed);

        if block_entity.has_structure_name() {
            let name = block_entity.structure_name();
            self.report_structure_action(update_type, block_entity, &name);
        } else {
            self.send_message(&TextComponent::translated(
                translations::STRUCTURE_BLOCK_INVALID_STRUCTURE_NAME
                    .message([TextComponent::plain(packet.name)]),
            ));
        }

        block_entity.set_changed();
        send_block_entity_update(self, &world, pos, block_entity);
    }

    /// Reports what the pressed button did.
    fn report_structure_action(
        &self,
        update_type: StructureUpdateType,
        block_entity: &StructureBlockEntity,
        name: &str,
    ) {
        let named = || [TextComponent::plain(name.to_owned())];
        let message = match update_type {
            // Vanilla parity: plain "done" says nothing at all.
            StructureUpdateType::UpdateData => return,
            // Steel cannot capture a structure yet, so this is always the
            // failure branch rather than a success message for nothing.
            StructureUpdateType::SaveArea => {
                translations::STRUCTURE_BLOCK_SAVE_FAILURE.message(named())
            }
            // Steel has no saved structures a block could find, so this is
            // vanilla's "not found" branch.
            StructureUpdateType::LoadArea => {
                translations::STRUCTURE_BLOCK_LOAD_NOT_FOUND.message(named())
            }
            StructureUpdateType::ScanArea => {
                if block_entity.detect_size() {
                    translations::STRUCTURE_BLOCK_SIZE_SUCCESS.message(named())
                } else {
                    translations::STRUCTURE_BLOCK_SIZE_FAILURE.msg()
                }
            }
        };
        self.send_message(&TextComponent::translated(message));
    }

    /// Stores what a player typed into a jigsaw block's editor.
    ///
    /// Vanilla parity: `handleSetJigsawBlock`.
    pub fn handle_set_jigsaw_block(self: &Arc<Self>, packet: SSetJigsawBlock) {
        if !self.can_use_game_master_blocks() {
            return;
        }

        let world = self.get_world();
        let pos = packet.pos;
        let Some(shared) = world.get_block_entity(pos) else {
            return;
        };
        let Some(block_entity) = shared.downcast_ref::<JigsawBlockEntity>() else {
            return;
        };

        // Vanilla parity: `JointType.CODEC.byName(..., ALIGNED)` -- a joint
        // name the server does not know falls back to aligned, not to the
        // orientation default.
        let joint = JigsawJointType::from_name(&packet.joint).unwrap_or(JigsawJointType::Aligned);
        block_entity.configure(
            identifier_or_empty(&packet.name),
            identifier_or_empty(&packet.target),
            identifier_or_empty(&packet.pool),
            packet.final_state,
            joint,
            packet.selection_priority,
            packet.placement_priority,
        );

        send_block_entity_update(self, &world, pos, block_entity);
    }
}

/// Resends one block entity to everyone tracking it, and to the editor.
fn send_block_entity_update(
    player: &Player,
    world: &Arc<World>,
    pos: steel_utils::BlockPos,
    block_entity: &dyn BlockEntity,
) {
    let Some(nbt) = block_entity.get_update_tag() else {
        return;
    };
    world.broadcast_block_entity_update(pos, block_entity.get_type(), nbt.clone());
    player.send_packet(CBlockEntityData {
        pos,
        block_entity_type: block_entity.get_type().id() as i32,
        nbt: steel_utils::serial::OptionalNbt(Some(nbt)),
    });
}

/// Parses an identifier field, falling back to `minecraft:empty`.
///
/// Vanilla reads these with `readIdentifier`, which rejects a malformed one at
/// the codec; Steel keeps the string and falls back here so a bad editor entry
/// cannot drop the connection.
fn identifier_or_empty(value: &str) -> Identifier {
    value
        .parse::<Identifier>()
        .unwrap_or_else(|_| Identifier::vanilla_static("empty"))
}
