//! Hold-ground attack goal.
//!
//! Vanilla parity: `Raider.HoldGroundAttackGoal`. A patrol that spots a player
//! does not charge one at a time: the first raider to see them stands still,
//! shouts, and hands the target to every other raider within eight blocks. Only
//! when the player closes to ten blocks -- or when the shouter gives up -- does
//! the whole group turn aggressive at once. It is what makes walking into a
//! patrol feel like being noticed rather than being chased.

use foton_utils::WorldAabb;

use super::selector::{Goal, GoalControls};
use crate::entity::ai::targeting::TargetingConditions;
use crate::entity::{Mob, PathfinderMob, SharedEntity};

/// How far a shout carries.
///
/// Vanilla parity: the `range(8.0)` of `shoutTargeting` and the matching
/// `inflate(8.0, 8.0, 8.0)` of the search box.
const SHOUT_RANGE: f64 = 8.0;

/// Chance each tick that the waiting raider shouts again.
///
/// Vanilla parity: the `nextInt(50) == 0` of `tick`.
const SHOUT_CHANCE_DENOMINATOR: i32 = 50;

/// How sharply the waiting raider turns to keep the target in view.
///
/// Vanilla parity: the `setLookAt(target, 30.0F, 30.0F)` of `tick`.
const LOOK_SPEED: f32 = 30.0;

/// Stands and calls the rest of the patrol over.
///
/// Vanilla parity: `Raider.HoldGroundAttackGoal`.
pub(crate) struct HoldGroundAttackGoal {
    /// Squared distance inside which the raider stops waiting and attacks.
    hostile_radius_sqr: f64,
    /// Who a shout is allowed to reach.
    shout_targeting: TargetingConditions,
}

impl HoldGroundAttackGoal {
    /// Creates the goal for a raider that turns hostile inside `hostile_radius`.
    #[must_use]
    pub(crate) fn new(hostile_radius: f32) -> Self {
        Self {
            hostile_radius_sqr: f64::from(hostile_radius) * f64::from(hostile_radius),
            shout_targeting: TargetingConditions::for_non_combat()
                .range(SHOUT_RANGE)
                .ignore_line_of_sight()
                .ignore_invisibility_testing(),
        }
    }

    /// Returns the raiders close enough to hear a shout.
    ///
    /// Vanilla parity: the `getNearbyEntities(Raider.class, shoutTargeting, ..)`
    /// both `start` and `stop` run.
    fn nearby_raiders(&self, mob: &dyn PathfinderMob) -> Vec<SharedEntity> {
        let Some(world) = mob.level() else {
            return Vec::new();
        };
        let search_box: WorldAabb =
            mob.bounding_box()
                .inflate_xyz(SHOUT_RANGE, SHOUT_RANGE, SHOUT_RANGE);
        let self_id = mob.id();
        let level = world.as_ref();
        world.get_entities_in_aabb_matching(&search_box, |entity| {
            if entity.id() == self_id || entity.as_raider().is_none() {
                return false;
            }
            entity
                .as_living_entity()
                .is_some_and(|living| self.shout_targeting.test(level, Some(mob), living))
        })
    }
}

impl Goal for HoldGroundAttackGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::MOVE | GoalControls::LOOK
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        let Some(raider) = mob.as_raider() else {
            return false;
        };
        // A raider that was shot at has already been provoked and skips
        // straight to fighting rather than standing there calling for help.
        let hurt_by_player = mob
            .last_hurt_by_mob()
            .is_some_and(|attacker| attacker.as_player().is_some());
        raider.current_raid_status().is_none()
            && raider.is_patrolling()
            && mob.target().is_some()
            && !mob.is_aggressive()
            && !hurt_by_player
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        mob.mob_base().navigation().lock().stop();

        let Some(target) = mob.target() else {
            return;
        };
        for raider in self.nearby_raiders(mob) {
            if let Some(other) = raider.as_mob() {
                let _ = other.set_target(Some(&target));
            }
        }
    }

    fn stop(&mut self, mob: &dyn PathfinderMob) {
        let Some(target) = mob.target() else {
            return;
        };
        for raider in self.nearby_raiders(mob) {
            let Some(other) = raider.as_mob() else {
                continue;
            };
            let _ = other.set_target(Some(&target));
            other.set_aggressive(true);
        }
        mob.set_aggressive(true);
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        let Some(target) = mob.target() else {
            return;
        };

        if mob.position().distance_squared(target.position()) <= self.hostile_radius_sqr {
            mob.set_aggressive(true);
            return;
        }

        Mob::look_at(mob, target.as_ref(), LOOK_SPEED, LOOK_SPEED);
        if rand::random_range(0..SHOUT_CHANCE_DENOMINATOR) == 0 {
            mob.play_ambient_sound();
        }
    }
}
