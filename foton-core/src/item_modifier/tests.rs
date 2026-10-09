//! What a modifier written in a datapack or on a command line means.

use foton_registry::{
    biome::BiomeRef, data_components::vanilla_components::CUSTOM_DATA, init_vanilla_registry,
    item_stack::ItemStack, loot_table::LootWorldView, vanilla_items,
};
use foton_utils::nbt::parse_snbt;

use super::{ItemModifier, ModifierContext, from_json, from_nbt};

struct NoWorld;

impl LootWorldView for NoWorld {
    fn loaded_block_state(&self, _x: i32, _y: i32, _z: i32) -> Option<foton_utils::BlockStateId> {
        None
    }

    fn loaded_biome(&self, _x: i32, _y: i32, _z: i32) -> Option<BiomeRef> {
        None
    }
}

fn run(modifier: &ItemModifier, stack: ItemStack) -> ItemStack {
    modifier.apply(
        stack,
        &ModifierContext {
            origin: (0.0, 0.0, 0.0),
            world: &NoWorld,
        },
    )
}

fn json(text: &str) -> ItemModifier {
    init_vanilla_registry();
    from_json(text).unwrap_or_else(|error| panic!("{text} should decode: {error}"))
}

/// A bare number is a constant, an object with no type is a uniform provider,
/// and a stack is limited to its own maximum afterwards -- so setting 99 on a
/// sword leaves one.
#[test]
fn set_count_reads_every_provider_form_and_respects_the_stack_limit() {
    let stone = ItemStack::new(&vanilla_items::STONE);

    let result = run(
        &json(r#"{"function":"minecraft:set_count","count":5}"#),
        stone.clone(),
    );
    assert_eq!(result.count(), 5);

    let result = run(
        &json(r#"{"function":"set_count","count":{"min":3,"max":3}}"#),
        stone.clone(),
    );
    assert_eq!(result.count(), 3);

    let result = run(
        &json(r#"{"function":"set_count","count":2,"add":true}"#),
        stone.clone(),
    );
    assert_eq!(result.count(), 3, "add keeps the one that was there");

    let sword = ItemStack::new(&vanilla_items::DIAMOND_SWORD);
    let result = run(&json(r#"{"function":"set_count","count":99}"#), sword);
    assert_eq!(result.count(), 1);
}

/// A list runs in order, and `limit_count` reads a bare number as an exact value.
#[test]
fn a_list_of_functions_runs_in_order() {
    let modifier = json(
        r#"[{"function":"set_count","count":50},
            {"function":"limit_count","limit":{"max":10}},
            {"function":"set_item","item":"minecraft:dirt"}]"#,
    );
    let result = run(&modifier, ItemStack::new(&vanilla_items::STONE));
    assert_eq!(result.count(), 10);
    assert_eq!(result.item().key.path.as_ref(), "dirt");
}

/// The command line writes the same thing as SNBT, with `1b` for a boolean and
/// typed numbers for the rest.
#[test]
fn inline_snbt_means_the_same_as_the_json_file() {
    init_vanilla_registry();
    let Ok(tag) = parse_snbt(r#"{function:"minecraft:set_count",count:7b,add:1b}"#) else {
        panic!("the SNBT should parse");
    };
    let Ok(modifier) = from_nbt(&tag) else {
        panic!("the modifier should decode");
    };
    let result = run(&modifier, ItemStack::new(&vanilla_items::STONE));
    assert_eq!(result.count(), 8);
}

/// A function this server cannot run, and any condition on one, must fail the
/// decode: silently skipping either would run a modifier its author did not
/// write.
#[test]
fn what_cannot_be_run_is_refused_rather_than_skipped() {
    init_vanilla_registry();
    let unsupported = r#"{"function":"minecraft:set_lore","lore":["x"],"mode":"append"}"#;
    assert!(from_json(unsupported).is_err());

    let conditional = r#"{"function":"set_count","count":1,
        "conditions":[{"condition":"minecraft:random_chance","chance":0.5}]}"#;
    assert!(from_json(conditional).is_err());

    assert!(from_json(r#"{"function":"set_count","count":1,"conditions":[]}"#).is_ok());
    assert!(from_json(r#"{"function":"set_count"}"#).is_err());
    assert!(from_json(r#"{"function":"set_count","count":{"type":"minecraft:score"}}"#).is_err());
    assert!(from_json(r#"{"function":"set_item","item":"minecraft:nothing"}"#).is_err());
}

/// `enchant_randomly` defaults to compatible enchantments only; the codec's
/// default is what keeps a sword from rolling a fishing enchantment.
#[test]
fn enchant_randomly_defaults_to_compatible_enchantments() {
    let modifier = json(r#"{"function":"minecraft:enchant_randomly"}"#);
    for _ in 0..20 {
        let result = run(&modifier, ItemStack::new(&vanilla_items::DIAMOND_SWORD));
        let Some(enchantments) = result.get_enchantments() else {
            panic!("a sword always has a compatible enchantment to roll");
        };
        assert_eq!(enchantments.iter().count(), 1);
    }
}

#[test]
fn set_custom_data_reads_its_tag_as_snbt() {
    let modifier = json(r#"{"function":"set_custom_data","tag":"{orbital:1b}"}"#);
    let result = run(&modifier, ItemStack::new(&vanilla_items::STONE));
    let Some(data) = result.get(CUSTOM_DATA) else {
        panic!("the tag should land in custom_data");
    };
    assert!(data.as_compound().contains("orbital"));
    assert!(from_json(r#"{"function":"set_custom_data","tag":"not snbt"}"#).is_err());
}
