//! The tempt goal a stray cat follows fish with.
//!
//! Vanilla parity: `Cat.CatTemptGoal`, an inner class of `Cat`. It adds two
//! things to `TemptGoal`: it only runs while the cat is untamed, and it picks
//! one player at random that it will not be scared of -- which is what lets a
//! player who stands still eventually get close enough to feed it.

use std::sync::Arc;

use steel_utils::Downcast as _;
use uuid::Uuid;

use super::CatEntity;
use crate::entity::ai::goal::{Goal, GoalControls, TemptGoal, TemptScareRule, reduced_tick_delay};
use crate::entity::{Entity, PathfinderMob, TamableAnimal};
use crate::player::Player;

/// One chance in this many ticks that the cat settles on a player.
///
/// Vanilla parity: the `nextInt(adjustedTickDelay(600))` of `CatTemptGoal.tick`.
const SELECT_PLAYER_CHANCE: i32 = 600;

/// One chance in this many ticks that it forgets them again.
///
/// Vanilla parity: the `nextInt(adjustedTickDelay(500))` of the same method.
const FORGET_PLAYER_CHANCE: i32 = 500;

/// The scare rule that remembers one trusted player.
struct CatTemptScareRule {
    selected_player: Option<Uuid>,
}

impl TemptScareRule for CatTemptScareRule {
    fn tick(&mut self, _mob: &dyn PathfinderMob, player: Option<&Arc<Player>>) {
        if self.selected_player.is_none() {
            if rand::random_range(0..reduced_tick_delay(SELECT_PLAYER_CHANCE)) == 0 {
                self.selected_player = player.map(|player| player.uuid());
            }
        } else if rand::random_range(0..reduced_tick_delay(FORGET_PLAYER_CHANCE)) == 0 {
            self.selected_player = None;
        }
    }

    fn can_scare(
        &mut self,
        _mob: &dyn PathfinderMob,
        player: Option<&Arc<Player>>,
        base: bool,
    ) -> bool {
        let selected = self.selected_player;
        if selected.is_some() && selected == player.map(|player| player.uuid()) {
            return false;
        }

        base
    }
}

/// Vanilla parity: `Cat.CatTemptGoal`.
pub(super) struct CatTemptGoal {
    tempt: TemptGoal,
}

impl CatTemptGoal {
    pub(super) fn new(speed_modifier: f64) -> Self {
        Self {
            tempt: TemptGoal::new(speed_modifier, CatEntity::is_cat_food, true).with_scare_rule(
                CatTemptScareRule {
                    selected_player: None,
                },
            ),
        }
    }

    fn is_tame(mob: &dyn PathfinderMob) -> bool {
        mob.downcast_ref::<CatEntity>()
            .is_some_and(TamableAnimal::is_tame)
    }
}

impl Goal for CatTemptGoal {
    fn controls(&self) -> GoalControls {
        self.tempt.controls()
    }

    fn is_tempt_goal(&self) -> bool {
        true
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        self.tempt.can_use(mob) && !Self::is_tame(mob)
    }

    fn can_continue_to_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        self.tempt.can_continue_to_use(mob)
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        self.tempt.start(mob);
    }

    fn stop(&mut self, mob: &dyn PathfinderMob) {
        self.tempt.stop(mob);
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        self.tempt.tick(mob);
    }
}
