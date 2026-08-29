//! Serverbound seen advancements packet -- which tab the client is looking at.

use std::io::{Cursor, Result};

use foton_macros::{ReadFrom, ServerPacket};
use foton_utils::Identifier;
use foton_utils::serial::ReadFrom;

/// What the client just did with the advancement screen.
///
/// Vanilla parity: `ServerboundSeenAdvancementsPacket.Action`. The wire form is
/// the ordinal, so the declaration order is protocol-observable.
#[derive(ReadFrom, Clone, Copy, Debug, PartialEq, Eq)]
#[read(as = VarInt)]
pub enum SeenAdvancementsAction {
    /// The client switched to a tab, naming its root advancement.
    OpenedTab = 0,
    /// The client closed the advancement screen.
    ClosedScreen = 1,
}

/// Tells the server which advancement tab the client has open, so the server
/// knows where to send a selection back to.
///
/// Vanilla parity: `ServerboundSeenAdvancementsPacket`. The tab is *not* a
/// nullable field: vanilla writes the identifier bare and only when the action
/// is `OpenedTab`, so there is no boolean between the action and the
/// identifier.
#[derive(ServerPacket, Clone, Debug, PartialEq, Eq)]
pub struct SSeenAdvancements {
    /// What the client did.
    pub action: SeenAdvancementsAction,
    /// The root advancement of the tab that was opened. Present exactly when
    /// `action` is `OpenedTab`.
    pub tab: Option<Identifier>,
}

impl ReadFrom for SSeenAdvancements {
    fn read(data: &mut Cursor<&[u8]>) -> Result<Self> {
        let action = SeenAdvancementsAction::read(data)?;
        // Vanilla reads the identifier straight after the action, with no
        // nullable prefix. Decoding it as an `Option<Identifier>` would take
        // the identifier's own length byte for the flag.
        let tab = match action {
            SeenAdvancementsAction::OpenedTab => Some(Identifier::read(data)?),
            SeenAdvancementsAction::ClosedScreen => None,
        };
        Ok(Self { action, tab })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_closed_screen_is_one_byte_and_carries_no_tab() {
        let buffer = [1u8];
        let mut data = Cursor::new(buffer.as_slice());
        let packet = SSeenAdvancements::read(&mut data).expect("packet should parse");

        assert_eq!(packet.action, SeenAdvancementsAction::ClosedScreen);
        assert_eq!(packet.tab, None);
        assert_eq!(data.position(), 1);
    }

    #[test]
    fn an_opened_tab_reads_its_identifier_straight_after_the_action() {
        let buffer = b"\x00\x14minecraft:story/root";
        let mut data = Cursor::new(buffer.as_slice());
        let packet = SSeenAdvancements::read(&mut data).expect("packet should parse");

        assert_eq!(packet.action, SeenAdvancementsAction::OpenedTab);
        assert_eq!(packet.tab, Some(Identifier::vanilla_static("story/root")));
        // Every byte and no more: `0x14` is the identifier's length, not a flag.
        assert_eq!(data.position(), buffer.len() as u64);
    }

    /// The trap. This buffer decodes cleanly both ways and the two readings
    /// disagree, so turning `tab` into an `Option<Identifier>` -- which reads
    /// like the obvious tidy-up -- fails here instead of silently handing the
    /// server a tab the client never opened.
    #[test]
    fn a_nullable_prefix_would_decode_a_different_identifier() {
        // 45 characters, which is also the byte value of the `-` in front of
        // it: an `Option` reader eats the length byte as its `true` flag, then
        // finds a perfectly good 45-byte length prefix waiting behind it.
        const NULLABLE_READING: &str = "minecraft:a_tab_the_client_never_asked_to_see";
        const ACTUAL_READING: &str = "-minecraft:a_tab_the_client_never_asked_to_see";

        assert_eq!(NULLABLE_READING.len(), 45);
        assert_eq!(ACTUAL_READING.len(), 46);
        assert_eq!(ACTUAL_READING.as_bytes()[0], 45);

        let mut buffer = vec![0, 46];
        buffer.extend_from_slice(ACTUAL_READING.as_bytes());
        let mut data = Cursor::new(buffer.as_slice());
        let packet = SSeenAdvancements::read(&mut data).expect("packet should parse");

        assert_eq!(
            packet.tab,
            Some(Identifier::new(
                "-minecraft",
                "a_tab_the_client_never_asked_to_see"
            ))
        );
        assert_ne!(
            packet.tab.map(|tab| tab.to_string()),
            Some(NULLABLE_READING.to_string())
        );
    }

    #[test]
    fn an_unknown_action_is_rejected() {
        let buffer = [2u8];
        let mut data = Cursor::new(buffer.as_slice());

        assert!(SSeenAdvancements::read(&mut data).is_err());
    }
}
