use std::io::{Cursor, Result};

use steel_macros::ServerPacket;
use steel_utils::BlockPos;
use steel_utils::codec::VarInt;
use steel_utils::serial::{PrefixedRead, ReadFrom};

/// Longest command a client may store in a command block.
///
/// Vanilla parity: the `readUtf()` default of `FriendlyByteBuf`, which is
/// `Short.MAX_VALUE` characters.
pub const MAX_COMMAND_LENGTH: usize = 32767;

/// Bit set when the block should keep its last output.
const FLAG_TRACK_OUTPUT: u8 = 1;

/// Bit set when the block only runs if the one behind it succeeded.
const FLAG_CONDITIONAL: u8 = 2;

/// Bit set when the block is "always active" rather than redstone-driven.
const FLAG_AUTOMATIC: u8 = 4;

/// A command block's editor being saved.
///
/// Vanilla parity: `ServerboundSetCommandBlockPacket`. The three booleans
/// travel packed into one byte, and the mode is an enum ordinal --
/// `SEQUENCE`, `AUTO`, `REDSTONE` in that order.
#[derive(ServerPacket, Clone, Debug)]
pub struct SSetCommandBlock {
    /// The block being edited.
    pub pos: BlockPos,
    /// The command to store.
    pub command: String,
    /// Which of the three command blocks the player chose, as an ordinal.
    pub mode: i32,
    /// Whether the block keeps its last output.
    pub track_output: bool,
    /// Whether the block is conditional.
    pub conditional: bool,
    /// Whether the block is "always active".
    pub automatic: bool,
}

impl ReadFrom for SSetCommandBlock {
    fn read(data: &mut Cursor<&[u8]>) -> Result<Self> {
        let pos = BlockPos::read(data)?;
        let command = String::read_prefixed_bound::<VarInt>(data, MAX_COMMAND_LENGTH)?;
        let mode = VarInt::read(data)?.0;
        let flags = u8::read(data)?;

        Ok(Self {
            pos,
            command,
            mode,
            track_output: flags & FLAG_TRACK_OUTPUT != 0,
            conditional: flags & FLAG_CONDITIONAL != 0,
            automatic: flags & FLAG_AUTOMATIC != 0,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn packet_bytes(mode: u8, flags: u8) -> Vec<u8> {
        // A block pos is one big-endian i64, then a var-int-prefixed string.
        let mut bytes = 0i64.to_be_bytes().to_vec();
        bytes.push(2);
        bytes.extend_from_slice(b"hi");
        bytes.push(mode);
        bytes.push(flags);
        bytes
    }

    /// The three booleans share one byte. Reading them in the wrong bit order
    /// would silently turn every conditional block a client saves into an
    /// always-active one.
    #[test]
    fn the_flag_byte_unpacks_track_output_conditional_and_automatic() {
        let bytes = packet_bytes(2, FLAG_TRACK_OUTPUT | FLAG_AUTOMATIC);
        let packet =
            SSetCommandBlock::read(&mut Cursor::new(bytes.as_slice())).expect("packet parses");

        assert!(packet.track_output);
        assert!(!packet.conditional);
        assert!(packet.automatic);
        assert_eq!(packet.command, "hi");
        assert_eq!(packet.mode, 2);
    }

    /// An empty flag byte clears all three, which is how a player turns output
    /// tracking off.
    #[test]
    fn an_empty_flag_byte_clears_every_flag() {
        let bytes = packet_bytes(0, 0);
        let packet =
            SSetCommandBlock::read(&mut Cursor::new(bytes.as_slice())).expect("packet parses");

        assert!(!packet.track_output);
        assert!(!packet.conditional);
        assert!(!packet.automatic);
        assert_eq!(packet.mode, 0);
    }
}
