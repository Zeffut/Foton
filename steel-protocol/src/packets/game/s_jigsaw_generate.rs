use std::io::{Cursor, Result};

use steel_macros::ServerPacket;
use steel_utils::BlockPos;
use steel_utils::codec::VarInt;
use steel_utils::serial::ReadFrom;

/// The generate button of a jigsaw block's editor being pressed.
///
/// Vanilla parity: `ServerboundJigsawGeneratePacket`. `levels` is the maximum
/// recursion depth the editor's slider asks for, and `keep_jigsaws` is the
/// checkbox that leaves the jigsaw blocks standing instead of replacing each
/// with its final state.
#[derive(ServerPacket, Clone, Debug)]
pub struct SJigsawGenerate {
    /// The jigsaw block whose button was pressed.
    pub pos: BlockPos,
    /// Maximum recursion depth for the assembly.
    pub levels: i32,
    /// Whether placed jigsaw blocks survive instead of turning into their final state.
    pub keep_jigsaws: bool,
}

impl ReadFrom for SJigsawGenerate {
    fn read(data: &mut Cursor<&[u8]>) -> Result<Self> {
        let pos = BlockPos::read(data)?;
        let levels = VarInt::read(data)?.0;
        let keep_jigsaws = bool::read(data)?;

        Ok(Self {
            pos,
            levels,
            keep_jigsaws,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The depth is a var-int and the checkbox a single byte after it. Reading
    /// them in the other order would turn a depth of 7 into a depth of 0 and a
    /// checked box, which looks like a working generate that places one piece.
    #[test]
    fn the_depth_precedes_the_keep_jigsaws_flag() {
        let mut bytes = 0i64.to_be_bytes().to_vec();
        bytes.push(7); // levels
        bytes.push(1); // keep_jigsaws

        let packet =
            SJigsawGenerate::read(&mut Cursor::new(bytes.as_slice())).expect("packet parses");

        assert_eq!(packet.levels, 7);
        assert!(packet.keep_jigsaws);
    }
}
