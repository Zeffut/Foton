//! Spellcaster casting-spell goal.
//!
//! Vanilla parity: `SpellcasterIllager.SpellcasterCastingSpellGoal`. This is
//! the goal that holds an evoker still with its arms up: while a spell is
//! running it takes the movement and look controls away from everything else,
//! so the caster cannot walk out of its own animation. Clearing the spell on
//! stop is what puts the arms down.

use super::selector::{Goal, GoalControls};
use crate::entity::{IllagerSpell, Mob, PathfinderMob, SharedEntity};

/// A subclass override of where the caster looks while casting.
///
/// Vanilla parity: overriding `SpellcasterCastingSpellGoal.tick`, which the
/// evoker does so it keeps looking at the sheep it is recoloring.
type LookTarget = fn(&dyn PathfinderMob) -> Option<SharedEntity>;

/// Stands still for the length of a spell.
///
/// Vanilla parity: `SpellcasterIllager.SpellcasterCastingSpellGoal`.
pub(crate) struct SpellcasterCastingSpellGoal {
    /// What the caster watches while casting.
    look_target: LookTarget,
}

impl SpellcasterCastingSpellGoal {
    /// Creates the goal, watching whatever the caster is attacking.
    #[must_use]
    pub(crate) const fn new() -> Self {
        Self {
            look_target: Mob::target,
        }
    }

    /// Replaces what the caster watches while casting.
    ///
    /// Vanilla parity: `Evoker.EvokerCastingSpellGoal`, which falls back to the
    /// wololo target when there is nothing to attack.
    #[must_use]
    pub(crate) const fn with_look_target(mut self, look_target: LookTarget) -> Self {
        self.look_target = look_target;
        self
    }
}

impl Default for SpellcasterCastingSpellGoal {
    fn default() -> Self {
        Self::new()
    }
}

impl Goal for SpellcasterCastingSpellGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::MOVE | GoalControls::LOOK
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        mob.as_spellcaster_illager()
            .is_some_and(|caster| caster.spell_casting_time() > 0)
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        mob.mob_base().navigation().lock().stop();
    }

    fn stop(&mut self, mob: &dyn PathfinderMob) {
        if let Some(caster) = mob.as_spellcaster_illager() {
            caster.set_is_casting_spell(IllagerSpell::None);
        }
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        let Some(watched) = (self.look_target)(mob) else {
            return;
        };
        Mob::look_at(
            mob,
            watched.as_ref(),
            mob.max_head_y_rot(),
            mob.max_head_x_rot(),
        );
    }
}
