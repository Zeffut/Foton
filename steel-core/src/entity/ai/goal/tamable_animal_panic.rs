use steel_utils::Identifier;

use super::panic_goal::PanicGoal;
use super::selector::{Goal, GoalControls};
use crate::entity::PathfinderMob;

/// A pet's panic, which also snaps it back to its owner if it has bolted too far.
///
/// Vanilla parity: `TamableAnimal.TamableAnimalPanicGoal`, which is only a
/// `PanicGoal` with one extra line in `tick`. That line is what stops a
/// frightened wolf from ending up half a chunk away and never coming back.
pub struct TamableAnimalPanicGoal {
    panic: PanicGoal,
}

impl TamableAnimalPanicGoal {
    #[must_use]
    pub(crate) fn new(speed_modifier: f64) -> Self {
        Self {
            panic: PanicGoal::new(speed_modifier),
        }
    }

    /// Creates a pet panic goal that only the given damage types set off.
    #[must_use]
    pub(crate) const fn with_damage_types(
        speed_modifier: f64,
        panic_causing_damage_types: Identifier,
    ) -> Self {
        Self {
            panic: PanicGoal::with_damage_types(speed_modifier, panic_causing_damage_types),
        }
    }
}

impl Goal for TamableAnimalPanicGoal {
    fn controls(&self) -> GoalControls {
        self.panic.controls()
    }

    fn is_panic_goal(&self) -> bool {
        true
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        self.panic.can_use(mob)
    }

    fn can_continue_to_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        self.panic.can_continue_to_use(mob)
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        self.panic.start(mob);
    }

    fn stop(&mut self, mob: &dyn PathfinderMob) {
        self.panic.stop(mob);
    }

    fn requires_update_every_tick(&self) -> bool {
        self.panic.requires_update_every_tick()
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        if let Some(pet) = mob.as_tamable_animal()
            && !pet.unable_to_move_to_owner()
            && pet.should_try_teleport_to_owner()
        {
            pet.try_to_teleport_to_owner();
        }

        self.panic.tick(mob);
    }
}
