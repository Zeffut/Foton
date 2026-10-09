//! Item modifiers: the loot functions `/item modify` runs on a stack.
//!
//! Vanilla parity: `ResourceOrIdArgument.lootModifier` and
//! `LootItemFunctions.ROOT_CODEC`. A modifier is a datapack `item_modifier/*.json`
//! file, or the same value written inline as SNBT; either way it is one
//! function, or a list of them run in order.
//!
//! The loot engine in `foton_registry::loot_table` is built from `&'static`
//! generated data, so a modifier read at run time cannot be expressed as
//! `LootFunction` for every function. Each function the engine can already
//! apply from owned data is decoded into it and run through
//! [`LootFunction::apply`]; the rest are applied to the stack directly, with
//! the same `ItemStack` calls the engine makes. A function outside the
//! supported set, or any function carrying `conditions`, fails to decode and
//! says so rather than being skipped.

mod decode;
#[cfg(test)]
mod tests;

use foton_registry::{
    item_stack::ItemStack,
    loot_table::{
        EnchantmentOptions, LootContext, LootFunction, LootWorldView, NameTarget, NumberProvider,
    },
};
use foton_utils::Identifier;
use simdnbt::owned::{NbtCompound, NbtTag};
use text_components::TextComponent;

pub use decode::{from_json, from_nbt};

/// A decoded item modifier.
#[derive(Debug, Clone)]
pub struct ItemModifier {
    functions: Vec<ModifierFunction>,
}

/// What a modifier needs from the command that runs it.
///
/// Vanilla parity: the `LootContextParamSets.COMMAND` params,
/// `ORIGIN` and an optional `THIS_ENTITY`.
pub struct ModifierContext<'a> {
    /// Where the command runs, which `Score`-free providers never read but
    /// functions such as `exploration_map` aim from.
    pub origin: (f64, f64, f64),
    /// The world the command runs in.
    pub world: &'a dyn LootWorldView,
}

#[derive(Debug, Clone)]
enum ModifierFunction {
    /// A function the loot engine applies from owned data.
    Loot(LootFunction),
    /// `SetNameFunction`, whose component cannot live in the engine's `fn()`.
    SetName {
        name: TextComponent,
        target: NameTarget,
    },
    /// `SetEnchantmentsFunction`, whose map the engine holds as `&'static`.
    SetEnchantments {
        enchantments: Vec<(Identifier, NumberProvider)>,
        add: bool,
    },
    /// `SetCustomDataFunction`.
    SetCustomData(NbtCompound),
}

impl ItemModifier {
    /// Runs every function in order on `stack`.
    ///
    /// Vanilla parity: `ItemCommands.applyModifier`, which ends by limiting the
    /// stack to its own maximum.
    #[must_use]
    pub fn apply(&self, mut stack: ItemStack, context: &ModifierContext<'_>) -> ItemStack {
        let mut rng = rand::rng();
        let mut loot = LootContext::new(&mut rng)
            .with_world(context.world)
            .with_origin(context.origin.0, context.origin.1, context.origin.2);
        for function in &self.functions {
            match function {
                ModifierFunction::Loot(function) => function.apply(&mut stack, &mut loot),
                ModifierFunction::SetName { name, target } => {
                    stack.set_name(name.clone(), *target);
                }
                ModifierFunction::SetEnchantments { enchantments, add } => {
                    let resolved = enchantments
                        .iter()
                        .map(|(key, provider)| {
                            let level = provider.get_int(loot.rng).max(0);
                            (key.clone(), u32::try_from(level).unwrap_or(0))
                        })
                        .collect::<Vec<_>>();
                    stack.set_enchantments(&resolved, *add);
                }
                ModifierFunction::SetCustomData(tag) => {
                    if let Some(data) =
                        foton_registry::data_components::CustomData::try_from_compound(tag.clone())
                    {
                        stack.set_custom_data(&data);
                    }
                }
            }
        }
        stack.count = stack.count.min(stack.max_stack_size());
        stack
    }
}

/// The options an `enchant_*` function draws from, when the modifier names a
/// tag. A list of explicit enchantments is not supported by the engine at run
/// time, and decoding says so.
fn tag_options(tag: Identifier) -> EnchantmentOptions {
    EnchantmentOptions::Tag(tag)
}

/// Reads a value's `NbtTag` as the JSON the codec decoder works on.
///
/// SNBT keeps the number types JSON loses; the decoder accepts any numeric
/// type where the codec accepts a number, and `1b`/`0b` where it accepts a
/// boolean, so nothing is lost by flattening here.
fn nbt_to_json(tag: &NbtTag) -> serde_json::Value {
    use serde_json::Value;
    match tag {
        NbtTag::Byte(value) => Value::from(*value),
        NbtTag::Short(value) => Value::from(*value),
        NbtTag::Int(value) => Value::from(*value),
        NbtTag::Long(value) => Value::from(*value),
        NbtTag::Float(value) => Value::from(f64::from(*value)),
        NbtTag::Double(value) => Value::from(*value),
        NbtTag::String(value) => Value::from(value.to_str().into_owned()),
        NbtTag::Compound(compound) => Value::Object(
            compound
                .iter()
                .map(|(key, value)| (key.to_str().into_owned(), nbt_to_json(value)))
                .collect(),
        ),
        other => foton_utils::nbt::nbt_collection_values(other).map_or(Value::Null, |values| {
            Value::Array(values.iter().map(nbt_to_json).collect())
        }),
    }
}
