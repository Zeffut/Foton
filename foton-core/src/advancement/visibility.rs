//! Which advancements a player is allowed to see.
//!
//! Vanilla parity: `AdvancementVisibilityEvaluator`. The rule is asymmetric:
//! a done advancement reveals its ancestors all the way up, but only reveals
//! its descendants two levels down.

use super::tree::AdvancementTree;

/// What one node says about the advancements below it.
///
/// Vanilla parity: `AdvancementVisibilityEvaluator.VisibilityRule`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VisibilityRule {
    /// Reveal what is under here.
    Show,
    /// Hide what is under here, and stop the search upwards.
    Hide,
    /// Say nothing; keep looking further up.
    NoChange,
}

/// How far up the tree an unfinished advancement looks for a finished one.
///
/// Vanilla parity: `AdvancementVisibilityEvaluator.VISIBILITY_DEPTH`, which is
/// what makes the children and grandchildren of a completed advancement appear.
const VISIBILITY_DEPTH: usize = 2;

/// Works out what one tab should show, calling `output` for every node in it.
///
/// `is_done` answers whether a node's advancement is complete for this player.
///
/// Vanilla parity: `AdvancementVisibilityEvaluator.evaluateVisibility`.
pub fn evaluate(
    tree: &AdvancementTree,
    root: usize,
    is_done: &mut impl FnMut(usize) -> bool,
    output: &mut impl FnMut(usize, bool),
) {
    // Vanilla seeds the stack with three NoChange entries so peeking at the
    // parent and grandparent of the root never underflows.
    let mut ascendants = vec![VisibilityRule::NoChange; VISIBILITY_DEPTH + 1];
    walk(tree, root, &mut ascendants, is_done, output);
}

/// Returns whether this node or any node below it is done.
fn walk(
    tree: &AdvancementTree,
    node: usize,
    ascendants: &mut Vec<VisibilityRule>,
    is_done: &mut impl FnMut(usize) -> bool,
    output: &mut impl FnMut(usize, bool),
) -> bool {
    let self_done = is_done(node);
    let rule = rule_for(tree, node, self_done);
    let mut subtree_done = self_done;

    ascendants.push(rule);
    for index in 0..tree.node(node).children.len() {
        let child = tree.node(node).children[index];
        subtree_done |= walk(tree, child, ascendants, is_done, output);
    }
    let visible = subtree_done || revealed_from_above(ascendants);
    ascendants.pop();

    output(node, visible);
    subtree_done
}

/// Vanilla parity: `AdvancementVisibilityEvaluator.evaluateVisibilityRule`.
fn rule_for(tree: &AdvancementTree, node: usize, is_done: bool) -> VisibilityRule {
    let Some(display) = tree.node(node).advancement.display.as_ref() else {
        return VisibilityRule::Hide;
    };
    if is_done {
        return VisibilityRule::Show;
    }
    if display.hidden {
        VisibilityRule::Hide
    } else {
        VisibilityRule::NoChange
    }
}

/// Looks at this node, its parent and its grandparent, nearest first, and stops
/// at whichever speaks first.
///
/// Vanilla parity: `AdvancementVisibilityEvaluator.evaluateVisiblityForUnfinishedNode`.
/// A `Hide` in the way blocks a completed grandparent from revealing anything
/// past it, which is what keeps a hidden advancement hidden.
fn revealed_from_above(ascendants: &[VisibilityRule]) -> bool {
    for depth in 0..=VISIBILITY_DEPTH {
        match ascendants[ascendants.len() - 1 - depth] {
            VisibilityRule::Show => return true,
            VisibilityRule::Hide => return false,
            VisibilityRule::NoChange => {}
        }
    }
    false
}
