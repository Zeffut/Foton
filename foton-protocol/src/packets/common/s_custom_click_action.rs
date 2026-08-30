use std::io::{Cursor, Read};

use foton_macros::{ReadFrom, ServerPacket};
use foton_utils::Identifier;
use foton_utils::codec::VarInt;
use foton_utils::serial::ReadFrom;
use simdnbt::owned::{NbtCompound, NbtTag};

/// A dialog button the player pressed, with whatever the form held.
///
/// Vanilla parity: `ServerboundCustomClickActionPacket`. The payload is
/// length-prefixed on the wire and read under a hard NBT budget, because it is
/// the one packet whose contents a player composes: a client is free to send
/// anything at all here, and the server must not be talked into allocating for
/// it.
#[derive(ReadFrom, ServerPacket, Clone, Debug)]
pub struct SCustomClickAction {
    /// The action id the dialog's button carried.
    pub id: Identifier,
    /// The form's values, keyed by the dialog's input names.
    pub payload: ClickPayload,
}

/// The optional, length-prefixed NBT half of a custom click action.
#[derive(Clone, Debug, Default)]
pub struct ClickPayload(pub Option<NbtCompound>);

impl ClickPayload {
    /// The largest payload a client may send.
    ///
    /// Vanilla parity: the `lengthPrefixed(65536)` of its stream codec.
    const MAX_BYTES: usize = 65_536;

    /// Reads a string field, if the client sent one.
    #[must_use]
    pub fn string(&self, key: &str) -> Option<String> {
        self.0
            .as_ref()
            .and_then(|tag| tag.string(key))
            .map(ToString::to_string)
    }
}

impl ReadFrom for ClickPayload {
    fn read(data: &mut Cursor<&[u8]>) -> Result<Self, std::io::Error> {
        let length = VarInt::read(data)?.0;
        let length = usize::try_from(length).map_err(|_| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, "negative payload length")
        })?;
        if length > Self::MAX_BYTES {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "custom click payload exceeds its maximum length",
            ));
        }
        let mut bytes = vec![0u8; length];
        data.read_exact(&mut bytes)?;

        // Vanilla writes a bare TAG_END for "no payload", so an empty body and
        // a single zero byte both mean the same thing.
        if bytes.first().is_none_or(|first| *first == 0) {
            return Ok(Self(None));
        }

        let mut cursor = Cursor::new(bytes.as_slice());
        Ok(Self(simdnbt::owned::read_tag(&mut cursor).ok().and_then(
            |tag| match tag {
                NbtTag::Compound(compound) => Some(compound),
                _ => None,
            },
        )))
    }
}
