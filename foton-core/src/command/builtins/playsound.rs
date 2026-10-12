//! Vanilla playsound command.

use foton_protocol::packets::game::{CSoundDirect, SoundSource};
use foton_utils::{Identifier, translations};
use glam::DVec3;
use text_components::TextComponent;

use super::super::{
    brigadier::{ArgumentType, CommandNodeBuilder, CommandSyntaxError},
    execution::{
        CommandSource, FotonArgumentType, FotonCommandContext, FotonCommandRuntime, argument,
        literal,
    },
    registration::CommandRegistration,
};
use crate::{entity::Entity, player::Player};
use std::sync::Arc;

/// `SoundEvent.getRange`: a sound carries 16 blocks, scaled up once the volume exceeds 1.
const BASE_RANGE: f32 = 16.0;

pub(super) fn registration() -> CommandRegistration<CommandSource> {
    CommandRegistration::new(Identifier::vanilla_static("playsound"), |_| command())
}

/// Vanilla parity: `PlaySoundCommand.register`. Permission level 2 is applied by the registry.
fn command() -> CommandNodeBuilder<CommandSource, FotonCommandRuntime> {
    let mut sound = argument("sound", FotonArgumentType::identifier())
        .executes(|c| play(c, SoundSource::Master, calling_player(c), None));
    for source in SoundSource::ALL {
        sound = sound.then(source_node(source));
    }
    literal("playsound").then(sound)
}

fn source_node(source: SoundSource) -> CommandNodeBuilder<CommandSource, FotonCommandRuntime> {
    literal(source.name())
        .executes(move |c| play(c, source, calling_player(c), None))
        .then(
            argument("targets", FotonArgumentType::players())
                .executes(move |c| play(c, source, c.players("targets")?, None))
                .then(
                    argument("pos", FotonArgumentType::vec3(true))
                        .executes(move |c| {
                            play(
                                c,
                                source,
                                c.players("targets")?,
                                Some((position(c)?, 1.0, 1.0, 0.0)),
                            )
                        })
                        .then(
                            argument("volume", ArgumentType::float(0.0, f32::MAX))
                                .executes(move |c| {
                                    play(
                                        c,
                                        source,
                                        c.players("targets")?,
                                        Some((position(c)?, c.float("volume")?, 1.0, 0.0)),
                                    )
                                })
                                .then(
                                    argument("pitch", ArgumentType::float(0.0, 2.0))
                                        .executes(move |c| {
                                            play(
                                                c,
                                                source,
                                                c.players("targets")?,
                                                Some((
                                                    position(c)?,
                                                    c.float("volume")?,
                                                    c.float("pitch")?,
                                                    0.0,
                                                )),
                                            )
                                        })
                                        .then(
                                            argument("minVolume", ArgumentType::float(0.0, 1.0))
                                                .executes(move |c| {
                                                    play(
                                                        c,
                                                        source,
                                                        c.players("targets")?,
                                                        Some((
                                                            position(c)?,
                                                            c.float("volume")?,
                                                            c.float("pitch")?,
                                                            c.float("minVolume")?,
                                                        )),
                                                    )
                                                }),
                                        ),
                                ),
                        ),
                ),
        )
}

/// Vanilla parity: `getCallingPlayerAsCollection`.
fn calling_player(context: &FotonCommandContext<CommandSource>) -> Vec<Arc<Player>> {
    context.source().player().cloned().into_iter().collect()
}

fn position(context: &FotonCommandContext<CommandSource>) -> Result<DVec3, CommandSyntaxError> {
    Ok(context.coordinates("pos")?.position(context.source()))
}

/// Vanilla parity: `PlaySoundCommand.playSound`. `settings` is position, volume, pitch and
/// minimum volume; `None` plays at the source's position with vanilla's defaults.
fn play(
    context: &FotonCommandContext<CommandSource>,
    source: SoundSource,
    players: Vec<Arc<Player>>,
    settings: Option<(DVec3, f32, f32, f32)>,
) -> Result<i32, CommandSyntaxError> {
    let sound = context.identifier("sound")?;
    let (position, volume, pitch, min_volume) =
        settings.unwrap_or((context.source().position(), 1.0, 1.0, 0.0));
    let range = if volume > 1.0 {
        BASE_RANGE * volume
    } else {
        BASE_RANGE
    };
    let max_distance_squared = f64::from(range) * f64::from(range);
    let seed = rand::random::<i64>();

    let mut played_for = Vec::new();
    for player in &players {
        if !Arc::ptr_eq(&player.get_world(), context.source().world()) {
            continue;
        }
        let delta = position - player.position();
        let distance_squared = delta.length_squared();
        let (local_position, local_volume) = if distance_squared > max_distance_squared {
            // Out of range: vanilla still plays it quietly, two blocks away in that direction.
            if min_volume <= 0.0 {
                continue;
            }
            let distance = distance_squared.sqrt();
            (player.position() + delta / distance * 2.0, min_volume)
        } else {
            (position, volume)
        };
        player.send_packet(CSoundDirect::new(
            sound.clone(),
            source,
            local_position,
            local_volume,
            pitch,
            seed,
        ));
        played_for.push(player);
    }

    let count = i32::try_from(played_for.len()).unwrap_or(i32::MAX);
    let sound_name = TextComponent::plain(sound.to_string());
    let message = match played_for.as_slice() {
        [] => {
            return Err(CommandSyntaxError::dynamic(TextComponent::from(
                &translations::COMMANDS_PLAYSOUND_FAILED,
            )));
        }
        [player] => translations::COMMANDS_PLAYSOUND_SUCCESS_SINGLE
            .message([sound_name, player.display_name()])
            .component(),
        _ => translations::COMMANDS_PLAYSOUND_SUCCESS_MULTIPLE
            .message([sound_name, TextComponent::plain(count.to_string())])
            .component(),
    };
    context.source().send_success(&message, true);
    Ok(count)
}
