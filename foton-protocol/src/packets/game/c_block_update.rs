use foton_macros::{ClientPacket, WriteTo};
use foton_registry::packets::play::C_BLOCK_UPDATE;
use foton_utils::{BlockPos, BlockStateId};

#[derive(ClientPacket, WriteTo, Clone, Debug)]
#[packet_id(Play = C_BLOCK_UPDATE)]
pub struct CBlockUpdate {
    pub pos: BlockPos,
    pub block_state: BlockStateId,
}
