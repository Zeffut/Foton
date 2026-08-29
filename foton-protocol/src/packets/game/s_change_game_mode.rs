use foton_macros::{ReadFrom, ServerPacket};
use foton_utils::types::GameType;

#[derive(ReadFrom, ServerPacket, Clone, Debug)]
pub struct SChangeGameMode {
    pub gamemode: GameType,
}
