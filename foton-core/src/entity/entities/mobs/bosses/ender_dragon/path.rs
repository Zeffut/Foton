//! The twenty-four flight nodes the dragon circles between.
//!
//! Vanilla parity: the `nodes`, `nodeAdjacency`, `openSet`, `findClosestNode`,
//! `findPath` and `reconstructPath` members of `EnderDragon`. The dragon does
//! not pathfind through the world -- it flies between twenty-four fixed points
//! arranged in three rings around the fight origin, and A*s over a hardcoded
//! adjacency bitmask between them.
//!
//! Nodes `0..12` are the outer ring at radius 60, `12..20` the middle ring at
//! radius 40, and `20..24` the inner ring at radius 20. While any crystal is
//! alive the dragon uses the outer ring; once they are all gone it is confined
//! to `12..24`, which is what brings it into range of the podium.

use std::f64::consts::PI;

use foton_math::{fast_floor, trig};
use foton_utils::BlockPos;

use crate::chunk::heightmap::HeightmapType;
use crate::entity::ai::node::{Node, NodeHeap, NodeStore};
use crate::entity::ai::path::Path;
use crate::world::World;

/// Flight nodes in the ring.
///
/// Vanilla parity: the `new Node[24]` of `EnderDragon`.
pub const NODE_COUNT: usize = 24;

/// First node of the inner two rings.
///
/// Vanilla parity: the `startIndex = 12` / `minimumNodeIndex = 12` that both
/// `findClosestNode` and `findPath` fall back to once the crystals are gone.
const INNER_RINGS_START: usize = 12;

/// Lowest a flight node may sit.
///
/// Vanilla parity: the `Math.max(73, ...)` of `findClosestNode`.
const MIN_NODE_Y: i32 = 73;

/// Which nodes each node may fly to, as a bitmask over node indices.
///
/// Vanilla parity: the `nodeAdjacency` assignments of `findClosestNode`.
const NODE_ADJACENCY: [i32; NODE_COUNT] = [
    6_146, 8_197, 8_202, 16_404, 32_808, 32_848, 65_696, 131_392, 131_712, 263_424, 526_848,
    525_313, 1_581_057, 3_166_214, 2_138_120, 6_373_424, 4_358_208, 12_910_976, 9_044_480,
    9_706_496, 15_216_640, 13_688_832, 11_763_712, 8_257_536,
];

/// The dragon's flight graph and its A* scratch space.
pub struct DragonPathfinder {
    /// Hash of each ring node, in ring order.
    ///
    /// Vanilla holds the `Node` objects themselves and compares them by
    /// identity. Foton's A* primitives are keyed by node hash against a
    /// [`NodeStore`], so the ring keeps the hashes and looks indices up
    /// through them -- the same linear scan vanilla does in `findPath`.
    hashes: Option<[i32; NODE_COUNT]>,
    store: NodeStore,
    open_set: NodeHeap,
}

impl Default for DragonPathfinder {
    fn default() -> Self {
        Self::new()
    }
}

impl DragonPathfinder {
    /// Creates an unbuilt flight graph.
    #[must_use]
    pub fn new() -> Self {
        Self {
            hashes: None,
            store: NodeStore::new(),
            open_set: NodeHeap::new(),
        }
    }

    /// Builds the twenty-four ring nodes if they do not exist yet.
    ///
    /// Vanilla parity: the `if (this.nodes[0] == null)` block of
    /// `findClosestNode`. The nodes are built once, against the heightmap as it
    /// stood the first time the dragon needed them.
    fn ensure_nodes(&mut self, world: &World) -> [i32; NODE_COUNT] {
        if let Some(hashes) = self.hashes {
            return hashes;
        }

        let mut hashes = [0_i32; NODE_COUNT];
        for (index, hash) in hashes.iter_mut().enumerate() {
            let (node_x, node_z, y_adjustment) = Self::ring_position(index);
            let surface = world
                .heightmap_pos(
                    HeightmapType::MotionBlockingNoLeaves,
                    BlockPos::new(node_x, 0, node_z),
                )
                .y();
            let node_y = MIN_NODE_Y.max(surface + y_adjustment);
            let node = self.store.get_node(node_x, node_y, node_z);
            *hash = node.hash();
        }

        self.hashes = Some(hashes);
        hashes
    }

    /// Returns a ring node's horizontal position and height offset.
    ///
    /// Vanilla parity: the three branches of the node-building loop, including
    /// the `yAdjustment += 10` that lifts the middle ring.
    fn ring_position(index: usize) -> (i32, i32, i32) {
        const BASE_Y_ADJUSTMENT: i32 = 5;
        const MIDDLE_RING_LIFT: i32 = 10;

        let (radius, step, multiplier, y_adjustment) = if index < INNER_RINGS_START {
            (60.0_f32, PI / 12.0, index, BASE_Y_ADJUSTMENT)
        } else if index < 20 {
            (
                40.0_f32,
                PI / 8.0,
                index - INNER_RINGS_START,
                BASE_Y_ADJUSTMENT + MIDDLE_RING_LIFT,
            )
        } else {
            (20.0_f32, PI / 4.0, index - 20, BASE_Y_ADJUSTMENT)
        };

        let angle = 2.0 * (-PI + step * multiplier as f64);
        let x = fast_floor(f64::from(radius * trig::cos(angle)));
        let z = fast_floor(f64::from(radius * trig::sin(angle)));
        (x, z, y_adjustment)
    }

    /// Returns the ring node nearest the dragon.
    ///
    /// Vanilla parity: the no-argument `EnderDragon.findClosestNode`.
    pub fn find_closest_node_to_self(
        &mut self,
        world: &World,
        position: glam::DVec3,
        alive_crystals: i32,
    ) -> usize {
        self.ensure_nodes(world);
        self.find_closest_node(world, position.x, position.y, position.z, alive_crystals)
    }

    /// Returns the ring node nearest a point.
    ///
    /// Vanilla parity: `EnderDragon.findClosestNode(double, double, double)`.
    pub fn find_closest_node(
        &mut self,
        world: &World,
        x: f64,
        y: f64,
        z: f64,
        alive_crystals: i32,
    ) -> usize {
        let hashes = self.ensure_nodes(world);
        let current = Node::new(fast_floor(x), fast_floor(y), fast_floor(z));

        let mut closest_dist = 10_000.0_f32;
        let mut closest_index = 0;
        let start_index = if alive_crystals == 0 {
            INNER_RINGS_START
        } else {
            0
        };

        for (index, hash) in hashes.iter().enumerate().skip(start_index) {
            let Some(node) = self.store.get(*hash) else {
                continue;
            };
            let dist = node.distance_to_sqr(&current);
            if dist < closest_dist {
                closest_dist = dist;
                closest_index = index;
            }
        }

        closest_index
    }

    /// A*s from one ring node to another.
    ///
    /// Vanilla parity: `EnderDragon.findPath`. `final_node` is the off-ring
    /// point some phases aim past the ring at -- a player, or the podium.
    pub fn find_path(
        &mut self,
        world: &World,
        start_index: usize,
        end_index: usize,
        final_node: Option<Node>,
        alive_crystals: i32,
    ) -> Option<Path> {
        let hashes = self.ensure_nodes(world);
        let (Some(start_hash), Some(end_hash)) = (
            hashes.get(start_index).copied(),
            hashes.get(end_index).copied(),
        ) else {
            return None;
        };

        self.reset_search_state(&hashes);

        let goal = self.store.get(end_hash)?.clone();
        {
            let from = self.store.get_mut(start_hash)?;
            from.g = 0.0;
            from.h = from.distance_to(&goal);
            from.f = from.h;
        }

        self.open_set.clear(&mut self.store);
        self.open_set.insert(&mut self.store, start_hash);

        let mut closest_hash = start_hash;
        let minimum_node_index = if alive_crystals == 0 {
            INNER_RINGS_START
        } else {
            0
        };

        while let Some(open_hash) = self.open_set.pop(&mut self.store) {
            if open_hash == end_hash {
                return Some(self.reconstruct_path(end_hash, final_node));
            }

            let (open_g, open_distance_to_goal) = {
                let open_node = self.store.get(open_hash)?;
                (open_node.g, open_node.distance_to(&goal))
            };
            let closest_distance = self
                .store
                .get(closest_hash)
                .map_or(f32::MAX, |node| node.distance_to(&goal));
            if open_distance_to_goal < closest_distance {
                closest_hash = open_hash;
            }

            if let Some(open_node) = self.store.get_mut(open_hash) {
                open_node.closed = true;
            }

            let open_index = hashes
                .iter()
                .position(|hash| *hash == open_hash)
                .unwrap_or(0);
            let adjacency = NODE_ADJACENCY[open_index];

            for (adjacent_index, adjacent_hash) in hashes
                .iter()
                .enumerate()
                .skip(minimum_node_index)
                .map(|(index, hash)| (index, *hash))
            {
                if adjacency & (1 << adjacent_index) == 0 {
                    continue;
                }

                let Some(adjacent) = self.store.get(adjacent_hash) else {
                    continue;
                };
                if adjacent.closed {
                    continue;
                }

                let in_open_set = adjacent.in_open_set();
                let tentative_g = open_g
                    + self
                        .store
                        .get(open_hash)
                        .map_or(0.0, |open_node| open_node.distance_to(adjacent));
                if in_open_set && tentative_g >= adjacent.g {
                    continue;
                }

                let heuristic = adjacent.distance_to(&goal);
                let Some(adjacent) = self.store.get_mut(adjacent_hash) else {
                    continue;
                };
                adjacent.came_from = Some(open_hash);
                adjacent.g = tentative_g;
                adjacent.h = heuristic;
                if in_open_set {
                    self.open_set.change_cost(
                        &mut self.store,
                        adjacent_hash,
                        tentative_g + heuristic,
                    );
                } else {
                    adjacent.f = tentative_g + heuristic;
                    self.open_set.insert(&mut self.store, adjacent_hash);
                }
            }
        }

        if closest_hash == start_hash {
            return None;
        }

        Some(self.reconstruct_path(closest_hash, final_node))
    }

    /// Clears the six A* fields on every ring node.
    ///
    /// Vanilla parity: the reset loop that opens `findPath`. Only the ring is
    /// reset; a previous search's off-ring final node is discarded with the
    /// path that held it.
    fn reset_search_state(&mut self, hashes: &[i32; NODE_COUNT]) {
        for hash in hashes {
            let Some(node) = self.store.get_mut(*hash) else {
                continue;
            };
            node.closed = false;
            node.f = 0.0;
            node.g = 0.0;
            node.h = 0.0;
            node.came_from = None;
            node.heap_idx = -1;
        }
    }

    /// Walks `came_from` back from `tail_hash` and appends any final node.
    ///
    /// Vanilla parity: `EnderDragon.reconstructPath`. Vanilla hangs the final
    /// node off the tail with `finalNode.cameFrom = to` and reconstructs from
    /// it; keeping it out of the store instead avoids an off-ring node whose
    /// coordinates happen to hash onto a ring node overwriting that node's
    /// search state.
    fn reconstruct_path(&self, tail_hash: i32, final_node: Option<Node>) -> Path {
        let mut nodes = Vec::new();
        let mut hash = tail_hash;
        while let Some(node) = self.store.get(hash) {
            let came_from = node.came_from;
            nodes.insert(0, node.clone());
            match came_from {
                Some(next) => hash = next,
                None => break,
            }
        }

        if let Some(final_node) = final_node {
            nodes.push(final_node);
        }

        let target = nodes
            .last()
            .map_or(BlockPos::new(0, 0, 0), Node::as_block_pos);
        Path::new(nodes, target, true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_three_rings_sit_at_the_radii_the_dragon_circles_at() {
        let radius_of = |index: usize| {
            let (x, z, _) = DragonPathfinder::ring_position(index);
            f64::from(x).hypot(f64::from(z))
        };

        assert!((radius_of(0) - 60.0).abs() <= 1.5);
        assert!((radius_of(12) - 40.0).abs() <= 1.5);
        assert!((radius_of(20) - 20.0).abs() <= 1.5);
    }

    #[test]
    fn the_outer_ring_never_names_itself_as_its_own_neighbour() {
        for (index, adjacency) in NODE_ADJACENCY.iter().enumerate() {
            assert_eq!(
                adjacency & (1 << index),
                0,
                "node {index} lists itself as adjacent"
            );
        }
    }

    #[test]
    fn adjacency_is_symmetric_so_the_dragon_can_always_fly_back() {
        for (index, adjacency) in NODE_ADJACENCY.iter().enumerate() {
            for (other, back) in NODE_ADJACENCY.iter().enumerate() {
                if adjacency & (1 << other) == 0 {
                    continue;
                }
                assert_ne!(
                    back & (1 << index),
                    0,
                    "node {index} can reach {other} but not the reverse"
                );
            }
        }
    }
}
