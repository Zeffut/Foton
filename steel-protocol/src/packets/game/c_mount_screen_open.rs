//! Clientbound mount screen open packet.

use steel_macros::{ClientPacket, WriteTo};
use steel_registry::packets::play::C_MOUNT_SCREEN_OPEN;

/// Tells the client to open the inventory screen of a mount.
///
/// Vanilla parity: `ClientboundMountScreenOpenPacket`. It replaces the
/// open-screen packet for a horse, a chested horse, a llama, a camel or a
/// nautilus: the client rebuilds the menu from the entity it already tracks,
/// so nothing but the container id, the width of the cargo grid and the entity
/// id travels.
#[derive(ClientPacket, WriteTo, Clone, Debug, PartialEq, Eq)]
#[packet_id(Play = C_MOUNT_SCREEN_OPEN)]
pub struct CMountScreenOpen {
    /// The container id the menu was opened with.
    #[write(as = VarInt)]
    pub container_id: i32,
    /// Columns of cargo the mount carries, zero for a mount with no chest.
    #[write(as = VarInt)]
    pub inventory_columns: i32,
    /// The mount the screen belongs to.
    pub entity_id: i32,
}

#[cfg(test)]
mod tests {
    use steel_utils::serial::WriteTo as _;

    use super::*;

    /// Vanilla writes the two counts as var-ints and the entity id as a raw
    /// int, which is what makes the last four bytes big-endian rather than one.
    #[test]
    fn mount_screen_open_matches_the_vanilla_field_encodings() {
        let packet = CMountScreenOpen {
            container_id: 3,
            inventory_columns: 5,
            entity_id: 300,
        };
        let mut bytes = Vec::new();

        packet.write(&mut bytes).expect("packet should encode");

        assert_eq!(bytes, vec![3, 5, 0, 0, 1, 44]);
    }
}
