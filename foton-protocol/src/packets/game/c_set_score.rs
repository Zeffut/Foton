//! Clientbound score packet: one holder's score on one objective.
//!
//! Vanilla parity: `ClientboundSetScorePacket`.

use std::io::{Result, Write};

use foton_macros::ClientPacket;
use foton_registry::packets::play::C_SET_SCORE;
use foton_utils::codec::VarInt;
use foton_utils::serial::{PrefixedWrite, WriteTo};
use text_components::TextComponent;

use super::scoreboard_types::NumberFormat;

#[derive(ClientPacket, Debug, Clone, PartialEq, Eq)]
#[packet_id(Play = C_SET_SCORE)]
pub struct CSetScore {
    pub owner: String,
    pub objective_name: String,
    pub score: i32,
    /// Replaces the holder's name where the objective is drawn.
    pub display: Option<TextComponent>,
    /// Replaces the objective's number format for this score.
    pub number_format: Option<NumberFormat>,
}

impl WriteTo for CSetScore {
    fn write(&self, writer: &mut impl Write) -> Result<()> {
        self.owner.write_prefixed::<VarInt>(writer)?;
        self.objective_name.write_prefixed::<VarInt>(writer)?;
        VarInt(self.score).write(writer)?;
        self.display.write(writer)?;
        self.number_format.write(writer)
    }
}
