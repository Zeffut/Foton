//! The illusioner's two spells.
//!
//! Vanilla parity: `Illusioner.IllusionerMirrorSpellGoal` and
//! `Illusioner.IllusionerBlindnessSpellGoal`. Neither does damage: one blinds
//! the target for twenty seconds, the other turns the illusioner invisible for
//! a minute and leaves the client drawing four decoys of it.

use steel_registry::{sound_events, vanilla_mob_effects};
use steel_utils::types::Difficulty;

use crate::entity::ai::goal::{
    DEFAULT_CAST_WARMUP_TIME, Goal, GoalControls, SpellcasterUseSpellBase,
};
use crate::entity::{IllagerSpell, MobEffectInstance, PathfinderMob};

/// Ticks the illusioner keeps its hands up while blinding.
const BLINDNESS_CASTING_TIME: i32 = 20;

/// Ticks between two blindings.
const BLINDNESS_CASTING_INTERVAL: i32 = 180;

/// How long the target stays blind.
///
/// Vanilla parity: the `new MobEffectInstance(BLINDNESS, 400)` of
/// `performSpellCasting`.
const BLINDNESS_DURATION: i32 = 400;

/// Ticks the illusioner keeps its hands up while vanishing.
const MIRROR_CASTING_TIME: i32 = 20;

/// Ticks between two vanishings.
const MIRROR_CASTING_INTERVAL: i32 = 340;

/// How long the illusioner stays invisible.
///
/// Vanilla parity: the `new MobEffectInstance(INVISIBILITY, 1200)` of
/// `performSpellCasting`.
const MIRROR_DURATION: i32 = 1200;

/// Blinds the target.
///
/// Vanilla parity: `Illusioner.IllusionerBlindnessSpellGoal`. It fires once per
/// target -- the illusioner remembers who it last blinded -- and only above
/// normal local difficulty.
pub(super) struct IllusionerBlindnessSpellGoal {
    base: SpellcasterUseSpellBase,
    /// The last target this goal blinded, so it does not blind them twice.
    last_target_id: i32,
}

impl IllusionerBlindnessSpellGoal {
    /// Creates the goal.
    #[must_use]
    pub(super) const fn new() -> Self {
        Self {
            base: SpellcasterUseSpellBase::new(
                IllagerSpell::Blindness,
                DEFAULT_CAST_WARMUP_TIME,
                BLINDNESS_CASTING_TIME,
                BLINDNESS_CASTING_INTERVAL,
                Some(&sound_events::ENTITY_ILLUSIONER_PREPARE_BLINDNESS),
            ),
            last_target_id: 0,
        }
    }
}

impl Goal for IllusionerBlindnessSpellGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::EMPTY
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        if !self.base.can_use(mob) {
            return false;
        }
        let Some(target) = mob.target() else {
            return false;
        };
        if target.id() == self.last_target_id {
            return false;
        }
        let Some(world) = mob.level() else {
            return false;
        };
        world
            .get_current_difficulty_at(mob.block_position())
            .is_harder_than(f32::from(Difficulty::Normal as u8))
    }

    fn can_continue_to_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        self.base.can_continue_to_use(mob)
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        self.base.start(mob);
        if let Some(target) = mob.target() {
            self.last_target_id = target.id();
        }
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        self.base.tick(mob, |mob| {
            let Some(target) = mob.target() else {
                return;
            };
            let Some(living) = target.as_living_entity() else {
                return;
            };
            living.add_mob_effect(MobEffectInstance::with_duration(
                vanilla_mob_effects::BLINDNESS,
                BLINDNESS_DURATION,
                0,
            ));
        });
    }
}

/// Turns the illusioner invisible.
///
/// Vanilla parity: `Illusioner.IllusionerMirrorSpellGoal`. The four decoys a
/// player sees are drawn entirely by the client off the invisibility; the
/// server only casts the effect.
pub(super) struct IllusionerMirrorSpellGoal {
    base: SpellcasterUseSpellBase,
}

impl IllusionerMirrorSpellGoal {
    /// Creates the goal.
    #[must_use]
    pub(super) const fn new() -> Self {
        Self {
            base: SpellcasterUseSpellBase::new(
                IllagerSpell::Disappear,
                DEFAULT_CAST_WARMUP_TIME,
                MIRROR_CASTING_TIME,
                MIRROR_CASTING_INTERVAL,
                Some(&sound_events::ENTITY_ILLUSIONER_PREPARE_MIRROR),
            ),
        }
    }
}

impl Goal for IllusionerMirrorSpellGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::EMPTY
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        self.base.can_use(mob) && !mob.has_mob_effect(vanilla_mob_effects::INVISIBILITY)
    }

    fn can_continue_to_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        self.base.can_continue_to_use(mob)
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        self.base.start(mob);
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        self.base.tick(mob, |mob| {
            mob.add_mob_effect(MobEffectInstance::with_duration(
                vanilla_mob_effects::INVISIBILITY,
                MIRROR_DURATION,
                0,
            ));
        });
    }
}
