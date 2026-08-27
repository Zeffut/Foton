//! Vanilla damage entity command.

use super::super::{
    brigadier::{ArgumentType, CommandNodeBuilder, CommandSyntaxError},
    execution::{
        CommandSource, SteelArgumentType, SteelCommandContext, SteelCommandRuntime, argument,
        literal,
    },
    registration::CommandRegistration,
};
use crate::entity::damage::DamageSource;
use steel_registry::vanilla_damage_types;
use steel_utils::Identifier;
use steel_utils::translations::{COMMANDS_DAMAGE_INVULNERABLE, COMMANDS_DAMAGE_SUCCESS};
use text_components::TextComponent;

pub(super) fn registration() -> CommandRegistration<CommandSource> {
    CommandRegistration::new(Identifier::vanilla_static("damage"), |_| command())
}

/// Builds the command graph.
///
/// Vanilla parity: `DamageCommand.register`, which gives every node its own
/// executor and resolves that node's arguments inside it. That shape is
/// load-bearing rather than stylistic: an optional argument probed with
/// `if let Ok(...)` cannot tell "this branch was not taken" from "this
/// branch's selector matched no entity", so `by @e[type=...]` with nothing to
/// match would quietly damage the target from an anonymous source instead of
/// failing the way vanilla does.
fn command() -> CommandNodeBuilder<CommandSource, SteelCommandRuntime> {
    literal("damage").then(
        argument("target", SteelArgumentType::entity()).then(
            argument("amount", ArgumentType::float(0.0, f32::MAX))
                .executes(|context| {
                    damage(
                        context,
                        DamageSource::environment(&vanilla_damage_types::GENERIC),
                    )
                })
                .then(
                    argument("damageType", SteelArgumentType::damage_type())
                        .executes(|context| damage(context, typed_source(context)?))
                        .then(literal("at").then(
                            argument("location", SteelArgumentType::vec3(true)).executes(
                                |context| {
                                    let position =
                                        context.coordinates("location")?.position(context.source());
                                    damage(
                                        context,
                                        typed_source(context)?.with_source_position(position),
                                    )
                                },
                            ),
                        ))
                        .then(
                            literal("by").then(
                                argument("entity", SteelArgumentType::entity())
                                    .executes(|context| {
                                        let entity = context.entity("entity")?.id();
                                        damage(
                                            context,
                                            typed_source(context)?
                                                .with_direct_entity(entity)
                                                .with_causing_entity(entity),
                                        )
                                    })
                                    .then(literal("from").then(
                                        argument("cause", SteelArgumentType::entity()).executes(
                                            |context| {
                                                let entity = context.entity("entity")?.id();
                                                let cause = context.entity("cause")?.id();
                                                damage(
                                                    context,
                                                    typed_source(context)?
                                                        .with_direct_entity(entity)
                                                        .with_causing_entity(cause),
                                                )
                                            },
                                        ),
                                    )),
                            ),
                        ),
                ),
        ),
    )
}

fn typed_source(
    context: &SteelCommandContext<CommandSource>,
) -> Result<DamageSource, CommandSyntaxError> {
    Ok(DamageSource::environment(
        context.damage_type("damageType")?,
    ))
}

fn damage(
    context: &SteelCommandContext<CommandSource>,
    damage_source: DamageSource,
) -> Result<i32, CommandSyntaxError> {
    let target = context.entity("target")?;
    let amount = context.float("amount")?;

    let Some(target_world) = target.level() else {
        return Err(CommandSyntaxError::dynamic(
            "The entity is not in a world or the world was dropped.",
        ));
    };

    // Vanilla parity: `DamageCommand.damage` throws `ERROR_INVULNERABLE` here.
    // Reporting it as a failure rather than a zero result is what a command
    // block's comparator and `execute store success` read.
    if !target.hurt(&target_world, &damage_source, amount) {
        return Err(CommandSyntaxError::dynamic(
            COMMANDS_DAMAGE_INVULNERABLE.msg().component(),
        ));
    }

    context.source().send_success(
        &COMMANDS_DAMAGE_SUCCESS
            .message([
                TextComponent::plain(format!("{amount:?}")),
                target.display_name(),
            ])
            .component(),
        true,
    );
    Ok(1)
}
