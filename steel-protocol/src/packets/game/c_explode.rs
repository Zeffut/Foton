//! Clientbound explode packet - the only thing an explosion tells a client.

use glam::DVec3;
use steel_macros::{ClientPacket, WriteTo};
use steel_registry::packets::play::C_EXPLODE;
use steel_registry::particle_type::{ExplosionParticleInfo, ParticleData};
use steel_registry::sound_event::SoundEventRef;
use steel_utils::random::weighted_list::WeightedList;

/// Sent to every player near a blast.
///
/// Vanilla parity: `ClientboundExplodePacket`, sent by `ServerLevel.explode` to
/// each player within 64 blocks. It carries the blast's whole client-side
/// presentation -- the sound, the emitter particle and the debris the broken
/// blocks throw off -- none of which the server sends any other way.
///
/// `player_knockback` is the reason this packet cannot be skipped. A player is
/// authoritative over their own movement, so the server changing their velocity
/// changes nothing they can feel: `ClientPacketListener.handleExplosion` ends
/// with `packet.playerKnockback().ifPresent(this.minecraft.player::addDeltaMovement)`,
/// and that is the only path by which a blast moves the player it hit.
#[derive(ClientPacket, WriteTo, Clone, Debug)]
#[packet_id(Play = C_EXPLODE)]
pub struct CExplode {
    /// Where the blast went off.
    pub center: DVec3,
    /// How far it reached, which sizes the client's debris cloud.
    pub radius: f32,
    /// How many blocks it broke, which sets how much debris to draw.
    pub block_count: i32,
    /// The push this recipient takes, if the blast pushed them.
    pub player_knockback: Option<DVec3>,
    /// The emitter particle drawn at the center.
    pub explosion_particle: ParticleData,
    /// The holder-encoded sound event id.
    #[write(as = VarInt)]
    pub explosion_sound: i32,
    /// The particles the broken blocks throw off.
    pub block_particles: WeightedList<ExplosionParticleInfo>,
}

impl CExplode {
    /// Builds the packet one recipient sees.
    #[must_use]
    pub fn new(
        center: DVec3,
        radius: f32,
        block_count: i32,
        player_knockback: Option<DVec3>,
        explosion_particle: ParticleData,
        explosion_sound: SoundEventRef,
        block_particles: WeightedList<ExplosionParticleInfo>,
    ) -> Self {
        Self {
            center,
            radius,
            block_count,
            player_knockback,
            explosion_particle,
            explosion_sound: explosion_sound.packet_holder_id(),
            block_particles,
        }
    }
}

#[cfg(test)]
mod tests {
    use glam::DVec3;
    use steel_registry::particle_type::{ExplosionParticleInfo, ParticleData, ParticleType};
    use steel_registry::{
        RegistryEntry, init_vanilla_registry, sound_events, vanilla_particle_types,
    };
    use steel_utils::random::weighted_list::{Weighted, WeightedList};
    use steel_utils::{codec::VarInt, serial::WriteTo};

    use super::CExplode;

    /// The wire order and the widths are the whole contract with the client:
    /// `block_count` is a plain big-endian int rather than a `VarInt`, the
    /// knockback is an optional guarded by one boolean, and the block particles
    /// are a length-prefixed list of value-then-weight pairs. Getting any of
    /// them wrong desynchronizes everything that follows on the same read.
    #[test]
    fn writes_fields_in_vanilla_wire_order() {
        init_vanilla_registry();

        let packet = CExplode::new(
            DVec3::new(1.5, 64.0, -2.25),
            3.0,
            17,
            Some(DVec3::new(0.25, 0.5, -0.75)),
            ParticleData::simple(&vanilla_particle_types::EXPLOSION_EMITTER),
            &sound_events::ENTITY_GENERIC_EXPLODE,
            WeightedList::new(vec![Weighted {
                value: ExplosionParticleInfo::new(
                    ParticleData::simple(&vanilla_particle_types::POOF),
                    0.5,
                    1.0,
                ),
                weight: 1,
            }]),
        );

        let mut encoded = Vec::new();
        let Ok(()) = packet.write(&mut encoded) else {
            panic!("explode packet should encode");
        };

        let mut expected = Vec::new();
        expected.extend_from_slice(&1.5_f64.to_be_bytes());
        expected.extend_from_slice(&64.0_f64.to_be_bytes());
        expected.extend_from_slice(&(-2.25_f64).to_be_bytes());
        expected.extend_from_slice(&3.0_f32.to_be_bytes());
        expected.extend_from_slice(&17_i32.to_be_bytes());
        expected.push(1);
        expected.extend_from_slice(&0.25_f64.to_be_bytes());
        expected.extend_from_slice(&0.5_f64.to_be_bytes());
        expected.extend_from_slice(&(-0.75_f64).to_be_bytes());

        let particle_id = |particle: &ParticleType| {
            i32::try_from(particle.id()).expect("particle id should fit in i32")
        };
        let Ok(()) =
            VarInt(particle_id(&vanilla_particle_types::EXPLOSION_EMITTER)).write(&mut expected)
        else {
            panic!("particle id should encode");
        };
        let Ok(()) =
            VarInt(sound_events::ENTITY_GENERIC_EXPLODE.packet_holder_id()).write(&mut expected)
        else {
            panic!("sound holder id should encode");
        };
        let Ok(()) = VarInt(1).write(&mut expected) else {
            panic!("block particle count should encode");
        };
        let Ok(()) = VarInt(particle_id(&vanilla_particle_types::POOF)).write(&mut expected) else {
            panic!("particle id should encode");
        };
        expected.extend_from_slice(&0.5_f32.to_be_bytes());
        expected.extend_from_slice(&1.0_f32.to_be_bytes());
        let Ok(()) = VarInt(1).write(&mut expected) else {
            panic!("block particle weight should encode");
        };

        assert_eq!(encoded, expected);
    }
}
