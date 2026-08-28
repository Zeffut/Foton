//! Clientbound select advancements tab packet.

use steel_macros::{ClientPacket, WriteTo};
use steel_registry::packets::play::C_SELECT_ADVANCEMENTS_TAB;
use steel_utils::Identifier;

/// Switches which tab the advancement screen shows.
///
/// Vanilla parity: `ClientboundSelectAdvancementsTabPacket`. The tab is named
/// by the identifier of the root advancement that heads it; an absent tab
/// leaves the client on whichever tab it had. It is the whole packet.
#[derive(ClientPacket, WriteTo, Clone, Debug, PartialEq, Eq)]
#[packet_id(Play = C_SELECT_ADVANCEMENTS_TAB)]
pub struct CSelectAdvancementsTab {
    /// The root advancement of the tab to show.
    pub tab: Option<Identifier>,
}

impl CSelectAdvancementsTab {
    /// Shows the tab headed by `tab`.
    #[must_use]
    pub const fn select(tab: Identifier) -> Self {
        Self { tab: Some(tab) }
    }

    /// Names no tab, leaving the client's selection alone.
    #[must_use]
    pub const fn none() -> Self {
        Self { tab: None }
    }
}

#[cfg(test)]
mod tests {
    use steel_utils::serial::WriteTo as _;

    use super::*;

    fn encoded(packet: &CSelectAdvancementsTab) -> Vec<u8> {
        let mut bytes = Vec::new();
        packet.write(&mut bytes).expect("packet should encode");
        bytes
    }

    #[test]
    fn a_named_tab_is_a_present_flag_and_its_identifier() {
        let bytes = encoded(&CSelectAdvancementsTab::select(Identifier::vanilla_static(
            "story/root",
        )));

        let mut expected = vec![1, 20];
        expected.extend_from_slice(b"minecraft:story/root");

        assert_eq!(bytes, expected);
    }

    #[test]
    fn no_tab_is_a_single_absent_flag() {
        assert_eq!(encoded(&CSelectAdvancementsTab::none()), vec![0]);
    }
}
