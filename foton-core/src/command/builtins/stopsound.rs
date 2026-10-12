//! Vanilla stopsound command.

use foton_protocol::packets::game::{CStopSound, SoundSource};
use foton_utils::{Identifier, translations};
use text_components::TextComponent;

use super::super::{
    brigadier::{CommandNodeBuilder, CommandSyntaxError},
    execution::{
        CommandSource, FotonArgumentType, FotonCommandContext, FotonCommandRuntime, argument,
        literal,
    },
    registration::CommandRegistration,
};

pub(super) fn registration() -> CommandRegistration<CommandSource> {
    CommandRegistration::new(Identifier::vanilla_static("stopsound"), |_| command())
}

/// Vanilla parity: `StopSoundCommand.register`. Permission level 2 is applied by the registry.
fn command() -> CommandNodeBuilder<CommandSource, FotonCommandRuntime> {
    let mut targets = argument("targets", FotonArgumentType::players())
        .executes(|c| stop(c, None, None))
        .then(
            literal("*").then(
                argument("sound", FotonArgumentType::identifier())
                    .executes(|c| stop(c, None, Some(c.identifier("sound")?.clone()))),
            ),
        );
    for source in SoundSource::ALL {
        targets = targets.then(
            literal(source.name())
                .executes(move |c| stop(c, Some(source), None))
                .then(
                    argument("sound", FotonArgumentType::identifier()).executes(move |c| {
                        stop(c, Some(source), Some(c.identifier("sound")?.clone()))
                    }),
                ),
        );
    }
    literal("stopsound").then(targets)
}

/// Vanilla parity: `StopSoundCommand.stopSound`.
fn stop(
    context: &FotonCommandContext<CommandSource>,
    source: Option<SoundSource>,
    sound: Option<Identifier>,
) -> Result<i32, CommandSyntaxError> {
    let targets = context.players("targets")?;
    let packet = CStopSound {
        sound: sound.clone(),
        source,
    };
    for player in &targets {
        player.send_packet(packet.clone());
    }

    let sound_name = sound.map(|sound| TextComponent::plain(sound.to_string()));
    let message = match (source, sound_name) {
        (Some(source), Some(sound)) => translations::COMMANDS_STOPSOUND_SUCCESS_SOURCE_SOUND
            .message([sound, TextComponent::plain(source.name())])
            .component(),
        (Some(source), None) => translations::COMMANDS_STOPSOUND_SUCCESS_SOURCE_ANY
            .message([TextComponent::plain(source.name())])
            .component(),
        (None, Some(sound)) => translations::COMMANDS_STOPSOUND_SUCCESS_SOURCELESS_SOUND
            .message([sound])
            .component(),
        (None, None) => {
            TextComponent::from(&translations::COMMANDS_STOPSOUND_SUCCESS_SOURCELESS_ANY)
        }
    };
    context.source().send_success(&message, true);
    Ok(i32::try_from(targets.len()).unwrap_or(i32::MAX))
}
