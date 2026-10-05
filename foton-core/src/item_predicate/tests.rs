use super::*;
use foton_registry::item_predicate::LockCode;
use foton_registry::{ItemStackTemplate, init_vanilla_registry, vanilla_items};
use foton_utils::nbt::parse_snbt;
use simdnbt::FromNbtTag;
use simdnbt::borrow::read_tag;
use std::io::Cursor;

fn predicate(snbt: &str) -> RegisteredItemPredicate {
    init_vanilla_registry();
    let mut bytes = Vec::new();
    parse_snbt(snbt).expect("predicate SNBT").write(&mut bytes);
    let tag = read_tag(&mut Cursor::new(bytes.as_slice())).expect("predicate NBT");
    LockCode::from_nbt_tag(tag.as_tag())
        .expect("typed predicate")
        .predicate()
        .clone()
}

#[test]
fn exact_components_preserve_types_nested_values_and_unconstrained_components() {
    let predicate = predicate(
        "{items:'minecraft:tripwire_hook',components:{'minecraft:custom_data':{key:1,nested:{value:2},array:[I;1,2]}}}",
    );
    let mut key = ItemStack::new(&vanilla_items::TRIPWIRE_HOOK);
    let data = |snbt| {
        vanilla_components::CustomData::from_nbt_value(&parse_snbt(snbt).expect("custom data"))
            .expect("typed custom data")
    };
    key.set(
        vanilla_components::CUSTOM_DATA,
        data("{key:1,nested:{value:2},array:[I;1,2]}"),
    );
    assert!(matches(&predicate, &key));
    key.set(
        vanilla_components::CUSTOM_DATA,
        data("{array:[I;1,2],nested:{value:2},key:1}"),
    );
    assert!(
        matches(&predicate, &key),
        "compound insertion order is irrelevant"
    );
    key.set(vanilla_components::ENCHANTMENT_GLINT_OVERRIDE, true);
    key.set_count(17);
    assert!(matches(&predicate, &key));
    for changed in [
        "{key:1b,nested:{value:2},array:[I;1,2]}",
        "{key:1,nested:{value:3},array:[I;1,2]}",
        "{key:1,nested:{value:2},array:[I;2,1]}",
        "{key:1,nested:{value:2},array:[I;1,2],extra:1}",
    ] {
        key.set(vanilla_components::CUSTOM_DATA, data(changed));
        assert!(!matches(&predicate, &key), "{changed}");
    }
}

#[test]
fn tags_count_bounds_empty_air_and_effective_component_values() {
    let logs = predicate("{items:'#minecraft:logs',count:{min:2,max:3}}");
    assert!(matches(
        &logs,
        &ItemStack::with_count(&vanilla_items::OAK_LOG, 2)
    ));
    assert!(!matches(
        &logs,
        &ItemStack::with_count(&vanilla_items::OAK_LOG, 4)
    ));
    assert!(!matches(
        &logs,
        &ItemStack::with_count(&vanilla_items::STONE, 2)
    ));
    assert!(matches(
        &predicate("{items:'minecraft:air'}"),
        &ItemStack::empty()
    ));
    assert!(matches(
        &RegisteredItemPredicate::any(),
        &ItemStack::empty()
    ));
    let mut sword = ItemStack::new(&vanilla_items::DIAMOND_SWORD);
    let exact = predicate("{items:'minecraft:diamond_sword',components:{'minecraft:damage':0}}");
    assert!(matches(&exact, &sword));
    sword.remove(vanilla_components::DAMAGE);
    assert!(!matches(&exact, &sword));
    assert!(matches(
        &predicate("{items:'minecraft:diamond_sword'}"),
        &sword
    ));
}

#[test]
fn recursive_collection_predicates_use_templates_without_losing_partial_semantics() {
    let predicate = predicate(
        "{items:'minecraft:chest',predicates:{'minecraft:container':{items:{contains:[{items:'minecraft:stick',predicates:{'minecraft:custom_data':{nested:{key:2}}}}],count:[{test:{items:'minecraft:stick',count:{min:2}},count:1}],size:1}}}}",
    );
    let mut nested = ItemStack::with_count(&vanilla_items::STICK, 2);
    nested.set(
        vanilla_components::CUSTOM_DATA,
        vanilla_components::CustomData::from_nbt_value(
            &parse_snbt("{nested:{key:2,extra:3}}").expect("data"),
        )
        .expect("custom data"),
    );
    let mut chest = ItemStack::new(&vanilla_items::CHEST);
    chest.set(
        vanilla_components::CONTAINER,
        vanilla_components::ItemContainerContents::new(vec![
            None,
            Some(ItemStackTemplate::from_stack(&nested).expect("template")),
        ])
        .expect("contents"),
    );
    assert!(matches(&predicate, &chest));
    assert!(matches_view(
        &predicate,
        &ItemStackTemplate::from_stack(&chest).expect("chest template")
    ));
    nested.set_count(1);
    chest.set(
        vanilla_components::CONTAINER,
        vanilla_components::ItemContainerContents::new(vec![Some(
            ItemStackTemplate::from_stack(&nested).expect("template"),
        )])
        .expect("contents"),
    );
    assert!(!matches(&predicate, &chest));
}
