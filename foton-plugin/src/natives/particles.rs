//! `World.spawnParticle` and `Player.spawnParticle`, with their data.
//!
//! The Java side has already checked the data against the particle's Bukkit
//! type and encoded it as `kind:field:...`; this decodes it into the payload
//! the particle is registered with. A payload that does not fit -- a plugin
//! built against a different registry, say -- spawns nothing rather than a
//! packet the client would disconnect over.

use std::ffi::c_void;

use foton_protocol::packets::game::CLevelParticles;
use foton_registry::item_stack_template::ItemStackTemplate;
use foton_registry::particle_type::{
    BlockParticleOption, ColorParticleOption, DustColorTransitionOptions, DustParticleOptions,
    GeyserParticleOptions, ItemParticleOption, ParticleData, ParticleTypeRef, PowerParticleOption,
    SculkChargeParticleOptions, ShriekParticleOption, SimpleParticleOptions, SpellParticleOption,
    TrailParticleOption, VibrationParticleOption,
};
use foton_registry::position_source::{BlockPositionSource, EntityPositionSource, PositionSource};
use foton_registry::{REGISTRY, RegistryExt as _, vanilla_position_source_types};
use foton_utils::{ArgbColor, BlockPos, RgbColor};
use glam::DVec3;
use jni::JNIEnv;
use jni::objects::{JClass, JObjectArray, JString};
use jni::sys::{jboolean, jdouble, jint};
use uuid::Uuid;

use super::support::{key, method, player, text, world};
use super::{entity_by_uuid, parse_slot, parse_state, read_string_array};

/// Decodes the Java side's payload for `particle_type`.
fn particle_data(particle_type: ParticleTypeRef, encoded: &str) -> Option<ParticleData> {
    if encoded.is_empty() {
        return ParticleData::try_new(particle_type, SimpleParticleOptions);
    }
    let (kind, rest) = encoded.split_once(':')?;
    match kind {
        // Block states and item encodings contain colons of their own.
        "block" => {
            ParticleData::try_new(particle_type, BlockParticleOption::new(parse_state(rest)?))
        }
        "item" => {
            let template = ItemStackTemplate::from_stack(&parse_slot(rest)?).ok()?;
            ParticleData::try_new(particle_type, ItemParticleOption::new(template))
        }
        _ => fields_data(particle_type, kind, &rest.split(':').collect::<Vec<_>>()),
    }
}

fn fields_data(
    particle_type: ParticleTypeRef,
    kind: &str,
    fields: &[&str],
) -> Option<ParticleData> {
    let int = |index: usize| fields.get(index)?.parse::<i32>().ok();
    let float = |index: usize| fields.get(index)?.parse::<f32>().ok();
    let double = |index: usize| fields.get(index)?.parse::<f64>().ok();
    let rgb = |index: usize| int(index).map(RgbColor::new);
    match kind {
        "dust" => {
            ParticleData::try_new(particle_type, DustParticleOptions::new(rgb(0)?, float(1)?))
        }
        "dust_transition" => ParticleData::try_new(
            particle_type,
            DustColorTransitionOptions::new(rgb(0)?, rgb(1)?, float(2)?),
        ),
        "spell" => {
            ParticleData::try_new(particle_type, SpellParticleOption::new(rgb(0)?, float(1)?))
        }
        "color" => ParticleData::try_new(
            particle_type,
            ColorParticleOption::new(ArgbColor::new(int(0)?)),
        ),
        // `Float` is a dragon breath's power or a sculk charge's roll, `Integer`
        // a shriek's delay or a geyser's height: the registered payload decides.
        "float" => ParticleData::try_new(particle_type, PowerParticleOption::new(float(0)?))
            .or_else(|| {
                ParticleData::try_new(particle_type, SculkChargeParticleOptions::new(float(0)?))
            }),
        "int" => ParticleData::try_new(particle_type, ShriekParticleOption::new(int(0)?))
            .or_else(|| ParticleData::try_new(particle_type, GeyserParticleOptions::new(int(0)?))),
        "trail" => ParticleData::try_new(
            particle_type,
            TrailParticleOption::new(
                DVec3::new(double(0)?, double(1)?, double(2)?),
                rgb(3)?,
                int(4)?,
            ),
        ),
        "vibration" => {
            let (destination, ticks) = match *fields.first()? {
                "block" => (
                    PositionSource::new(
                        &vanilla_position_source_types::BLOCK,
                        BlockPositionSource::new(BlockPos::new(int(1)?, int(2)?, int(3)?)),
                    ),
                    int(4)?,
                ),
                "entity" => {
                    let (_, entity) = entity_by_uuid(&Uuid::parse_str(fields.get(1)?).ok()?)?;
                    // CraftParticle aims an entity vibration at the eyes.
                    let source =
                        EntityPositionSource::new(entity.id(), entity.get_eye_height() as f32);
                    (
                        PositionSource::new(&vanilla_position_source_types::ENTITY, source),
                        int(2)?,
                    )
                }
                _ => return None,
            };
            ParticleData::try_new(
                particle_type,
                VibrationParticleOption::new(destination, ticks),
            )
        }
        _ => None,
    }
}

/// The particle a Java call names, with its decoded payload.
fn requested(
    env: &mut JNIEnv<'_>,
    particle: &JString<'_>,
    data: &JString<'_>,
) -> Option<ParticleData> {
    let particle_type = REGISTRY.particle_types.by_key(&key(env, particle)?)?;
    particle_data(particle_type, &text(env, data).unwrap_or_default())
}

/// `force` is Bukkit's word for vanilla's `overrideLimiter`: the 512-block
/// recipient radius, and a client that shows the particle whatever its
/// particle setting.
extern "system" fn spawn_particles(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    world_name: JString<'_>,
    receivers: JObjectArray<'_>,
    particle: JString<'_>,
    x: jdouble,
    y: jdouble,
    z: jdouble,
    count: jint,
    offset_x: jdouble,
    offset_y: jdouble,
    offset_z: jdouble,
    extra: jdouble,
    data: JString<'_>,
    force: jboolean,
) {
    let Some(world) = world(&mut env, &world_name) else {
        return;
    };
    let Some(particle) = requested(&mut env, &particle, &data) else {
        return;
    };
    let position = DVec3::new(x, y, z);
    let spread = DVec3::new(offset_x, offset_y, offset_z);
    if receivers.is_null() {
        world.send_particles_with_options(
            particle,
            force != 0,
            false,
            position,
            count,
            spread,
            extra,
        );
        return;
    }
    let Some(receivers) = read_string_array(&mut env, &receivers) else {
        return;
    };
    let Some(server) = super::server() else {
        return;
    };
    for receiver in receivers {
        let Some(receiver) = Uuid::parse_str(&receiver)
            .ok()
            .and_then(|id| server.online_players().get_by_uuid(&id))
        else {
            continue;
        };
        world.send_particles_to(
            &receiver,
            particle.clone(),
            force != 0,
            false,
            position,
            count,
            spread,
            extra,
        );
    }
}

/// `CraftPlayer` sends the packet straight to the player: no world or range
/// check, since the plugin chose the recipient.
extern "system" fn player_particles(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    particle: JString<'_>,
    x: jdouble,
    y: jdouble,
    z: jdouble,
    count: jint,
    offset_x: jdouble,
    offset_y: jdouble,
    offset_z: jdouble,
    extra: jdouble,
    data: JString<'_>,
    force: jboolean,
) {
    let Some(player) = player(&mut env, &uuid) else {
        return;
    };
    let Some(particle) = requested(&mut env, &particle, &data) else {
        return;
    };
    player.send_packet(CLevelParticles {
        override_limiter: force != 0,
        always_show: false,
        x,
        y,
        z,
        x_dist: offset_x as f32,
        y_dist: offset_y as f32,
        z_dist: offset_z as f32,
        max_speed: extra as f32,
        count,
        particle,
    });
}

pub(super) fn bindings() -> Vec<jni::NativeMethod> {
    vec![
        method(
            "spawnParticles",
            "(Ljava/lang/String;[Ljava/lang/String;Ljava/lang/String;DDDIDDDDLjava/lang/String;Z)V",
            spawn_particles as *mut c_void,
        ),
        method(
            "playerParticles",
            "(Ljava/lang/String;Ljava/lang/String;DDDIDDDDLjava/lang/String;Z)V",
            player_particles as *mut c_void,
        ),
    ]
}

#[cfg(test)]
mod tests {
    use foton_registry::particle_type::{DustParticleOptions, ShriekParticleOption};
    use foton_registry::{init_vanilla_registry, vanilla_particle_types};

    use super::particle_data;

    #[test]
    fn payload_is_decoded_into_the_registered_type_or_refused() {
        init_vanilla_registry();
        let dust = particle_data(&vanilla_particle_types::DUST, "dust:16711680:2.0");
        assert!(
            dust.as_ref()
                .and_then(|data| data.downcast_ref::<DustParticleOptions>())
                .is_some()
        );
        let shriek = particle_data(&vanilla_particle_types::SHRIEK, "int:5");
        assert!(
            shriek
                .as_ref()
                .and_then(|data| data.downcast_ref::<ShriekParticleOption>())
                .is_some()
        );
        // A flame takes no data, and dust without a colour is not dust.
        assert!(particle_data(&vanilla_particle_types::FLAME, "dust:0:1.0").is_none());
        assert!(particle_data(&vanilla_particle_types::DUST, "").is_none());
    }
}
