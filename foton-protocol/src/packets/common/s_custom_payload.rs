use std::io::{Cursor, Read};

use foton_macros::{ReadFrom, ServerPacket};
use foton_utils::Identifier;

use foton_utils::serial::ReadFrom;

#[derive(ReadFrom, ServerPacket, Clone, Debug)]
pub struct SCustomPayload {
    pub identifier: Identifier,
    //#[read(as = "vec")]
    pub payload: Payload,
}

#[derive(Clone, Debug)]
pub struct Payload(pub Vec<u8>);

/// Largest plugin-channel payload the server accepts, in bytes.
///
/// Vanilla parity: `ServerboundCustomPayloadPacket.MAX_PAYLOAD_SIZE`.
pub const MAX_PAYLOAD_SIZE: usize = 32767;

impl ReadFrom for Payload {
    fn read(data: &mut Cursor<&[u8]>) -> Result<Self, std::io::Error> {
        // Vanilla refuses anything past 32767 bytes. Reading to the end instead
        // accepted a whole 8 MiB packet frame, 256 times the limit, on a channel
        // the connection may use before it even has a player in the world.
        let mut buf = vec![];
        data.take(MAX_PAYLOAD_SIZE as u64 + 1)
            .read_to_end(&mut buf)?;
        if buf.len() > MAX_PAYLOAD_SIZE {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "custom payload exceeds the maximum size",
            ));
        }
        Ok(Self(buf))
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use foton_utils::serial::ReadFrom;

    use super::{MAX_PAYLOAD_SIZE, Payload};

    /// A payload at the limit decodes; one byte past it does not.
    ///
    /// Vanilla caps this at `MAX_PAYLOAD_SIZE` and rejects the rest. Reading to
    /// the end of the frame instead accepted a whole 8 MiB packet -- 256 times
    /// the limit -- on a channel a connection may use before it has a player in
    /// the world at all.
    #[test]
    fn payload_is_capped_at_vanillas_limit() {
        let at_limit = vec![0u8; MAX_PAYLOAD_SIZE];
        let decoded = Payload::read(&mut Cursor::new(at_limit.as_slice()))
            .expect("a payload at the limit is legal");
        assert_eq!(decoded.0.len(), MAX_PAYLOAD_SIZE);

        let past_limit = vec![0u8; MAX_PAYLOAD_SIZE + 1];
        assert!(
            Payload::read(&mut Cursor::new(past_limit.as_slice())).is_err(),
            "a payload past the limit must be refused, not truncated"
        );
    }
}
