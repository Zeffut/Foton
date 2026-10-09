//! Vanilla recipe book command.
//!
//! Vanilla parity: `RecipeCommand`.

use foton_registry::REGISTRY;
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
use crate::{entity::Entity as _, player::Player};

pub(super) fn registration() -> CommandRegistration<CommandSource> {
    CommandRegistration::new(Identifier::vanilla_static("recipe"), |_| command())
}

fn command() -> CommandNodeBuilder<CommandSource, FotonCommandRuntime> {
    literal("recipe")
        .then(literal("give").then(targets_then_recipes(give_one, give_all)))
        .then(literal("take").then(targets_then_recipes(take_one, take_all)))
}

fn targets_then_recipes(
    one: fn(&FotonCommandContext<CommandSource>) -> Result<i32, CommandSyntaxError>,
    all: fn(&FotonCommandContext<CommandSource>) -> Result<i32, CommandSyntaxError>,
) -> CommandNodeBuilder<CommandSource, FotonCommandRuntime> {
    argument("targets", FotonArgumentType::players())
        .then(argument("recipe", FotonArgumentType::recipe()).executes(one))
        .then(literal("*").executes(all))
}

fn give_one(context: &FotonCommandContext<CommandSource>) -> Result<i32, CommandSyntaxError> {
    give(context, &[named_recipe(context)?])
}

fn give_all(context: &FotonCommandContext<CommandSource>) -> Result<i32, CommandSyntaxError> {
    give(context, &REGISTRY.recipes.recipe_keys())
}

fn take_one(context: &FotonCommandContext<CommandSource>) -> Result<i32, CommandSyntaxError> {
    take(context, &[named_recipe(context)?])
}

fn take_all(context: &FotonCommandContext<CommandSource>) -> Result<i32, CommandSyntaxError> {
    take(context, &REGISTRY.recipes.recipe_keys())
}

/// Vanilla parity: `ResourceKeyArgument.getRecipe`, which looks the key up when
/// the command runs rather than when it parses, so a recipe a later reload
/// adds is not a syntax error.
fn named_recipe(
    context: &FotonCommandContext<CommandSource>,
) -> Result<Identifier, CommandSyntaxError> {
    let key = context.recipe_key("recipe")?;
    if !REGISTRY.recipes.contains_key(key) {
        let message = translations::RECIPE_NOT_FOUND
            .message([key.to_string()])
            .component();
        return Err(CommandSyntaxError::dynamic(message));
    }
    Ok(key.clone())
}

/// Vanilla parity: `RecipeCommand.giveRecipes`. The success message counts the
/// recipes asked for, while the command's result counts the displays actually
/// added.
fn give(
    context: &FotonCommandContext<CommandSource>,
    recipes: &[Identifier],
) -> Result<i32, CommandSyntaxError> {
    let players = context.players("targets")?;
    let success = players
        .iter()
        .map(|player| player.award_recipes(recipes))
        .sum::<usize>();
    if success == 0 {
        return Err(CommandSyntaxError::dynamic(TextComponent::from(
            &translations::COMMANDS_RECIPE_GIVE_FAILED,
        )));
    }
    let message = if let [player] = players.as_slice() {
        translations::COMMANDS_RECIPE_GIVE_SUCCESS_SINGLE
            .message([
                TextComponent::from(recipes.len().to_string()),
                display_name(player),
            ])
            .component()
    } else {
        translations::COMMANDS_RECIPE_GIVE_SUCCESS_MULTIPLE
            .message([
                TextComponent::from(recipes.len().to_string()),
                TextComponent::from(players.len().to_string()),
            ])
            .component()
    };
    context.source().send_success(&message, true);
    result(success)
}

/// Vanilla parity: `RecipeCommand.takeRecipes`.
fn take(
    context: &FotonCommandContext<CommandSource>,
    recipes: &[Identifier],
) -> Result<i32, CommandSyntaxError> {
    let players = context.players("targets")?;
    let success = players
        .iter()
        .map(|player| player.reset_recipes(recipes))
        .sum::<usize>();
    if success == 0 {
        return Err(CommandSyntaxError::dynamic(TextComponent::from(
            &translations::COMMANDS_RECIPE_TAKE_FAILED,
        )));
    }
    let message = if let [player] = players.as_slice() {
        translations::COMMANDS_RECIPE_TAKE_SUCCESS_SINGLE
            .message([
                TextComponent::from(recipes.len().to_string()),
                display_name(player),
            ])
            .component()
    } else {
        translations::COMMANDS_RECIPE_TAKE_SUCCESS_MULTIPLE
            .message([
                TextComponent::from(recipes.len().to_string()),
                TextComponent::from(players.len().to_string()),
            ])
            .component()
    };
    context.source().send_success(&message, true);
    result(success)
}

fn display_name(player: &Player) -> TextComponent {
    TextComponent::plain(player.plain_text_name())
}

fn result(success: usize) -> Result<i32, CommandSyntaxError> {
    i32::try_from(success)
        .map_err(|_| CommandSyntaxError::dynamic("Recipe count exceeds the command result range"))
}
