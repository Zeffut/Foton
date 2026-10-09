//! Clientbound score reset: one score, or every score of a holder.
//!
//! Vanilla parity: `ClientboundResetScorePacket`.

use std::io::{Result, Write};

use foton_macros::ClientPacket;
use foton_registry::packets::play::C_RESET_SCORE;
use foton_utils::codec::VarInt;
use foton_utils::serial::{PrefixedWrite, WriteTo};

#[derive(ClientPacket, Debug, Clone, PartialEq, Eq)]
#[packet_id(Play = C_RESET_SCORE)]
pub struct CResetScore {
    pub owner: String,
    /// `None` resets the holder's scores on every objective.
    pub objective_name: Option<String>,
}

impl WriteTo for CResetScore {
    fn write(&self, writer: &mut impl Write) -> Result<()> {
        self.owner.write_prefixed::<VarInt>(writer)?;
        match &self.objective_name {
            Some(name) => {
                true.write(writer)?;
                name.write_prefixed::<VarInt>(writer)
            }
            None => false.write(writer),
        }
    }
}
