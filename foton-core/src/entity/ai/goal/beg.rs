use std::sync::Arc;

use foton_registry::item_stack::ItemStack;
use foton_utils::Downcast as _;
use glam::DVec3;

use super::reduced_tick_delay;
use super::selector::{Goal, GoalControls};
use crate::entity::ai::targeting::TargetingConditions;
use crate::entity::entities::WolfEntity;
use crate::entity::{Entity, LivingEntity, PathfinderMob};
use crate::player::Player;

/// Shortest time a wolf keeps begging for.
///
/// Vanilla parity: the `40 + random.nextInt(40)` of `BegGoal.start`.
const MIN_LOOK_TICKS: i32 = 40;

/// Extra random time on top of [`MIN_LOOK_TICKS`].
const EXTRA_LOOK_TICKS: i32 = 40;

/// Makes a wolf sit up and stare at food a player is holding.
///
/// Vanilla parity: `BegGoal`. The only server-visible effect is the synced
/// "interested" flag, which is what tilts the wolf's head on the client.
pub struct BegGoal {
    player: Option<Arc<Player>>,
    look_distance: f64,
    look_time: i32,
    interesting: Box<dyn Fn(&ItemStack) -> bool + Send>,
}

impl BegGoal {
    /// Creates the goal with the predicate for "food worth begging for".
    ///
    /// Vanilla hardcodes `itemStack.is(Items.BONE) || wolf.isFood(itemStack)`;
    /// the predicate is a parameter here so the wolf keeps owning what its own
    /// food is.
    #[must_use]
    pub(crate) fn new(
        look_distance: f64,
        interesting: impl Fn(&ItemStack) -> bool + Send + 'static,
    ) -> Self {
        Self {
            player: None,
            look_distance,
            look_time: 0,
            interesting: Box::new(interesting),
        }
    }

    fn player_holding_interesting(&self, player: &Player) -> bool {
        player.is_holding(&mut |item_stack| (self.interesting)(item_stack))
    }

    fn find_player(&mut self, mob: &dyn PathfinderMob) {
        let Some(world) = mob.level() else {
            self.player = None;
            return;
        };

        let beg_targeting = TargetingConditions::for_non_combat().range(self.look_distance);
        self.player = world.nearest_player(mob.position(), self.look_distance, |player| {
            beg_targeting.test(world.as_ref(), Some(mob), player)
        });
    }
}

impl Goal for BegGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::LOOK
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        self.find_player(mob);
        let Some(player) = self.player.clone() else {
            return false;
        };
        self.player_holding_interesting(&player)
    }

    fn can_continue_to_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        let Some(player) = self.player.clone() else {
            return false;
        };
        if !Entity::is_alive(player.as_ref()) {
            return false;
        }
        if mob.position().distance_squared(player.position())
            > self.look_distance * self.look_distance
        {
            return false;
        }

        self.look_time > 0 && self.player_holding_interesting(&player)
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        set_interested(mob, true);
        self.look_time =
            reduced_tick_delay(MIN_LOOK_TICKS + rand::random_range(0..EXTRA_LOOK_TICKS));
    }

    fn stop(&mut self, mob: &dyn PathfinderMob) {
        set_interested(mob, false);
        self.player = None;
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        let Some(player) = &self.player else {
            return;
        };

        let position = player.position();
        mob.mob_base().controls().lock().look_control.set_look_at(
            DVec3::new(position.x, player.get_eye_y(), position.z),
            10.0,
            mob.max_head_x_rot(),
        );
        self.look_time -= 1;
    }
}

/// Sets vanilla `Wolf.setIsInterested`.
///
/// Vanilla's `BegGoal` is typed on `Wolf` and calls the setter directly; only
/// the wolf has the synced flag, so the goal reaches for the wolf here too.
fn set_interested(mob: &dyn PathfinderMob, interested: bool) {
    if let Some(wolf) = mob.downcast_ref::<WolfEntity>() {
        wolf.set_is_interested(interested);
    }
}
