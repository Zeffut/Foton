use foton_macros::{ClientPacket, WriteTo};
use foton_registry::packets::play::C_BLOCK_DESTRUCTION;
use foton_utils::BlockPos;

#[derive(ClientPacket, WriteTo, Clone, Debug)]
#[packet_id(Play = C_BLOCK_DESTRUCTION)]
pub struct CBlockDestruction {
    #[write(as = VarInt)]
    pub id: i32,
    pub pos: BlockPos,
    pub progress: u8,
}
