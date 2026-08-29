//! Clientbound award stats packet -- the statistics screen.

use std::io::{Result, Write};

use foton_macros::ClientPacket;
use foton_registry::packets::play::C_AWARD_STATS;
use foton_registry::stat::Stat;
use foton_utils::codec::VarInt;
use foton_utils::serial::WriteTo;

/// The statistics whose counters moved since the client was last told.
///
/// Vanilla parity: `ClientboundAwardStatsPacket`. Vanilla holds an
/// `Object2IntMap`; Foton keeps an ordered vector so the same set of counters
/// always encodes to the same bytes and can be tested against them.
#[derive(ClientPacket, Clone, Debug, PartialEq, Eq)]
#[packet_id(Play = C_AWARD_STATS)]
pub struct CAwardStats {
    /// Each statistic and the value it now holds.
    pub stats: Vec<(Stat, i32)>,
}

impl WriteTo for CAwardStats {
    fn write(&self, writer: &mut impl Write) -> Result<()> {
        // Vanilla: `ByteBufCodecs.map(.., Stat.STREAM_CODEC, VAR_INT)`, and
        // `Stat.STREAM_CODEC` dispatches on the stat type: the type registry id
        // first, then the value id out of whichever registry that type names.
        VarInt(i32::try_from(self.stats.len()).unwrap_or(i32::MAX)).write(writer)?;
        for (stat, value) in &self.stats {
            VarInt(i32::try_from(stat.stat_type).unwrap_or(i32::MAX)).write(writer)?;
            VarInt(i32::try_from(stat.value).unwrap_or(i32::MAX)).write(writer)?;
            VarInt(*value).write(writer)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use foton_registry::init_vanilla_registry;
    use foton_registry::stat::Stat;
    use foton_registry::vanilla_custom_stats;

    use super::*;

    fn encoded(packet: &CAwardStats) -> Vec<u8> {
        let mut bytes = Vec::new();
        packet.write(&mut bytes).expect("packet should encode");
        bytes
    }

    /// Three var-ints per entry, and the stat type comes first: the client
    /// reads the type to know which registry the value id belongs to, so
    /// swapping the two would relabel every statistic on the screen.
    #[test]
    fn a_statistic_is_its_type_its_value_and_its_count() {
        init_vanilla_registry();

        let jump = Stat::custom(&vanilla_custom_stats::JUMP);
        let bytes = encoded(&CAwardStats {
            stats: vec![(jump, 7)],
        });

        assert_eq!(bytes.len(), 4, "one count and three single-byte var-ints");
        assert_eq!(bytes[0], 1);
        assert_eq!(usize::from(bytes[1]), jump.stat_type);
        assert_eq!(usize::from(bytes[2]), jump.value);
        assert_eq!(bytes[3], 7);
    }

    #[test]
    fn an_empty_award_is_a_single_zero() {
        init_vanilla_registry();

        assert_eq!(encoded(&CAwardStats { stats: Vec::new() }), vec![0]);
    }
}
