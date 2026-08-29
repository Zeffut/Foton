use foton_macros::{ClientPacket, WriteTo};
use foton_registry::packets::play::C_MOVE_VEHICLE;
use glam::DVec3;

/// Clientbound controlled-vehicle position correction packet.
#[derive(ClientPacket, WriteTo, Clone, Debug)]
#[packet_id(Play = C_MOVE_VEHICLE)]
pub struct CMoveVehicle {
    pub position: DVec3,
    pub y_rot: f32,
    pub x_rot: f32,
}
