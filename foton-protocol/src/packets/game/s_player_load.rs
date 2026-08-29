use foton_macros::{ReadFrom, ServerPacket};

#[derive(ReadFrom, ServerPacket, Clone, Debug)]
pub struct SPlayerLoad {}
