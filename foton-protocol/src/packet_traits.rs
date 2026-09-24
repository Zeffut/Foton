//! # Foton Protocol Packet Traits
//!
//! This module contains the traits for the packets.
use std::{
    io::{Cursor, Read, Write},
    num::NonZeroU32,
    sync::Arc,
};

use flate2::{Compression, read::ZlibDecoder, write::ZlibEncoder};
use foton_utils::{
    FrontVec,
    codec::VarInt,
    serial::{ReadFrom, WriteTo},
};
use serde::Deserialize;

use crate::utils::{ConnectionProtocol, MAX_PACKET_DATA_SIZE, MAX_PACKET_SIZE, PacketError};

// These are the network read/write traits
/// A trait for packets sent from the server to the client.
pub trait ServerPacket: ReadFrom {
    /// Reads a packet from the given data.
    fn read_packet(data: &mut Cursor<&[u8]>) -> Result<Self, PacketError> {
        Self::read(data).map_err(PacketError::from)
    }
}

/// A trait for packets sent from the client to the server.
pub trait ClientPacket: WriteTo {
    /// Writes the packet to the given writer.
    ///
    /// # Errors
    /// - If the packet fails to write.
    /// - If the protocol is invalid.
    fn write_packet(
        &self,
        writer: &mut impl Write,
        protocol: ConnectionProtocol,
    ) -> Result<(), PacketError> {
        let packet_id = self
            .get_id(protocol)
            .ok_or(PacketError::InvalidProtocol(format!(
                "Invalid protocol {protocol:?}"
            )))?;
        VarInt(packet_id).write(writer)?;
        self.write(writer).map_err(PacketError::from)
    }

    /// Gets the ID of the packet for the given protocol.
    fn get_id(&self, protocol: ConnectionProtocol) -> Option<i32>;
}

/// Information about compression.
#[derive(Copy, Clone, Debug, Deserialize)]
pub struct CompressionInfo {
    /// The compression threshold used when compression is enabled.
    /// Its an `NonZeroU32` to allow for nullptr optimization in `Option<Self>` cases
    pub threshold: NonZeroU32,
    /// A value between `0..9`.
    /// `1` = Optimize for the best speed of encoding.
    /// `9` = Optimize for the size of data being encoded.
    pub level: i32,
}

impl Default for CompressionInfo {
    #[expect(
        clippy::unwrap_used,
        reason = "256 is a known nonzero compression threshold"
    )]
    fn default() -> Self {
        Self {
            threshold: NonZeroU32::new(256).unwrap(),
            level: 4,
        }
    }
}

/// Represents an encoded clientbound packet, optionally applying compression based on threshold and level.
///
/// # Packet Size Limits
/// - Maximum packet size: 2097151 bytes (2^21 - 1, max 3-byte `VarInt`)
/// - Maximum uncompressed size for compressed packets: 8388608 bytes (2^23)
/// - Length field must not exceed 3 bytes
///
/// # Packet Encoding Format
///
/// **Without Compression:**
/// ```text
/// [Length: VarInt]     Length of (Packet ID + Data)
/// [Packet ID: VarInt]  Protocol ID from packet report
/// [Data: Byte Array]   Packet payload
/// ```
///
/// **With Compression (size >= threshold):**
/// ```text
/// [Length: VarInt]     Length of (Data Length + compressed data)
/// [Data Length: VarInt] Length of uncompressed (Packet ID + Data)
/// [Compressed Data]    zlib compressed (Packet ID + Data)
/// ```
///
/// **With Compression (size < threshold):**
/// ```text
/// [Length: VarInt]     Length of (Data Length + uncompressed data)
/// [Data Length: VarInt] 0 to indicate uncompressed
/// [Packet ID: VarInt]  Protocol ID from packet report
/// [Data: Byte Array]   Uncompressed packet payload
/// ```
///
/// Compression is only applied when:
/// 1. Compression is enabled via Set Compression packet
/// 2. The uncompressed data length meets/exceeds the threshold
/// 3. The threshold is non-negative
#[derive(Clone)]
pub struct EncodedPacket {
    // This is optimized for reduces allocation
    /// The encoded data.
    pub encoded_data: Arc<FrontVec>,
    /// The packet id, read before the frame was built.
    ///
    /// Kept beside the bytes because once the frame is compressed the id sits
    /// inside the zlib stream: a caller routing on it would otherwise have to
    /// inflate every packet just to learn which one it is.
    id: i32,
}

impl EncodedPacket {
    fn from_data_uncompressed(mut packet_data: FrontVec, id: i32) -> Result<Self, PacketError> {
        let data_len = packet_data.len();
        let varint_size = VarInt::written_size(data_len as i32);

        let complete_len = varint_size + data_len;
        if complete_len > MAX_PACKET_SIZE {
            return Err(PacketError::TooLong(complete_len));
        }

        VarInt(data_len as i32).set_in_front(&mut packet_data, varint_size);

        Ok(Self {
            encoded_data: Arc::new(packet_data),
            id,
        })
    }

    fn from_packet_data(
        mut packet_data: FrontVec,
        compression: CompressionInfo,
        id: i32,
    ) -> Result<Self, PacketError> {
        let data_len = packet_data.len();
        // We dont need any more size check to convert to i32 as MAX_PACKET_DATA_SIZE < i32::MAX
        if data_len + VarInt::MAX_SIZE * 2 > MAX_PACKET_DATA_SIZE {
            Err(PacketError::TooLong(data_len))?;
        }

        if data_len >= compression.threshold.get() as _ {
            let mut buf = FrontVec::new(10);
            let mut compressor =
                ZlibEncoder::new(&mut buf, Compression::new(compression.level as u32));

            compressor
                .write_all(&packet_data)
                .map_err(|e| PacketError::CompressionFailed(e.to_string()))?;
            compressor
                .finish()
                .map_err(|e| PacketError::CompressionFailed(e.to_string()))?;

            // compressed data cant be larger so we dont need to check the size again
            let varint_size = VarInt::written_size(data_len as i32);
            let full_len = varint_size + buf.len();
            let full_varint_size = VarInt::written_size(full_len as i32);

            VarInt(data_len as i32).set_in_front(&mut buf, varint_size);
            VarInt(full_len as i32).set_in_front(&mut buf, full_varint_size);
            log::trace!(
                "data length: {data_len}, full length: {full_len}, varint size: {varint_size}, full varint size: {full_varint_size}"
            );

            Ok(Self {
                encoded_data: Arc::new(buf),
                id,
            })
        } else {
            // Pushed before data:
            // Length of (Data Length) + length of compressed (Packet ID + Data)
            // 0 to indicate uncompressed

            let data_len_with_header = data_len + 1;
            let varint_size = VarInt::written_size(data_len_with_header as i32);

            VarInt(0).set_in_front(&mut packet_data, 1);
            VarInt(data_len_with_header as i32).set_in_front(&mut packet_data, varint_size);

            Ok(Self {
                encoded_data: Arc::new(packet_data),
                id,
            })
        }
    }

    /// Creates a new `EncodedPacket` from a bare packet.
    ///
    /// # Errors
    /// - If the packet fails to write.
    /// - If the packet fails to compress.
    pub fn from_bare<P: ClientPacket>(
        packet: P,
        compression: Option<CompressionInfo>,
        protocol: ConnectionProtocol,
    ) -> Result<Self, PacketError> {
        let buf = Self::write_vec(packet, protocol)?;
        Self::from_data(buf, compression)
    }

    fn write_vec<P: ClientPacket>(
        packet: P,
        protocol: ConnectionProtocol,
    ) -> Result<FrontVec, PacketError> {
        let mut buf = FrontVec::new(6);
        packet.write_packet(&mut buf, protocol)?;
        Ok(buf)
    }

    fn from_data(buf: FrontVec, compression: Option<CompressionInfo>) -> Result<Self, PacketError> {
        // Every buffer reaching here was written id first, by `write_packet`
        // or by `from_id_and_payload`.
        let id = VarInt::read(&mut Cursor::new(buf.as_slice()))?.0;
        if let Some(compression) = compression {
            Self::from_packet_data(buf, compression, id)
        } else {
            Self::from_data_uncompressed(buf, id)
        }
    }

    /// The protocol id of the packet inside the frame.
    #[must_use]
    pub const fn id(&self) -> i32 {
        self.id
    }

    /// Frames an id and a payload that were produced outside a `ClientPacket`.
    ///
    /// This is how a packet a plugin rewrote goes back on the wire: the bytes
    /// are already in protocol form, only the framing is Foton's.
    ///
    /// # Errors
    /// If the packet is too long or fails to compress.
    pub fn from_id_and_payload(
        id: i32,
        payload: &[u8],
        compression: Option<CompressionInfo>,
    ) -> Result<Self, PacketError> {
        let mut buf = FrontVec::capacity(6, payload.len() + VarInt::MAX_SIZE);
        VarInt(id).write(&mut buf)?;
        buf.extend_from_slice(payload);
        Self::from_data(buf, compression)
    }

    /// The payload, without its id, as it was before framing and compression.
    ///
    /// `compression` must be the setting the packet was framed with; the frame
    /// itself does not say whether a data-length field is present.
    ///
    /// # Errors
    /// If the frame is malformed or fails to decompress.
    pub fn payload(&self, compression: Option<CompressionInfo>) -> Result<Vec<u8>, PacketError> {
        let frame = self.encoded_data.as_slice();
        let mut cursor = Cursor::new(frame);
        VarInt::read(&mut cursor)?;
        let body = if compression.is_some() {
            let data_len = VarInt::read(&mut cursor)?.0;
            let rest = &frame[cursor.position() as usize..];
            if data_len > 0 {
                let data_len = usize::try_from(data_len)
                    .map_err(|_| PacketError::TooLong(MAX_PACKET_DATA_SIZE))?;
                if data_len > MAX_PACKET_DATA_SIZE {
                    return Err(PacketError::TooLong(data_len));
                }
                let mut inflated = vec![0; data_len];
                ZlibDecoder::new(rest)
                    .read_exact(&mut inflated)
                    .map_err(|e| PacketError::DecompressionFailed(e.to_string()))?;
                inflated
            } else {
                rest.to_vec()
            }
        } else {
            frame[cursor.position() as usize..].to_vec()
        };
        let mut body_cursor = Cursor::new(body.as_slice());
        VarInt::read(&mut body_cursor)?;
        Ok(body[body_cursor.position() as usize..].to_vec())
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use super::{CompressionInfo, EncodedPacket};

    fn compression(threshold: u32) -> Option<CompressionInfo> {
        NonZeroU32::new(threshold).map(|threshold| CompressionInfo {
            threshold,
            level: 4,
        })
    }

    /// A packet a plugin rewrote is framed from raw bytes and read back the
    /// same way; the three frame shapes must all survive that round trip, or
    /// a rewrite silently corrupts whatever the client receives next.
    #[test]
    fn payload_survives_every_frame_shape() {
        let small = vec![7_u8; 10];
        let large: Vec<u8> = (0..4096_u32).map(|i| (i % 251) as u8).collect();
        for (payload, setting) in [
            (&small, None),
            (&large, None),
            (&small, compression(256)),
            (&large, compression(256)),
        ] {
            let packet = EncodedPacket::from_id_and_payload(99, payload, setting)
                .expect("a small packet frames");
            assert_eq!(packet.id(), 99);
            assert_eq!(
                &packet.payload(setting).expect("the frame decodes"),
                payload
            );
        }
    }
}
