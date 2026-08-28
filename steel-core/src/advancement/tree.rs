//! The parent/child tree the advancement screen is drawn from.
//!
//! Vanilla parity: `AdvancementTree` and `AdvancementNode`. Steel's advancement
//! set is generated at build time and never reloaded, so the tree is built once
//! from the registry and shared; there is no `addAll`/`remove`/listener surface
//! for a datapack reload to drive.

use std::mem;
use std::sync::LazyLock;

use rustc_hash::FxHashMap;
use steel_registry::REGISTRY;
use steel_registry::advancement::AdvancementRef;
use steel_utils::Identifier;

/// One advancement and its place in the tree.
///
/// Vanilla parity: `AdvancementNode`.
pub struct AdvancementNode {
    /// The advancement itself.
    pub advancement: AdvancementRef,
    /// The node's parent, absent on a root.
    pub parent: Option<usize>,
    /// The root of this node's tab.
    ///
    /// Vanilla walks the parent chain in `AdvancementNode.root()` every time;
    /// the answer never changes here, so it is resolved once.
    pub root: usize,
    /// The nodes hanging off this one, in advancement key order.
    pub children: Vec<usize>,
}

/// Every advancement, wired into its tab.
pub struct AdvancementTree {
    nodes: Vec<AdvancementNode>,
    by_key: FxHashMap<Identifier, usize>,
    roots: Vec<usize>,
}

impl AdvancementTree {
    /// Builds the tree from the generated registry.
    ///
    /// Vanilla parity: `AdvancementTree.addAll`, which sweeps repeatedly until
    /// every parent has been inserted and drops whatever is left. Here the
    /// parents are guaranteed to exist -- `every_parent_link_resolves_to_a_registered_advancement`
    /// is the test that says so -- so one pass over a parent-first ordering does.
    fn build() -> Self {
        let mut nodes: Vec<AdvancementNode> = Vec::new();
        let mut by_key: FxHashMap<Identifier, usize> = FxHashMap::default();
        let mut roots = Vec::new();

        // Parents before children: the registry is in key order, which does not
        // guarantee that, so insertion sweeps until it stops making progress
        // exactly the way vanilla's does.
        let mut pending: Vec<AdvancementRef> = REGISTRY.advancements.iter().collect();
        while !pending.is_empty() {
            let before = pending.len();
            pending.retain(|advancement| {
                let parent = match advancement.parent.as_ref() {
                    None => None,
                    Some(parent) => match by_key.get(parent) {
                        Some(&index) => Some(index),
                        // The parent has not been inserted yet; try again next sweep.
                        None => return true,
                    },
                };

                let index = nodes.len();
                let root = parent.map_or(index, |parent| nodes[parent].root);
                nodes.push(AdvancementNode {
                    advancement,
                    parent,
                    root,
                    children: Vec::new(),
                });
                by_key.insert(advancement.key.clone(), index);
                match parent {
                    Some(parent) => nodes[parent].children.push(index),
                    None => roots.push(index),
                }
                false
            });

            assert!(
                pending.len() < before,
                "advancement tree stalled with {} entries whose parents never appeared",
                pending.len()
            );
        }

        // Vanilla's child order comes out of an identity hash set and is not
        // reproducible; sorting by key is, and it matches the order the build
        // script laid the tree out in.
        for index in 0..nodes.len() {
            let mut children = mem::take(&mut nodes[index].children);
            children.sort_by(|&left, &right| {
                nodes[left]
                    .advancement
                    .key
                    .to_string()
                    .cmp(&nodes[right].advancement.key.to_string())
            });
            nodes[index].children = children;
        }

        Self {
            nodes,
            by_key,
            roots,
        }
    }

    /// The node at `index`.
    #[must_use]
    pub fn node(&self, index: usize) -> &AdvancementNode {
        &self.nodes[index]
    }

    /// The node holding the advancement with this key.
    #[must_use]
    pub fn index_of(&self, key: &Identifier) -> Option<usize> {
        self.by_key.get(key).copied()
    }

    /// The advancement with this key.
    #[must_use]
    pub fn get(&self, key: &Identifier) -> Option<AdvancementRef> {
        self.index_of(key)
            .map(|index| self.nodes[index].advancement)
    }

    /// How many advancements the tree holds.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Whether the tree is empty.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Every node index, in insertion order.
    pub fn indices(&self) -> impl Iterator<Item = usize> + '_ {
        0..self.nodes.len()
    }

    /// The tab roots, in insertion order.
    #[must_use]
    pub fn roots(&self) -> &[usize] {
        &self.roots
    }
}

/// The one tree every player's progress is evaluated against.
///
/// Vanilla parity: `ServerAdvancementManager.tree()`. It is a `LazyLock` rather
/// than server state because the advancement set is compiled in.
pub static ADVANCEMENT_TREE: LazyLock<AdvancementTree> = LazyLock::new(AdvancementTree::build);
