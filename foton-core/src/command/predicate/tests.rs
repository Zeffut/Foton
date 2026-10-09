use std::ptr;

use foton_registry::advancement::predicate::{ConditionTerm, ContextAwarePredicate};
use foton_registry::data_components::vanilla_components::{CUSTOM_DATA, CustomData};
use foton_registry::item_stack::ItemStack;
use foton_registry::{init_vanilla_registry, vanilla_items};
use foton_utils::nbt::parse_snbt;

use super::load_predicate;
use crate::advancement::predicate::item_matches;

/// The shape the Orbital Strike Cannon datapack ships: a fishing rod in the
/// main hand whose `custom_data` carries a marker.
const HELD_ROD: &str = r#"{
    "condition": "minecraft:entity_properties",
    "entity": "this",
    "predicate": {
        "equipment": {
            "mainhand": {
                "items": ["minecraft:fishing_rod"],
                "components": { "minecraft:custom_data": { "nuke_1": 1 } }
            }
        }
    }
}"#;

fn rod_with(snbt: Option<&str>) -> ItemStack {
    let mut rod = ItemStack::new(&vanilla_items::FISHING_ROD);
    if let Some(snbt) = snbt {
        let data = CustomData::from_nbt_value(&parse_snbt(snbt).expect("custom data SNBT"))
            .expect("typed custom data");
        rod.set(CUSTOM_DATA, data);
    }
    rod
}

fn only_term(predicate: ContextAwarePredicate) -> &'static ConditionTerm {
    match predicate {
        [term] => term,
        other => panic!("expected one term, found {}", other.len()),
    }
}

#[test]
fn equipment_with_custom_data_matches_what_vanilla_json_decoding_stores() {
    init_vanilla_registry();
    let predicate = load_predicate(HELD_ROD).expect("the datapack predicate should load");
    let ConditionTerm::EntityProperties(entity) = only_term(predicate) else {
        panic!("entity_properties should lower to an entity predicate");
    };
    let Some(mainhand) = entity
        .equipment
        .and_then(|equipment| equipment.mainhand.as_ref())
    else {
        panic!("the main hand check should be kept");
    };

    // JSON `1` decodes to the smallest NBT type, a byte, and custom_data is
    // compared whole -- so the marker has to be stored as `1b`.
    assert!(item_matches(mainhand, &rod_with(Some("{nuke_1:1b}"))));
    assert!(!item_matches(mainhand, &rod_with(Some("{nuke_1:1}"))));
    assert!(!item_matches(mainhand, &rod_with(Some("{nuke_2:1b}"))));
    assert!(!item_matches(mainhand, &rod_with(None)));
    let mut stick = ItemStack::new(&vanilla_items::STICK);
    stick.set(
        CUSTOM_DATA,
        CustomData::from_nbt_value(&parse_snbt("{nuke_1:1b}").expect("SNBT")).expect("data"),
    );
    assert!(!item_matches(mainhand, &stick));
}

#[test]
fn a_list_is_an_implicit_all_of_and_inverted_nests() {
    let predicate = load_predicate(
        r#"[
            {"condition": "minecraft:weather_check", "raining": false},
            {"condition": "minecraft:inverted", "term": {"condition": "minecraft:random_chance", "chance": 0.25}}
        ]"#,
    )
    .expect("a list of conditions should load");
    assert_eq!(predicate.len(), 2);
    assert!(matches!(
        predicate[0],
        ConditionTerm::WeatherCheck {
            raining: Some(false),
            thundering: None
        }
    ));
    let ConditionTerm::Inverted(ConditionTerm::RandomChance(chance)) = &predicate[1] else {
        panic!("inverted random_chance should nest");
    };
    assert!((chance - 0.25).abs() < f32::EPSILON);
}

#[test]
fn what_foton_cannot_model_is_refused_with_the_path_instead_of_loaded_weaker() {
    init_vanilla_registry();
    let unsupported_key = load_predicate(
        r#"{"condition":"minecraft:entity_properties","entity":"this",
            "predicate":{"equipment":{"mainhand":{"items":"minecraft:stick"}},"effects":{}}}"#,
    )
    .expect_err("an unmodeled sub-predicate must not be dropped");
    assert!(unsupported_key.contains("effects"), "{unsupported_key}");

    let unsupported_condition =
        load_predicate(r#"{"condition":"minecraft:value_check","value":1,"range":1}"#)
            .expect_err("an unmodeled condition must not load");
    assert!(
        unsupported_condition.contains("value_check"),
        "{unsupported_condition}"
    );

    let invalid_item = load_predicate(
        r#"{"condition":"minecraft:entity_properties","entity":"this",
            "predicate":{"equipment":{"mainhand":{"items":"minecraft:not_an_item"}}}}"#,
    )
    .expect_err("an unknown item id must not load");
    assert!(
        invalid_item.contains("equipment.mainhand"),
        "{invalid_item}"
    );

    assert!(load_predicate("{ not json").is_err());
}

#[test]
fn reloading_unchanged_text_reuses_the_first_parse() {
    let first =
        load_predicate(r#"{"condition":"minecraft:random_chance","chance":0.5}"#).expect("loads");
    let second =
        load_predicate(r#"{"condition":"minecraft:random_chance","chance":0.5}"#).expect("loads");
    assert!(ptr::eq(first, second));
}
