//! Clientbound team packet: a team's look, and who is on it.
//!
//! Vanilla parity: `ClientboundSetPlayerTeamPacket`. A byte after the team's
//! name selects the operation; adding and changing carry the team's
//! parameters, and adding, joining and leaving carry a list of entries.
//! Without it the client never learns a team exists, so a prefix, a suffix or
//! a colored name set on the server is never drawn.

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

/// The color a team's member names are drawn in.
///
/// Named colors retain their ids 0–15. Reset is an API/persistence value;
/// the 26.2 `TeamColor` codec represents it as an absent optional color.
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
    /// Every color, in `ChatFormatting` order.
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

    /// Legacy API ordinal; Reset is never written as this value on the wire.
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
        self.prefix.write(writer)?;
        self.suffix.write(writer)?;
        let options =
            u8::from(self.allow_friendly_fire) | (u8::from(self.see_friendly_invisibles) << 1);
        VarInt(self.name_tag_visibility as i32).write(writer)?;
        VarInt(self.collision_rule as i32).write(writer)?;
        let color = (self.color != TeamColor::Reset).then(|| VarInt(self.color.ordinal()));
        color.write(writer)?;
        options.write(writer)
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

impl CSetPlayerTeam {
    #[must_use]
    pub fn create(
        name: impl Into<String>,
        parameters: TeamParameters,
        players: Vec<String>,
    ) -> Self {
        Self {
            name: name.into(),
            method: TeamMethod::Add(parameters, players),
        }
    }

    #[must_use]
    pub fn remove(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            method: TeamMethod::Remove,
        }
    }

    #[must_use]
    pub fn update(name: impl Into<String>, parameters: TeamParameters) -> Self {
        Self {
            name: name.into(),
            method: TeamMethod::Change(parameters),
        }
    }

    #[must_use]
    pub fn add_player(name: impl Into<String>, player: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            method: TeamMethod::Join(vec![player.into()]),
        }
    }

    #[must_use]
    pub fn remove_player(name: impl Into<String>, player: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            method: TeamMethod::Leave(vec![player.into()]),
        }
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

    /// The default color is `ChatFormatting.RESET`, ordinal 21, not 16: the
    /// five formatting codes sit between the colors and reset.
    #[test]
    fn reset_is_ordinal_twenty_one() {
        assert_eq!(TeamColor::Reset.ordinal(), 21);
        assert_eq!(TeamColor::White.ordinal(), 15);
    }

    #[test]
    fn operation_tags_and_player_lists_match_vanilla() {
        let mut remove = Vec::new();
        CSetPlayerTeam::remove("red")
            .write(&mut remove)
            .expect("packet should encode");
        assert_eq!(remove, [3, b'r', b'e', b'd', 1]);

        let mut add_player = Vec::new();
        CSetPlayerTeam::add_player("red", "Steve")
            .write(&mut add_player)
            .expect("packet should encode");
        assert_eq!(
            add_player,
            [3, b'r', b'e', b'd', 3, 1, 5, b'S', b't', b'e', b'v', b'e']
        );

        let mut remove_player = Vec::new();
        CSetPlayerTeam::remove_player("red", "Steve")
            .write(&mut remove_player)
            .expect("packet should encode");
        assert_eq!(remove_player[4], 4);
    }

    #[test]
    fn create_and_update_encode_parameters_in_vanilla_order() {
        let parameters = TeamParameters {
            display_name: TextComponent::plain("Red team"),
            prefix: TextComponent::plain("[R] "),
            suffix: TextComponent::plain("!"),
            name_tag_visibility: TeamVisibility::Always,
            collision_rule: TeamCollisionRule::PushOtherTeams,
            color: TeamColor::Gray,
            allow_friendly_fire: true,
            see_friendly_invisibles: true,
        };

        let mut encoded_parameters = Vec::new();
        parameters
            .display_name
            .write(&mut encoded_parameters)
            .expect("display name should encode");
        parameters
            .prefix
            .write(&mut encoded_parameters)
            .expect("prefix should encode");
        parameters
            .suffix
            .write(&mut encoded_parameters)
            .expect("suffix should encode");
        // Team.Visibility and CollisionRule use ByteBufCodecs.idMapper in 26.2.
        encoded_parameters.extend_from_slice(b"\x00\x02\x01\x07\x03");

        let mut expected_create = b"\x03red\x00".to_vec();
        expected_create.extend_from_slice(&encoded_parameters);
        expected_create.extend_from_slice(b"\x01\x05Steve");
        let mut actual_create = Vec::new();
        CSetPlayerTeam::create("red", parameters.clone(), vec!["Steve".to_owned()])
            .write(&mut actual_create)
            .expect("create packet should encode");
        assert_eq!(actual_create, expected_create);

        let mut expected_update = b"\x03red\x02".to_vec();
        expected_update.extend_from_slice(&encoded_parameters);
        let mut actual_update = Vec::new();
        CSetPlayerTeam::update("red", parameters)
            .write(&mut actual_update)
            .expect("update packet should encode");
        assert_eq!(actual_update, expected_update);
    }

    #[test]
    fn reset_color_is_absent_and_flags_follow_the_optional_color() {
        let mut parameters = TeamParameters {
            display_name: TextComponent::plain("Team"),
            prefix: TextComponent::plain("["),
            suffix: TextComponent::plain("]"),
            name_tag_visibility: TeamVisibility::HideForOwnTeam,
            collision_rule: TeamCollisionRule::PushOwnTeam,
            color: TeamColor::Reset,
            allow_friendly_fire: false,
            see_friendly_invisibles: true,
        };
        let mut text = Vec::new();
        for component in [
            &parameters.display_name,
            &parameters.prefix,
            &parameters.suffix,
        ] {
            component.write(&mut text).expect("component wire");
        }
        for (color, tail) in [
            (TeamColor::Reset, &[3, 3, 0, 2][..]),
            (TeamColor::White, &[3, 3, 1, 15, 2][..]),
        ] {
            parameters.color = color;
            let mut encoded = Vec::new();
            parameters.write(&mut encoded).expect("parameters wire");
            assert_eq!(&encoded[..text.len()], text.as_slice());
            assert_eq!(&encoded[text.len()..], tail);
        }
    }
}
