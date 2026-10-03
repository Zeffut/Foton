//! Borrowed source traversal before recursive semantic cloning.

mod components;
mod predicates;
mod text;

use foton_registry::ItemStackTemplate;
use foton_registry::data_component_predicate::{DataComponentMatchers, DataComponentPredicateData};
use foton_registry::data_components::{ComponentData, ComponentPatchEntry, DataComponentPatch};
use foton_registry::item_stack::ItemStack;
use foton_registry::mob_effect_instance::MobEffectInstanceDetails;
use simdnbt::owned::{NbtCompound, NbtList, NbtTag};
use text_components::TextComponent;

use super::ItemBridgeError;

/// Operational clone admission, independent of Minecraft's codec depth limits.
pub(super) const MAX_DEPTH: usize = 128;
/// Temporary item-only restriction, matching the tested Java-thread clone shape.
pub(super) const MAX_TEMPLATE_DEPTH: usize = 4;

enum Node<'a> {
    Patch(&'a DataComponentPatch),
    Component(&'a ComponentData),
    Template(&'a ItemStackTemplate),
    Text(&'a TextComponent),
    Compound(&'a NbtCompound),
    Tag(&'a NbtTag),
    List(&'a NbtList),
    Matchers(&'a DataComponentMatchers),
    Predicate(&'a DataComponentPredicateData),
    Effect(&'a MobEffectInstanceDetails),
}

struct Walker<'a> {
    pending: Vec<(Node<'a>, usize, usize)>,
    depth: usize,
    template_depth: usize,
    java_strings: bool,
}

impl<'a> Walker<'a> {
    fn push(&mut self, node: Node<'a>) {
        self.pending
            .push((node, self.depth + 1, self.template_depth));
    }

    fn run(&mut self) -> Result<(), ItemBridgeError> {
        while let Some((node, depth, template_depth)) = self.pending.pop() {
            if depth > MAX_DEPTH {
                return Err(ItemBridgeError::Depth(MAX_DEPTH));
            }
            self.depth = depth;
            self.template_depth = template_depth + usize::from(matches!(node, Node::Template(_)));
            if self.template_depth > MAX_TEMPLATE_DEPTH {
                return Err(ItemBridgeError::TemplateDepth(MAX_TEMPLATE_DEPTH));
            }
            match node {
                Node::Patch(patch) => {
                    for (_, entry) in patch.iter() {
                        if let ComponentPatchEntry::Set(value) = entry {
                            self.push(Node::Component(value));
                        }
                    }
                }
                Node::Component(value) => self.component(value),
                Node::Template(value) => self.push(Node::Patch(value.components())),
                Node::Text(value) => self.text(value),
                Node::Compound(value) => {
                    for (key, tag) in value.iter() {
                        if self.java_strings {
                            check_string(key)?;
                        }
                        self.push(Node::Tag(tag));
                    }
                }
                Node::Tag(NbtTag::Compound(value)) => self.push(Node::Compound(value)),
                Node::Tag(NbtTag::List(value)) => self.push(Node::List(value)),
                Node::Tag(NbtTag::String(value)) if self.java_strings => check_string(value)?,
                Node::Tag(_) => {}
                Node::List(NbtList::Compound(values)) => {
                    for value in values {
                        self.push(Node::Compound(value));
                    }
                }
                Node::List(NbtList::List(values)) => {
                    for value in values {
                        self.push(Node::List(value));
                    }
                }
                Node::List(NbtList::String(values)) if self.java_strings => {
                    for value in values {
                        check_string(value)?;
                    }
                }
                Node::List(_) => {}
                Node::Matchers(value) => {
                    for (_, component) in value.exact().values() {
                        self.push(Node::Component(component));
                    }
                    for predicate in value.partial() {
                        self.push(Node::Predicate(predicate));
                    }
                }
                Node::Predicate(value) => self.predicate(value),
                Node::Effect(value) => {
                    if let Some(hidden) = value.hidden_effect() {
                        self.push(Node::Effect(hidden));
                    }
                }
            }
        }
        Ok(())
    }
}

pub(super) fn check(stack: &ItemStack) -> Result<(), ItemBridgeError> {
    Walker {
        pending: vec![(Node::Patch(stack.components_patch()), 0, 0)],
        depth: 0,
        template_depth: 0,
        java_strings: false,
    }
    .run()
}

pub(super) fn check_component(value: &ComponentData) -> Result<(), ItemBridgeError> {
    // A SET value occupies the same level as a component in the root patch.
    Walker {
        pending: vec![(Node::Component(value), 1, 0)],
        depth: 0,
        template_depth: 0,
        java_strings: false,
    }
    .run()
}

fn check_string(value: &simdnbt::Mutf8Str) -> Result<(), ItemBridgeError> {
    if value.len() > usize::from(u16::MAX) {
        return Err(ItemBridgeError::TransportLimit);
    }
    // simdnbt returns empty on decoding failure; never expose that as the native value.
    if !value.is_empty() && value.to_str().is_empty() {
        return Err(ItemBridgeError::Unsupported(
            "NBT modified UTF-8 materialization",
        ));
    }
    Ok(())
}

pub(super) fn check_java_nbt(value: &NbtCompound) -> Result<(), ItemBridgeError> {
    Walker {
        pending: vec![(Node::Compound(value), 0, 0)],
        depth: 0,
        template_depth: 0,
        java_strings: true,
    }
    .run()
}
