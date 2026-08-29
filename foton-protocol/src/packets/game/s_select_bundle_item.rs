//! Serverbound packet sent when a player picks which stack a bundle hands out next.

use std::io::{Cursor, Error, Result};

use foton_macros::ServerPacket;
use foton_utils::codec::VarInt;
use foton_utils::serial::ReadFrom;

/// Vanilla parity: `BundleContents.NO_SELECTED_ITEM_INDEX`, the only negative
/// index `ServerboundSelectBundleItemPacket` accepts.
pub const NO_SELECTED_ITEM_INDEX: i32 = -1;

/// Sent by the client while hovering a bundle in a container screen.
#[derive(ServerPacket, Clone, Copy, Debug)]
pub struct SSelectBundleItem {
    /// The menu slot holding the bundle.
    pub slot_id: i32,
    /// The index inside the bundle, or `-1` to clear the selection.
    pub selected_item_index: i32,
}

impl ReadFrom for SSelectBundleItem {
    fn read(data: &mut Cursor<&[u8]>) -> Result<Self> {
        let slot_id = VarInt::read(data)?.0;
        let selected_item_index = VarInt::read(data)?.0;
        // Vanilla rejects the packet in its constructor rather than clamping.
        if selected_item_index < 0 && selected_item_index != NO_SELECTED_ITEM_INDEX {
            return Err(Error::other(format!(
                "Invalid selectedItemIndex: {selected_item_index}"
            )));
        }
        Ok(Self {
            slot_id,
            selected_item_index,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use foton_utils::codec::VarInt;
    use foton_utils::serial::{ReadFrom as _, WriteTo as _};

    use super::SSelectBundleItem;

    fn packet(slot_id: i32, selected_item_index: i32) -> Vec<u8> {
        let mut encoded = Vec::new();
        VarInt(slot_id).write(&mut encoded).expect("slot encodes");
        VarInt(selected_item_index)
            .write(&mut encoded)
            .expect("index encodes");
        encoded
    }

    #[test]
    fn a_cleared_selection_decodes() {
        let bytes = packet(9, -1);
        let decoded =
            SSelectBundleItem::read(&mut Cursor::new(bytes.as_slice())).expect("packet parses");

        assert_eq!(decoded.slot_id, 9);
        assert_eq!(decoded.selected_item_index, -1);
    }

    #[test]
    fn a_negative_index_other_than_minus_one_is_rejected() {
        let bytes = packet(9, -2);

        assert!(SSelectBundleItem::read(&mut Cursor::new(bytes.as_slice())).is_err());
    }
}
