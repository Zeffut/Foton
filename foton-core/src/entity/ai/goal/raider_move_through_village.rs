//! Walk a village house by house, looking for something to break.
//!
//! Vanilla parity: `Raider.RaiderMoveThroughVillageGoal`. Once a raider is
//! inside the village and has nothing to fight, it picks a bed it has not
//! visited yet and walks to it. The visited list is only three long, so a small
//! village is walked in a loop rather than exhausted -- which is what keeps
//! raiders moving through the streets instead of standing on the last house.

use core::f64::consts::{FRAC_PI_2, PI};

use glam::DVec3;

use super::default_random_pos_towards;
use super::selector::{Goal, GoalControls};
use crate::entity::{PathfinderMob, Raider};
use crate::poi::OccupationStatus;
use foton_registry::{RegistryEntry as _, vanilla_poi_types};
use foton_utils::BlockPos;

/// How far a raider looks for the next house.
///
/// Vanilla parity: the `48` of `getRandom(.., pos, 48, random)`.
const HOUSE_SEARCH_RADIUS: i32 = 48;

/// How many houses a raider remembers visiting.
///
/// Vanilla parity: the `visited.size() > 2` of `updateVisited`.
const VISITED_MEMORY: usize = 2;

/// First fallback hop when the path to the house runs out.
///
/// Vanilla parity: the `getPosTowards(raider, 16, 7, poiVec, PI / 10)` of `tick`.
const NEAR_HOP_HORIZONTAL: i32 = 16;
const NEAR_HOP_VERTICAL: i32 = 7;
const NEAR_HOP_ANGLE: f64 = PI / 10.0;

/// Second fallback hop, wider and shorter.
///
/// Vanilla parity: the `getPosTowards(raider, 8, 7, poiVec, PI / 2)` of `tick`.
const WIDE_HOP_HORIZONTAL: i32 = 8;
const WIDE_HOP_ANGLE: f64 = FRAC_PI_2;

/// Walks a raider through the village it is besieging.
///
/// Vanilla parity: `Raider.RaiderMoveThroughVillageGoal`.
pub(crate) struct RaiderMoveThroughVillageGoal {
    speed_modifier: f64,
    /// How close counts as having arrived.
    ///
    /// Vanilla parity: the `distanceToPoi` constructor argument, which every
    /// raider passes as one.
    distance_to_poi: f64,
    /// The house currently being walked to.
    poi_pos: Option<BlockPos>,
    /// The last few houses, so the raider does not circle one of them.
    visited: Vec<BlockPos>,
    /// Whether the last two fallback hops both failed.
    stuck: bool,
}

impl RaiderMoveThroughVillageGoal {
    /// Creates the goal with vanilla's speed and arrival distance.
    #[must_use]
    pub(crate) const fn new(speed_modifier: f64, distance_to_poi: f64) -> Self {
        Self {
            speed_modifier,
            distance_to_poi,
            poi_pos: None,
            visited: Vec::new(),
            stuck: false,
        }
    }

    /// Vanilla parity: `isValidRaid`.
    fn is_valid_raid(mob: &dyn PathfinderMob) -> bool {
        mob.as_raider()
            .and_then(Raider::current_raid_status)
            .is_some_and(|status| status.active && !status.over)
    }

    /// Picks the nearest unvisited bed, if there is one.
    ///
    /// Vanilla parity: `hasSuitablePoi`.
    fn has_suitable_poi(&mut self, mob: &dyn PathfinderMob) -> bool {
        let Some(world) = mob.level() else {
            return false;
        };
        let home = vanilla_poi_types::HOME.id();
        let visited = self.visited.clone();
        let mut rng = rand::rng();
        let found = world.poi_storage.lock().get_random(
            &|type_id| type_id == home,
            &|pos| !visited.contains(&pos),
            OccupationStatus::Any,
            mob.block_position(),
            HOUSE_SEARCH_RADIUS,
            &mut rng,
        );
        let Some(pos) = found else {
            return false;
        };
        self.poi_pos = Some(pos);
        true
    }

    /// Vanilla parity: `updateVisited`, which drops the oldest entry.
    fn update_visited(&mut self) {
        if self.visited.len() > VISITED_MEMORY {
            self.visited.remove(0);
        }
    }
}

impl Goal for RaiderMoveThroughVillageGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::MOVE
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        self.update_visited();
        Self::is_valid_raid(mob) && self.has_suitable_poi(mob) && mob.target().is_none()
    }

    fn can_continue_to_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        let Some(poi_pos) = self.poi_pos else {
            return false;
        };
        if !mob.is_path_finding() {
            return false;
        }
        let arrival = f64::from(mob.base().dimensions().width) + self.distance_to_poi;
        mob.target().is_none() && !closer_to_center_than(poi_pos, mob, arrival) && !self.stuck
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        let Some(poi_pos) = self.poi_pos else {
            return;
        };
        mob.set_no_action_time(0);
        let (x, y, z) = poi_pos.get_bottom_center();
        mob.move_to_pos(DVec3::new(x, y, z), self.speed_modifier);
        self.stuck = false;
    }

    fn stop(&mut self, mob: &dyn PathfinderMob) {
        let Some(poi_pos) = self.poi_pos else {
            return;
        };
        if closer_to_center_than(poi_pos, mob, self.distance_to_poi) {
            self.visited.push(poi_pos);
        }
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        if mob.is_path_finding() {
            return;
        }
        let Some(poi_pos) = self.poi_pos else {
            return;
        };
        let (x, y, z) = poi_pos.get_bottom_center();
        let poi_vec = DVec3::new(x, y, z);

        let next = default_random_pos_towards(
            mob,
            NEAR_HOP_HORIZONTAL,
            NEAR_HOP_VERTICAL,
            poi_vec,
            NEAR_HOP_ANGLE,
        )
        .or_else(|| {
            default_random_pos_towards(
                mob,
                WIDE_HOP_HORIZONTAL,
                NEAR_HOP_VERTICAL,
                poi_vec,
                WIDE_HOP_ANGLE,
            )
        });
        let Some(next) = next else {
            self.stuck = true;
            return;
        };
        mob.move_to_pos(next, self.speed_modifier);
    }
}

/// Vanilla parity: `Vec3i.closerToCenterThan`.
fn closer_to_center_than(pos: BlockPos, mob: &dyn PathfinderMob, distance: f64) -> bool {
    let (x, y, z) = pos.get_center();
    DVec3::new(x, y, z).distance_squared(mob.position()) < distance * distance
}
