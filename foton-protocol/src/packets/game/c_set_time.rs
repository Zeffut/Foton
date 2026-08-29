use foton_macros::ClientPacket;
use foton_registry::packets::play::C_SET_TIME;
use foton_utils::codec::{VarInt, VarLong};
use foton_utils::serial::WriteTo;
use std::io::{Result, Write};

#[derive(ClientPacket, Clone, Debug)]
#[packet_id(Play = C_SET_TIME)]
pub struct CSetTime {
    pub game_time: i64,
    /// (`clock_registry_id`, `total_ticks`, `partial_tick`, rate)
    pub clock_updates: Vec<(i32, i64, f32, f32)>,
}

impl WriteTo for CSetTime {
    fn write(&self, writer: &mut impl Write) -> Result<()> {
        self.game_time.write(writer)?;
        VarInt(self.clock_updates.len() as i32).write(writer)?;
        for &(clock_id, total_ticks, partial_tick, rate) in &self.clock_updates {
            VarInt(clock_id).write(writer)?;
            VarLong(total_ticks).write(writer)?;
            partial_tick.write(writer)?;
            rate.write(writer)?;
        }
        Ok(())
    }
}

impl CSetTime {
    #[must_use]
    pub const fn new(game_time: i64, clock_updates: Vec<(i32, i64, f32, f32)>) -> Self {
        Self {
            game_time,
            clock_updates,
        }
    }
}
