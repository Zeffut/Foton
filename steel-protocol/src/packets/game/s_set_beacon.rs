use std::io::{Cursor, Result};

use steel_macros::ServerPacket;
use steel_utils::codec::VarInt;
use steel_utils::serial::ReadFrom;

/// The two effects a player picked in a beacon menu.
///
/// Vanilla parity: `ServerboundSetBeaconPacket`. Each effect is an optional
/// mob-effect registry id; absent means "no effect", which is how a beacon is
/// set back to doing nothing.
#[derive(ServerPacket, Clone, Debug)]
pub struct SSetBeacon {
    pub primary: Option<i32>,
    pub secondary: Option<i32>,
}

impl ReadFrom for SSetBeacon {
    fn read(data: &mut Cursor<&[u8]>) -> Result<Self> {
        let primary = Option::<VarInt>::read(data)?.map(|id| id.0);
        let secondary = Option::<VarInt>::read(data)?.map(|id| id.0);
        Ok(Self { primary, secondary })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_both_effects_absent() {
        let mut data = Cursor::new([0, 0].as_slice());
        let packet = SSetBeacon::read(&mut data).expect("packet should parse");

        assert_eq!(packet.primary, None);
        assert_eq!(packet.secondary, None);
    }

    /// A level-three beacon can set a primary effect with no secondary, which
    /// is the common case and the one that would break if the two optionals
    /// were read in the wrong order.
    #[test]
    fn reads_a_primary_with_no_secondary() {
        let mut data = Cursor::new([1, 5, 0].as_slice());
        let packet = SSetBeacon::read(&mut data).expect("packet should parse");

        assert_eq!(packet.primary, Some(5));
        assert_eq!(packet.secondary, None);
    }

    #[test]
    fn reads_both_effects() {
        let mut data = Cursor::new([1, 1, 1, 10].as_slice());
        let packet = SSetBeacon::read(&mut data).expect("packet should parse");

        assert_eq!(packet.primary, Some(1));
        assert_eq!(packet.secondary, Some(10));
    }
}
