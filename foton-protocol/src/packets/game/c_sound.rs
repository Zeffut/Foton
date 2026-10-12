use foton_macros::{ClientPacket, WriteTo};
use foton_registry::packets::play::C_SOUND;
use foton_registry::sound_event::SoundEventRef;
use foton_utils::Identifier;
use glam::{DVec3, IVec3};

/// Sound source categories (matches vanilla `SoundSource` enum order).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum SoundSource {
    Master = 0,
    Music = 1,
    Records = 2,
    Weather = 3,
    Blocks = 4,
    Hostile = 5,
    Neutral = 6,
    Players = 7,
    Ambient = 8,
    Voice = 9,
    Ui = 10,
}

impl SoundSource {
    /// Every source, in vanilla's declaration order.
    pub const ALL: [Self; 11] = [
        Self::Master,
        Self::Music,
        Self::Records,
        Self::Weather,
        Self::Blocks,
        Self::Hostile,
        Self::Neutral,
        Self::Players,
        Self::Ambient,
        Self::Voice,
        Self::Ui,
    ];

    /// Returns the `VarInt` value for the enum.
    #[must_use]
    pub const fn as_varint(self) -> i32 {
        self as i32
    }

    /// Returns vanilla `SoundSource.getName`, the word the sound commands take.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Master => "master",
            Self::Music => "music",
            Self::Records => "record",
            Self::Weather => "weather",
            Self::Blocks => "block",
            Self::Hostile => "hostile",
            Self::Neutral => "neutral",
            Self::Players => "player",
            Self::Ambient => "ambient",
            Self::Voice => "voice",
            Self::Ui => "ui",
        }
    }
}

/// Sent to play a sound effect at a specific position.
///
/// The position is encoded at 8x precision (divide by 8 to get actual block coordinates).
/// This allows sub-block positioning for more accurate sound placement.
#[derive(WriteTo, ClientPacket, Clone, Debug)]
#[packet_id(Play = C_SOUND)]
pub struct CSound {
    /// The holder-encoded sound event ID (`VarInt`).
    ///
    /// Vanilla reserves `0` for direct sound events, so registered sound events
    /// are encoded as `registry_id + 1`.
    #[write(as = VarInt)]
    pub sound_id: i32,
    /// The sound source category (`VarInt`).
    #[write(as = VarInt)]
    pub source: i32,
    /// X position multiplied by 8 (fixed-point).
    pub pos: IVec3,
    /// Volume (1.0 = normal).
    pub volume: f32,
    /// Pitch (1.0 = normal).
    pub pitch: f32,
    /// Random seed for sound variations.
    pub seed: i64,
}

impl CSound {
    /// Creates a new sound packet.
    ///
    /// # Arguments
    /// * `sound` - Sound event to play
    /// * `source` - Sound source category
    /// * `x`, `y`, `z` - Position in block coordinates (will be scaled by 8)
    /// * `volume` - Volume multiplier (1.0 = normal)
    /// * `pitch` - Pitch multiplier (1.0 = normal)
    /// * `seed` - Random seed for sound variations
    #[must_use]
    pub fn new(
        sound: SoundEventRef,
        source: SoundSource,
        pos: DVec3,
        volume: f32,
        pitch: f32,
        seed: i64,
    ) -> Self {
        Self {
            sound_id: sound.packet_holder_id(),
            source: source.as_varint(),
            pos: IVec3::new(
                (pos.x * 8.0) as i32,
                (pos.y * 8.0) as i32,
                (pos.z * 8.0) as i32,
            ),
            volume,
            pitch,
            seed,
        }
    }

    /// Creates a block sound packet at the center of a block position.
    ///
    /// # Arguments
    /// * `sound` - Sound event to play
    /// * `pos` - Block position (will be centered at +0.5)
    /// * `volume` - Volume multiplier
    /// * `pitch` - Pitch multiplier
    /// * `seed` - Random seed
    #[must_use]
    pub fn block_sound(
        sound: SoundEventRef,
        pos: foton_utils::BlockPos,
        volume: f32,
        pitch: f32,
        seed: i64,
    ) -> Self {
        Self::new(
            sound,
            SoundSource::Blocks,
            pos.0.as_dvec3().map(|v| v + 0.5),
            volume,
            pitch,
            seed,
        )
    }
}

/// [`CSound`] for a sound event the registry does not hold.
///
/// Vanilla `/playsound` wraps whatever identifier it is given in `Holder.direct`, which
/// `ByteBufCodecs.holder` writes as holder id `0`, then the `SoundEvent` itself: its
/// identifier and an optional fixed range. The client resolves the identifier against
/// its sound manager, so a resource pack's own sounds play too.
#[derive(WriteTo, ClientPacket, Clone, Debug)]
#[packet_id(Play = C_SOUND)]
pub struct CSoundDirect {
    /// Always `0`, the holder id vanilla reserves for direct sound events.
    #[write(as = VarInt)]
    pub holder_id: i32,
    /// The sound's identifier, for example `minecraft:item.shield.break`.
    pub sound: Identifier,
    /// A range that overrides the volume-derived one. `/playsound` never sets it.
    pub fixed_range: Option<f32>,
    /// The sound source category (`VarInt`).
    #[write(as = VarInt)]
    pub source: i32,
    /// Position multiplied by 8 (fixed-point).
    pub pos: IVec3,
    /// Volume (1.0 = normal).
    pub volume: f32,
    /// Pitch (1.0 = normal).
    pub pitch: f32,
    /// Random seed for sound variations.
    pub seed: i64,
}

impl CSoundDirect {
    /// Creates a packet for a variable-range direct sound event.
    #[must_use]
    pub fn new(
        sound: Identifier,
        source: SoundSource,
        pos: DVec3,
        volume: f32,
        pitch: f32,
        seed: i64,
    ) -> Self {
        Self {
            holder_id: 0,
            sound,
            fixed_range: None,
            source: source.as_varint(),
            pos: IVec3::new(
                (pos.x * 8.0) as i32,
                (pos.y * 8.0) as i32,
                (pos.z * 8.0) as i32,
            ),
            volume,
            pitch,
            seed,
        }
    }
}

#[cfg(test)]
mod tests {

    use foton_registry::init_vanilla_registry;
    use foton_registry::{RegistryEntry, sound_events};
    use foton_utils::serial::WriteTo;
    use foton_utils::{BlockPos, Identifier};
    use glam::DVec3;

    use super::{CSound, CSoundDirect, SoundSource};

    #[test]
    fn registered_sound_packet_uses_holder_id() {
        init_vanilla_registry();

        let packet = CSound::block_sound(
            &sound_events::BLOCK_WOODEN_BUTTON_CLICK_ON,
            BlockPos::ZERO,
            1.0,
            1.0,
            0,
        );

        let expected_holder_id = sound_events::BLOCK_WOODEN_BUTTON_CLICK_ON.id() as i32 + 1;
        assert_eq!(
            sound_events::BLOCK_WOODEN_BUTTON_CLICK_ON.packet_holder_id(),
            expected_holder_id
        );
        assert_eq!(packet.sound_id, expected_holder_id);
    }

    #[test]
    fn direct_sound_packet_writes_holder_zero_then_the_event() {
        let packet = CSoundDirect::new(
            Identifier::vanilla_static("item.shield.break"),
            SoundSource::Players,
            DVec3::new(1.0, 2.0, 3.0),
            1.0,
            1.0,
            0,
        );
        let mut bytes = Vec::new();
        assert!(packet.write(&mut bytes).is_ok());

        // Holder id 0, the identifier, then an absent fixed range, then the source (player = 7).
        let name = b"minecraft:item.shield.break";
        let mut expected = vec![0, name.len() as u8];
        expected.extend_from_slice(name);
        expected.extend_from_slice(&[0, 7]);
        assert_eq!(&bytes[..expected.len()], expected.as_slice());
        // Position is fixed-point at 8x, big-endian, so x = 8.
        assert_eq!(
            &bytes[expected.len()..expected.len() + 4],
            &8_i32.to_be_bytes()
        );
    }
}
