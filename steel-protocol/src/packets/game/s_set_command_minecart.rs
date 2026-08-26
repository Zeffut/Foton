use std::io::{Cursor, Result};

use steel_macros::ServerPacket;
use steel_utils::codec::VarInt;
use steel_utils::serial::{PrefixedRead, ReadFrom};

use super::s_set_command_block::MAX_COMMAND_LENGTH;

/// A command block minecart's editor being saved.
///
/// Vanilla parity: `ServerboundSetCommandMinecartPacket`. A minecart has no
/// mode and no conditional flag -- it runs when an activator rail says so -- so
/// the packet is only the entity, the command and the output checkbox.
#[derive(ServerPacket, Clone, Debug)]
pub struct SSetCommandMinecart {
    /// Runtime id of the minecart being edited.
    pub entity: i32,
    /// The command to store.
    pub command: String,
    /// Whether the minecart keeps its last output.
    pub track_output: bool,
}

impl ReadFrom for SSetCommandMinecart {
    fn read(data: &mut Cursor<&[u8]>) -> Result<Self> {
        let entity = VarInt::read(data)?.0;
        let command = String::read_prefixed_bound::<VarInt>(data, MAX_COMMAND_LENGTH)?;
        let track_output = bool::read(data)?;

        Ok(Self {
            entity,
            command,
            track_output,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The entity id is a var int and the command a var-int-prefixed string;
    /// reading the id as a plain `i32` would desynchronise everything after it.
    #[test]
    fn the_entity_id_is_a_var_int_before_the_command() {
        let bytes = [0x80, 0x02, 2, b'h', b'i', 1];
        let packet =
            SSetCommandMinecart::read(&mut Cursor::new(bytes.as_slice())).expect("packet parses");

        assert_eq!(packet.entity, 256);
        assert_eq!(packet.command, "hi");
        assert!(packet.track_output);
    }
}
