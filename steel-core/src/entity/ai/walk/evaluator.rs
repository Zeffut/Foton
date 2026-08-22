//! The contract the A* search needs from whatever decides where a mob may go.

use crate::entity::ai::node::{Node, NodeStore};
use crate::entity::ai::path::PathfindingContext;

use super::collision::WalkNodeCollision;
use super::node_evaluator::Neighbors;

/// Decides which positions a mob can occupy and which it can step to.
///
/// Vanilla parity: `NodeEvaluator`. The A* search itself is the same whether a
/// mob walks, swims or flies; only this changes, which is why vanilla has one
/// `PathFinder` and several evaluators.
pub trait NodeEvaluator {
    /// Returns the node the search starts from.
    ///
    /// Vanilla parity: `NodeEvaluator.getStart`.
    fn get_start(&mut self, context: &mut PathfindingContext<'_>) -> i32;

    /// Returns the nodes reachable in one step from `pos_hash`.
    ///
    /// Vanilla parity: `NodeEvaluator.getNeighbors`.
    fn get_neighbors(
        &mut self,
        context: &mut PathfindingContext<'_>,
        collision: &mut dyn WalkNodeCollision,
        pos_hash: i32,
    ) -> Neighbors;

    /// Returns the node behind `hash`, if the evaluator has seen it.
    fn node(&self, hash: i32) -> Option<&Node>;

    /// Returns the node behind `hash` for the search to score.
    fn node_mut(&mut self, hash: i32) -> Option<&mut Node>;

    /// Returns the node store the open set orders.
    fn nodes_mut(&mut self) -> &mut NodeStore;

    /// Clears the per-search scoring left on every node.
    fn reset_search_state(&mut self);

    /// Drops every node, so the next search starts from nothing.
    ///
    /// Vanilla parity: `NodeEvaluator.done`.
    fn clear_nodes(&mut self);
}
