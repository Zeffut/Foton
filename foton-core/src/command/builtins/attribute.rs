//! Vanilla attribute command.

use std::borrow::Cow;

use foton_registry::attribute::{AttributeModifierOperation, AttributeRef};
use foton_utils::{Identifier, nbt::java_double_string, translations};
use text_components::{TextComponent, translation::TranslatedMessage};

use super::super::{
    brigadier::{ArgumentType, CommandNodeBuilder, CommandSyntaxError},
    execution::{
        CommandSource, FotonArgumentType, FotonCommandContext, FotonCommandRuntime, argument,
        literal,
    },
    registration::CommandRegistration,
};
use crate::entity::{
    LivingEntity, SharedEntity,
    attribute::{AttributeMap, AttributeModifier},
};

pub(super) fn registration() -> CommandRegistration<CommandSource> {
    CommandRegistration::new(Identifier::vanilla_static("attribute"), |_| command())
}

fn command() -> CommandNodeBuilder<CommandSource, FotonCommandRuntime> {
    literal("attribute").then(
        argument("target", FotonArgumentType::entity()).then(
            argument("attribute", FotonArgumentType::attribute())
                .then(
                    literal("get")
                        .executes(|c| get_value(c, 1.0))
                        .then(scaled(get_value)),
                )
                .then(
                    literal("base")
                        .then(
                            literal("set").then(
                                argument("value", ArgumentType::double(f64::MIN, f64::MAX))
                                    .executes(set_base),
                            ),
                        )
                        .then(
                            literal("get")
                                .executes(|c| get_base(c, 1.0))
                                .then(scaled(get_base)),
                        )
                        .then(literal("reset").executes(reset_base)),
                )
                .then(
                    literal("modifier")
                        .then(
                            literal("add").then(
                                argument("id", FotonArgumentType::identifier()).then(
                                    argument("value", ArgumentType::double(f64::MIN, f64::MAX))
                                        .then(operation_node(
                                            "add_value",
                                            AttributeModifierOperation::AddValue,
                                        ))
                                        .then(operation_node(
                                            "add_multiplied_base",
                                            AttributeModifierOperation::AddMultipliedBase,
                                        ))
                                        .then(operation_node(
                                            "add_multiplied_total",
                                            AttributeModifierOperation::AddMultipliedTotal,
                                        )),
                                ),
                            ),
                        )
                        .then(
                            literal("remove").then(
                                argument("id", FotonArgumentType::identifier())
                                    .executes(remove_modifier),
                            ),
                        )
                        .then(
                            literal("value").then(
                                literal("get").then(
                                    argument("id", FotonArgumentType::identifier())
                                        .executes(|c| get_modifier(c, 1.0))
                                        .then(scaled(get_modifier)),
                                ),
                            ),
                        ),
                ),
        ),
    )
}

type Executor = fn(&FotonCommandContext<CommandSource>, f64) -> Result<i32, CommandSyntaxError>;

/// The trailing `<scale>` argument shared by the three `get` forms.
fn scaled(executor: Executor) -> CommandNodeBuilder<CommandSource, FotonCommandRuntime> {
    argument("scale", ArgumentType::double(f64::MIN, f64::MAX))
        .executes(move |c| executor(c, c.double("scale")?))
}

fn operation_node(
    name: &'static str,
    operation: AttributeModifierOperation,
) -> CommandNodeBuilder<CommandSource, FotonCommandRuntime> {
    literal(name).executes(move |c| add_modifier(c, operation))
}

/// The target resolved to a living entity, as `AttributeCommand.getLivingEntity` does.
fn living_target(target: &SharedEntity) -> Result<&dyn LivingEntity, CommandSyntaxError> {
    target.as_living_entity().ok_or_else(|| {
        CommandSyntaxError::dynamic(
            translations::COMMANDS_ATTRIBUTE_FAILED_ENTITY
                .message([TextComponent::plain(target.plain_text_name())])
                .component(),
        )
    })
}

fn no_such_attribute(target: &SharedEntity, attribute: AttributeRef) -> CommandSyntaxError {
    CommandSyntaxError::dynamic(
        translations::COMMANDS_ATTRIBUTE_FAILED_NO_ATTRIBUTE
            .message([
                TextComponent::plain(target.plain_text_name()),
                description(attribute),
            ])
            .component(),
    )
}

/// Vanilla parity: `AttributeCommand.getEntityWithAttribute`.
fn living_with_attribute(
    target: &SharedEntity,
    attribute: AttributeRef,
) -> Result<&dyn LivingEntity, CommandSyntaxError> {
    let living = living_target(target)?;
    if !living.attributes().lock().has_attribute(attribute) {
        return Err(no_such_attribute(target, attribute));
    }
    Ok(living)
}

/// Vanilla parity: `AttributeCommand.getAttributeDescription`.
const fn description(attribute: AttributeRef) -> TextComponent {
    TextComponent::translated(TranslatedMessage {
        key: Cow::Borrowed(attribute.translation_key),
        args: None,
        fallback: None,
    })
}

fn number(value: f64) -> TextComponent {
    TextComponent::plain(java_double_string(value))
}

/// Java's `(int) (result * scale)`; Rust's saturating `as` cast is the same conversion.
#[expect(
    clippy::cast_possible_truncation,
    reason = "vanilla narrows with a Java (int) cast, which saturates the same way"
)]
const fn scaled_result(value: f64, scale: f64) -> i32 {
    (value * scale) as i32
}

fn get_value(
    context: &FotonCommandContext<CommandSource>,
    scale: f64,
) -> Result<i32, CommandSyntaxError> {
    let target = context.entity("target")?;
    let attribute = context.attribute("attribute")?;
    let living = living_with_attribute(&target, attribute)?;
    let result = living
        .attributes()
        .lock()
        .get_value(attribute)
        .ok_or_else(|| no_such_attribute(&target, attribute))?;
    context.source().send_success(
        &translations::COMMANDS_ATTRIBUTE_VALUE_GET_SUCCESS
            .message([
                description(attribute),
                TextComponent::plain(target.plain_text_name()),
                number(result),
            ])
            .component(),
        false,
    );
    Ok(scaled_result(result, scale))
}

fn get_base(
    context: &FotonCommandContext<CommandSource>,
    scale: f64,
) -> Result<i32, CommandSyntaxError> {
    let target = context.entity("target")?;
    let attribute = context.attribute("attribute")?;
    let living = living_with_attribute(&target, attribute)?;
    let result = living
        .attributes()
        .lock()
        .get_base_value(attribute)
        .ok_or_else(|| no_such_attribute(&target, attribute))?;
    context.source().send_success(
        &translations::COMMANDS_ATTRIBUTE_BASE_VALUE_GET_SUCCESS
            .message([
                description(attribute),
                TextComponent::plain(target.plain_text_name()),
                number(result),
            ])
            .component(),
        false,
    );
    Ok(scaled_result(result, scale))
}

fn set_base(context: &FotonCommandContext<CommandSource>) -> Result<i32, CommandSyntaxError> {
    let target = context.entity("target")?;
    let attribute = context.attribute("attribute")?;
    let value = context.double("value")?;
    let living = living_target(&target)?;
    {
        let mut attributes = living.attributes().lock();
        if !attributes.has_attribute(attribute) {
            return Err(no_such_attribute(&target, attribute));
        }
        attributes.set_base_value(attribute, value);
    }
    context.source().send_success(
        &translations::COMMANDS_ATTRIBUTE_BASE_VALUE_SET_SUCCESS
            .message([
                description(attribute),
                TextComponent::plain(target.plain_text_name()),
                number(value),
            ])
            .component(),
        false,
    );
    Ok(1)
}

fn reset_base(context: &FotonCommandContext<CommandSource>) -> Result<i32, CommandSyntaxError> {
    let target = context.entity("target")?;
    let attribute = context.attribute("attribute")?;
    let living = living_target(&target)?;
    let value = living
        .attributes()
        .lock()
        .reset_base_value(attribute, target.entity_type())
        .ok_or_else(|| no_such_attribute(&target, attribute))?;
    context.source().send_success(
        &translations::COMMANDS_ATTRIBUTE_BASE_VALUE_RESET_SUCCESS
            .message([
                description(attribute),
                TextComponent::plain(target.plain_text_name()),
                number(value),
            ])
            .component(),
        false,
    );
    Ok(1)
}

fn add_modifier(
    context: &FotonCommandContext<CommandSource>,
    operation: AttributeModifierOperation,
) -> Result<i32, CommandSyntaxError> {
    let target = context.entity("target")?;
    let attribute = context.attribute("attribute")?;
    let id = context.identifier("id")?.clone();
    let amount = context.double("value")?;
    let living = living_with_attribute(&target, attribute)?;
    let added = living.attributes().lock().add_modifier(
        attribute,
        AttributeModifier {
            id: id.clone(),
            amount,
            operation,
        },
        true,
    );
    if !added {
        return Err(CommandSyntaxError::dynamic(
            translations::COMMANDS_ATTRIBUTE_FAILED_MODIFIER_ALREADY_PRESENT
                .message([
                    TextComponent::plain(id.to_string()),
                    description(attribute),
                    TextComponent::plain(target.plain_text_name()),
                ])
                .component(),
        ));
    }
    context.source().send_success(
        &translations::COMMANDS_ATTRIBUTE_MODIFIER_ADD_SUCCESS
            .message([
                TextComponent::plain(id.to_string()),
                description(attribute),
                TextComponent::plain(target.plain_text_name()),
            ])
            .component(),
        false,
    );
    Ok(1)
}

fn remove_modifier(
    context: &FotonCommandContext<CommandSource>,
) -> Result<i32, CommandSyntaxError> {
    let target = context.entity("target")?;
    let attribute = context.attribute("attribute")?;
    let id = context.identifier("id")?;
    let living = living_with_attribute(&target, attribute)?;
    if !living.attributes().lock().remove_modifier(attribute, id) {
        return Err(no_such_modifier(&target, attribute, id));
    }
    context.source().send_success(
        &translations::COMMANDS_ATTRIBUTE_MODIFIER_REMOVE_SUCCESS
            .message([
                TextComponent::plain(id.to_string()),
                description(attribute),
                TextComponent::plain(target.plain_text_name()),
            ])
            .component(),
        false,
    );
    Ok(1)
}

fn get_modifier(
    context: &FotonCommandContext<CommandSource>,
    scale: f64,
) -> Result<i32, CommandSyntaxError> {
    let target = context.entity("target")?;
    let attribute = context.attribute("attribute")?;
    let id = context.identifier("id")?;
    let living = living_with_attribute(&target, attribute)?;
    let result = modifier_value(&living.attributes().lock(), attribute, id)
        .ok_or_else(|| no_such_modifier(&target, attribute, id))?;
    context.source().send_success(
        &translations::COMMANDS_ATTRIBUTE_MODIFIER_VALUE_GET_SUCCESS
            .message([
                TextComponent::plain(id.to_string()),
                description(attribute),
                TextComponent::plain(target.plain_text_name()),
                number(result),
            ])
            .component(),
        false,
    );
    Ok(scaled_result(result, scale))
}

/// Vanilla parity: `AttributeMap.getModifierValue`.
fn modifier_value(
    attributes: &AttributeMap,
    attribute: AttributeRef,
    id: &Identifier,
) -> Option<f64> {
    attributes
        .get_instance(attribute)?
        .modifiers()
        .iter()
        .find(|modifier| modifier.id == *id)
        .map(|modifier| modifier.amount)
}

fn no_such_modifier(
    target: &SharedEntity,
    attribute: AttributeRef,
    id: &Identifier,
) -> CommandSyntaxError {
    CommandSyntaxError::dynamic(
        translations::COMMANDS_ATTRIBUTE_FAILED_NO_MODIFIER
            .message([
                description(attribute),
                TextComponent::plain(target.plain_text_name()),
                TextComponent::plain(id.to_string()),
            ])
            .component(),
    )
}
