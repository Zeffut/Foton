//! Value types shared by the clientbound scoreboard packets.
//!
//! Vanilla parity: `DisplaySlot`, `ObjectiveCriteria.RenderType` and the
//! `net.minecraft.network.chat.numbers` number formats. They live beside the
//! packets because the packets define their wire form, and the scoreboard
//! stores the same values.

use std::io::{Result, Write};

use foton_utils::codec::VarInt;
use foton_utils::serial::{WriteTo, nbt_stream::LimitedWriter, text_stream::write_style};
use serde::{Deserialize, Serialize};
use text_components::{TextComponent, format::Format, interactivity::Interactivity};

/// Where a scoreboard objective is drawn.
///
/// Vanilla parity: `DisplaySlot`; the wire form is its id, which is the
/// declaration order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DisplaySlot {
    List,
    Sidebar,
    BelowName,
    TeamBlack,
    TeamDarkBlue,
    TeamDarkGreen,
    TeamDarkAqua,
    TeamDarkRed,
    TeamDarkPurple,
    TeamGold,
    TeamGray,
    TeamDarkGray,
    TeamBlue,
    TeamGreen,
    TeamAqua,
    TeamRed,
    TeamLightPurple,
    TeamYellow,
    TeamWhite,
}

impl DisplaySlot {
    /// Every slot, in id order.
    pub const VALUES: [Self; 19] = [
        Self::List,
        Self::Sidebar,
        Self::BelowName,
        Self::TeamBlack,
        Self::TeamDarkBlue,
        Self::TeamDarkGreen,
        Self::TeamDarkAqua,
        Self::TeamDarkRed,
        Self::TeamDarkPurple,
        Self::TeamGold,
        Self::TeamGray,
        Self::TeamDarkGray,
        Self::TeamBlue,
        Self::TeamGreen,
        Self::TeamAqua,
        Self::TeamRed,
        Self::TeamLightPurple,
        Self::TeamYellow,
        Self::TeamWhite,
    ];

    /// `DisplaySlot.id`, the value sent on the wire.
    #[must_use]
    pub const fn id(self) -> i32 {
        self as i32
    }

    /// `DisplaySlot.getSerializedName`, the spelling commands and saves use.
    #[must_use]
    pub const fn serialized_name(self) -> &'static str {
        match self {
            Self::List => "list",
            Self::Sidebar => "sidebar",
            Self::BelowName => "below_name",
            Self::TeamBlack => "sidebar.team.black",
            Self::TeamDarkBlue => "sidebar.team.dark_blue",
            Self::TeamDarkGreen => "sidebar.team.dark_green",
            Self::TeamDarkAqua => "sidebar.team.dark_aqua",
            Self::TeamDarkRed => "sidebar.team.dark_red",
            Self::TeamDarkPurple => "sidebar.team.dark_purple",
            Self::TeamGold => "sidebar.team.gold",
            Self::TeamGray => "sidebar.team.gray",
            Self::TeamDarkGray => "sidebar.team.dark_gray",
            Self::TeamBlue => "sidebar.team.blue",
            Self::TeamGreen => "sidebar.team.green",
            Self::TeamAqua => "sidebar.team.aqua",
            Self::TeamRed => "sidebar.team.red",
            Self::TeamLightPurple => "sidebar.team.light_purple",
            Self::TeamYellow => "sidebar.team.yellow",
            Self::TeamWhite => "sidebar.team.white",
        }
    }

    /// `DisplaySlot.CODEC.byName`.
    #[must_use]
    pub fn by_name(name: &str) -> Option<Self> {
        Self::VALUES
            .into_iter()
            .find(|slot| slot.serialized_name() == name)
    }
}

/// How an objective's value is drawn.
///
/// Vanilla parity: `ObjectiveCriteria.RenderType`; the wire form is its
/// ordinal.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObjectiveRenderType {
    #[default]
    Integer,
    Hearts,
}

impl ObjectiveRenderType {
    /// Every render type, in declaration order.
    pub const VALUES: [Self; 2] = [Self::Integer, Self::Hearts];

    /// `RenderType.getId`, the spelling `/scoreboard ... rendertype` takes.
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::Integer => "integer",
            Self::Hearts => "hearts",
        }
    }
}

/// The formatting fields of a text component, without its content.
///
/// Vanilla parity: `Style`, whose codec shares its field names with the
/// component codec, so the components crate's own types describe it.
#[derive(Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct TextStyle {
    #[serde(flatten)]
    pub format: Format,
    #[serde(flatten)]
    pub interactions: Interactivity,
}

impl std::fmt::Debug for TextStyle {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("TextStyle { .. }")
    }
}

impl TextStyle {
    /// The style a component carries.
    #[must_use]
    pub fn of(component: &TextComponent) -> Self {
        Self {
            format: component.format.clone(),
            interactions: component.interactions.clone(),
        }
    }

    /// Applies this style to a piece of text.
    #[must_use]
    pub fn apply(&self, mut text: TextComponent) -> TextComponent {
        text.format = self.format.clone();
        text.interactions = self.interactions.clone();
        text
    }
}

impl WriteTo for TextStyle {
    fn write(&self, writer: &mut impl Write) -> Result<()> {
        write_style(
            &self.format,
            &self.interactions,
            &mut LimitedWriter::new(writer, usize::MAX),
        )
    }
}

/// How a score is drawn beside its holder.
///
/// Vanilla parity: `NumberFormat` and its three implementations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NumberFormat {
    /// `BlankFormat`: the number is not drawn.
    Blank,
    /// `FixedFormat`: this text is drawn in place of the number.
    Fixed(TextComponent),
    /// `StyledFormat`: the number, in this style.
    Styled(TextStyle),
}

impl NumberFormat {
    /// `NumberFormat.format`, the text drawn for a score value.
    #[must_use]
    pub fn format(&self, value: i32) -> TextComponent {
        match self {
            Self::Blank => TextComponent::plain(""),
            Self::Fixed(text) => text.clone(),
            Self::Styled(style) => style.apply(TextComponent::plain(value.to_string())),
        }
    }
}

impl WriteTo for NumberFormat {
    /// `NumberFormatTypes.STREAM_CODEC`: the registry id of the format type
    /// (`blank`, `styled`, `fixed`, in registration order), then its payload.
    fn write(&self, writer: &mut impl Write) -> Result<()> {
        match self {
            Self::Blank => VarInt(0).write(writer),
            Self::Styled(style) => {
                VarInt(1).write(writer)?;
                style.write(writer)
            }
            Self::Fixed(text) => {
                VarInt(2).write(writer)?;
                text.write(writer)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::packets::game::{
        CResetScore, CSetDisplayObjective, CSetObjective, CSetScore, ObjectiveLook, ObjectiveMethod,
    };
    use text_components::TextComponent;

    fn encoded(packet: &impl WriteTo) -> Vec<u8> {
        let mut bytes = Vec::new();
        packet
            .write(&mut bytes)
            .expect("writing to a Vec cannot fail");
        bytes
    }

    /// The slot is its id as a `VarInt`, then the objective name; `team_black` is 3.
    #[test]
    fn a_display_slot_is_its_id_then_the_name() {
        let packet = CSetDisplayObjective {
            slot: DisplaySlot::TeamBlack,
            objective_name: "kills".to_owned(),
        };
        assert_eq!(encoded(&packet), [&[3, 5][..], b"kills"].concat());
    }

    /// A removal is the name and the method byte alone; an add carries the
    /// look, ending in the absent-number-format flag.
    #[test]
    fn objective_methods_carry_the_look_only_for_add_and_change() {
        let remove = CSetObjective {
            name: "a".to_owned(),
            method: ObjectiveMethod::Remove,
        };
        assert_eq!(encoded(&remove), [1, b'a', 1]);

        let add = CSetObjective {
            name: "a".to_owned(),
            method: ObjectiveMethod::Add(ObjectiveLook {
                display_name: TextComponent::plain("A"),
                render_type: ObjectiveRenderType::Hearts,
                number_format: None,
            }),
        };
        let bytes = encoded(&add);
        assert_eq!(bytes[..3], [1, b'a', 0]);
        assert_eq!(bytes[bytes.len() - 2..], [1, 0]);
    }

    /// Number formats are the registry id of their type: blank 0, styled 1,
    /// fixed 2 -- the order `NumberFormatTypes.bootstrap` registers them in.
    #[test]
    fn number_format_types_use_their_registry_ids() {
        let score = |format| CSetScore {
            owner: "o".to_owned(),
            objective_name: "b".to_owned(),
            score: 300,
            display: None,
            number_format: Some(format),
        };
        // owner, objective, score 300 as VarInt (0xAC 0x02), no display, present format.
        let blank = encoded(&score(NumberFormat::Blank));
        assert_eq!(blank, [1, b'o', 1, b'b', 0xAC, 0x02, 0, 1, 0]);
        let fixed = encoded(&score(NumberFormat::Fixed(TextComponent::plain("x"))));
        assert_eq!(fixed[..8], [1, b'o', 1, b'b', 0xAC, 0x02, 0, 1]);
        assert_eq!(fixed[8], 2);
    }

    #[test]
    fn a_reset_names_an_objective_only_when_it_has_one() {
        let all = CResetScore {
            owner: "o".to_owned(),
            objective_name: None,
        };
        assert_eq!(encoded(&all), [1, b'o', 0]);
        let one = CResetScore {
            owner: "o".to_owned(),
            objective_name: Some("b".to_owned()),
        };
        assert_eq!(encoded(&one), [1, b'o', 1, 1, b'b']);
    }
}
