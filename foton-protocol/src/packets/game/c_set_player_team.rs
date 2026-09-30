//! Clientbound team packet: a team's look, and who is on it.
//!
//! Vanilla parity: `ClientboundSetPlayerTeamPacket`. A byte after the team's
//! name selects the operation; adding and changing carry the team's
//! parameters, and adding, joining and leaving carry a list of entries.
//! Without it the client never learns a team exists, so a prefix, a suffix or
//! a coloured name set on the server is never drawn.

use std::io::{Result, Write};

use foton_macros::ClientPacket;
use foton_registry::packets::play::C_SET_PLAYER_TEAM;
use foton_utils::codec::VarInt;
use foton_utils::serial::{PrefixedWrite, WriteTo};
use serde::{Deserialize, Serialize};
use text_components::TextComponent;

/// Who sees the name tags of a team's members.
///
/// Vanilla parity: `Team.Visibility`; the wire form is its id, which is the
/// declaration order.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TeamVisibility {
    #[default]
    Always,
    Never,
    HideForOtherTeams,
    HideForOwnTeam,
}

/// Whom a team's members push.
///
/// Vanilla parity: `Team.CollisionRule`; the wire form is its id.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TeamCollisionRule {
    #[default]
    Always,
    Never,
    PushOtherTeams,
    PushOwnTeam,
}

/// The colour a team's member names are drawn in.
///
/// Vanilla parity: the colour entries of `ChatFormatting`, whose ordinal is
/// the wire form; `Reset`, vanilla's default, is ordinal 21, after the five
/// formatting codes a team cannot take.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TeamColor {
    Black,
    DarkBlue,
    DarkGreen,
    DarkAqua,
    DarkRed,
    DarkPurple,
    Gold,
    Gray,
    DarkGray,
    Blue,
    Green,
    Aqua,
    Red,
    LightPurple,
    Yellow,
    White,
    #[default]
    Reset,
}

impl TeamColor {
    /// Every colour, in `ChatFormatting` order.
    pub const VALUES: [Self; 17] = [
        Self::Black,
        Self::DarkBlue,
        Self::DarkGreen,
        Self::DarkAqua,
        Self::DarkRed,
        Self::DarkPurple,
        Self::Gold,
        Self::Gray,
        Self::DarkGray,
        Self::Blue,
        Self::Green,
        Self::Aqua,
        Self::Red,
        Self::LightPurple,
        Self::Yellow,
        Self::White,
        Self::Reset,
    ];

    /// The `ChatFormatting` ordinal the packet carries.
    #[must_use]
    pub const fn ordinal(self) -> i32 {
        match self {
            Self::Reset => 21,
            color => color as i32,
        }
    }
}

/// A team's settable look and rules.
///
/// Vanilla parity: `ClientboundSetPlayerTeamPacket.Parameters`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeamParameters {
    pub display_name: TextComponent,
    pub allow_friendly_fire: bool,
    pub see_friendly_invisibles: bool,
    pub name_tag_visibility: TeamVisibility,
    pub collision_rule: TeamCollisionRule,
    pub color: TeamColor,
    pub prefix: TextComponent,
    pub suffix: TextComponent,
}

impl WriteTo for TeamParameters {
    fn write(&self, writer: &mut impl Write) -> Result<()> {
        self.display_name.write(writer)?;
        let options =
            u8::from(self.allow_friendly_fire) | (u8::from(self.see_friendly_invisibles) << 1);
        options.write(writer)?;
        VarInt(self.name_tag_visibility as i32).write(writer)?;
        VarInt(self.collision_rule as i32).write(writer)?;
        VarInt(self.color.ordinal()).write(writer)?;
        self.prefix.write(writer)?;
        self.suffix.write(writer)
    }
}

/// What the packet does to the named team.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TeamMethod {
    /// Creates the team with these members.
    Add(TeamParameters, Vec<String>),
    /// Removes the team.
    Remove,
    /// Replaces the team's parameters.
    Change(TeamParameters),
    /// Puts entries on the team.
    Join(Vec<String>),
    /// Takes entries off the team.
    Leave(Vec<String>),
}

#[derive(ClientPacket, Debug, Clone, PartialEq, Eq)]
#[packet_id(Play = C_SET_PLAYER_TEAM)]
pub struct CSetPlayerTeam {
    pub name: String,
    pub method: TeamMethod,
}

impl WriteTo for CSetPlayerTeam {
    fn write(&self, writer: &mut impl Write) -> Result<()> {
        self.name.write_prefixed::<VarInt>(writer)?;
        let (method, parameters, entries): (i8, Option<&TeamParameters>, Option<&Vec<String>>) =
            match &self.method {
                TeamMethod::Add(parameters, entries) => (0, Some(parameters), Some(entries)),
                TeamMethod::Remove => (1, None, None),
                TeamMethod::Change(parameters) => (2, Some(parameters), None),
                TeamMethod::Join(entries) => (3, None, Some(entries)),
                TeamMethod::Leave(entries) => (4, None, Some(entries)),
            };
        method.write(writer)?;
        if let Some(parameters) = parameters {
            parameters.write(writer)?;
        }
        if let Some(entries) = entries {
            VarInt(i32::try_from(entries.len()).unwrap_or(i32::MAX)).write(writer)?;
            for entry in entries {
                entry.write_prefixed::<VarInt>(writer)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encoded(packet: &CSetPlayerTeam) -> Vec<u8> {
        let mut bytes = Vec::new();
        packet
            .write(&mut bytes)
            .expect("writing to a Vec cannot fail");
        bytes
    }

    /// Name, then the method byte, then the entries -- nothing between them
    /// for a join, which carries no parameters.
    #[test]
    fn a_join_is_the_name_the_method_and_the_entries() {
        let packet = CSetPlayerTeam {
            name: "red".to_owned(),
            method: TeamMethod::Join(vec!["alice".to_owned()]),
        };
        assert_eq!(
            encoded(&packet),
            [&[3, b'r', b'e', b'd', 3, 1, 5][..], b"alice"].concat()
        );
    }

    /// Removal is the name and the method byte alone.
    #[test]
    fn a_removal_carries_nothing_else() {
        let packet = CSetPlayerTeam {
            name: "red".to_owned(),
            method: TeamMethod::Remove,
        };
        assert_eq!(encoded(&packet), [3, b'r', b'e', b'd', 1]);
    }

    /// The default colour is `ChatFormatting.RESET`, ordinal 21, not 16: the
    /// five formatting codes sit between the colours and reset.
    #[test]
    fn reset_is_ordinal_twenty_one() {
        assert_eq!(TeamColor::Reset.ordinal(), 21);
        assert_eq!(TeamColor::White.ordinal(), 15);
    }
}
