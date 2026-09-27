//! Clientbound scoreboard-team packet.
//!
//! Vanilla parity: `ClientboundSetPlayerTeamPacket`. One packet id carries
//! create, remove, metadata update, member join and member leave operations.

use foton_macros::ClientPacket;
use foton_registry::packets::play::C_SET_PLAYER_TEAM;
use foton_utils::{
    codec::VarInt,
    serial::{PrefixedWrite, WriteTo},
};
use text_components::TextComponent;

/// Presentation fields sent for team create and metadata update operations.
#[derive(Clone, Debug, PartialEq)]
pub struct PlayerTeamParameters {
    pub display_name: TextComponent,
    pub prefix: TextComponent,
    pub suffix: TextComponent,
    pub name_tag_visibility: TeamVisibility,
    pub collision_rule: TeamCollisionRule,
    pub color: Option<VarInt>,
    pub options: u8,
}

/// Vanilla `Team.Visibility` wire IDs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TeamVisibility {
    Always = 0,
    Never = 1,
    HideForOtherTeams = 2,
    HideForOwnTeam = 3,
}

/// Vanilla `Team.CollisionRule` wire IDs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TeamCollisionRule {
    Always = 0,
    Never = 1,
    PushOtherTeams = 2,
    PushOwnTeam = 3,
}

impl PlayerTeamParameters {
    /// Vanilla's defaults for a newly-created `PlayerTeam`.
    #[must_use]
    pub fn vanilla_defaults(name: &str, prefix: TextComponent, options: u8) -> Self {
        Self {
            display_name: TextComponent::plain(name.to_owned()),
            prefix,
            suffix: TextComponent::new(),
            name_tag_visibility: TeamVisibility::Always,
            collision_rule: TeamCollisionRule::Always,
            color: None,
            options,
        }
    }
}

impl WriteTo for PlayerTeamParameters {
    fn write(&self, writer: &mut impl std::io::Write) -> std::io::Result<()> {
        self.display_name.write(writer)?;
        self.prefix.write(writer)?;
        self.suffix.write(writer)?;
        VarInt(self.name_tag_visibility as i32).write(writer)?;
        VarInt(self.collision_rule as i32).write(writer)?;
        self.color.write(writer)?;
        self.options.write(writer)
    }
}

/// The five vanilla operations carried by `ClientboundSetPlayerTeamPacket`.
#[derive(Clone, Debug, PartialEq)]
pub enum PlayerTeamOperation {
    Create {
        parameters: PlayerTeamParameters,
        players: Vec<String>,
    },
    Remove,
    Update(PlayerTeamParameters),
    AddPlayers(Vec<String>),
    RemovePlayers(Vec<String>),
}

impl PlayerTeamOperation {
    const fn method(&self) -> u8 {
        match self {
            Self::Create { .. } => 0,
            Self::Remove => 1,
            Self::Update(_) => 2,
            Self::AddPlayers(_) => 3,
            Self::RemovePlayers(_) => 4,
        }
    }
}

#[derive(ClientPacket, Clone, Debug, PartialEq)]
#[packet_id(Play = C_SET_PLAYER_TEAM)]
pub struct CSetPlayerTeam {
    pub name: String,
    pub operation: PlayerTeamOperation,
}

impl CSetPlayerTeam {
    #[must_use]
    pub fn create(
        name: impl Into<String>,
        parameters: PlayerTeamParameters,
        players: Vec<String>,
    ) -> Self {
        Self {
            name: name.into(),
            operation: PlayerTeamOperation::Create {
                parameters,
                players,
            },
        }
    }

    #[must_use]
    pub fn remove(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            operation: PlayerTeamOperation::Remove,
        }
    }

    #[must_use]
    pub fn update(name: impl Into<String>, parameters: PlayerTeamParameters) -> Self {
        Self {
            name: name.into(),
            operation: PlayerTeamOperation::Update(parameters),
        }
    }

    #[must_use]
    pub fn add_player(name: impl Into<String>, player: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            operation: PlayerTeamOperation::AddPlayers(vec![player.into()]),
        }
    }

    #[must_use]
    pub fn remove_player(name: impl Into<String>, player: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            operation: PlayerTeamOperation::RemovePlayers(vec![player.into()]),
        }
    }
}

impl WriteTo for CSetPlayerTeam {
    fn write(&self, writer: &mut impl std::io::Write) -> std::io::Result<()> {
        self.name.as_str().write_prefixed::<VarInt>(writer)?;
        self.operation.method().write(writer)?;
        match &self.operation {
            PlayerTeamOperation::Create {
                parameters,
                players,
            } => {
                parameters.write(writer)?;
                write_strings(players, writer)
            }
            PlayerTeamOperation::Remove => Ok(()),
            PlayerTeamOperation::Update(parameters) => parameters.write(writer),
            PlayerTeamOperation::AddPlayers(players)
            | PlayerTeamOperation::RemovePlayers(players) => write_strings(players, writer),
        }
    }
}

fn write_strings(strings: &[String], writer: &mut impl std::io::Write) -> std::io::Result<()> {
    VarInt(strings.len() as i32).write(writer)?;
    for string in strings {
        string.as_str().write_prefixed::<VarInt>(writer)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let parameters = PlayerTeamParameters {
            display_name: TextComponent::plain("Red team"),
            prefix: TextComponent::plain("[R] "),
            suffix: TextComponent::plain("!"),
            name_tag_visibility: TeamVisibility::Always,
            collision_rule: TeamCollisionRule::PushOtherTeams,
            color: Some(VarInt(7)),
            options: 3,
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
}
