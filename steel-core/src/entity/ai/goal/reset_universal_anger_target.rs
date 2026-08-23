use steel_registry::vanilla_game_rules::UNIVERSAL_ANGER;
use steel_utils::WorldAabb;

use super::selector::{Goal, GoalControls};
use super::target_goal::follow_distance;
use crate::entity::PathfinderMob;

/// Vertical reach of the shout that spreads universal anger.
///
/// Vanilla parity: `ResetUniversalAngerTargetGoal.ALERT_RANGE_Y`.
const ALERT_RANGE_Y: f64 = 10.0;

/// Turns a neutral mob on everyone once a player has hit it.
///
/// Vanilla parity: `ResetUniversalAngerTargetGoal`. It only does anything while
/// the `universalAnger` game rule is on, which is the point: one player's
/// mistake becomes everyone's problem.
pub struct ResetUniversalAngerTargetGoal {
    alert_others_of_same_type: bool,
    last_hurt_by_player_timestamp: i32,
}

impl ResetUniversalAngerTargetGoal {
    #[must_use]
    pub(crate) const fn new(alert_others_of_same_type: bool) -> Self {
        Self {
            alert_others_of_same_type,
            last_hurt_by_player_timestamp: 0,
        }
    }

    fn was_hurt_by_player(&self, mob: &dyn PathfinderMob) -> bool {
        mob.last_hurt_by_mob()
            .is_some_and(|attacker| attacker.as_player().is_some())
            && mob.last_hurt_by_mob_timestamp() > self.last_hurt_by_player_timestamp
    }

    fn alert_nearby_mobs_of_same_type(mob: &dyn PathfinderMob) {
        let Some(world) = mob.level() else {
            return;
        };

        let within = follow_distance(mob);
        let position = mob.position();
        let search_box = WorldAabb::new(
            position.x,
            position.y,
            position.z,
            position.x + 1.0,
            position.y + 1.0,
            position.z + 1.0,
        )
        .inflate_xyz(within, ALERT_RANGE_Y, within);
        let mob_type_key = mob.downcast_type_key();
        let mob_uuid = mob.uuid();

        for entity in world.get_entities_in_aabb_matching(&search_box, |entity| {
            entity.uuid() != mob_uuid
                && entity.downcast_type_key() == mob_type_key
                && !entity.is_spectator()
        }) {
            if let Some(other) = entity.as_neutral_mob() {
                other.forget_current_target_and_refresh_universal_anger();
            }
        }
    }
}

impl Goal for ResetUniversalAngerTargetGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::EMPTY
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        mob.level()
            .is_some_and(|world| world.get_game_rule(&UNIVERSAL_ANGER))
            && self.was_hurt_by_player(mob)
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        self.last_hurt_by_player_timestamp = mob.last_hurt_by_mob_timestamp();
        let Some(neutral) = mob.as_neutral_mob() else {
            return;
        };
        neutral.forget_current_target_and_refresh_universal_anger();

        if self.alert_others_of_same_type {
            Self::alert_nearby_mobs_of_same_type(mob);
        }
    }
}
