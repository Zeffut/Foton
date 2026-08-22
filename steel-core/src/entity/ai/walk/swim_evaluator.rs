//! Node evaluation for mobs that swim.
//!
//! Vanilla parity: `SwimNodeEvaluator`. Where the walking evaluator asks what a
//! mob can stand on, this one asks what it can be inside: every node is a block
//! of water, and the search is free in all three axes rather than bound to a
//! floor.

use rustc_hash::FxHashMap;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_utils::{BlockPos, Direction, PackedBlockPos};

use crate::behavior::BlockStateBehaviorExt as _;

use super::collision::WalkNodeCollision;
use super::evaluator::NodeEvaluator;
use super::node_evaluator::Neighbors;
use super::settings::MobPathSettings;
use crate::entity::ai::node::{Node, NodeStore};
use crate::entity::ai::path::{PathComputationType, PathType, PathfindingContext};
use crate::fluid::is_water_fluid;

/// Extra cost of a node the mob would have to leave the water for.
///
/// Vanilla parity: the `8.0F` `SwimNodeEvaluator.findAcceptedNode` adds when the
/// block holds no fluid, which is what keeps a fish in the pond when it could
/// technically flop onto the bank.
const DRY_NODE_MALUS: f32 = 8.0;

/// The four horizontal directions, in vanilla's order.
///
/// Vanilla parity: `Direction.Plane.HORIZONTAL`, walked to build the diagonals.
const HORIZONTAL: [Direction; 4] = [
    Direction::North,
    Direction::East,
    Direction::South,
    Direction::West,
];

/// Decides where a swimming mob may go.
///
/// Vanilla parity: `SwimNodeEvaluator`.
///
/// Nothing constructs one yet. `WaterBoundPathNavigation` is the next step, and
/// it is what will hand this to the pathfinder.
#[derive(Debug, Clone)]
pub struct SwimNodeEvaluator {
    settings: MobPathSettings,
    nodes: NodeStore,
    /// Whether the mob may break the surface.
    ///
    /// Vanilla parity: the `allowBreaching` constructor flag, true for dolphins
    /// and false for fish, which is why a dolphin jumps and a cod does not.
    allow_breaching: bool,
    /// Path types already computed this search.
    ///
    /// Vanilla parity: `pathTypesByPosCache`. The swim evaluator asks for the
    /// same position from several directions, and each answer walks the mob's
    /// whole bounding box.
    path_type_cache: FxHashMap<i64, PathType>,
}

impl SwimNodeEvaluator {
    /// Creates the evaluator for one swimming mob.
    #[must_use]
    pub fn new(settings: MobPathSettings, allow_breaching: bool) -> Self {
        Self {
            settings,
            nodes: NodeStore::new(),
            allow_breaching,
            path_type_cache: FxHashMap::default(),
        }
    }

    /// Returns the settings the evaluator was built from.
    #[must_use]
    pub const fn settings(&self) -> &MobPathSettings {
        &self.settings
    }

    /// Returns the node at `hash`, if it has been reached.
    #[must_use]
    pub fn node(&self, hash: i32) -> Option<&Node> {
        self.nodes.get(hash)
    }

    /// Returns whether a node can still be stepped to.
    ///
    /// Vanilla parity: `SwimNodeEvaluator.isNodeValid`.
    fn is_node_valid(&self, hash: Option<i32>) -> bool {
        hash.and_then(|hash| self.nodes.get(hash))
            .is_some_and(|node| !node.closed)
    }

    /// Returns whether a node is reachable at all.
    ///
    /// Vanilla parity: `SwimNodeEvaluator.hasMalus`, the gate on building a
    /// diagonal out of two straight steps.
    fn has_malus(&self, hash: Option<i32>) -> bool {
        hash.and_then(|hash| self.nodes.get(hash))
            .is_some_and(|node| node.cost_malus >= 0.0)
    }

    /// Returns the node at these coordinates if a swimming mob may occupy it.
    ///
    /// Vanilla parity: `SwimNodeEvaluator.findAcceptedNode`.
    fn find_accepted_node(
        &mut self,
        context: &mut PathfindingContext<'_>,
        x: i32,
        y: i32,
        z: i32,
    ) -> Option<i32> {
        let path_type = self.cached_path_type(context, x, y, z);
        let accepted =
            path_type == PathType::Water || (self.allow_breaching && path_type == PathType::Breach);
        if !accepted {
            return None;
        }

        let path_cost = self.settings.pathfinding_malus(path_type);
        if path_cost < 0.0 {
            return None;
        }

        // Read the fluid before borrowing the node store, which borrows `self`.
        let dry = context
            .get_block_state(BlockPos::new(x, y, z))
            .get_fluid_state()
            .is_empty();

        let node = self.nodes.get_node(x, y, z);
        node.path_type = path_type;
        node.cost_malus = node.cost_malus.max(path_cost);
        if dry {
            node.cost_malus += DRY_NODE_MALUS;
        }
        Some(node.hash())
    }

    /// Returns the path type at these coordinates, computing it once per search.
    ///
    /// Vanilla parity: `SwimNodeEvaluator.getCachedBlockType`.
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
        let path_type = self.path_type_of_mob(context, x, y, z);
        self.path_type_cache.insert(key, path_type);
        path_type
    }

    /// Classifies the block a swimming mob would occupy.
    ///
    /// Vanilla parity: `SwimNodeEvaluator.getPathTypeOfMob`, which walks the
    /// mob's whole bounding box and refuses the position as soon as one cell is
    /// not water.
    fn path_type_of_mob(
        &self,
        context: &mut PathfindingContext<'_>,
        x: i32,
        y: i32,
        z: i32,
    ) -> PathType {
        for offset_x in x..x + self.settings.entity_width() {
            for offset_y in y..y + self.settings.entity_height() {
                for offset_z in z..z + self.settings.entity_depth() {
                    let pos = BlockPos::new(offset_x, offset_y, offset_z);
                    let state = context.get_block_state(pos);
                    let fluid = state.get_fluid_state();
                    let below = context.get_block_state(pos.below());

                    if fluid.is_empty()
                        && below.is_pathfindable(PathComputationType::Water)
                        && state.is_air()
                    {
                        return PathType::Breach;
                    }

                    if !is_water_fluid(fluid.fluid_id) {
                        return PathType::Blocked;
                    }
                }
            }
        }

        let state = context.get_block_state(BlockPos::new(x, y, z));
        if state.is_pathfindable(PathComputationType::Water) {
            PathType::Water
        } else {
            PathType::Blocked
        }
    }
}

impl NodeEvaluator for SwimNodeEvaluator {
    /// Vanilla parity: `SwimNodeEvaluator.getStart`, which starts from the
    /// bottom corner of the mob's box rather than the block it stands on.
    fn get_start(&mut self, _context: &mut PathfindingContext<'_>) -> i32 {
        let bounding_box = self.settings.bounding_box();
        let node = self.nodes.get_node(
            bounding_box.min_x().floor() as i32,
            (bounding_box.min_y() + 0.5).floor() as i32,
            bounding_box.min_z().floor() as i32,
        );
        node.hash()
    }

    /// Vanilla parity: `SwimNodeEvaluator.getNeighbors`: all six faces, then a
    /// diagonal for each pair of horizontal neighbors that both worked out.
    fn get_neighbors(
        &mut self,
        context: &mut PathfindingContext<'_>,
        _collision: &mut dyn WalkNodeCollision,
        pos_hash: i32,
    ) -> Neighbors {
        let mut neighbors = Neighbors::new();
        let Some(node) = self.nodes.get(pos_hash) else {
            return neighbors;
        };
        let (x, y, z) = (node.x, node.y, node.z);

        let mut by_direction: [Option<i32>; Direction::ALL.len()] = [None; Direction::ALL.len()];
        for (index, direction) in Direction::ALL.into_iter().enumerate() {
            let (step_x, step_y, step_z) = direction.offset();
            let found = self.find_accepted_node(context, x + step_x, y + step_y, z + step_z);
            by_direction[index] = found;
            if self.is_node_valid(found) {
                neighbors.push(found.unwrap_or_default());
            }
        }

        for direction in HORIZONTAL {
            let clockwise = direction.rotate_y_clockwise();
            let straight = by_direction[direction_index(direction)];
            let turned = by_direction[direction_index(clockwise)];
            if !self.has_malus(straight) || !self.has_malus(turned) {
                continue;
            }

            let (step_x, _, step_z) = direction.offset();
            let (turn_x, _, turn_z) = clockwise.offset();
            let diagonal =
                self.find_accepted_node(context, x + step_x + turn_x, y, z + step_z + turn_z);
            if self.is_node_valid(diagonal) {
                neighbors.push(diagonal.unwrap_or_default());
            }
        }

        neighbors
    }

    fn node(&self, hash: i32) -> Option<&Node> {
        self.nodes.get(hash)
    }

    fn node_mut(&mut self, hash: i32) -> Option<&mut Node> {
        self.nodes.get_mut(hash)
    }

    fn nodes_mut(&mut self) -> &mut NodeStore {
        &mut self.nodes
    }

    fn reset_search_state(&mut self) {
        self.nodes.reset_search_state();
    }

    /// Vanilla parity: `SwimNodeEvaluator.done`, which drops the path-type cache
    /// along with the nodes.
    fn clear_nodes(&mut self) {
        self.nodes.clear();
        self.path_type_cache.clear();
    }
}

/// Returns a direction's slot in [`Direction::ALL`].
fn direction_index(direction: Direction) -> usize {
    Direction::ALL
        .into_iter()
        .position(|candidate| candidate == direction)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use steel_registry::{init_vanilla_registry, vanilla_blocks};
    use steel_utils::BlockStateId;

    use super::*;
    use crate::behavior::init_behaviors;
    use crate::entity::ai::path::PathfindingMalus;
    use crate::world::LevelReader;

    /// A world that is water everywhere except where told otherwise.
    struct Pool {
        overrides: Vec<(BlockPos, BlockStateId)>,
    }

    impl Pool {
        fn new() -> Self {
            // Building the pool already reads block states, so the registry has
            // to be up before the first one is asked for.
            init_vanilla_registry();
            init_behaviors();
            Self {
                overrides: Vec::new(),
            }
        }

        fn with(mut self, pos: BlockPos, state: BlockStateId) -> Self {
            self.overrides.push((pos, state));
            self
        }
    }

    impl LevelReader for Pool {
        fn get_block_state(&self, pos: BlockPos) -> BlockStateId {
            self.overrides
                .iter()
                .find_map(|(at, state)| (*at == pos).then_some(*state))
                .unwrap_or_else(|| vanilla_blocks::WATER.default_state())
        }

        fn raw_brightness(&self, _pos: BlockPos, _sky_darkening: u8) -> u8 {
            0
        }

        fn min_y(&self) -> i32 {
            -64
        }

        fn height(&self) -> i32 {
            384
        }
    }

    fn settings() -> MobPathSettings {
        init_vanilla_registry();
        init_behaviors();
        let mut malus = PathfindingMalus::new();
        malus.set(PathType::Water, 0.0);
        malus.set(PathType::Breach, 4.0);
        MobPathSettings::new(1, 1, 1, BlockPos::new(0, 64, 0), &malus)
    }

    fn evaluator(allow_breaching: bool) -> SwimNodeEvaluator {
        SwimNodeEvaluator::new(settings(), allow_breaching)
    }

    fn context(level: &Pool) -> PathfindingContext<'_> {
        PathfindingContext::new(level, BlockPos::new(0, 64, 0))
    }

    /// Vanilla parity: `getPathTypeOfMob` calls open water `WATER`.
    #[test]
    fn open_water_is_swimmable() {
        let evaluator = evaluator(false);
        let level = Pool::new();
        let mut context = context(&level);

        assert_eq!(
            evaluator.path_type_of_mob(&mut context, 0, 64, 0),
            PathType::Water
        );
    }

    /// Vanilla parity: one non-water cell in the mob's box blocks the position
    /// outright, which is what keeps a fish from clipping into the pond wall.
    #[test]
    fn a_solid_block_in_the_way_blocks_the_position() {
        let evaluator = evaluator(false);
        let level = Pool::new().with(
            BlockPos::new(0, 64, 0),
            vanilla_blocks::STONE.default_state(),
        );
        let mut context = context(&level);

        assert_eq!(
            evaluator.path_type_of_mob(&mut context, 0, 64, 0),
            PathType::Blocked
        );
    }

    /// Vanilla parity: air over water is `BREACH`, the surface a dolphin jumps
    /// through and a cod refuses.
    #[test]
    fn air_above_water_is_a_breach() {
        let evaluator = evaluator(false);
        let level = Pool::new().with(BlockPos::new(0, 65, 0), vanilla_blocks::AIR.default_state());
        let mut context = context(&level);

        assert_eq!(
            evaluator.path_type_of_mob(&mut context, 0, 65, 0),
            PathType::Breach
        );
    }

    /// Vanilla parity: `findAcceptedNode` takes a breach only when the mob is
    /// allowed to leave the water.
    #[test]
    fn only_a_breaching_mob_accepts_the_surface() {
        let level = Pool::new().with(BlockPos::new(0, 65, 0), vanilla_blocks::AIR.default_state());

        let mut cod = evaluator(false);
        let mut cod_context = context(&level);
        assert!(cod.find_accepted_node(&mut cod_context, 0, 65, 0).is_none());

        let mut dolphin = evaluator(true);
        let mut dolphin_context = context(&level);
        assert!(
            dolphin
                .find_accepted_node(&mut dolphin_context, 0, 65, 0)
                .is_some()
        );
    }

    /// Vanilla parity: the `8.0F` surcharge on a node with no fluid in it, which
    /// is what keeps a swimming mob in the water when the bank is reachable.
    #[test]
    fn leaving_the_water_costs_extra() {
        let level = Pool::new().with(BlockPos::new(0, 65, 0), vanilla_blocks::AIR.default_state());
        let mut dolphin = evaluator(true);
        let mut context = context(&level);

        let hash = dolphin
            .find_accepted_node(&mut context, 0, 65, 0)
            .expect("a breaching mob accepts the surface");
        let cost = dolphin.node(hash).expect("node exists").cost_malus;

        assert!(
            cost >= DRY_NODE_MALUS,
            "a dry node should carry the surcharge, cost was {cost}"
        );
    }

    /// Vanilla parity: `getNeighbors` offers all six faces in open water, unlike
    /// the walking evaluator which is bound to a floor.
    #[test]
    fn open_water_offers_every_direction() {
        let mut evaluator = evaluator(false);
        let level = Pool::new();
        let mut context = context(&level);
        let start = evaluator.get_start(&mut context);

        let neighbors = evaluator.get_neighbors(&mut context, &mut |_| false, start);

        // Six faces, plus the four diagonals every pair of horizontals allows.
        assert_eq!(neighbors.len(), 10);
    }

    /// The cache must answer the same thing it computed, since the swim search
    /// asks for one position from several directions.
    #[test]
    fn the_path_type_cache_is_consistent() {
        let mut evaluator = evaluator(false);
        let level = Pool::new().with(
            BlockPos::new(1, 64, 0),
            vanilla_blocks::STONE.default_state(),
        );
        let mut context = context(&level);

        let first = evaluator.cached_path_type(&mut context, 1, 64, 0);
        let second = evaluator.cached_path_type(&mut context, 1, 64, 0);

        assert_eq!(first, PathType::Blocked);
        assert_eq!(second, first);
    }

    #[test]
    fn clearing_drops_the_cache_with_the_nodes() {
        let mut evaluator = evaluator(false);
        let level = Pool::new();
        let mut context = context(&level);
        let _ = evaluator.get_start(&mut context);
        let _ = evaluator.cached_path_type(&mut context, 0, 64, 0);

        evaluator.clear_nodes();

        assert!(evaluator.path_type_cache.is_empty());
        assert!(evaluator.nodes.is_empty());
    }

    /// Guards the registry lookup the evaluator relies on: water has to be
    /// pathfindable for the water computation type, or nothing swims anywhere.
    #[test]
    fn water_is_pathfindable_for_swimming() {
        init_vanilla_registry();
        init_behaviors();
        assert!(
            vanilla_blocks::WATER
                .default_state()
                .is_pathfindable(PathComputationType::Water)
        );
    }
}
