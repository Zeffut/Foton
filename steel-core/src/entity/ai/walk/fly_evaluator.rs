//! Node evaluation for mobs that fly.
//!
//! Vanilla parity: `FlyNodeEvaluator`. It extends the walking evaluator rather
//! than replacing it: the mob's bounding box is classified the same way, but
//! open air counts as somewhere to be rather than somewhere to fall through,
//! and a step may go up or down as freely as sideways. That is the whole
//! difference between a parrot and a chicken.

use rustc_hash::FxHashMap;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;

use crate::fluid::FluidStateExt as _;
use steel_utils::{BlockPos, PackedBlockPos};

use super::collision::WalkNodeCollision;
use super::evaluator::NodeEvaluator;
use super::node_evaluator::{Neighbors, PathTypeMode, WalkNodeEvaluator};
use super::path_evaluator::WalkPathEvaluator;
use super::settings::MobPathSettings;
use crate::entity::ai::node::{Node, NodeStore};
use crate::entity::ai::path::{PathType, PathfindingContext};

/// Extra cost of a node a flier could land on.
///
/// Vanilla parity: the `best.costMalus++` of `FlyNodeEvaluator.findAcceptedNode`
/// when the node is walkable, which nudges a bird toward open air rather than
/// scraping along the ground.
const WALKABLE_NODE_MALUS: f32 = 1.0;

/// How many random start positions a small mob tries.
///
/// Vanilla parity: `FlyNodeEvaluator.MAX_START_NODE_CANDIDATES`.
const MAX_START_NODE_CANDIDATES: i32 = 10;

/// Size below which a mob counts as small for the start search.
///
/// Vanilla parity: `FlyNodeEvaluator.SMALL_MOB_SIZE`.
const SMALL_MOB_SIZE: f64 = 1.0;

/// Box a small mob's start candidates are drawn from.
///
/// Vanilla parity: `FlyNodeEvaluator.SMALL_MOB_INFLATED_START_NODE_BOUNDING_BOX`.
const SMALL_MOB_INFLATED_START_BOX: f64 = 1.1;

/// Classifies one block the way a flying mob sees it.
///
/// Vanilla parity: `FlyNodeEvaluator.getPathType`. Open air one block above
/// anything that is neither air, water nor walkable becomes `WALKABLE`, which
/// is what makes a bird treat a rooftop as a destination.
#[must_use]
pub(super) fn fly_path_type(
    context: &mut PathfindingContext<'_>,
    x: i32,
    y: i32,
    z: i32,
) -> PathType {
    let mut block_path_type = context.get_path_type_from_state(x, y, z);
    if block_path_type == PathType::Open && y > context.level().min_y() {
        let below_pos = BlockPos::new(x, y - 1, z);
        block_path_type = match context.get_path_type_from_state(x, y - 1, z) {
            PathType::Fire | PathType::Lava => PathType::Fire,
            PathType::Damaging => PathType::Damaging,
            PathType::Cocoa => PathType::Cocoa,
            PathType::Fence => {
                if below_pos == context.mob_position() {
                    PathType::Open
                } else {
                    PathType::Fence
                }
            }
            PathType::Walkable | PathType::Open | PathType::Water => PathType::Open,
            _ => PathType::Walkable,
        };
    }

    if matches!(block_path_type, PathType::Walkable | PathType::Open) {
        block_path_type =
            WalkPathEvaluator::check_neighbour_blocks(context, x, y, z, block_path_type);
    }

    block_path_type
}

/// Decides where a flying mob may go.
///
/// Vanilla parity: `FlyNodeEvaluator`.
#[derive(Debug, Clone)]
pub struct FlyNodeEvaluator {
    /// The walking evaluator this one extends.
    ///
    /// Vanilla parity: the superclass. It owns the node store, the mob settings
    /// and the bounding-box classification; only the neighbor search, the start
    /// node and the per-block path type differ, and the last of those is a mode
    /// on the walker rather than a second copy of the scan.
    walk: WalkNodeEvaluator,
    /// Path types already computed this search.
    ///
    /// Vanilla parity: `FlyNodeEvaluator.pathTypeByPosCache`. The neighbor
    /// search asks about the same twenty-six positions from several directions.
    path_type_cache: FxHashMap<i64, PathType>,
}

impl FlyNodeEvaluator {
    /// Creates the evaluator for one flying mob.
    #[must_use]
    pub fn new(settings: MobPathSettings) -> Self {
        Self {
            walk: WalkNodeEvaluator::new(settings).with_path_type_mode(PathTypeMode::Fly),
            path_type_cache: FxHashMap::default(),
        }
    }

    /// Vanilla parity: `FlyNodeEvaluator.getCachedPathType`.
    fn cached_path_type(
        &mut self,
        context: &mut PathfindingContext<'_>,
        x: i32,
        y: i32,
        z: i32,
    ) -> PathType {
        let key = PackedBlockPos::from(BlockPos::new(x, y, z)).as_raw();
        if let Some(path_type) = self.path_type_cache.get(&key) {
            return *path_type;
        }
        let path_type = self.walk.get_path_type_of_mob(context, x, y, z);
        self.path_type_cache.insert(key, path_type);
        path_type
    }

    /// Vanilla parity: `FlyNodeEvaluator.findAcceptedNode`.
    fn find_accepted_node(
        &mut self,
        context: &mut PathfindingContext<'_>,
        x: i32,
        y: i32,
        z: i32,
    ) -> Option<i32> {
        let path_type = self.cached_path_type(context, x, y, z);
        let path_cost = self.walk.settings().pathfinding_malus(path_type);
        if path_cost < 0.0 {
            return None;
        }

        let node = self.walk.nodes_mut().get_node(x, y, z);
        node.path_type = path_type;
        node.cost_malus = node.cost_malus.max(path_cost);
        if path_type == PathType::Walkable {
            node.cost_malus += WALKABLE_NODE_MALUS;
        }
        Some(node.hash())
    }

    /// Vanilla parity: `FlyNodeEvaluator.isOpen`.
    fn is_open(&self, hash: Option<i32>) -> bool {
        hash.and_then(|hash| self.walk.node(hash))
            .is_some_and(|node| !node.closed)
    }

    /// Vanilla parity: `FlyNodeEvaluator.hasMalus`.
    fn has_malus(&self, hash: Option<i32>) -> bool {
        hash.and_then(|hash| self.walk.node(hash))
            .is_some_and(|node| node.cost_malus >= 0.0)
    }

    /// Vanilla parity: `FlyNodeEvaluator.canStartAt`.
    ///
    /// Unlike the walking check this accepts an open node, because open air is
    /// exactly where a flier starts.
    fn can_start_at(&mut self, context: &mut PathfindingContext<'_>, pos: BlockPos) -> bool {
        let path_type = self.cached_path_type(context, pos.x(), pos.y(), pos.z());
        self.walk.settings().pathfinding_malus(path_type) >= 0.0
    }

    /// Vanilla parity: `FlyNodeEvaluator.iteratePathfindingStartNodeCandidatePositions`.
    fn start_node_candidates(&self) -> Vec<BlockPos> {
        let settings = self.walk.settings();
        let bounding_box = settings.bounding_box();
        let mob_y = settings.mob_position().y();

        if bounding_box.size() >= SMALL_MOB_SIZE {
            return vec![
                BlockPos::containing(bounding_box.min_x(), f64::from(mob_y), bounding_box.min_z()),
                BlockPos::containing(bounding_box.min_x(), f64::from(mob_y), bounding_box.max_z()),
                BlockPos::containing(bounding_box.max_x(), f64::from(mob_y), bounding_box.min_z()),
                BlockPos::containing(bounding_box.max_x(), f64::from(mob_y), bounding_box.max_z()),
            ];
        }

        let x_padding = (SMALL_MOB_INFLATED_START_BOX - bounding_box.width()).max(0.0);
        let y_padding = (SMALL_MOB_INFLATED_START_BOX - bounding_box.height()).max(0.0);
        let z_padding = (SMALL_MOB_INFLATED_START_BOX - bounding_box.depth()).max(0.0);
        let inflated = bounding_box.inflate_xyz(x_padding, y_padding, z_padding);

        let min = BlockPos::containing(inflated.min_x(), inflated.min_y(), inflated.min_z());
        let max = BlockPos::containing(inflated.max_x(), inflated.max_y(), inflated.max_z());
        (0..MAX_START_NODE_CANDIDATES)
            .map(|_| {
                BlockPos::new(
                    rand::random_range(min.x()..=max.x()),
                    rand::random_range(min.y()..=max.y()),
                    rand::random_range(min.z()..=max.z()),
                )
            })
            .collect()
    }
}

impl NodeEvaluator for FlyNodeEvaluator {
    /// Vanilla parity: `FlyNodeEvaluator.getStart`.
    fn get_start(&mut self, context: &mut PathfindingContext<'_>) -> i32 {
        let settings = self.walk.settings();
        let position = settings.mob_position_vec();
        let mut start_y = if settings.can_float() && settings.in_water() {
            let mut surface = settings.mob_position().y();
            while context
                .get_block_state(BlockPos::containing(
                    position.x,
                    f64::from(surface),
                    position.z,
                ))
                .get_fluid_state()
                .is_water()
            {
                surface += 1;
            }
            surface
        } else {
            steel_math::fast_floor(position.y + 0.5)
        };
        if start_y < context.level().min_y() {
            start_y = context.level().min_y();
        }

        let start_pos = BlockPos::containing(position.x, f64::from(start_y), position.z);
        if !self.can_start_at(context, start_pos) {
            for candidate in self.start_node_candidates() {
                if self.can_start_at(context, candidate) {
                    return self.walk.get_start_node(context, candidate);
                }
            }
        }

        self.walk.get_start_node(context, start_pos)
    }

    /// Vanilla parity: `FlyNodeEvaluator.getNeighbors`: the six faces, then the
    /// twelve edge diagonals, then the eight corners, each gated on every
    /// straight step it is built from.
    #[expect(
        clippy::too_many_lines,
        reason = "vanilla lists all twenty-six steps and their gates by hand; \
                  compressing them hides which gate belongs to which corner"
    )]
    fn get_neighbors(
        &mut self,
        context: &mut PathfindingContext<'_>,
        _collision: &mut dyn WalkNodeCollision,
        pos_hash: i32,
    ) -> Neighbors {
        let mut neighbors = Neighbors::new();
        let Some(node) = self.walk.node(pos_hash) else {
            return neighbors;
        };
        let (x, y, z) = (node.x, node.y, node.z);

        let south = self.find_accepted_node(context, x, y, z + 1);
        let west = self.find_accepted_node(context, x - 1, y, z);
        let east = self.find_accepted_node(context, x + 1, y, z);
        let north = self.find_accepted_node(context, x, y, z - 1);
        let up = self.find_accepted_node(context, x, y + 1, z);
        let down = self.find_accepted_node(context, x, y - 1, z);
        for face in [south, west, east, north, up, down] {
            if self.is_open(face) {
                neighbors.push(face.unwrap_or_default());
            }
        }

        let south_up = self.find_accepted_node(context, x, y + 1, z + 1);
        let west_up = self.find_accepted_node(context, x - 1, y + 1, z);
        let east_up = self.find_accepted_node(context, x + 1, y + 1, z);
        let north_up = self.find_accepted_node(context, x, y + 1, z - 1);
        let south_down = self.find_accepted_node(context, x, y - 1, z + 1);
        let west_down = self.find_accepted_node(context, x - 1, y - 1, z);
        let east_down = self.find_accepted_node(context, x + 1, y - 1, z);
        let north_down = self.find_accepted_node(context, x, y - 1, z - 1);
        let north_east = self.find_accepted_node(context, x + 1, y, z - 1);
        let south_east = self.find_accepted_node(context, x + 1, y, z + 1);
        let north_west = self.find_accepted_node(context, x - 1, y, z - 1);
        let south_west = self.find_accepted_node(context, x - 1, y, z + 1);

        for (edge, gates) in [
            (south_up, [south, up]),
            (west_up, [west, up]),
            (east_up, [east, up]),
            (north_up, [north, up]),
            (south_down, [south, down]),
            (west_down, [west, down]),
            (east_down, [east, down]),
            (north_down, [north, down]),
            (north_east, [north, east]),
            (south_east, [south, east]),
            (north_west, [north, west]),
            (south_west, [south, west]),
        ] {
            if self.is_open(edge) && gates.iter().all(|gate| self.has_malus(*gate)) {
                neighbors.push(edge.unwrap_or_default());
            }
        }

        for (corner_x, corner_y, corner_z, gates) in [
            (
                x + 1,
                y + 1,
                z - 1,
                [north_east, north, east, up, north_up, east_up],
            ),
            (
                x + 1,
                y + 1,
                z + 1,
                [south_east, south, east, up, south_up, east_up],
            ),
            (
                x - 1,
                y + 1,
                z - 1,
                [north_west, north, west, up, north_up, west_up],
            ),
            (
                x - 1,
                y + 1,
                z + 1,
                [south_west, south, west, up, south_up, west_up],
            ),
            (
                x + 1,
                y - 1,
                z - 1,
                [north_east, north, east, down, north_down, east_down],
            ),
            (
                x + 1,
                y - 1,
                z + 1,
                [south_east, south, east, down, south_down, east_down],
            ),
            (
                x - 1,
                y - 1,
                z - 1,
                [north_west, north, west, down, north_down, west_down],
            ),
            (
                x - 1,
                y - 1,
                z + 1,
                [south_west, south, west, down, south_down, west_down],
            ),
        ] {
            let corner = self.find_accepted_node(context, corner_x, corner_y, corner_z);
            if self.is_open(corner) && gates.iter().all(|gate| self.has_malus(*gate)) {
                neighbors.push(corner.unwrap_or_default());
            }
        }

        neighbors
    }

    fn node(&self, hash: i32) -> Option<&Node> {
        self.walk.node(hash)
    }

    fn node_mut(&mut self, hash: i32) -> Option<&mut Node> {
        self.walk.node_mut(hash)
    }

    fn nodes_mut(&mut self) -> &mut NodeStore {
        self.walk.nodes_mut()
    }

    fn reset_search_state(&mut self) {
        self.walk.reset_search_state();
    }

    /// Vanilla parity: `FlyNodeEvaluator.done`, which drops the path-type cache
    /// along with the nodes.
    fn clear_nodes(&mut self) {
        self.walk.clear_nodes();
        self.path_type_cache.clear();
    }
}
