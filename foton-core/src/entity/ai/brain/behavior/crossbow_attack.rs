//! Vanilla `CrossbowAttack`, the brain half of a mob's crossbow.

use foton_registry::data_components::vanilla_components::{
    CHARGED_PROJECTILES, ChargedProjectiles,
};
use foton_registry::vanilla_items;
use foton_utils::types::InteractionHand;

use super::{Behavior, BrainContext, TimedBehavior, utils};
use crate::behavior::items::crossbow_charge_duration;
use crate::entity::SharedEntity;
use crate::entity::ai::brain::memory::{MemoryModuleId, MemoryStatus, memory_module_types};
use crate::entity::ai::brain::position_tracker::PositionTracker;

/// Vanilla parity: the `1200` timeout of `CrossbowAttack`'s constructor.
const TIMEOUT: i32 = 1200;
/// Vanilla parity: the `20 +` of the `attackDelay` roll once charged.
const ATTACK_DELAY_MIN: i32 = 20;
/// Vanilla parity: the `nextInt(20)` of the same roll.
const ATTACK_DELAY_SPREAD: i32 = 20;
/// Vanilla parity: the `1.0F` power `CrossbowAttack` passes to `performRangedAttack`.
const RANGED_ATTACK_POWER: f32 = 1.0;

/// Where the crossbow is in its cycle.
///
/// Vanilla parity: `CrossbowAttack.CrossbowState`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CrossbowState {
    Uncharged,
    Charging,
    Charged,
    ReadyToAttack,
}

/// How the mob announces it is winding, and how it fires.
///
/// Vanilla reaches these through the `CrossbowAttackMob` interface; Foton's
/// brain behaviors take `&dyn PathfinderMob`, so the mob-specific halves arrive
/// as function pointers the way [`crate::entity::ai::goal`] takes its shots.
pub struct CrossbowAttackHooks {
    /// Vanilla parity: `CrossbowAttackMob.setChargingCrossbow`.
    pub set_charging_crossbow: fn(&BrainContext<'_>, bool),
    /// Vanilla parity: `CrossbowAttackMob.performRangedAttack`.
    pub perform_ranged_attack: fn(&BrainContext<'_>, &SharedEntity, f32),
}

/// Winds a crossbow and shoots the attack target with it.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.behavior.CrossbowAttack`.
/// Unlike [`crate::entity::ai::goal::RangedCrossbowAttackGoal`], which counts
/// the charge itself, this drives the real item pipeline: `start_using_item`
/// makes `CrossbowItem::on_use_tick` load the ammunition and set
/// `charged_projectiles`, which is what a piglin's arm pose reads back.
pub struct CrossbowAttack {
    hooks: CrossbowAttackHooks,
    entry_condition: [(MemoryModuleId, MemoryStatus); 2],
    crossbow_state: CrossbowState,
    attack_delay: i32,
}

impl CrossbowAttack {
    /// Vanilla parity: the no-argument `CrossbowAttack()` constructor.
    #[must_use]
    pub const fn new(hooks: CrossbowAttackHooks) -> Self {
        Self {
            hooks,
            entry_condition: [
                (
                    memory_module_types::LOOK_TARGET.id(),
                    MemoryStatus::Registered,
                ),
                (
                    memory_module_types::ATTACK_TARGET.id(),
                    MemoryStatus::ValuePresent,
                ),
            ],
            crossbow_state: CrossbowState::Uncharged,
            attack_delay: 0,
        }
    }

    /// Boxes this behavior for a brain's activity list.
    #[must_use]
    pub fn boxed(hooks: CrossbowAttackHooks) -> Box<dyn super::BehaviorControl> {
        Behavior::boxed(Self::new(hooks))
    }

    fn attack_target(ctx: &BrainContext<'_>) -> Option<SharedEntity> {
        ctx.brain()
            .get_memory(memory_module_types::ATTACK_TARGET)
            .and_then(|memory| memory.get())
    }

    fn holds_a_crossbow(ctx: &BrainContext<'_>) -> bool {
        ctx.mob()
            .is_holding(&mut |item| item.is(&vanilla_items::CROSSBOW))
    }

    /// Vanilla parity: `CrossbowAttack.crossbowAttack`.
    fn crossbow_attack(&mut self, ctx: &BrainContext<'_>, target: &SharedEntity) {
        match self.crossbow_state {
            CrossbowState::Uncharged => {
                let hand =
                    utils::weapon_holding_hand(ctx.mob(), |item| item.is(&vanilla_items::CROSSBOW));
                ctx.mob().start_using_item(hand);
                self.crossbow_state = CrossbowState::Charging;
                (self.hooks.set_charging_crossbow)(ctx, true);
            }
            CrossbowState::Charging => {
                if !ctx.mob().is_using_item() {
                    self.crossbow_state = CrossbowState::Uncharged;
                    return;
                }
                let Some(use_item) = ctx.mob().use_item() else {
                    self.crossbow_state = CrossbowState::Uncharged;
                    return;
                };
                if ctx.mob().ticks_using_item() >= crossbow_charge_duration(&use_item) {
                    ctx.mob().release_using_item();
                    self.crossbow_state = CrossbowState::Charged;
                    self.attack_delay =
                        ATTACK_DELAY_MIN + rand::random_range(0..ATTACK_DELAY_SPREAD);
                    (self.hooks.set_charging_crossbow)(ctx, false);
                }
            }
            CrossbowState::Charged => {
                self.attack_delay -= 1;
                if self.attack_delay == 0 {
                    self.crossbow_state = CrossbowState::ReadyToAttack;
                }
            }
            CrossbowState::ReadyToAttack => {
                (self.hooks.perform_ranged_attack)(ctx, target, RANGED_ATTACK_POWER);
                self.crossbow_state = CrossbowState::Uncharged;
            }
        }
    }
}

impl TimedBehavior for CrossbowAttack {
    fn entry_condition(&self) -> &[(MemoryModuleId, MemoryStatus)] {
        &self.entry_condition
    }

    fn duration(&self) -> (i32, i32) {
        (TIMEOUT, TIMEOUT)
    }

    /// Vanilla parity: `CrossbowAttack.checkExtraStartConditions`.
    fn check_extra_start_conditions(&mut self, ctx: &BrainContext<'_>) -> bool {
        let Some(target) = Self::attack_target(ctx) else {
            return false;
        };
        let Some(living_target) = target.as_living_entity() else {
            return false;
        };
        Self::holds_a_crossbow(ctx)
            && utils::can_see(ctx.brain(), target.as_ref())
            && utils::is_within_attack_range(ctx.mob(), living_target, 0)
    }

    fn can_still_use(&mut self, ctx: &BrainContext<'_>) -> bool {
        ctx.brain()
            .has_memory_value(memory_module_types::ATTACK_TARGET.id())
            && self.check_extra_start_conditions(ctx)
    }

    fn tick(&mut self, ctx: &BrainContext<'_>) {
        let Some(target) = Self::attack_target(ctx) else {
            return;
        };
        ctx.brain().set_memory(
            memory_module_types::LOOK_TARGET,
            PositionTracker::of_entity(&target, true),
        );
        self.crossbow_attack(ctx, &target);
    }

    /// Vanilla parity: `CrossbowAttack.stop`, which drops a half-wound charge
    /// and empties the crossbow so a mob that loses its target is not left
    /// holding a loaded one.
    fn stop(&mut self, ctx: &BrainContext<'_>) {
        if ctx.mob().is_using_item() {
            ctx.mob().stop_using_item();
        }
        if !Self::holds_a_crossbow(ctx) {
            return;
        }
        (self.hooks.set_charging_crossbow)(ctx, false);
        for hand in [InteractionHand::MainHand, InteractionHand::OffHand] {
            let mut item = ctx.mob().get_item_in_hand(hand);
            if !item.is(&vanilla_items::CROSSBOW) {
                continue;
            }
            item.set(CHARGED_PROJECTILES, ChargedProjectiles::empty());
            ctx.mob().set_item_in_hand(hand, item);
        }
    }

    fn debug_name(&self) -> &'static str {
        "CrossbowAttack"
    }
}
