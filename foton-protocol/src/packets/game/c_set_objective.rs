//! Clientbound objective packet: an objective's existence and look.
//!
//! Vanilla parity: `ClientboundSetObjectivePacket`. A byte after the name
//! selects add, remove or change; add and change carry the objective's
//! display name, render type and number format.

use std::io::{Result, Write};

use foton_macros::ClientPacket;
use foton_registry::packets::play::C_SET_OBJECTIVE;
use foton_utils::codec::VarInt;
use foton_utils::serial::{PrefixedWrite, WriteTo};
use text_components::TextComponent;

use super::scoreboard_types::{NumberFormat, ObjectiveRenderType};

/// What the packet does to the named objective.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObjectiveMethod {
    /// Creates the objective.
    Add(ObjectiveLook),
    /// Removes the objective, its scores and its display slots.
    Remove,
    /// Replaces the objective's look.
    Change(ObjectiveLook),
}

/// The part of an objective a client draws.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectiveLook {
    pub display_name: TextComponent,
    pub render_type: ObjectiveRenderType,
    pub number_format: Option<NumberFormat>,
}

impl WriteTo for ObjectiveLook {
    fn write(&self, writer: &mut impl Write) -> Result<()> {
        self.display_name.write(writer)?;
        VarInt(self.render_type as i32).write(writer)?;
        self.number_format.write(writer)
    }
}

#[derive(ClientPacket, Debug, Clone, PartialEq, Eq)]
#[packet_id(Play = C_SET_OBJECTIVE)]
pub struct CSetObjective {
    pub name: String,
    pub method: ObjectiveMethod,
}

impl WriteTo for CSetObjective {
    fn write(&self, writer: &mut impl Write) -> Result<()> {
        self.name.write_prefixed::<VarInt>(writer)?;
        match &self.method {
            ObjectiveMethod::Add(look) => {
                0_i8.write(writer)?;
                look.write(writer)
            }
            ObjectiveMethod::Remove => 1_i8.write(writer),
            ObjectiveMethod::Change(look) => {
                2_i8.write(writer)?;
                look.write(writer)
            }
        }
    }
}
