//! Vanilla effect command.

use std::{borrow::Cow, slice};

use foton_registry::mob_effect::MobEffectRef;
use foton_utils::{Identifier, translations};
use text_components::{TextComponent, translation::TranslatedMessage};

use super::super::{
    brigadier::{ArgumentType, CommandNodeBuilder, CommandSyntaxError},
    execution::{
        CommandSource, FotonArgumentType, FotonCommandContext, FotonCommandRuntime, argument,
        literal,
    },
    registration::CommandRegistration,
};
use crate::entity::{MobEffectInstance, SharedEntity};

/// Vanilla's `MobEffectInstance.INFINITE_DURATION`.
const INFINITE_DURATION: i32 = -1;
/// Duration `give` uses when the command names none: 30 seconds.
const DEFAULT_DURATION_TICKS: i32 = 600;
const TICKS_PER_SECOND: i32 = 20;

pub(super) fn registration() -> CommandRegistration<CommandSource> {
    CommandRegistration::new(Identifier::vanilla_static("effect"), |_| command())
}

fn command() -> CommandNodeBuilder<CommandSource, FotonCommandRuntime> {
    literal("effect")
        .then(
            literal("clear").executes(clear_self).then(
                argument("targets", FotonArgumentType::entities())
                    .executes(clear_all)
                    .then(argument("effect", FotonArgumentType::mob_effect()).executes(clear_one)),
            ),
        )
        .then(
            literal("give").then(
                argument("targets", FotonArgumentType::entities()).then(
                    argument("effect", FotonArgumentType::mob_effect())
                        .executes(|c| give(c, None, 0, true))
                        .then(
                            argument("seconds", ArgumentType::integer(1, 1_000_000))
                                .executes(|c| give(c, Some(seconds(c)?), 0, true))
                                .then(amplifier_node(false)),
                        )
                        .then(
                            literal("infinite")
                                .executes(|c| give(c, Some(-1), 0, true))
                                .then(amplifier_node(true)),
                        ),
                ),
            ),
        )
}

/// `<amplifier> [<hideParticles>]`, shared by the timed and infinite forms.
fn amplifier_node(infinite: bool) -> CommandNodeBuilder<CommandSource, FotonCommandRuntime> {
    let duration = move |c: &FotonCommandContext<CommandSource>| {
        if infinite { Ok(-1) } else { seconds(c) }
    };
    argument("amplifier", ArgumentType::integer(0, 255))
        .executes(move |c| give(c, Some(duration(c)?), c.integer("amplifier")?, true))
        .then(
            argument("hideParticles", ArgumentType::bool()).executes(move |c| {
                give(
                    c,
                    Some(duration(c)?),
                    c.integer("amplifier")?,
                    !c.boolean("hideParticles")?,
                )
            }),
        )
}

fn seconds(context: &FotonCommandContext<CommandSource>) -> Result<i32, CommandSyntaxError> {
    context.integer("seconds")
}

/// Vanilla parity: `EffectCommands.giveEffect`. Vanilla also passes the duration
/// in seconds as a third message argument, which no 26.2 translation reads.
fn give(
    context: &FotonCommandContext<CommandSource>,
    seconds: Option<i32>,
    amplifier: i32,
    particles: bool,
) -> Result<i32, CommandSyntaxError> {
    let targets = context.entities("targets")?;
    let effect = context.mob_effect("effect")?;
    let duration = effect_duration(effect.is_instantaneous(), seconds);

    let mut count = 0_i32;
    for target in &targets {
        let Some(living) = target.as_living_entity() else {
            continue;
        };
        let instance = MobEffectInstance::with_duration(effect, duration, amplifier)
            .with_visible(particles)
            .with_show_icon(particles);
        if living.add_mob_effect(instance) {
            count += 1;
        }
    }
    if count == 0 {
        return Err(CommandSyntaxError::dynamic(TextComponent::from(
            &translations::COMMANDS_EFFECT_GIVE_FAILED,
        )));
    }

    let message = if let [target] = targets.as_slice() {
        translations::COMMANDS_EFFECT_GIVE_SUCCESS_SINGLE
            .message([
                effect_display_name(effect),
                TextComponent::plain(target.plain_text_name()),
            ])
            .component()
    } else {
        translations::COMMANDS_EFFECT_GIVE_SUCCESS_MULTIPLE
            .message([
                effect_display_name(effect),
                TextComponent::plain(targets.len().to_string()),
            ])
            .component()
    };
    context.source().send_success(&message, true);
    Ok(count)
}

/// Ticks an effect lasts when `give` is called with `seconds`.
///
/// Vanilla parity: the duration branch of `EffectCommands.giveEffect`. An
/// instantaneous effect takes the argument raw, since it is a count and not a
/// time; `-1` is the infinite sentinel.
const fn effect_duration(instantaneous: bool, seconds: Option<i32>) -> i32 {
    match seconds {
        Some(seconds) if instantaneous => seconds,
        Some(INFINITE_DURATION) => INFINITE_DURATION,
        Some(seconds) => seconds.saturating_mul(TICKS_PER_SECOND),
        None if instantaneous => 1,
        None => DEFAULT_DURATION_TICKS,
    }
}

fn clear_self(context: &FotonCommandContext<CommandSource>) -> Result<i32, CommandSyntaxError> {
    let Some(entity) = context.source().entity() else {
        return Err(CommandSyntaxError::dynamic(TextComponent::from(
            &translations::PERMISSIONS_REQUIRES_ENTITY,
        )));
    };
    clear_effects(context, slice::from_ref(entity))
}

fn clear_all(context: &FotonCommandContext<CommandSource>) -> Result<i32, CommandSyntaxError> {
    let targets = context.entities("targets")?;
    clear_effects(context, &targets)
}

/// Vanilla parity: `EffectCommands.clearEffects`, i.e. `LivingEntity.removeAllEffects`.
fn clear_effects(
    context: &FotonCommandContext<CommandSource>,
    targets: &[SharedEntity],
) -> Result<i32, CommandSyntaxError> {
    let mut count = 0_i32;
    for target in targets {
        let Some(living) = target.as_living_entity() else {
            continue;
        };
        let mut removed = false;
        for active in living.active_mob_effects() {
            removed |= living.remove_mob_effect(active.effect());
        }
        if removed {
            count += 1;
        }
    }
    if count == 0 {
        return Err(CommandSyntaxError::dynamic(TextComponent::from(
            &translations::COMMANDS_EFFECT_CLEAR_EVERYTHING_FAILED,
        )));
    }

    let message = if let [target] = targets {
        translations::COMMANDS_EFFECT_CLEAR_EVERYTHING_SUCCESS_SINGLE
            .message([TextComponent::plain(target.plain_text_name())])
            .component()
    } else {
        translations::COMMANDS_EFFECT_CLEAR_EVERYTHING_SUCCESS_MULTIPLE
            .message([TextComponent::plain(targets.len().to_string())])
            .component()
    };
    context.source().send_success(&message, true);
    Ok(count)
}

/// Vanilla parity: `EffectCommands.clearEffect`.
fn clear_one(context: &FotonCommandContext<CommandSource>) -> Result<i32, CommandSyntaxError> {
    let targets = context.entities("targets")?;
    let effect = context.mob_effect("effect")?;
    let count = targets
        .iter()
        .filter_map(|target| target.as_living_entity())
        .filter(|living| living.remove_mob_effect(effect))
        .count();
    if count == 0 {
        return Err(CommandSyntaxError::dynamic(TextComponent::from(
            &translations::COMMANDS_EFFECT_CLEAR_SPECIFIC_FAILED,
        )));
    }

    let message = if let [target] = targets.as_slice() {
        translations::COMMANDS_EFFECT_CLEAR_SPECIFIC_SUCCESS_SINGLE
            .message([
                effect_display_name(effect),
                TextComponent::plain(target.plain_text_name()),
            ])
            .component()
    } else {
        translations::COMMANDS_EFFECT_CLEAR_SPECIFIC_SUCCESS_MULTIPLE
            .message([
                effect_display_name(effect),
                TextComponent::plain(targets.len().to_string()),
            ])
            .component()
    };
    context.source().send_success(&message, true);
    i32::try_from(count).map_err(|_| {
        CommandSyntaxError::dynamic("Affected entity count exceeds the command result range")
    })
}

/// Vanilla parity: `MobEffect.getDisplayName`, the effect's description id.
fn effect_display_name(effect: MobEffectRef) -> TextComponent {
    TextComponent::translated(TranslatedMessage {
        key: Cow::Owned(format!(
            "effect.{}.{}",
            effect.key.namespace, effect.key.path
        )),
        args: None,
        fallback: None,
    })
}

#[cfg(test)]
mod tests {
    use super::effect_duration;

    #[test]
    fn duration_follows_vanilla_for_timed_infinite_and_instant_effects() {
        assert_eq!(effect_duration(false, None), 600);
        assert_eq!(effect_duration(false, Some(30)), 600);
        assert_eq!(effect_duration(false, Some(-1)), -1);
        assert_eq!(effect_duration(true, None), 1);
        // An instant effect's "seconds" is passed through untouched.
        assert_eq!(effect_duration(true, Some(5)), 5);
        assert_eq!(effect_duration(false, Some(1_000_000)), 20_000_000);
    }
}
