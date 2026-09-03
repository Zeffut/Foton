use foton_macros::ClientPacket;
use foton_registry::packets::play::C_STOP_SOUND;
use foton_utils::Identifier;
use foton_utils::codec::VarInt;
use foton_utils::serial::WriteTo;

use super::SoundSource;

/// Stops sounds matching the optional sound and mixer source filters.
#[derive(ClientPacket, Clone, Debug)]
#[packet_id(Play = C_STOP_SOUND)]
pub struct CStopSound {
    pub sound: Option<Identifier>,
    pub source: Option<SoundSource>,
}

impl WriteTo for CStopSound {
    fn write(&self, writer: &mut impl std::io::Write) -> std::io::Result<()> {
        let flags = u8::from(self.sound.is_some()) | (u8::from(self.source.is_some()) << 1);
        flags.write(writer)?;
        if let Some(sound) = &self.sound {
            sound.write(writer)?;
        }
        if let Some(source) = self.source {
            VarInt(source.as_varint()).write(writer)?;
        }
        Ok(())
    }
}
