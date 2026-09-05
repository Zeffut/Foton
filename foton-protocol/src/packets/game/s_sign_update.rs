use std::io::Cursor;

use foton_macros::ServerPacket;
use foton_utils::BlockPos;
use foton_utils::serial::ReadFrom;

/// Maximum characters per sign line.
pub const MAX_SIGN_LINE_LENGTH: usize = 384;

/// Serverbound packet sent when a player finishes editing a sign.
#[derive(ServerPacket, Clone, Debug)]
pub struct SSignUpdate {
    /// The position of the sign block.
    pub pos: BlockPos,
    /// Whether updating the front text (true) or back text (false).
    pub is_front_text: bool,
    /// The four lines of text. Each line is max 384 characters.
    pub lines: [String; 4],
}

impl ReadFrom for SSignUpdate {
    fn read(data: &mut Cursor<&[u8]>) -> std::io::Result<Self> {
        use foton_utils::serial::prefixed_read::read_utf;

        let pos = BlockPos::read(data)?;
        let is_front_text = bool::read(data)?;
        // Vanilla parity: `input.readUtf(384)`. `read_prefixed_bound` counts
        // bytes and decodes UTF-8 strictly, where vanilla counts UTF-16 units
        // and replaces bad sequences. A line of 384 section signs is 768 bytes
        // and perfectly legal to vanilla, but failed to decode here -- and a
        // decode failure drops the player. `s_rename_item` already reads its
        // text this way.
        let lines = [
            read_utf(data, MAX_SIGN_LINE_LENGTH)?,
            read_utf(data, MAX_SIGN_LINE_LENGTH)?,
            read_utf(data, MAX_SIGN_LINE_LENGTH)?,
            read_utf(data, MAX_SIGN_LINE_LENGTH)?,
        ];

        Ok(Self {
            pos,
            is_front_text,
            lines,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use foton_utils::codec::VarInt;
    use foton_utils::serial::{ReadFrom, WriteTo};

    use super::{MAX_SIGN_LINE_LENGTH, SSignUpdate};

    /// Builds the wire form of a sign update carrying `lines`.
    fn encode(lines: [&str; 4]) -> Vec<u8> {
        let mut bytes = Vec::new();
        // BlockPos, then the front/back flag.
        0i64.write(&mut bytes).expect("position should encode");
        true.write(&mut bytes).expect("flag should encode");
        for line in lines {
            VarInt(i32::try_from(line.len()).expect("test line fits"))
                .write(&mut bytes)
                .expect("length should encode");
            bytes.extend_from_slice(line.as_bytes());
        }
        bytes
    }

    /// A full line of multi-byte characters is legal and must decode.
    ///
    /// Vanilla reads these with `readUtf(384)`, which counts UTF-16 units.
    /// Foton bounded bytes instead, so 384 section signs -- 768 bytes, and
    /// exactly what a player writing a decorated sign produces -- were refused,
    /// and a decode failure drops the player.
    #[test]
    fn a_line_of_multi_byte_characters_still_fits() {
        let line = "\u{00a7}".repeat(MAX_SIGN_LINE_LENGTH);
        assert_eq!(
            line.len(),
            MAX_SIGN_LINE_LENGTH * 2,
            "the test line is two bytes per character"
        );

        let bytes = encode([&line, "", "", ""]);
        let packet = SSignUpdate::read(&mut Cursor::new(bytes.as_slice()))
            .expect("a 384-character line is within vanilla's limit");
        assert_eq!(packet.lines[0], line);
    }

    /// The limit is still a limit.
    #[test]
    fn a_line_past_the_limit_is_still_refused() {
        let line = "a".repeat(MAX_SIGN_LINE_LENGTH * 3 + 1);
        let bytes = encode([&line, "", "", ""]);
        assert!(
            SSignUpdate::read(&mut Cursor::new(bytes.as_slice())).is_err(),
            "past three bytes per UTF-16 unit there is nothing legitimate left"
        );
    }
}
