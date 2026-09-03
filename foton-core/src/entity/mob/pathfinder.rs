use std::sync::Arc;

use foton_math::fast_floor;
use foton_registry::blocks::block_state_ext::BlockStateExt as _;
use foton_registry::{vanilla_attributes, vanilla_blocks};
use foton_utils::{BlockPos, ChunkPos};
use glam::DVec3;

use super::{Mob, TARGET_REACH_DISTANCE_SQR};
use crate::entity::ai::brain::memory::memory_module_types;
use crate::entity::ai::navigation::{
    NavigationPathRequest, NavigationRecomputeRequest, NavigationTickContext,
};
use crate::entity::ai::path::Path;
use crate::entity::ai::walk::{
    FlyNodeEvaluator, MobPathSettings, NodeEvaluator, SwimNodeEvaluator, WalkNodeEvaluator,
};
use crate::entity::{Entity, LivingEntity, SharedEntity};
use crate::physics::WorldCollisionProvider;
use crate::world::{LevelReader, World};

pub(super) fn tick_path_navigation_target<M: Mob + ?Sized>(
    mob: &M,
    world: &Arc<World>,
    game_time: i64,
    can_update_path: bool,
) {
    let flying = mob
        .as_pathfinder_mob()
        .is_some_and(|pathfinder| pathfinder.navigation_kind() == NavigationKind::Flying);
    let (target, speed_modifier) = {
        let mut navigation = mob.mob_base().navigation().lock();
        let mob_position = if flying {
            // Vanilla parity: `FlyingPathNavigation.getTempMobPos` returns the
            // mob's own position, because a flier is not standing on anything.
            mob.position()
        } else {
            ground_navigation_temp_mob_pos(mob, world.as_ref(), navigation.can_float())
        };
        let context = NavigationTickContext {
            mob_position,
            mob_bounding_box_width: mob.bounding_box().width(),
            mob_speed: mob.get_speed(),
            game_time,
        };
        let next_target = if can_update_path {
            navigation.next_move_target(context)
        } else {
            navigation.next_move_target_without_path_update(context, mob.on_ground())
        };
        let Some(target) = next_target else {
            return;
        };
        target
    };

    if flying {
        // Vanilla parity: `FlyingPathNavigation.tick` feeds the path position
        // straight to the move control rather than dropping it to the floor.
        mob.set_wanted_position(target, speed_modifier);
        return;
    }

    let target_pos = BlockPos::containing(target.x, target.y, target.z);
    let ground_y = if world.get_block_state(target_pos.below()).is_air() {
        target.y
    } else {
        WalkNodeEvaluator::floor_level(world.as_ref(), target_pos)
    };
    mob.set_wanted_position(DVec3::new(target.x, ground_y, target.z), speed_modifier);
}

fn ground_navigation_temp_mob_pos<M: Mob + ?Sized>(
    mob: &M,
    world: &World,
    can_float: bool,
) -> DVec3 {
    let position = mob.position();
    DVec3::new(
        position.x,
        f64::from(ground_navigation_surface_y(mob, world, can_float)),
        position.z,
    )
}

fn ground_navigation_surface_y<M: Mob + ?Sized>(mob: &M, world: &World, can_float: bool) -> i32 {
    if !mob.is_in_water() || !can_float {
        return fast_floor(mob.position().y + 0.5);
    }

    let position = mob.position();
    let block_y = mob.block_position().y();
    let mut surface = block_y;
    let mut state = world.get_block_state(BlockPos::containing(
        position.x,
        f64::from(surface),
        position.z,
    ));
    let mut steps = 0;
    while state.get_block() == &vanilla_blocks::WATER {
        surface += 1;
        state = world.get_block_state(BlockPos::containing(
            position.x,
            f64::from(surface),
            position.z,
        ));
        steps += 1;
        if steps > 16 {
            return block_y;
        }
    }

    surface
}

/// How a mob finds its way through the world.
///
/// Vanilla parity: the choice `Mob.createNavigation` makes between
/// `GroundPathNavigation` and `WaterBoundPathNavigation`. Foton keeps one
/// navigation and swaps the node evaluator, which is the only part that
/// actually differs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavigationKind {
    /// Walks, climbs and jumps on land.
    Ground,
    /// Flies.
    ///
    /// Vanilla parity: `FlyingPathNavigation`.
    Flying,
    /// Swims.
    WaterBound {
        /// Whether the mob may leave the water.
        ///
        /// Vanilla parity: the `allowBreaching` of
        /// `WaterBoundPathNavigation.createPathFinder`, true only for the
        /// dolphin.
        allow_breaching: bool,
    },
}

/// A mob that finds its way around, vanilla's `PathfinderMob`.
///
/// Almost every method here forwards to the mount first: a mob steering
/// another mob paths with the mount's body, not its own, so a rider never
/// computes a path its legs cannot walk.
pub trait PathfinderMob: Mob {
    /// The mount this mob steers, when that mount can path itself.
    ///
    /// `None` for a mob walking on its own legs, which is what makes the
    /// forwarding at the head of the other methods fall through.
    fn controlled_pathfinder_vehicle(&self) -> Option<SharedEntity> {
        let vehicle = self.controlled_mob_vehicle()?;
        vehicle.as_pathfinder_mob()?;
        Some(vehicle)
    }

    /// How much this mob wants to stand at `pos`, higher being better.
    ///
    /// Vanilla parity: `PathfinderMob.getWalkTargetValue`. Zero is
    /// indifference; an animal overrides this to prefer the block it grazes.
    fn get_walk_target_value(&self, pos: BlockPos) -> f32 {
        self.as_animal()
            .map_or(0.0, |animal| animal.animal_walk_target_value(pos))
    }

    /// Whether this mob can see `target`, answered from the sensing cache.
    ///
    /// Vanilla parity: `Sensing.hasLineOfSight`, which memoises the ray cast
    /// for one tick so several goals asking about the same target pay for one
    /// trace rather than one each.
    fn has_line_of_sight_cached(&self, target: &dyn Entity) -> bool {
        self.mob_base()
            .sensing()
            .lock()
            .has_line_of_sight(target.id(), || self.has_line_of_sight(target))
    }

    /// Returns how this mob finds its way.
    ///
    /// Vanilla parity: `Mob.createNavigation`.
    fn navigation_kind(&self) -> NavigationKind {
        NavigationKind::Ground
    }

    /// Vanilla parity: `PathNavigation.canUpdatePath`, and
    /// `WaterBoundPathNavigation.canUpdatePath` for a swimmer.
    fn can_update_path(&self) -> bool {
        can_update_path_for(
            self.navigation_kind(),
            PathSupport {
                on_ground: self.on_ground(),
                in_water: self.is_in_water(),
                in_lava: self.is_in_lava(),
                is_passenger: self.is_passenger(),
                can_float: self.mob_base().navigation().lock().can_float(),
            },
        )
    }

    /// Whether this mob will path to a target under the surface it walks on.
    fn can_path_to_targets_below_surface(&self) -> bool {
        if let Some(vehicle) = self.controlled_pathfinder_vehicle()
            && let Some(pathfinder) = vehicle.as_pathfinder_mob()
        {
            return pathfinder.can_path_to_targets_below_surface();
        }

        self.mob_base()
            .navigation()
            .lock()
            .can_path_to_targets_below_surface()
    }

    /// Whether a path exists that actually arrives at `target`.
    ///
    /// A path is computed and then checked for reaching its destination:
    /// navigation happily returns a partial path that stops short, and a goal
    /// asking this wants to know the difference.
    fn can_reach_living_target(&self, target: &dyn LivingEntity) -> bool {
        let target_pos = target.block_position();
        self.create_path_to(target_pos, 0)
            .is_some_and(|path| path_end_node_can_reach_target(&path, target_pos))
    }

    /// Advances navigation by one tick and honors any delayed recompute.
    ///
    /// Vanilla parity: `PathNavigation.tick`. The recompute is taken out from
    /// under the navigation lock before being run, because recomputing takes
    /// the same lock again.
    fn tick_pathfinder_path_navigation(&self) {
        let Some(world) = self.level() else {
            return;
        };
        let game_time = world.game_time();
        // `can_update_path` reads the navigation's float flag, so it has to be
        // answered before the navigation lock is taken rather than inside it.
        let can_update_path = self.can_update_path();
        let recompute_request = {
            let mut navigation = self.mob_base().navigation().lock();
            navigation.tick();
            navigation.take_delayed_recompute_request(game_time, can_update_path)
        };
        if let Some(request) = recompute_request {
            self.recompute_path(request);
        }

        tick_path_navigation_target(self, &world, game_time, can_update_path);
    }

    /// Runs this mob's goal and target selectors for one tick.
    ///
    /// Vanilla parity: the selector ticks inside `Mob.serverAiStep`. On odd
    /// ticks only the running goals are ticked, not the full re-selection --
    /// vanilla's way of halving goal cost -- and the parity is offset by the
    /// entity id so mobs do not all re-select on the same tick.
    fn tick_pathfinder_goal_selectors(&self)
    where
        Self: Sized,
    {
        let id_based_tick_count = self.tick_count().wrapping_add(self.id());
        let mut target_selector = self.mob_base().target_selector().lock();
        let mut goal_selector = self.mob_base().goal_selector().lock();
        if id_based_tick_count % 2 != 0 && self.tick_count() > 1 {
            target_selector.tick_running_goals(self, false);
            goal_selector.tick_running_goals(self, false);
        } else {
            target_selector.tick(self);
            goal_selector.tick(self);
        }
    }

    /// Whether `pos` is somewhere this mob could stop.
    ///
    /// Vanilla parity: `PathNavigation.isStableDestination`, and the flying
    /// override, which asks about the block itself rather than the one below.
    fn is_stable_destination(&self, pos: BlockPos) -> bool {
        if self.navigation_kind() == NavigationKind::Flying {
            // Vanilla parity: `FlyingPathNavigation.isStableDestination` asks
            // whether the block itself can be stood on, not the one below.
            return self
                .level()
                .is_some_and(|world| world.get_block_state(pos).is_solid_render());
        }

        self.level()
            .is_some_and(|world| world.get_block_state(pos.below()).is_solid_render())
    }

    /// Computes a path to `target`, or `None` if none was found.
    ///
    /// Vanilla parity: `PathNavigation.createPath`. An unloaded destination
    /// chunk yields `None` rather than a path through terrain that has not
    /// been generated.
    fn create_path_to(&self, target: BlockPos, reach_range: i32) -> Option<Path> {
        if let Some(vehicle) = self.controlled_pathfinder_vehicle()
            && let Some(pathfinder) = vehicle.as_pathfinder_mob()
        {
            return pathfinder.create_path_to(target, reach_range);
        }

        let world = self.level()?;
        if !world.has_full_chunk(ChunkPos::from_block_pos(target)) {
            return None;
        }

        let target = path_target_for_mob(self, world.as_ref(), target);
        let targets = [target];
        self.create_path_to_targets(&world, &targets, reach_range)
    }

    /// Recomputes the current path, vanilla's `PathNavigation.recomputePath`.
    fn recompute_path(&self, request: NavigationRecomputeRequest) {
        if let Some(vehicle) = self.controlled_pathfinder_vehicle()
            && let Some(pathfinder) = vehicle.as_pathfinder_mob()
        {
            pathfinder.recompute_path(request);
            return;
        }

        let path = self.create_path_to(request.target_pos, request.reach_range);
        self.mob_base()
            .navigation()
            .lock()
            .complete_recompute_path(path, request.game_time);
    }

    /// Paths to `target` and starts walking, vanilla's
    /// `PathNavigation.moveTo(double, double, double, double)`.
    ///
    /// Returns whether a path was found and accepted.
    fn move_to_pos(&self, target: DVec3, speed_modifier: f64) -> bool {
        self.move_to_pos_with_reach(target, 1, speed_modifier)
    }

    /// [`Self::move_to_pos`] with an explicit reach range: how close counts as
    /// arriving.
    ///
    /// A missing world or an unloaded destination stops navigation and returns
    /// `false`, rather than leaving the mob walking toward nothing.
    fn move_to_pos_with_reach(&self, target: DVec3, reach_range: i32, speed_modifier: f64) -> bool {
        if let Some(vehicle) = self.controlled_pathfinder_vehicle()
            && let Some(pathfinder) = vehicle.as_pathfinder_mob()
        {
            return pathfinder.move_to_pos_with_reach(target, reach_range, speed_modifier);
        }

        let target_pos = BlockPos::containing(target.x, target.y, target.z);
        let Some(world) = self.level() else {
            self.mob_base().navigation().lock().stop();
            return false;
        };
        if !world.has_full_chunk(ChunkPos::from_block_pos(target_pos)) {
            self.mob_base().navigation().lock().stop();
            return false;
        }

        let target_pos = path_target_for_mob(self, world.as_ref(), target_pos);
        let targets = [target_pos];
        if self
            .mob_base()
            .navigation()
            .lock()
            .reuse_current_path_to_targets(
                world.as_ref(),
                &targets,
                speed_modifier,
                self.position(),
            )
        {
            return true;
        }

        let path = self.create_path_to_targets(&world, &targets, reach_range);
        self.move_to_path(path, speed_modifier)
    }

    /// Starts following an already computed path.
    ///
    /// Vanilla parity: `PathNavigation.moveTo(Path, double)`. A `None` path
    /// stops navigation, which is how a goal cancels movement.
    fn move_to_path(&self, path: Option<Path>, speed_modifier: f64) -> bool {
        if let Some(vehicle) = self.controlled_pathfinder_vehicle()
            && let Some(pathfinder) = vehicle.as_pathfinder_mob()
        {
            return pathfinder.move_to_path(path, speed_modifier);
        }

        let Some(world) = self.level() else {
            self.mob_base().navigation().lock().stop();
            return false;
        };
        let mut navigation = self.mob_base().navigation().lock();
        let Some(path) = path else {
            navigation.stop();
            return false;
        };

        navigation.move_to(world.as_ref(), path, speed_modifier, self.position())
    }

    /// Whether this mob is currently following a path, vanilla's
    /// `PathNavigation.isInProgress`.
    fn is_path_finding(&self) -> bool {
        if let Some(vehicle) = self.controlled_pathfinder_vehicle()
            && let Some(pathfinder) = vehicle.as_pathfinder_mob()
        {
            return pathfinder.is_path_finding();
        }

        !self.mob_base().navigation().lock().is_done()
    }

    /// Vanilla parity: `PathfinderMob.isPanicking`, which asks the brain first
    /// and only falls back to the goal selector for a mob without one.
    ///
    /// Vanilla guards the brain half with `!isBrainDead()`; Foton does not need
    /// to, because a mob only answers [`Mob::brain`] at all once it owns one.
    /// Reading the memory rather than the brain's own state also keeps this to
    /// the brain's leaf lock, so a behavior may call it from inside its tick.
    fn is_panicking(&self) -> bool {
        if Mob::brain(self)
            .is_some_and(|brain| brain.has_memory_value(memory_module_types::IS_PANICKING.id()))
        {
            return true;
        }

        self.mob_base()
            .goal_selector()
            .lock()
            .has_running_panic_goal()
    }

    /// Returns whether a tempt goal is currently leading this mob.
    ///
    /// Vanilla parity: the `isBeingTempted` several mobs implement by holding
    /// on to the `TemptGoal` they registered. Foton's goals live behind the
    /// selector rather than in the mob, so the selector is asked instead --
    /// exactly as [`Self::is_panicking`] already does.
    fn is_being_tempted(&self) -> bool {
        self.mob_base()
            .goal_selector()
            .lock()
            .has_running_tempt_goal()
    }

    /// Computes one path toward the best of several candidate destinations.
    ///
    /// Vanilla parity: `PathNavigation.createPath(Set<BlockPos>, int)`. An
    /// empty candidate set, a mob below the world floor, or a navigation that
    /// cannot update right now all yield `None`.
    fn create_path_to_targets(
        &self,
        world: &Arc<World>,
        targets: &[BlockPos],
        reach_range: i32,
    ) -> Option<Path> {
        if let Some(vehicle) = self.controlled_pathfinder_vehicle()
            && let Some(pathfinder) = vehicle.as_pathfinder_mob()
        {
            return pathfinder.create_path_to_targets(world, targets, reach_range);
        }

        if targets.is_empty()
            || self.position().y < f64::from(world.min_y())
            || !self.can_update_path()
        {
            return None;
        }

        let follow_range = self
            .attributes()
            .lock()
            .required_value(vanilla_attributes::FOLLOW_RANGE);
        let max_path_length = {
            let mut navigation = self.mob_base().navigation().lock();
            navigation.update_pathfinder_max_visited_nodes(follow_range);
            navigation.max_path_length(follow_range)
        };

        let mob_position = self.block_position();
        let settings = MobPathSettings::from_mob(self);
        let mut evaluator: Box<dyn NodeEvaluator> = match self.navigation_kind() {
            NavigationKind::Ground => Box::new(WalkNodeEvaluator::new(settings)),
            NavigationKind::Flying => Box::new(FlyNodeEvaluator::new(settings)),
            NavigationKind::WaterBound { allow_breaching } => {
                Box::new(SwimNodeEvaluator::new(settings, allow_breaching))
            }
        };
        let collision_world =
            WorldCollisionProvider::for_path_navigation(world, self.as_entity_event_source());
        let mut collision = |aabb| {
            collision_world.has_entity_context_collision(
                aabb,
                self.position().y,
                self.is_descending(),
            )
        };

        self.mob_base().navigation().lock().create_path(
            evaluator.as_mut(),
            world.as_ref(),
            &mut collision,
            NavigationPathRequest {
                mob_position,
                targets,
                max_path_length,
                reach_range,
            },
        )
    }
}

/// Returns whether a mob in this state may look for a new path.
///
/// Vanilla parity: `PathNavigation.canUpdatePath` and its
/// `WaterBoundPathNavigation` override. A walker needs something under it or
/// around it; a swimmer needs to be in the water at all, unless it is the kind
/// that leaves it.
#[must_use]
pub const fn can_update_path_for(kind: NavigationKind, support: PathSupport) -> bool {
    match kind {
        NavigationKind::Ground => {
            support.on_ground || support.in_water || support.in_lava || support.is_passenger
        }
        // Vanilla parity: `FlyingPathNavigation.canUpdatePath`, which lets a
        // flier re-path in mid-air but not while it is riding something.
        NavigationKind::Flying => support.in_water && support.can_float || !support.is_passenger,
        NavigationKind::WaterBound { allow_breaching } => {
            allow_breaching || support.in_water || support.in_lava
        }
    }
}

/// What a mob currently has to move against.
///
/// Grouped rather than passed loose so the four flags cannot be swapped by
/// accident at a call site.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PathSupport {
    /// Whether the mob is standing on something.
    pub on_ground: bool,
    /// Whether the mob is in water.
    pub in_water: bool,
    /// Whether the mob is in lava.
    pub in_lava: bool,
    /// Whether the mob is riding something.
    pub is_passenger: bool,
    /// Whether the mob swims rather than sinking.
    ///
    /// Vanilla parity: `PathNavigation.canFloat`, which only the flying
    /// navigation reads in `canUpdatePath`.
    pub can_float: bool,
}

pub(super) fn path_end_node_can_reach_target(path: &Path, target: BlockPos) -> bool {
    let Some(end_node) = path.end_node() else {
        return false;
    };
    let dx = end_node.x - target.x();
    let dz = end_node.z - target.z();
    f64::from(dx * dx + dz * dz) <= TARGET_REACH_DISTANCE_SQR
}

fn path_target_for_mob<M: PathfinderMob + ?Sized>(
    mob: &M,
    level: &dyn LevelReader,
    target: BlockPos,
) -> BlockPos {
    if mob.can_path_to_targets_below_surface() {
        target
    } else {
        find_ground_path_target_surface(level, target)
    }
}

pub(super) fn find_ground_path_target_surface(
    level: &dyn LevelReader,
    mut pos: BlockPos,
) -> BlockPos {
    if level.get_block_state(pos).is_air() {
        let mut column_pos = pos.below();
        while column_pos.y() >= level.min_y() && level.get_block_state(column_pos).is_air() {
            column_pos = column_pos.below();
        }
        if column_pos.y() >= level.min_y() {
            return column_pos.above();
        }

        column_pos = pos.at_y(pos.y() + 1);
        while column_pos.y() < level.max_y_exclusive() && level.get_block_state(column_pos).is_air()
        {
            column_pos = column_pos.above();
        }
        pos = column_pos;
    }

    if !level.get_block_state(pos).is_solid() {
        return pos;
    }

    let mut column_pos = pos.above();
    while column_pos.y() < level.max_y_exclusive() && level.get_block_state(column_pos).is_solid() {
        column_pos = column_pos.above();
    }
    column_pos
}

#[cfg(test)]
mod navigation_kind_tests {
    use super::{NavigationKind, PathSupport, can_update_path_for};

    const SWIMMER: NavigationKind = NavigationKind::WaterBound {
        allow_breaching: false,
    };
    const BREACHER: NavigationKind = NavigationKind::WaterBound {
        allow_breaching: true,
    };

    /// Vanilla parity: `PathNavigation.canUpdatePath`.
    #[test]
    fn a_walker_needs_ground_water_lava_or_a_ride() {
        assert!(can_update_path_for(
            NavigationKind::Ground,
            PathSupport {
                on_ground: true,
                in_water: false,
                in_lava: false,
                is_passenger: false,
                can_float: false,
            }
        ));
        assert!(can_update_path_for(
            NavigationKind::Ground,
            PathSupport {
                on_ground: false,
                in_water: true,
                in_lava: false,
                is_passenger: false,
                can_float: false,
            }
        ));
        assert!(can_update_path_for(
            NavigationKind::Ground,
            PathSupport {
                on_ground: false,
                in_water: false,
                in_lava: true,
                is_passenger: false,
                can_float: false,
            }
        ));
        assert!(can_update_path_for(
            NavigationKind::Ground,
            PathSupport {
                on_ground: false,
                in_water: false,
                in_lava: false,
                is_passenger: true,
                can_float: false,
            }
        ));
    }

    /// A walker in mid-air with nothing to ride cannot re-path, which is what
    /// stops a falling mob from steering.
    #[test]
    fn a_falling_walker_cannot_repath() {
        assert!(!can_update_path_for(
            NavigationKind::Ground,
            PathSupport {
                on_ground: false,
                in_water: false,
                in_lava: false,
                is_passenger: false,
                can_float: false,
            }
        ));
    }

    /// Vanilla parity: `WaterBoundPathNavigation.canUpdatePath`. Standing on the
    /// bank is not enough for a fish.
    #[test]
    fn a_swimmer_needs_to_be_in_the_water() {
        assert!(can_update_path_for(
            SWIMMER,
            PathSupport {
                on_ground: false,
                in_water: true,
                in_lava: false,
                is_passenger: false,
                can_float: false,
            }
        ));
        assert!(!can_update_path_for(
            SWIMMER,
            PathSupport {
                on_ground: true,
                in_water: false,
                in_lava: false,
                is_passenger: false,
                can_float: false,
            }
        ));
        assert!(!can_update_path_for(
            SWIMMER,
            PathSupport {
                on_ground: false,
                in_water: false,
                in_lava: false,
                is_passenger: true,
                can_float: false,
            }
        ));
    }

    /// A dolphin keeps pathing out of the water, which is how it jumps.
    #[test]
    fn a_breaching_swimmer_paths_anywhere() {
        assert!(can_update_path_for(
            BREACHER,
            PathSupport {
                on_ground: false,
                in_water: false,
                in_lava: false,
                is_passenger: false,
                can_float: false,
            }
        ));
    }
}
