//! Shared body of the five illager spell goals.
//!
//! Vanilla parity: `SpellcasterIllager.SpellcasterUseSpellGoal`. Every spell
//! runs the same three-part shape -- a warmup the caster stands through, the
//! moment the spell lands, and a cooldown before the next one -- and differs
//! only in four numbers and a body. Vanilla expresses that with an abstract
//! inner class; Steel expresses it with a base struct each concrete spell goal
//! embeds, the way `TargetGoalBase` is embedded by the target goals.

use steel_registry::sound_event::SoundEventRef;

use super::reduced_tick_delay;
use crate::entity::{IllagerSpell, LivingEntity, PathfinderMob};

/// Warmup vanilla gives a spell that does not ask for its own.
///
/// Vanilla parity: `SpellcasterUseSpellGoal.getCastWarmupTime`.
pub(crate) const DEFAULT_CAST_WARMUP_TIME: i32 = 20;

/// The parts of a spell goal that are the same for every spell.
///
/// Vanilla parity: `SpellcasterIllager.SpellcasterUseSpellGoal`.
pub(crate) struct SpellcasterUseSpellBase {
    /// Which spell the client is told to draw.
    spell: IllagerSpell,
    /// Ticks between the cast starting and the spell landing.
    cast_warmup_time: i32,
    /// Ticks the caster keeps its arms up for.
    casting_time: i32,
    /// Ticks before this spell may be cast again.
    casting_interval: i32,
    /// Sound played as the cast begins, if the spell has one.
    prepare_sound: Option<SoundEventRef>,
    /// Ticks left of the current warmup.
    attack_warmup_delay: i32,
    /// The caster tick count this spell becomes available again on.
    next_attack_tick_count: i32,
}

impl SpellcasterUseSpellBase {
    /// Creates the shared half of one spell goal.
    #[must_use]
    pub(crate) const fn new(
        spell: IllagerSpell,
        cast_warmup_time: i32,
        casting_time: i32,
        casting_interval: i32,
        prepare_sound: Option<SoundEventRef>,
    ) -> Self {
        Self {
            spell,
            cast_warmup_time,
            casting_time,
            casting_interval,
            prepare_sound,
            attack_warmup_delay: 0,
            next_attack_tick_count: 0,
        }
    }

    /// Returns ticks left of the current warmup.
    ///
    /// Vanilla parity: the `attackWarmupDelay` two subclasses read directly.
    #[must_use]
    pub(crate) const fn attack_warmup_delay(&self) -> i32 {
        self.attack_warmup_delay
    }

    /// Returns whether the cooldown has run out.
    ///
    /// Vanilla parity: the `tickCount >= nextAttackTickCount` of `canUse`,
    /// split out because the wololo goal tests it without the target check.
    #[must_use]
    pub(crate) fn is_off_cooldown(&self, mob: &dyn PathfinderMob) -> bool {
        mob.tick_count() >= self.next_attack_tick_count
    }

    /// Returns vanilla `SpellcasterUseSpellGoal.canUse`.
    #[must_use]
    pub(crate) fn can_use(&self, mob: &dyn PathfinderMob) -> bool {
        let target_alive = mob.target().is_some_and(|target| {
            target
                .as_living_entity()
                .is_some_and(LivingEntity::is_alive)
        });
        let Some(caster) = mob.as_spellcaster_illager() else {
            return false;
        };
        target_alive && !caster.is_casting_spell() && self.is_off_cooldown(mob)
    }

    /// Returns vanilla `SpellcasterUseSpellGoal.canContinueToUse`.
    #[must_use]
    pub(crate) fn can_continue_to_use(&self, mob: &dyn PathfinderMob) -> bool {
        let target_alive = mob.target().is_some_and(|target| {
            target
                .as_living_entity()
                .is_some_and(LivingEntity::is_alive)
        });
        target_alive && self.attack_warmup_delay > 0
    }

    /// Runs vanilla `SpellcasterUseSpellGoal.start`.
    ///
    /// The warmup is halved because a spell goal does not ask to be ticked
    /// every tick, and a mob's goals only tick on even ticks otherwise; that is
    /// vanilla's `Goal.adjustedTickDelay`.
    pub(crate) fn start(&mut self, mob: &dyn PathfinderMob) {
        let Some(caster) = mob.as_spellcaster_illager() else {
            return;
        };
        self.attack_warmup_delay = reduced_tick_delay(self.cast_warmup_time);
        caster.set_spell_casting_time(self.casting_time);
        self.next_attack_tick_count = mob.tick_count() + self.casting_interval;
        if let Some(sound) = self.prepare_sound {
            mob.play_sound(sound, 1.0, 1.0);
        }
        caster.set_is_casting_spell(self.spell);
    }

    /// Runs vanilla `SpellcasterUseSpellGoal.tick`, firing `perform` on the
    /// tick the warmup runs out.
    pub(crate) fn tick(
        &mut self,
        mob: &dyn PathfinderMob,
        perform: impl FnOnce(&dyn PathfinderMob),
    ) {
        self.attack_warmup_delay -= 1;
        if self.attack_warmup_delay != 0 {
            return;
        }
        perform(mob);
        if let Some(caster) = mob.as_spellcaster_illager() {
            mob.play_sound(caster.casting_sound_event(), 1.0, 1.0);
        }
    }
}
