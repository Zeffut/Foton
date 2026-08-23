use super::nearest_attackable_target::NearestAttackableTargetGoal;
use super::selector::{Goal, GoalControls};
use crate::entity::{LivingEntity, PathfinderMob, TamableAnimal};
use crate::world::World;

/// How often an untamed pet looks for prey.
///
/// Vanilla parity: the `10` `NonTameRandomTargetGoal` passes to its super.
const RANDOM_INTERVAL: i32 = 10;

/// Lets an untamed pet hunt, and stops the moment it is tamed.
///
/// Vanilla parity: `NonTameRandomTargetGoal`. This is why a wild wolf chases
/// sheep and a tamed one walks past them.
pub struct NonTameRandomTargetGoal {
    nearest: NearestAttackableTargetGoal,
}

impl NonTameRandomTargetGoal {
    #[must_use]
    pub(crate) fn new(
        must_see: bool,
        selector: impl Fn(Option<&dyn LivingEntity>, &dyn LivingEntity, &World) -> bool
        + Send
        + Sync
        + 'static,
    ) -> Self {
        Self {
            nearest: NearestAttackableTargetGoal::new_with_interval(
                RANDOM_INTERVAL,
                must_see,
                false,
                selector,
            ),
        }
    }
}

impl Goal for NonTameRandomTargetGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::TARGET
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        let is_tame = mob.as_tamable_animal().is_some_and(TamableAnimal::is_tame);
        !is_tame && self.nearest.can_use(mob)
    }

    fn can_continue_to_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        self.nearest.current_target_passes_conditions(mob)
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        self.nearest.start(mob);
    }

    fn stop(&mut self, mob: &dyn PathfinderMob) {
        self.nearest.stop(mob);
    }
}
