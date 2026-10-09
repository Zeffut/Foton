//! Clientbound display-slot packet: which objective a slot draws.
//!
//! Vanilla parity: `ClientboundSetDisplayObjectivePacket`.

use std::io::{Result, Write};

use foton_macros::ClientPacket;
use foton_registry::packets::play::C_SET_DISPLAY_OBJECTIVE;
use foton_utils::codec::VarInt;
use foton_utils::serial::{PrefixedWrite, WriteTo};

use super::scoreboard_types::DisplaySlot;

#[derive(ClientPacket, Debug, Clone, PartialEq, Eq)]
#[packet_id(Play = C_SET_DISPLAY_OBJECTIVE)]
pub struct CSetDisplayObjective {
    pub slot: DisplaySlot,
    /// Empty clears the slot.
    pub objective_name: String,
}

impl WriteTo for CSetDisplayObjective {
    fn write(&self, writer: &mut impl Write) -> Result<()> {
        VarInt(self.slot.id()).write(writer)?;
        self.objective_name.write_prefixed::<VarInt>(writer)
    }
}
