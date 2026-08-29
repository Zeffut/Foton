//! Clientbound boss event packet -- the bar across the top of the screen.
//!
//! Vanilla parity: `ClientboundBossEventPacket`. One packet id carries six
//! different operations, tagged by a `VarInt` after the bar's UUID, so the
//! payload is a tagged union rather than a fixed struct.

use foton_macros::ClientPacket;
use foton_registry::packets::play::C_BOSS_EVENT;
use foton_utils::codec::VarInt;
use foton_utils::serial::WriteTo;
use text_components::{TextComponent, format::Color};
use uuid::Uuid;

/// The color a boss bar is drawn in.
///
/// Vanilla parity: `BossEvent.BossBarColor`. The wire form is the ordinal, so
/// the declaration order is protocol-observable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum BossBarColor {
    Pink = 0,
    Blue = 1,
    Red = 2,
    Green = 3,
    Yellow = 4,
    Purple = 5,
    White = 6,
}

impl BossBarColor {
    /// Every color, in vanilla declaration order.
    pub const VALUES: [Self; 7] = [
        Self::Pink,
        Self::Blue,
        Self::Red,
        Self::Green,
        Self::Yellow,
        Self::Purple,
        Self::White,
    ];

    /// Returns the name this color is stored and typed under.
    ///
    /// Vanilla parity: `BossBarColor.getSerializedName`.
    #[must_use]
    pub const fn serialized_name(self) -> &'static str {
        match self {
            Self::Pink => "pink",
            Self::Blue => "blue",
            Self::Red => "red",
            Self::Green => "green",
            Self::Yellow => "yellow",
            Self::Purple => "purple",
            Self::White => "white",
        }
    }

    /// Looks a color up by its serialized name.
    #[must_use]
    pub fn from_serialized_name(name: &str) -> Option<Self> {
        Self::VALUES
            .into_iter()
            .find(|color| color.serialized_name() == name)
    }

    /// Returns the chat color a bar's name is written in.
    ///
    /// Vanilla parity: `BossBarColor.getFormatting`, which `/bossbar` uses to
    /// tint the bracketed display name. It is not the bar's own color: pink
    /// writes in red and purple writes in dark blue.
    #[must_use]
    pub const fn chat_color(self) -> Color {
        match self {
            Self::Pink => Color::Red,
            Self::Blue => Color::Blue,
            Self::Red => Color::DarkRed,
            Self::Green => Color::Green,
            Self::Yellow => Color::Yellow,
            Self::Purple => Color::DarkBlue,
            Self::White => Color::White,
        }
    }
}

/// How a boss bar is segmented.
///
/// Vanilla parity: `BossEvent.BossBarOverlay`, likewise written as its ordinal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum BossBarOverlay {
    Progress = 0,
    Notched6 = 1,
    Notched10 = 2,
    Notched12 = 3,
    Notched20 = 4,
}

impl BossBarOverlay {
    /// Every overlay, in vanilla declaration order.
    pub const VALUES: [Self; 5] = [
        Self::Progress,
        Self::Notched6,
        Self::Notched10,
        Self::Notched12,
        Self::Notched20,
    ];

    /// Returns the name this overlay is stored and typed under.
    ///
    /// Vanilla parity: `BossBarOverlay.getSerializedName`.
    #[must_use]
    pub const fn serialized_name(self) -> &'static str {
        match self {
            Self::Progress => "progress",
            Self::Notched6 => "notched_6",
            Self::Notched10 => "notched_10",
            Self::Notched12 => "notched_12",
            Self::Notched20 => "notched_20",
        }
    }

    /// Looks an overlay up by its serialized name.
    #[must_use]
    pub fn from_serialized_name(name: &str) -> Option<Self> {
        Self::VALUES
            .into_iter()
            .find(|overlay| overlay.serialized_name() == name)
    }
}

/// The three client-side effects a boss bar can switch on.
///
/// Vanilla parity: the `FLAG_DARKEN` / `FLAG_MUSIC` / `FLAG_FOG` byte of
/// `ClientboundBossEventPacket.encodeProperties`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BossBarProperties {
    /// Dims the sky the way the wither's arrival does.
    pub darken_screen: bool,
    /// Replaces the music with the boss track.
    pub play_boss_music: bool,
    /// Draws the dragon fight's fog.
    pub create_world_fog: bool,
}

impl BossBarProperties {
    const FLAG_DARKEN: u8 = 1;
    const FLAG_MUSIC: u8 = 2;
    const FLAG_FOG: u8 = 4;

    /// Packs the three flags into the wire byte.
    ///
    /// Vanilla parity: `ClientboundBossEventPacket.encodeProperties`.
    #[must_use]
    pub const fn encode(self) -> u8 {
        let mut properties = 0;
        if self.darken_screen {
            properties |= Self::FLAG_DARKEN;
        }
        if self.play_boss_music {
            properties |= Self::FLAG_MUSIC;
        }
        if self.create_world_fog {
            properties |= Self::FLAG_FOG;
        }
        properties
    }
}

/// What this packet does to the bar named by its UUID.
///
/// Vanilla parity: `ClientboundBossEventPacket.Operation` and the
/// `OperationType` ordinals that tag it.
#[derive(Debug, Clone)]
pub enum BossEventOperation {
    /// Shows a bar the client does not have yet.
    Add {
        name: TextComponent,
        progress: f32,
        color: BossBarColor,
        overlay: BossBarOverlay,
        properties: BossBarProperties,
    },
    /// Takes the bar off the screen.
    Remove,
    /// Moves the bar's fill.
    UpdateProgress(f32),
    /// Retitles the bar.
    UpdateName(TextComponent),
    /// Recolors or re-segments the bar.
    UpdateStyle {
        color: BossBarColor,
        overlay: BossBarOverlay,
    },
    /// Switches the screen darkening, boss music and fog.
    UpdateProperties(BossBarProperties),
}

impl BossEventOperation {
    /// Returns the `OperationType` ordinal that tags this operation.
    const fn type_id(&self) -> i32 {
        match self {
            Self::Add { .. } => 0,
            Self::Remove => 1,
            Self::UpdateProgress(_) => 2,
            Self::UpdateName(_) => 3,
            Self::UpdateStyle { .. } => 4,
            Self::UpdateProperties(_) => 5,
        }
    }
}

/// Adds, updates or removes one boss bar on a client.
#[derive(ClientPacket, Clone, Debug)]
#[packet_id(Play = C_BOSS_EVENT)]
pub struct CBossEvent {
    /// The bar this packet is about.
    pub id: Uuid,
    /// What to do with it.
    pub operation: BossEventOperation,
}

impl CBossEvent {
    /// Builds the packet that puts a bar on a client's screen.
    ///
    /// Vanilla parity: `ClientboundBossEventPacket.createAddPacket`.
    #[must_use]
    pub const fn add(
        id: Uuid,
        name: TextComponent,
        progress: f32,
        color: BossBarColor,
        overlay: BossBarOverlay,
        properties: BossBarProperties,
    ) -> Self {
        Self {
            id,
            operation: BossEventOperation::Add {
                name,
                progress,
                color,
                overlay,
                properties,
            },
        }
    }

    /// Vanilla parity: `ClientboundBossEventPacket.createRemovePacket`.
    #[must_use]
    pub const fn remove(id: Uuid) -> Self {
        Self {
            id,
            operation: BossEventOperation::Remove,
        }
    }

    /// Vanilla parity: `ClientboundBossEventPacket.createUpdateProgressPacket`.
    #[must_use]
    pub const fn update_progress(id: Uuid, progress: f32) -> Self {
        Self {
            id,
            operation: BossEventOperation::UpdateProgress(progress),
        }
    }

    /// Vanilla parity: `ClientboundBossEventPacket.createUpdateNamePacket`.
    #[must_use]
    pub const fn update_name(id: Uuid, name: TextComponent) -> Self {
        Self {
            id,
            operation: BossEventOperation::UpdateName(name),
        }
    }

    /// Vanilla parity: `ClientboundBossEventPacket.createUpdateStylePacket`.
    #[must_use]
    pub const fn update_style(id: Uuid, color: BossBarColor, overlay: BossBarOverlay) -> Self {
        Self {
            id,
            operation: BossEventOperation::UpdateStyle { color, overlay },
        }
    }

    /// Vanilla parity: `ClientboundBossEventPacket.createUpdatePropertiesPacket`.
    #[must_use]
    pub const fn update_properties(id: Uuid, properties: BossBarProperties) -> Self {
        Self {
            id,
            operation: BossEventOperation::UpdateProperties(properties),
        }
    }
}

impl WriteTo for CBossEvent {
    fn write(&self, writer: &mut impl std::io::Write) -> std::io::Result<()> {
        self.id.write(writer)?;
        VarInt(self.operation.type_id()).write(writer)?;

        match &self.operation {
            BossEventOperation::Add {
                name,
                progress,
                color,
                overlay,
                properties,
            } => {
                name.write(writer)?;
                progress.write(writer)?;
                VarInt(*color as i32).write(writer)?;
                VarInt(*overlay as i32).write(writer)?;
                properties.encode().write(writer)
            }
            BossEventOperation::Remove => Ok(()),
            BossEventOperation::UpdateProgress(progress) => progress.write(writer),
            BossEventOperation::UpdateName(name) => name.write(writer),
            BossEventOperation::UpdateStyle { color, overlay } => {
                VarInt(*color as i32).write(writer)?;
                VarInt(*overlay as i32).write(writer)
            }
            BossEventOperation::UpdateProperties(properties) => properties.encode().write(writer),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The bar id every case writes first; its 16 bytes are the packet prefix.
    const ID: Uuid = Uuid::from_u128(0x0123_4567_89ab_cdef_0123_4567_89ab_cdef);

    fn encoded(packet: &CBossEvent) -> Vec<u8> {
        let mut bytes = Vec::new();
        packet
            .write(&mut bytes)
            .expect("writing to a Vec cannot fail");
        bytes
    }

    #[test]
    fn every_operation_is_tagged_with_its_vanilla_ordinal() {
        let cases = [
            (
                CBossEvent::add(
                    ID,
                    TextComponent::plain("Wither"),
                    0.5,
                    BossBarColor::Purple,
                    BossBarOverlay::Progress,
                    BossBarProperties::default(),
                ),
                0u8,
            ),
            (CBossEvent::remove(ID), 1),
            (CBossEvent::update_progress(ID, 0.5), 2),
            (
                CBossEvent::update_name(ID, TextComponent::plain("Wither")),
                3,
            ),
            (
                CBossEvent::update_style(ID, BossBarColor::Purple, BossBarOverlay::Progress),
                4,
            ),
            (
                CBossEvent::update_properties(ID, BossBarProperties::default()),
                5,
            ),
        ];

        for (packet, expected_tag) in cases {
            let bytes = encoded(&packet);
            assert_eq!(&bytes[..16], &ID.as_u128().to_be_bytes());
            assert_eq!(
                bytes[16], expected_tag,
                "wrong operation tag for {:?}",
                packet.operation
            );
        }
    }

    #[test]
    fn a_progress_update_is_the_uuid_the_tag_and_a_big_endian_float() {
        let bytes = encoded(&CBossEvent::update_progress(ID, 0.25));

        assert_eq!(bytes.len(), 16 + 1 + 4);
        assert_eq!(&bytes[17..], &0.25f32.to_be_bytes());
    }

    #[test]
    fn a_remove_writes_nothing_after_its_tag() {
        assert_eq!(encoded(&CBossEvent::remove(ID)).len(), 16 + 1);
    }

    #[test]
    fn a_style_update_writes_the_color_then_the_overlay_as_ordinals() {
        let bytes = encoded(&CBossEvent::update_style(
            ID,
            BossBarColor::Yellow,
            BossBarOverlay::Notched12,
        ));

        assert_eq!(bytes[17..], [4, 3]);
    }

    #[test]
    fn the_property_flags_pack_darken_music_and_fog_into_one_bit_each() {
        assert_eq!(BossBarProperties::default().encode(), 0);
        assert_eq!(
            BossBarProperties {
                darken_screen: true,
                play_boss_music: false,
                create_world_fog: false,
            }
            .encode(),
            1
        );
        assert_eq!(
            BossBarProperties {
                darken_screen: false,
                play_boss_music: true,
                create_world_fog: false,
            }
            .encode(),
            2
        );
        assert_eq!(
            BossBarProperties {
                darken_screen: false,
                play_boss_music: false,
                create_world_fog: true,
            }
            .encode(),
            4
        );
        assert_eq!(
            BossBarProperties {
                darken_screen: true,
                play_boss_music: true,
                create_world_fog: true,
            }
            .encode(),
            7
        );
    }

    #[test]
    fn an_add_ends_with_the_style_and_the_property_flags() {
        let bytes = encoded(&CBossEvent::add(
            ID,
            TextComponent::plain("Wither"),
            1.0,
            BossBarColor::Purple,
            BossBarOverlay::Notched6,
            BossBarProperties {
                darken_screen: true,
                play_boss_music: false,
                create_world_fog: false,
            },
        ));

        assert_eq!(bytes[bytes.len() - 3..], [5, 1, 1]);
    }
}
