//! Clientbound map item data packet - the colors and markers of one filled map.

use std::io::{Result, Write};

use foton_macros::ClientPacket;
use foton_registry::map_decoration_type::MapDecorationTypeRef;
use foton_registry::{RegistryEntry, packets::play::C_MAP_ITEM_DATA};
use foton_utils::codec::VarInt;
use foton_utils::serial::{PrefixedWrite, WriteTo};
use text_components::TextComponent;

/// One marker drawn on top of a map.
///
/// Vanilla parity: `net.minecraft.world.level.saveddata.maps.MapDecoration`.
/// `x`, `y` and `rot` are already in the map's own coordinate space: `x` and
/// `y` span the image at two units per pixel, and `rot` is a sixteenth of a
/// turn.
#[derive(Clone, Debug, PartialEq)]
pub struct MapDecorationData {
    pub decoration_type: MapDecorationTypeRef,
    pub x: i8,
    pub y: i8,
    pub rot: i8,
    pub name: Option<TextComponent>,
}

impl WriteTo for MapDecorationData {
    fn write(&self, writer: &mut impl Write) -> Result<()> {
        // Registry holder reference: id + 1, because 0 means an inline value.
        VarInt(self.decoration_type.id() as i32 + 1).write(writer)?;
        self.x.write(writer)?;
        self.y.write(writer)?;
        self.rot.write(writer)?;
        self.name.write(writer)
    }
}

/// A rectangle of the map's color array that changed since the last packet.
///
/// Vanilla parity: `MapItemSavedData.MapPatch`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MapPatch {
    pub start_x: u8,
    pub start_y: u8,
    pub width: u8,
    pub height: u8,
    /// `width * height` packed `MapColor` bytes, row-major.
    pub map_colors: Vec<u8>,
}

impl WriteTo for MapPatch {
    fn write(&self, writer: &mut impl Write) -> Result<()> {
        // Vanilla writes width first and uses a zero width as the "absent"
        // marker, so the field order here is not the field order of the record.
        self.width.write(writer)?;
        self.height.write(writer)?;
        self.start_x.write(writer)?;
        self.start_y.write(writer)?;
        self.map_colors.write_prefixed::<VarInt>(writer)
    }
}

/// Sends one map's scale, lock state, markers and changed colors to a client.
///
/// Vanilla: `ClientboundMapItemDataPacket`.
#[derive(ClientPacket, Clone, Debug)]
#[packet_id(Play = C_MAP_ITEM_DATA)]
pub struct CMapItemData {
    /// The `minecraft:map_id` component value the client keys its copy by.
    pub map_id: i32,
    pub scale: u8,
    pub locked: bool,
    /// `None` leaves the client's existing markers alone; `Some` replaces them
    /// wholesale.
    pub decorations: Option<Vec<MapDecorationData>>,
    /// `None` leaves the client's existing pixels alone.
    pub color_patch: Option<MapPatch>,
}

impl WriteTo for CMapItemData {
    fn write(&self, writer: &mut impl Write) -> Result<()> {
        VarInt(self.map_id).write(writer)?;
        self.scale.write(writer)?;
        self.locked.write(writer)?;
        self.decorations.write(writer)?;
        match &self.color_patch {
            Some(patch) => patch.write(writer),
            // Vanilla signals an absent patch with a zero width byte rather
            // than the usual optional boolean.
            None => 0u8.write(writer),
        }
    }
}

#[cfg(test)]
mod tests {
    use foton_utils::serial::WriteTo as _;

    use super::{CMapItemData, MapPatch};

    /// The patch header is written width-first and an absent patch is a single
    /// zero byte, neither of which matches the shape of any other optional in
    /// the protocol -- so getting it wrong would desync every map silently.
    #[test]
    fn an_absent_patch_is_one_zero_byte_and_a_present_one_leads_with_its_width() {
        let mut absent = Vec::new();
        CMapItemData {
            map_id: 7,
            scale: 0,
            locked: false,
            decorations: None,
            color_patch: None,
        }
        .write(&mut absent)
        .expect("map packet should encode");
        assert_eq!(absent, vec![7, 0, 0, 0, 0]);

        let mut present = Vec::new();
        CMapItemData {
            map_id: 1,
            scale: 2,
            locked: true,
            decorations: None,
            color_patch: Some(MapPatch {
                start_x: 3,
                start_y: 4,
                width: 2,
                height: 1,
                map_colors: vec![0x0c, 0x0d],
            }),
        }
        .write(&mut present)
        .expect("map packet should encode");
        assert_eq!(present, vec![1, 2, 1, 0, 2, 1, 3, 4, 2, 0x0c, 0x0d]);
    }
}
