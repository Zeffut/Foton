use super::*;
use crate::command::execution::ItemModifierArgument;

fn parse_error(argument_type: FotonArgumentType, input: &str) -> bool {
    let dispatcher = resource_dispatcher(argument_type);
    let command = format!("resource {input}");
    let parse = dispatcher.parse(&command, TestSource::new());
    dispatcher.context_chain(parse).is_err()
}

/// `/item` writes one slot, so a range such as `weapon.*` is an error of its
/// own, and a name no range has is another.
#[test]
fn slot_argument_takes_exactly_one_slot() {
    init_vanilla_registry();
    let dispatcher = resource_dispatcher(FotonArgumentType::slot());
    let parse = dispatcher.parse("resource weapon.offhand", TestSource::new());
    let Ok(chain) = dispatcher.context_chain(parse) else {
        panic!("a single slot should parse");
    };
    assert_eq!(chain.top_context().slot("value"), Ok(99));

    assert!(parse_error(FotonArgumentType::slot(), "weapon.*"));
    assert!(parse_error(FotonArgumentType::slot(), "container.54"));
    assert!(!parse_error(FotonArgumentType::slot(), "container.53"));
}

/// A recipe key is read as an identifier; whether the recipe exists is decided
/// when the command runs, as `ResourceKeyArgument.getRecipe` does.
#[test]
fn recipe_argument_accepts_a_key_nobody_has_registered_yet() {
    init_vanilla_registry();
    let dispatcher = resource_dispatcher(FotonArgumentType::recipe());
    let parse = dispatcher.parse("resource orbital:nuke_1_old", TestSource::new());
    let Ok(chain) = dispatcher.context_chain(parse) else {
        panic!("an unregistered recipe key should still parse");
    };
    assert_eq!(
        chain.top_context().recipe_key("value"),
        Ok(&Identifier::new(
            "orbital".to_owned(),
            "nuke_1_old".to_owned()
        ))
    );
    assert!(parse_error(FotonArgumentType::recipe(), "Not A Key"));
}

/// An id is a reference, `{` or `[` starts an inline modifier, and an inline
/// one that cannot be run is a parse error rather than a command that does
/// nothing.
#[test]
fn item_modifier_argument_reads_an_id_or_an_inline_value() {
    init_vanilla_registry();
    let dispatcher = resource_dispatcher(FotonArgumentType::item_modifier());

    let parse = dispatcher.parse("resource orbital:gold", TestSource::new());
    let Ok(chain) = dispatcher.context_chain(parse) else {
        panic!("a modifier id should parse");
    };
    assert!(matches!(
        chain.top_context().item_modifier("value"),
        Ok(ItemModifierArgument::Reference(_))
    ));

    let parse = dispatcher.parse(
        r#"resource {function:"minecraft:set_count",count:3}"#,
        TestSource::new(),
    );
    let Ok(chain) = dispatcher.context_chain(parse) else {
        panic!("an inline modifier should parse");
    };
    assert!(matches!(
        chain.top_context().item_modifier("value"),
        Ok(ItemModifierArgument::Inline(_))
    ));

    assert!(parse_error(
        FotonArgumentType::item_modifier(),
        r#"[{function:"minecraft:nothing"}]"#
    ));
    assert!(parse_error(
        FotonArgumentType::item_modifier(),
        r#"{function:"minecraft:set_count"}"#
    ));
}
