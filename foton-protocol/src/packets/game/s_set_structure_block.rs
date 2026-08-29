use std::io::{Cursor, Result};

use foton_macros::ServerPacket;
use foton_utils::BlockPos;
use foton_utils::codec::{VarInt, VarLong};
use foton_utils::serial::{PrefixedRead, ReadFrom};

use super::s_set_command_block::MAX_COMMAND_LENGTH;

/// Longest metadata string a data-mode structure block may carry.
///
/// Vanilla parity: the `readUtf(128)` of the packet's `data` field.
pub const MAX_STRUCTURE_METADATA_LENGTH: usize = 128;

/// Furthest a structure block's corner may sit from the block itself.
///
/// Vanilla parity: `StructureBlockEntity.MAX_OFFSET_PER_AXIS`.
pub const MAX_OFFSET_PER_AXIS: i8 = 48;

/// Largest structure a structure block may capture on one axis.
///
/// Vanilla parity: `StructureBlockEntity.MAX_SIZE_PER_AXIS`.
pub const MAX_SIZE_PER_AXIS: i8 = 48;

/// Bit set when entities inside the box are left out of the capture.
const FLAG_IGNORE_ENTITIES: u8 = 1;

/// Bit set when the client should draw air blocks in the preview.
const FLAG_SHOW_AIR: u8 = 2;

/// Bit set when the client should draw the bounding box.
const FLAG_SHOW_BOUNDING_BOX: u8 = 4;

/// Bit set when placement must not fall back to a looser match.
const FLAG_STRICT: u8 = 8;

/// Which button the player pressed in the structure block editor.
///
/// Vanilla parity: `StructureBlockEntity.UpdateType`, in ordinal order --
/// plain "done" is `UpdateData`, and the other three are the three action
/// buttons the editor offers depending on the mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StructureUpdateType {
    /// Store the fields and do nothing else.
    UpdateData,
    /// Capture the box into a saved structure.
    SaveArea,
    /// Place a saved structure into the world.
    LoadArea,
    /// Work the box out from the matching corner blocks.
    ScanArea,
}

impl StructureUpdateType {
    /// Returns the update type a wire value names.
    #[must_use]
    pub const fn from_wire(value: i32) -> Option<Self> {
        match value {
            0 => Some(Self::UpdateData),
            1 => Some(Self::SaveArea),
            2 => Some(Self::LoadArea),
            3 => Some(Self::ScanArea),
            _ => None,
        }
    }
}

/// A structure block's editor being saved.
///
/// Vanilla parity: `ServerboundSetStructureBlockPacket`. The offsets and the
/// size travel as signed bytes and are clamped on the way in, so a hostile
/// client cannot ask for a structure larger than vanilla allows.
#[derive(ServerPacket, Clone, Debug)]
pub struct SSetStructureBlock {
    /// The block being edited.
    pub pos: BlockPos,
    /// Which button was pressed, as an ordinal.
    pub update_type: i32,
    /// Which mode the block was set to, as an ordinal.
    pub mode: i32,
    /// The structure's name.
    pub name: String,
    /// Offset from the block to the low corner of the box.
    pub offset: (i8, i8, i8),
    /// Size of the box.
    pub size: (i8, i8, i8),
    /// Mirror to place with, as an ordinal.
    pub mirror: i32,
    /// Rotation to place with, as an ordinal.
    pub rotation: i32,
    /// The data-mode marker string.
    pub data: String,
    /// Fraction of blocks kept when placing.
    pub integrity: f32,
    /// Seed for the integrity roll, or zero for a fresh one.
    pub seed: i64,
    /// Whether entities are left out of the capture.
    pub ignore_entities: bool,
    /// Whether placement must not fall back to a looser match.
    pub strict: bool,
    /// Whether the client draws air blocks in the preview.
    pub show_air: bool,
    /// Whether the client draws the bounding box.
    pub show_bounding_box: bool,
}

impl ReadFrom for SSetStructureBlock {
    fn read(data_in: &mut Cursor<&[u8]>) -> Result<Self> {
        let pos = BlockPos::read(data_in)?;
        let update_type = VarInt::read(data_in)?.0;
        let mode = VarInt::read(data_in)?.0;
        let name = String::read_prefixed_bound::<VarInt>(data_in, MAX_COMMAND_LENGTH)?;

        let offset = (
            clamp_offset(i8::read(data_in)?),
            clamp_offset(i8::read(data_in)?),
            clamp_offset(i8::read(data_in)?),
        );
        let size = (
            clamp_size(i8::read(data_in)?),
            clamp_size(i8::read(data_in)?),
            clamp_size(i8::read(data_in)?),
        );

        let mirror = VarInt::read(data_in)?.0;
        let rotation = VarInt::read(data_in)?.0;
        let data = String::read_prefixed_bound::<VarInt>(data_in, MAX_STRUCTURE_METADATA_LENGTH)?;
        let integrity = f32::read(data_in)?.clamp(0.0, 1.0);
        let seed = VarLong::read(data_in)?.0;
        let flags = u8::read(data_in)?;

        Ok(Self {
            pos,
            update_type,
            mode,
            name,
            offset,
            size,
            mirror,
            rotation,
            data,
            integrity,
            seed,
            ignore_entities: flags & FLAG_IGNORE_ENTITIES != 0,
            strict: flags & FLAG_STRICT != 0,
            show_air: flags & FLAG_SHOW_AIR != 0,
            show_bounding_box: flags & FLAG_SHOW_BOUNDING_BOX != 0,
        })
    }
}

/// Vanilla parity: the `Mth.clamp(input.readByte(), -48, 48)` of the offsets.
fn clamp_offset(value: i8) -> i8 {
    value.clamp(-MAX_OFFSET_PER_AXIS, MAX_OFFSET_PER_AXIS)
}

/// Vanilla parity: the `Mth.clamp(input.readByte(), 0, 48)` of the size.
fn clamp_size(value: i8) -> i8 {
    value.clamp(0, MAX_SIZE_PER_AXIS)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn packet_bytes(offset: [i8; 3], size: [i8; 3], integrity: f32, flags: u8) -> Vec<u8> {
        let mut bytes = 0i64.to_be_bytes().to_vec();
        bytes.push(3); // update type: SCAN_AREA
        bytes.push(0); // mode: SAVE
        bytes.push(2);
        bytes.extend_from_slice(b"hi");
        bytes.extend(offset.iter().map(|value| *value as u8));
        bytes.extend(size.iter().map(|value| *value as u8));
        bytes.push(0); // mirror: NONE
        bytes.push(0); // rotation: NONE
        bytes.push(0); // empty data string
        bytes.extend_from_slice(&integrity.to_be_bytes());
        bytes.push(0); // seed varlong 0
        bytes.push(flags);
        bytes
    }

    /// Offsets and sizes arrive as raw bytes from the client and are clamped
    /// here. Without the clamp a hostile client could ask a structure block to
    /// scan a box far larger than vanilla allows.
    #[test]
    fn the_offset_and_size_are_clamped_to_vanillas_limits() {
        let bytes = packet_bytes([-127, 100, 0], [127, -5, 12], 1.0, 0);
        let packet =
            SSetStructureBlock::read(&mut Cursor::new(bytes.as_slice())).expect("packet parses");

        assert_eq!(packet.offset, (-48, 48, 0));
        assert_eq!(packet.size, (48, 0, 12));
    }

    /// The four booleans share one byte, and `strict` is bit three rather than
    /// bit two -- swapping it with `show_air` would silently change how every
    /// structure a client saves is placed.
    #[test]
    fn the_flag_byte_unpacks_all_four_booleans() {
        let bytes = packet_bytes(
            [0, 0, 0],
            [1, 1, 1],
            1.0,
            FLAG_IGNORE_ENTITIES | FLAG_STRICT,
        );
        let packet =
            SSetStructureBlock::read(&mut Cursor::new(bytes.as_slice())).expect("packet parses");

        assert!(packet.ignore_entities);
        assert!(packet.strict);
        assert!(!packet.show_air);
        assert!(!packet.show_bounding_box);

        let bytes = packet_bytes(
            [0, 0, 0],
            [1, 1, 1],
            1.0,
            FLAG_SHOW_AIR | FLAG_SHOW_BOUNDING_BOX,
        );
        let packet =
            SSetStructureBlock::read(&mut Cursor::new(bytes.as_slice())).expect("packet parses");

        assert!(!packet.ignore_entities);
        assert!(!packet.strict);
        assert!(packet.show_air);
        assert!(packet.show_bounding_box);
    }

    /// Integrity is a fraction; anything outside it is clamped rather than
    /// letting a client ask for a 200%-dense placement.
    #[test]
    fn integrity_is_clamped_to_a_fraction() {
        let bytes = packet_bytes([0, 0, 0], [1, 1, 1], 4.5, 0);
        let packet =
            SSetStructureBlock::read(&mut Cursor::new(bytes.as_slice())).expect("packet parses");
        assert!((packet.integrity - 1.0).abs() < f32::EPSILON);

        let bytes = packet_bytes([0, 0, 0], [1, 1, 1], -2.0, 0);
        let packet =
            SSetStructureBlock::read(&mut Cursor::new(bytes.as_slice())).expect("packet parses");
        assert!(packet.integrity.abs() < f32::EPSILON);
    }

    /// `readEnum` is ordinal order for both enums this packet carries.
    #[test]
    fn the_update_type_wire_order_is_data_save_load_scan() {
        assert_eq!(
            StructureUpdateType::from_wire(0),
            Some(StructureUpdateType::UpdateData)
        );
        assert_eq!(
            StructureUpdateType::from_wire(1),
            Some(StructureUpdateType::SaveArea)
        );
        assert_eq!(
            StructureUpdateType::from_wire(2),
            Some(StructureUpdateType::LoadArea)
        );
        assert_eq!(
            StructureUpdateType::from_wire(3),
            Some(StructureUpdateType::ScanArea)
        );
        assert_eq!(StructureUpdateType::from_wire(4), None);
    }
}
