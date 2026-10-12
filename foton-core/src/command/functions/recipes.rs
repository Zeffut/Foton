//! Crafting recipes read from datapacks.
//!
//! Vanilla parity: `RecipeManager` over `Recipe.CODEC`, restricted to
//! `minecraft:crafting_shaped` and `minecraft:crafting_shapeless`, the two the
//! runtime registry can take after it froze. Any other type is refused with its
//! name instead of being skipped, so a pack that depends on one says so in the
//! log. An ingredient in the pre-1.21.2 `{"item": ...}` form is refused too, as
//! vanilla refuses it.

use std::borrow::Cow;

use foton_registry::{
    ItemStackTemplate, REGISTRY, RegistryExt as _,
    data_components::DataComponentPatch,
    item_stack::ItemStack,
    items::ItemRef,
    recipe::{CraftingCategory, Ingredient, RecipeResult, ShapedRecipe, ShapelessRecipe},
};
use foton_utils::{Identifier, nbt::parse_snbt_argument};
use rustc_hash::{FxHashMap, FxHashSet};
use serde_json::{Map, Value};
use simdnbt::owned::{NbtCompound, NbtList, NbtTag};

/// `ShapedRecipePattern.MAX_WIDTH` and `MAX_HEIGHT`.
const MAX_GRID: usize = 3;
const CUSTOM_DATA: &str = "minecraft:custom_data";

/// Reads one recipe file and registers it. `Ok(false)` means a recipe with that
/// id already exists, which a reload meets for every recipe it loaded before.
pub(super) fn register(id: Identifier, text: &str) -> Result<bool, String> {
    let value: Value = serde_json::from_str(text).map_err(|error| error.to_string())?;
    let object = value.as_object().ok_or("a recipe is a JSON object")?;
    let kind = object
        .get("type")
        .and_then(Value::as_str)
        .ok_or("missing recipe type")?;
    match kind {
        "minecraft:crafting_shaped" => Ok(REGISTRY
            .recipes
            .register_runtime_shaped(shaped(id, object)?)),
        "minecraft:crafting_shapeless" => Ok(REGISTRY
            .recipes
            .register_runtime_shapeless(shapeless(id, object)?)),
        other => Err(format!("recipe type {other} is not supported")),
    }
}

fn shaped(id: Identifier, object: &Map<String, Value>) -> Result<ShapedRecipe, String> {
    let rows = object
        .get("pattern")
        .and_then(Value::as_array)
        .ok_or("missing pattern")?
        .iter()
        .map(|row| row.as_str().ok_or("a pattern row is a string"))
        .collect::<Result<Vec<_>, _>>()?;
    let keys = object
        .get("key")
        .and_then(Value::as_object)
        .ok_or("missing key")?;

    // Vanilla parity: `ShapedRecipePattern.unpack`.
    if rows.is_empty() || rows.len() > MAX_GRID {
        return Err(format!(
            "invalid pattern: a recipe has 1 to {MAX_GRID} rows"
        ));
    }
    let width = rows[0].chars().count();
    if width == 0 || width > MAX_GRID || rows.iter().any(|row| row.chars().count() != width) {
        return Err(format!(
            "invalid pattern: rows are 1 to {MAX_GRID} wide and all the same width"
        ));
    }
    let mut ingredients: FxHashMap<char, Ingredient> = FxHashMap::default();
    for (name, value) in keys {
        let mut chars = name.chars();
        let (Some(symbol), None) = (chars.next(), chars.next()) else {
            return Err(format!("key '{name}' must be a single character"));
        };
        if symbol == ' ' {
            return Err("invalid key entry: ' ' is reserved for empty slots".to_owned());
        }
        ingredients.insert(symbol, ingredient(value)?);
    }
    let mut unused: FxHashSet<char> = ingredients.keys().copied().collect();
    let mut pattern = Vec::with_capacity(width * rows.len());
    for row in &rows {
        for symbol in row.chars() {
            if symbol == ' ' {
                pattern.push(Ingredient::Empty);
                continue;
            }
            let Some(found) = ingredients.get(&symbol) else {
                return Err(format!(
                    "pattern references symbol '{symbol}' but it's not defined in the key"
                ));
            };
            unused.remove(&symbol);
            pattern.push(found.clone());
        }
    }
    if !unused.is_empty() {
        return Err("key defines symbols that aren't used in pattern".to_owned());
    }

    Ok(ShapedRecipe::new(
        id,
        category(object)?,
        width,
        rows.len(),
        Box::leak(pattern.into_boxed_slice()),
        result(object)?,
        notification(object),
        group(object),
    ))
}

fn shapeless(id: Identifier, object: &Map<String, Value>) -> Result<ShapelessRecipe, String> {
    let ingredients = object
        .get("ingredients")
        .and_then(Value::as_array)
        .ok_or("missing ingredients")?
        .iter()
        .map(ingredient)
        .collect::<Result<Vec<_>, _>>()?;
    if ingredients.is_empty() {
        return Err("a shapeless recipe needs at least one ingredient".to_owned());
    }
    if ingredients.len() > MAX_GRID * MAX_GRID {
        return Err("too many ingredients for a shapeless recipe".to_owned());
    }
    Ok(ShapelessRecipe {
        id,
        category: category(object)?,
        ingredients: Box::leak(ingredients.into_boxed_slice()),
        result: result(object)?,
        show_notification: notification(object),
        group: group(object),
    })
}

/// Vanilla parity: `Ingredient.CODEC`, an item id, a `#tag`, or a list of item ids.
fn ingredient(value: &Value) -> Result<Ingredient, String> {
    match value {
        Value::String(text) => match text.strip_prefix('#') {
            Some(tag) => Ok(Ingredient::Tag(parse_id(tag)?)),
            None => Ok(Ingredient::Item(item(text)?)),
        },
        Value::Array(values) => {
            let items = values
                .iter()
                .map(|value| match value {
                    Value::String(text) => item(text),
                    other => Err(not_an_ingredient(other)),
                })
                .collect::<Result<Vec<_>, _>>()?;
            match items.as_slice() {
                [] => Err("an ingredient list cannot be empty".to_owned()),
                [only] => Ok(Ingredient::Item(only)),
                _ => Ok(Ingredient::Choice(Box::leak(items.into_boxed_slice()))),
            }
        }
        other => Err(not_an_ingredient(other)),
    }
}

/// An ingredient is an item id, a `#tag` or a list of item ids. The `{"item": ...}` object form
/// died with 1.21.2, and older packs still carry it.
fn not_an_ingredient(value: &Value) -> String {
    if value.is_object() {
        "an ingredient must be an item id, a #tag or a list of item ids; the {\"item\": ...} \
         object form was removed in 1.21.2"
            .to_owned()
    } else {
        "an ingredient must be an item id, a #tag or a list of item ids".to_owned()
    }
}

fn item(text: &str) -> Result<ItemRef, String> {
    REGISTRY
        .items
        .by_key(&parse_id(text)?)
        .ok_or_else(|| format!("unknown item {text}"))
}

fn parse_id(text: &str) -> Result<Identifier, String> {
    text.parse()
        .map_err(|_| format!("'{text}' is not a valid identifier"))
}

/// Vanilla parity: `CraftingRecipeCategory`, which defaults to misc.
fn category(object: &Map<String, Value>) -> Result<CraftingCategory, String> {
    match object.get("category").and_then(Value::as_str) {
        None | Some("misc") => Ok(CraftingCategory::Misc),
        Some("building") => Ok(CraftingCategory::Building),
        Some("redstone") => Ok(CraftingCategory::Redstone),
        Some("equipment") => Ok(CraftingCategory::Equipment),
        Some(other) => Err(format!("unknown crafting category {other}")),
    }
}

fn notification(object: &Map<String, Value>) -> bool {
    object
        .get("show_notification")
        .and_then(Value::as_bool)
        .unwrap_or(true)
}

fn group(object: &Map<String, Value>) -> Cow<'static, str> {
    Cow::Owned(
        object
            .get("group")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
    )
}

/// Vanilla parity: `ItemStackTemplate.CODEC`, an `id`, a `count` that defaults to 1
/// and a map of `components`.
fn result(object: &Map<String, Value>) -> Result<RecipeResult, String> {
    let result = object
        .get("result")
        .and_then(Value::as_object)
        .ok_or("missing result")?;
    let id = result
        .get("id")
        .and_then(Value::as_str)
        .ok_or("the result needs an id")?;
    let count = match result.get("count") {
        None => 1,
        Some(count) => count
            .as_i64()
            .and_then(|count| i32::try_from(count).ok())
            .filter(|count| (1..=99).contains(count))
            .ok_or("the result count is 1 to 99")?,
    };
    let mut patch = DataComponentPatch::new();
    if let Some(components) = result.get("components") {
        for (name, value) in components
            .as_object()
            .ok_or("result components are an object")?
        {
            read_component(&mut patch, name, value)?;
        }
    }
    let stack = ItemStack::with_count_and_patch(item(id)?, count, patch);
    let template = ItemStackTemplate::from_stack(&stack).map_err(|error| error.to_string())?;
    Ok(RecipeResult::from_template(template))
}

fn read_component(patch: &mut DataComponentPatch, name: &str, value: &Value) -> Result<(), String> {
    let key = parse_id(name.strip_prefix('!').unwrap_or(name))?;
    if name.starts_with('!') {
        patch.remove_raw(key);
        return Ok(());
    }
    let entry = REGISTRY
        .data_components
        .by_key(&key)
        .filter(|entry| entry.is_persistent())
        .ok_or_else(|| format!("unknown component {key}"))?;
    // `custom_data` is written as a compound or as SNBT text; both reach the codec as a compound.
    let tag = match value {
        Value::String(text) if key.to_string() == CUSTOM_DATA => {
            let (tag, consumed) = parse_snbt_argument(text).map_err(|error| error.to_string())?;
            if consumed != text.trim_end().len() {
                return Err(format!("trailing data after the {CUSTOM_DATA} value"));
            }
            tag
        }
        _ => to_nbt(value)?,
    };
    let data = entry
        .read_nbt_owned(&tag)
        .ok_or_else(|| format!("component {key} does not match its codec"))?;
    if !patch.set_raw(key.clone(), data) {
        return Err(format!(
            "component {key} does not match its registered type"
        ));
    }
    Ok(())
}

/// The NBT a component codec reads for a JSON value, as vanilla's `JsonOps` -> `NbtOps` gives it.
fn to_nbt(value: &Value) -> Result<NbtTag, String> {
    Ok(match value {
        Value::Null => return Err("null has no NBT form".to_owned()),
        Value::Bool(flag) => NbtTag::Byte(i8::from(*flag)),
        Value::Number(number) => match (number.as_i64(), number.as_f64()) {
            (Some(whole), _) => i32::try_from(whole).map_or(NbtTag::Long(whole), NbtTag::Int),
            (None, Some(float)) => NbtTag::Double(float),
            (None, None) => return Err("a number out of range".to_owned()),
        },
        Value::String(text) => NbtTag::String(text.as_str().into()),
        Value::Object(map) => {
            let mut compound = NbtCompound::new();
            for (key, value) in map {
                compound.insert(key.as_str(), to_nbt(value)?);
            }
            NbtTag::Compound(compound)
        }
        Value::Array(values) => NbtTag::List(to_list(values)?),
    })
}

fn to_list(values: &[Value]) -> Result<NbtList, String> {
    let tags = values.iter().map(to_nbt).collect::<Result<Vec<_>, _>>()?;
    let mut compounds = Vec::new();
    let mut strings = Vec::new();
    let mut ints = Vec::new();
    for tag in tags {
        match tag {
            NbtTag::Compound(compound) => compounds.push(compound),
            NbtTag::String(text) => strings.push(text),
            NbtTag::Int(int) => ints.push(int),
            _ => {
                return Err("only lists of compounds, strings or integers are supported".to_owned());
            }
        }
    }
    match (compounds.is_empty(), strings.is_empty(), ints.is_empty()) {
        (true, true, true) => Ok(NbtList::Empty),
        (false, true, true) => Ok(NbtList::Compound(compounds)),
        (true, false, true) => Ok(NbtList::String(strings)),
        (true, true, false) => Ok(NbtList::Int(ints)),
        _ => Err("a list mixes element types".to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use foton_registry::{
        REGISTRY,
        data_components::vanilla_components::{CUSTOM_DATA, CUSTOM_NAME},
        init_vanilla_registry,
    };
    use foton_utils::Identifier;

    use super::register;

    fn id(path: &str) -> Identifier {
        Identifier::new("recipe_test", path.to_owned())
    }

    /// The shape of `orbital:nuke_1` from Orbital Strike Cannon V7: components on the result.
    const SHAPED: &str = r#"{
        "type": "minecraft:crafting_shaped",
        "pattern": [" T ", "TFT", " T "],
        "key": {"T": ["minecraft:tnt"], "F": ["minecraft:fishing_rod"]},
        "result": {
            "id": "minecraft:knowledge_book",
            "components": {
                "minecraft:custom_data": "{recipe_nuke_1:1b}",
                "minecraft:custom_name": {"text": "NUKE POWER 1", "italic": false}
            }
        }
    }"#;

    #[test]
    fn a_shaped_recipe_keeps_the_result_components() {
        init_vanilla_registry();
        assert_eq!(register(id("nuke_1"), SHAPED), Ok(true));
        // A reload meets every recipe it loaded before; that is not an error.
        assert_eq!(register(id("nuke_1"), SHAPED), Ok(false));

        let Some(recipe) = REGISTRY.recipes.get_shaped(&id("nuke_1")) else {
            panic!("the recipe should be registered");
        };
        assert_eq!((recipe.width, recipe.height), (3, 3));
        let result = recipe.assemble();
        assert_eq!(
            result.item.key,
            Identifier::vanilla_static("knowledge_book")
        );
        assert!(
            result.get(CUSTOM_DATA).is_some(),
            "custom_data should survive"
        );
        assert!(
            result.get(CUSTOM_NAME).is_some(),
            "custom_name should survive"
        );
    }

    #[test]
    fn what_vanilla_refuses_is_refused_with_its_reason() {
        init_vanilla_registry();
        let legacy = SHAPED.replace("[\"minecraft:tnt\"]", "[{\"item\": \"minecraft:tnt\"}]");
        assert!(register(id("legacy"), &legacy).is_err());

        let unused_key = SHAPED.replace("\"TFT\"", "\"T T\"");
        assert!(
            register(id("unused_key"), &unused_key)
                .is_err_and(|error| error.contains("aren't used"))
        );

        let smelting = r#"{"type":"minecraft:smelting","ingredient":"minecraft:iron_ore"}"#;
        assert!(
            register(id("smelting"), smelting)
                .is_err_and(|error| error.contains("minecraft:smelting"))
        );
    }

    #[test]
    fn a_shapeless_recipe_takes_tags_and_choices() {
        init_vanilla_registry();
        let text = r##"{
            "type": "minecraft:crafting_shapeless",
            "ingredients": ["#minecraft:logs", ["minecraft:stick", "minecraft:coal"]],
            "result": {"id": "minecraft:torch", "count": 4}
        }"##;
        assert_eq!(register(id("torches"), text), Ok(true));
        assert!(REGISTRY.recipes.get_shapeless(&id("torches")).is_some());
    }
}
