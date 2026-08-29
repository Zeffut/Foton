//! Stringing llamas into a caravan behind a leashed one.
//!
//! Vanilla parity: `LlamaFollowCaravanGoal`. A llama joins the tail of a line
//! that ultimately hangs off a lead, which is what turns one leashed llama into
//! the pack train a wandering trader drags around.

use foton_registry::vanilla_entities;
use glam::DVec3;

use super::reduced_tick_delay;
use super::selector::{Goal, GoalControls};
use crate::entity::{Llama, Mob, PathfinderMob, SharedEntity, is_llama};

/// How deep the leash chain is walked before the caravan is called unanchored.
///
/// Vanilla parity: `LlamaFollowCaravanGoal.CARAVAN_LIMIT`.
const CARAVAN_LIMIT: i32 = 8;

/// How far around itself a llama looks for a caravan to join.
///
/// Vanilla parity: the `inflate(9.0, 4.0, 9.0)` of `canUse`.
const SEARCH_HORIZONTAL_RANGE: f64 = 9.0;

/// The vertical half of that search box.
const SEARCH_VERTICAL_RANGE: f64 = 4.0;

/// How close a llama has to already be for joining to be pointless.
///
/// Vanilla parity: the `closestDistSquare < 4.0` of `canUse`.
const ALREADY_TOGETHER_DISTANCE_SQR: f64 = 4.0;

/// How far behind the caravan may stretch before the llama gives up.
///
/// Vanilla parity: the `distSqr > 676.0` of `canContinueToUse`.
const CARAVAN_BREAK_DISTANCE_SQR: f64 = 676.0;

/// The gap a follower tries to keep to the llama ahead.
///
/// Vanilla parity: the `wantedDistance` of `tick`.
const WANTED_FOLLOW_DISTANCE: f64 = 2.0;

/// Ceiling on the catch-up speed multiplier.
///
/// Vanilla parity: the `speedModifier <= 3.0` of `canContinueToUse`.
const MAX_CATCH_UP_SPEED: f64 = 3.0;

/// How much faster a straggler walks each time it falls behind.
const CATCH_UP_SPEED_FACTOR: f64 = 1.2;

/// Grace ticks a straggler gets at the boosted speed.
const CATCH_UP_GRACE_TICKS: i32 = 40;

/// Follows the llama ahead in a caravan.
///
/// Vanilla parity: `LlamaFollowCaravanGoal`.
pub struct LlamaFollowCaravanGoal {
    base_speed_modifier: f64,
    speed_modifier: f64,
    dist_check_counter: i32,
}

impl LlamaFollowCaravanGoal {
    #[must_use]
    pub(crate) const fn new(speed_modifier: f64) -> Self {
        Self {
            base_speed_modifier: speed_modifier,
            speed_modifier,
            dist_check_counter: 0,
        }
    }

    /// Returns whether the head of this chain hangs off a lead.
    ///
    /// Vanilla parity: `LlamaFollowCaravanGoal.firstIsLeashed`.
    fn first_is_leashed(llama: &dyn Llama, mut counter: i32) -> bool {
        let mut current: Option<SharedEntity> = llama.caravan_head();
        loop {
            if counter > CARAVAN_LIMIT {
                return false;
            }
            let Some(head) = current else {
                return false;
            };
            let Some(head_llama) = head.as_llama() else {
                return false;
            };
            if Mob::is_leashed(head_llama) {
                return true;
            }
            counter += 1;
            current = head_llama.caravan_head();
        }
    }
}

impl Goal for LlamaFollowCaravanGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::MOVE
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        let Some(llama) = mob.as_llama() else {
            return false;
        };
        if Mob::is_leashed(llama) || llama.in_caravan() {
            return false;
        }
        let Some(world) = mob.level() else {
            return false;
        };

        let search_box = mob.bounding_box().inflate_xyz(
            SEARCH_HORIZONTAL_RANGE,
            SEARCH_VERTICAL_RANGE,
            SEARCH_HORIZONTAL_RANGE,
        );
        let candidates = world.get_entities_in_aabb_matching(&search_box, |entity| {
            entity.id() != mob.id() && is_llama(entity)
        });

        // Vanilla walks the list twice: llamas already in a caravan win over
        // llamas that are only leashed, so a line grows from its tail.
        let mut closest: Option<SharedEntity> = None;
        let mut closest_dist_sqr = f64::MAX;
        for pass_wants_caravan in [true, false] {
            for candidate in &candidates {
                let Some(candidate_llama) = candidate.as_llama() else {
                    continue;
                };
                let qualifies = if pass_wants_caravan {
                    candidate_llama.in_caravan()
                } else {
                    Mob::is_leashed(candidate_llama)
                };
                if !qualifies || candidate_llama.has_caravan_tail() {
                    continue;
                }

                let dist_sqr = mob.position().distance_squared(candidate.position());
                if dist_sqr <= closest_dist_sqr {
                    closest_dist_sqr = dist_sqr;
                    closest = Some(candidate.clone());
                }
            }
            if closest.is_some() {
                break;
            }
        }

        let Some(closest) = closest else {
            return false;
        };
        if closest_dist_sqr < ALREADY_TOGETHER_DISTANCE_SQR {
            return false;
        }

        let Some(closest_llama) = closest.as_llama() else {
            return false;
        };
        if !Mob::is_leashed(closest_llama) && !Self::first_is_leashed(closest_llama, 1) {
            return false;
        }

        llama.join_caravan(closest_llama);
        true
    }

    fn can_continue_to_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        let Some(llama) = mob.as_llama() else {
            return false;
        };
        if !llama.in_caravan() {
            return false;
        }
        let Some(head) = llama.caravan_head() else {
            return false;
        };
        if !head.is_alive() || !Self::first_is_leashed(llama, 0) {
            return false;
        }

        if mob.position().distance_squared(head.position()) > CARAVAN_BREAK_DISTANCE_SQR {
            if self.speed_modifier <= MAX_CATCH_UP_SPEED {
                self.speed_modifier *= CATCH_UP_SPEED_FACTOR;
                self.dist_check_counter = reduced_tick_delay(CATCH_UP_GRACE_TICKS);
                return true;
            }
            if self.dist_check_counter == 0 {
                return false;
            }
        }

        if self.dist_check_counter > 0 {
            self.dist_check_counter -= 1;
        }
        true
    }

    fn stop(&mut self, mob: &dyn PathfinderMob) {
        if let Some(llama) = mob.as_llama() {
            llama.leave_caravan();
        }
        self.speed_modifier = self.base_speed_modifier;
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        let Some(llama) = mob.as_llama() else {
            return;
        };
        if !llama.in_caravan() {
            return;
        }
        // Vanilla skips the follow while the llama hangs off a fence knot, so a
        // tied-up pack train stays put instead of dragging itself in circles.
        if llama
            .leash_holder()
            .is_some_and(|holder| holder.entity_type() == &vanilla_entities::LEASH_KNOT)
        {
            return;
        }

        let Some(head) = llama.caravan_head() else {
            return;
        };
        let position = mob.position();
        let distance = position.distance(head.position());
        let delta = (head.position() - position).normalize_or_zero()
            * (distance - WANTED_FOLLOW_DISTANCE).max(0.0);
        mob.move_to_pos(
            position + DVec3::new(delta.x, delta.y, delta.z),
            self.speed_modifier,
        );
    }
}
