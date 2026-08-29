use std::io::{Cursor, Result};

use foton_macros::ServerPacket;
use foton_utils::BlockPos;
use foton_utils::codec::VarInt;
use foton_utils::serial::{PrefixedRead, ReadFrom};

use super::s_set_command_block::MAX_COMMAND_LENGTH;

/// A jigsaw block's editor being saved.
///
/// Vanilla parity: `ServerboundSetJigsawBlockPacket`. Unlike the structure
/// block this one has no update type: a jigsaw editor only ever stores, and the
/// separate `ServerboundJigsawGeneratePacket` is what runs a generation.
///
/// The joint travels as a plain string rather than an ordinal, and vanilla
/// falls back to `aligned` for a name it does not know.
#[derive(ServerPacket, Clone, Debug)]
pub struct SSetJigsawBlock {
    /// The block being edited.
    pub pos: BlockPos,
    /// The name other jigsaws aim at to connect here.
    pub name: String,
    /// The name this jigsaw aims at.
    pub target: String,
    /// The template pool this jigsaw draws its next piece from.
    pub pool: String,
    /// The block state left behind once the jigsaw has been used.
    pub final_state: String,
    /// Whether a connected piece may roll around the joint axis.
    pub joint: String,
    /// How early this jigsaw is chosen among its pool's candidates.
    pub selection_priority: i32,
    /// How early the piece it places is expanded.
    pub placement_priority: i32,
}

impl ReadFrom for SSetJigsawBlock {
    fn read(data: &mut Cursor<&[u8]>) -> Result<Self> {
        let pos = BlockPos::read(data)?;
        let name = String::read_prefixed_bound::<VarInt>(data, MAX_COMMAND_LENGTH)?;
        let target = String::read_prefixed_bound::<VarInt>(data, MAX_COMMAND_LENGTH)?;
        let pool = String::read_prefixed_bound::<VarInt>(data, MAX_COMMAND_LENGTH)?;
        let final_state = String::read_prefixed_bound::<VarInt>(data, MAX_COMMAND_LENGTH)?;
        let joint = String::read_prefixed_bound::<VarInt>(data, MAX_COMMAND_LENGTH)?;
        let selection_priority = VarInt::read(data)?.0;
        let placement_priority = VarInt::read(data)?.0;

        Ok(Self {
            pos,
            name,
            target,
            pool,
            final_state,
            joint,
            selection_priority,
            placement_priority,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Five strings in a fixed order, then the two priorities -- and the
    /// selection priority comes first on the wire even though the block entity
    /// saves the placement one first. Reading them the other way round would
    /// quietly swap two numbers a map-maker tuned.
    #[test]
    fn the_five_strings_and_two_priorities_read_in_wire_order() {
        let mut bytes = 0i64.to_be_bytes().to_vec();
        for text in ["a", "bb", "ccc", "dddd", "aligned"] {
            bytes.push(text.len() as u8);
            bytes.extend_from_slice(text.as_bytes());
        }
        bytes.push(7); // selection priority
        bytes.push(3); // placement priority

        let packet =
            SSetJigsawBlock::read(&mut Cursor::new(bytes.as_slice())).expect("packet parses");

        assert_eq!(packet.name, "a");
        assert_eq!(packet.target, "bb");
        assert_eq!(packet.pool, "ccc");
        assert_eq!(packet.final_state, "dddd");
        assert_eq!(packet.joint, "aligned");
        assert_eq!(packet.selection_priority, 7);
        assert_eq!(packet.placement_priority, 3);
    }
}
