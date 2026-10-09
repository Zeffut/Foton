//! Graph-shape checks for `/attribute`, `/effect`, `/data` and `/fill`.
//!
//! These trees are assembled by hand from vanilla's builders, so the
//! regression worth catching is a branch that lost its executor or its place.

use foton_registry::init_vanilla_registry;

use super::create_dispatcher;
use crate::command::{
    brigadier::{ArgumentType, CommandDispatcher, NodeId},
    execution::{CommandSource, FotonArgumentType, FotonCommandRuntime},
};

type Dispatcher = CommandDispatcher<CommandSource, FotonCommandRuntime>;

fn dispatcher() -> Dispatcher {
    init_vanilla_registry();
    let Ok(dispatcher) = create_dispatcher() else {
        panic!("built-in commands should register");
    };
    dispatcher
}

fn child(dispatcher: &Dispatcher, parent: NodeId, name: &str) -> NodeId {
    let Some(child) = dispatcher.children(parent).and_then(|children| {
        children.iter().copied().find(|child| {
            dispatcher
                .node(*child)
                .is_some_and(|node| node.name() == name)
        })
    }) else {
        panic!("`{name}` should exist below {parent:?}");
    };
    child
}

fn walk(dispatcher: &Dispatcher, path: &[&str]) -> NodeId {
    path.iter().fold(dispatcher.root(), |node, name| {
        child(dispatcher, node, name)
    })
}

fn names(dispatcher: &Dispatcher, node: NodeId) -> Vec<String> {
    dispatcher
        .children(node)
        .unwrap_or_default()
        .iter()
        .filter_map(|child| dispatcher.node(*child))
        .map(|node| node.name().to_owned())
        .collect()
}

#[expect(
    clippy::redundant_closure_for_method_calls,
    reason = "the node type is crate-private and cannot be named in this module"
)]
fn assert_executable(dispatcher: &Dispatcher, path: &[&str], expected: bool) {
    let node = walk(dispatcher, path);
    let executable = dispatcher
        .node(node)
        .is_some_and(|node| node.is_executable());
    assert_eq!(executable, expected, "{}", path.join(" "));
}

#[test]
fn fill_offers_every_mode_with_and_without_a_filter() {
    let dispatcher = dispatcher();
    let block = walk(&dispatcher, &["fill", "from", "to", "block"]);
    assert_eq!(
        names(&dispatcher, block),
        ["outline", "hollow", "destroy", "strict", "replace", "keep"]
    );

    let prefix = ["fill", "from", "to", "block"];
    for tail in [
        &[][..],
        &["outline"],
        &["hollow"],
        &["destroy"],
        &["strict"],
        &["replace"],
        &["keep"],
        &["replace", "filter"],
        &["replace", "filter", "outline"],
        &["replace", "filter", "hollow"],
        &["replace", "filter", "destroy"],
        &["replace", "filter", "strict"],
    ] {
        let path = [&prefix[..], tail].concat();
        assert_executable(&dispatcher, &path, true);
    }
    // `keep` takes no filter of its own.
    assert!(
        names(
            &dispatcher,
            walk(&dispatcher, &["fill", "from", "to", "block", "keep"])
        )
        .is_empty()
    );
}

#[test]
fn fill_filter_is_a_predicate_and_the_block_a_state() {
    let dispatcher = dispatcher();
    let argument_type = |path: &[&str]| {
        dispatcher
            .node(walk(&dispatcher, path))
            .and_then(|node| node.argument_type().cloned())
    };
    assert_eq!(
        argument_type(&["fill", "from", "to", "block"]),
        Some(FotonArgumentType::block_state())
    );
    assert_eq!(
        argument_type(&["fill", "from", "to", "block", "replace", "filter"]),
        Some(FotonArgumentType::block_predicate())
    );
}

#[test]
fn effect_give_matches_the_vanilla_argument_chain() {
    let dispatcher = dispatcher();
    for path in [
        &["effect", "give", "targets", "effect"][..],
        &["effect", "give", "targets", "effect", "seconds"],
        &[
            "effect",
            "give",
            "targets",
            "effect",
            "seconds",
            "amplifier",
        ],
        &[
            "effect",
            "give",
            "targets",
            "effect",
            "seconds",
            "amplifier",
            "hideParticles",
        ],
        &["effect", "give", "targets", "effect", "infinite"],
        &[
            "effect",
            "give",
            "targets",
            "effect",
            "infinite",
            "amplifier",
        ],
        &[
            "effect",
            "give",
            "targets",
            "effect",
            "infinite",
            "amplifier",
            "hideParticles",
        ],
        &["effect", "clear"],
        &["effect", "clear", "targets"],
        &["effect", "clear", "targets", "effect"],
    ] {
        assert_executable(&dispatcher, path, true);
    }
    assert_executable(&dispatcher, &["effect", "give"], false);

    let bounds = |path: &[&str]| {
        dispatcher
            .node(walk(&dispatcher, path))
            .and_then(|node| node.argument_type().cloned())
    };
    assert_eq!(
        bounds(&["effect", "give", "targets", "effect", "seconds"]),
        Some(FotonArgumentType::from(ArgumentType::integer(1, 1_000_000)))
    );
    assert_eq!(
        bounds(&[
            "effect",
            "give",
            "targets",
            "effect",
            "infinite",
            "amplifier"
        ]),
        Some(FotonArgumentType::from(ArgumentType::integer(0, 255)))
    );
}

#[test]
fn attribute_exposes_value_base_and_modifier_branches() {
    let dispatcher = dispatcher();
    let attribute = ["attribute", "target", "attribute"];
    assert_eq!(
        names(&dispatcher, walk(&dispatcher, &attribute)),
        ["get", "base", "modifier"]
    );
    for tail in [
        &["get"][..],
        &["get", "scale"],
        &["base", "get"],
        &["base", "get", "scale"],
        &["base", "set", "value"],
        &["base", "reset"],
        &["modifier", "add", "id", "value", "add_value"],
        &["modifier", "add", "id", "value", "add_multiplied_base"],
        &["modifier", "add", "id", "value", "add_multiplied_total"],
        &["modifier", "remove", "id"],
        &["modifier", "value", "get", "id"],
        &["modifier", "value", "get", "id", "scale"],
    ] {
        assert_executable(&dispatcher, &[&attribute[..], tail].concat(), true);
    }
    assert_executable(&dispatcher, &["attribute", "target", "attribute"], false);
}

#[test]
fn data_get_merge_and_remove_exist_for_every_provider() {
    let dispatcher = dispatcher();
    for (provider, argument) in [
        ("entity", "target"),
        ("block", "targetPos"),
        ("storage", "target"),
    ] {
        assert_executable(&dispatcher, &["data", "get", provider, argument], true);
        assert_executable(
            &dispatcher,
            &["data", "get", provider, argument, "path"],
            true,
        );
        assert_executable(
            &dispatcher,
            &["data", "get", provider, argument, "path", "scale"],
            true,
        );
        assert_executable(
            &dispatcher,
            &["data", "merge", provider, argument, "nbt"],
            true,
        );
        assert_executable(
            &dispatcher,
            &["data", "remove", provider, argument, "path"],
            true,
        );
    }
    assert_eq!(
        names(&dispatcher, walk(&dispatcher, &["data"])),
        ["merge", "get", "remove", "modify"]
    );
}

#[test]
fn data_modify_reaches_every_operation_source_and_provider() {
    let dispatcher = dispatcher();
    let operations: [(&[&str], &str); 5] = [
        (&["insert", "index"], "insert"),
        (&["prepend"], "prepend"),
        (&["append"], "append"),
        (&["set"], "set"),
        (&["merge"], "merge"),
    ];
    for (target, target_argument) in [
        ("entity", "target"),
        ("block", "targetPos"),
        ("storage", "target"),
    ] {
        let base = ["data", "modify", target, target_argument, "targetPath"];
        assert_eq!(
            names(&dispatcher, walk(&dispatcher, &base)),
            ["insert", "prepend", "append", "set", "merge"]
        );
        for (operation, _) in operations {
            let operation_path = [&base[..], operation].concat();
            assert_eq!(
                names(&dispatcher, walk(&dispatcher, &operation_path)),
                ["from", "string", "value"]
            );
            assert_executable(
                &dispatcher,
                &[&operation_path[..], &["value", "value"]].concat(),
                true,
            );
            for (source, source_argument) in [
                ("entity", "source"),
                ("block", "sourcePos"),
                ("storage", "source"),
            ] {
                for form in ["from", "string"] {
                    let from = [&operation_path[..], &[form, source, source_argument]].concat();
                    assert_executable(&dispatcher, &from, true);
                    assert_executable(&dispatcher, &[&from[..], &["sourcePath"]].concat(), true);
                }
                let range = [
                    &operation_path[..],
                    &["string", source, source_argument, "sourcePath"],
                ]
                .concat();
                assert_executable(&dispatcher, &[&range[..], &["start"]].concat(), true);
                assert_executable(&dispatcher, &[&range[..], &["start", "end"]].concat(), true);
            }
        }
    }
}
