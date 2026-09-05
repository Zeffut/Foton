use std::io::Cursor;

use foton_macros::ServerPacket;
use foton_utils::codec::VarInt;
use foton_utils::serial::ReadFrom;

/// Maximum chat message length, in UTF-16 units.
///
/// Vanilla parity: the `input.readUtf(256)` of `ServerboundChatPacket`, and the
/// `setMaxLength(256)` the client's chat screen applies -- both count
/// characters, not bytes.
pub const MAX_CHAT_MESSAGE_LENGTH: usize = 256;

#[derive(ServerPacket, Clone, Debug)]
pub struct SChat {
    pub message: String,

    pub timestamp: i64,

    pub salt: i64,

    pub signature: Option<[u8; 256]>,

    pub offset: i32,

    pub acknowledged: [u8; 3],

    pub checksum: u8,
}

impl ReadFrom for SChat {
    fn read(data: &mut Cursor<&[u8]>) -> std::io::Result<Self> {
        use foton_utils::serial::prefixed_read::read_utf;

        // Vanilla parity: `input.readUtf(256)` counts UTF-16 units, so a message
        // of 256 accented characters is 512 bytes and perfectly legal. The byte
        // bound this replaced rejected it, and a decode failure is only logged
        // -- so a player typing French, Russian or Chinese watched their message
        // vanish with nothing said to them. `s_sign_update` and `s_rename_item`
        // already read their text this way.
        let message = read_utf(data, MAX_CHAT_MESSAGE_LENGTH)?;
        let timestamp = i64::read(data)?;
        let salt = i64::read(data)?;
        let signature = Option::<[u8; 256]>::read(data)?;
        let offset = VarInt::read(data)?.0;
        let acknowledged = <[u8; 3]>::read(data)?;
        let checksum = u8::read(data)?;

        Ok(Self {
            message,
            timestamp,
            salt,
            signature,
            offset,
            acknowledged,
            checksum,
        })
    }
}

/// Client -> Server: Acknowledges messages received from the server.
///
/// The client sends this to indicate it has received and processed
/// messages up to the specified offset.
///
/// Equivalent to `ServerboundChatAckPacket` in Minecraft.
#[derive(ServerPacket, Clone, Debug)]
pub struct SChatAck {
    /// The message offset being acknowledged
    pub offset: VarInt,
}

impl foton_utils::serial::ReadFrom for SChatAck {
    fn read(reader: &mut Cursor<&[u8]>) -> std::io::Result<Self> {
        Ok(Self {
            offset: VarInt::read(reader)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use foton_utils::codec::VarInt;
    use foton_utils::serial::{ReadFrom, WriteTo};

    use super::{MAX_CHAT_MESSAGE_LENGTH, SChat};

    /// Builds the wire form of a chat packet carrying `message`.
    fn encode(message: &str) -> Vec<u8> {
        let mut bytes = Vec::new();
        VarInt(i32::try_from(message.len()).expect("test message fits"))
            .write(&mut bytes)
            .expect("length should encode");
        bytes.extend_from_slice(message.as_bytes());
        0i64.write(&mut bytes).expect("timestamp should encode");
        0i64.write(&mut bytes).expect("salt should encode");
        false.write(&mut bytes).expect("no signature should encode");
        VarInt(0).write(&mut bytes).expect("offset should encode");
        bytes.extend_from_slice(&[0u8; 3]);
        bytes.push(0);
        bytes
    }

    /// A full message of accented characters is legal and must decode.
    ///
    /// Vanilla reads it with `readUtf(256)`, which counts UTF-16 units, and the
    /// client's own chat box caps input at 256 *characters*. Bounding bytes
    /// instead refused any message past 128 accented characters -- and because
    /// a decode failure is only logged, the player watched it vanish with no
    /// explanation. This is the same defect `s_sign_update` carried.
    #[test]
    fn a_message_of_accented_characters_still_fits() {
        let message = "é".repeat(MAX_CHAT_MESSAGE_LENGTH);
        assert_eq!(
            message.len(),
            MAX_CHAT_MESSAGE_LENGTH * 2,
            "the test message is two bytes per character"
        );

        let packet = SChat::read(&mut Cursor::new(encode(&message).as_slice()))
            .expect("256 accented characters are within vanilla's limit");
        assert_eq!(packet.message, message);
    }

    /// Past the limit is still refused, so the bound was widened and not dropped.
    #[test]
    fn a_message_past_the_limit_is_refused() {
        let message = "a".repeat(MAX_CHAT_MESSAGE_LENGTH * 3 + 1);
        assert!(
            SChat::read(&mut Cursor::new(encode(&message).as_slice())).is_err(),
            "a message beyond 256 UTF-16 units must not decode"
        );
    }
}
