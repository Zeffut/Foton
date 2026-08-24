//! Ranged crossbow attack goal.
//!
//! Vanilla parity: `RangedCrossbowAttackGoal`. A crossbow is not a bow: the
//! mob has to stop, wind the weapon, wait, and only then shoot, and it walks
//! at half speed while the crossbow is loaded. The four-state machine below is
//! the whole of a pillager's rhythm, and the pause between the click and the
//! bolt is the window a player has to break line of sight.
//!
//! Vanilla drives the winding through the item-use pipeline --
//! `startUsingItem`, `onUseTick`, `releaseUsingItem`. Steel now ticks that
//! pipeline for every living entity, but `CrossbowItem`'s own hooks still take
//! a player: `on_use_tick` returns early for anything else, and the shooting
//! path reads ammunition out of a `PlayerInventory`. Until those are widened
//! and `Mob.getProjectile` exists, the goal counts the charge itself against
//! the same `CrossbowItem.getChargeDuration`; the state machine, the timings
//! and the synced charging flag are unchanged. What is lost is the loading
//! sound triple, which vanilla plays from `onUseTick`, and any ammunition the
//! mob might have been carrying: the shot always leaves as one plain arrow,
//! which is what `Mob.getProjectile` falls back to for an empty quiver.

use super::selector::{Goal, GoalControls};
use crate::behavior::items::crossbow_charge_duration;
use crate::entity::{LivingEntity, Mob, PathfinderMob, SharedEntity};
use crate::inventory::equipment::EquipmentSlot;
use steel_registry::item_stack::ItemStack;
use steel_registry::vanilla_items;

/// Ticks the target must have been visible before the mob stops walking.
///
/// Vanilla parity: the `seeTime < 5` of `tick`.
const SEEN_TIME_BEFORE_STANDING: i32 = 5;

/// Shortest pause between finishing the wind and pulling the trigger.
///
/// Vanilla parity: the `20 + random.nextInt(20)` of the charging branch.
const ATTACK_DELAY_MIN: i32 = 20;

/// Width of the random part of that pause.
const ATTACK_DELAY_SPREAD: i32 = 20;

/// Shortest gap between two path updates while closing in, in ticks.
///
/// Vanilla parity: `RangedCrossbowAttackGoal.PATHFINDING_DELAY_RANGE`, a
/// `TimeUtil.rangeOfSeconds(1, 2)`.
const PATHFINDING_DELAY_MIN: i32 = 20;

/// Longest gap between two path updates while closing in, in ticks.
const PATHFINDING_DELAY_MAX: i32 = 40;

/// How sharply the mob turns to keep its target in view.
///
/// Vanilla parity: the `setLookAt(target, 30.0F, 30.0F)` of `tick`.
const LOOK_SPEED: f32 = 30.0;

/// How much a loaded mob slows down.
///
/// Vanilla parity: the `speedModifier * 0.5` of the non-`canRun` branch.
const LOADED_SPEED_SCALE: f64 = 0.5;

/// Where the crossbow is in its cycle.
///
/// Vanilla parity: `RangedCrossbowAttackGoal.CrossbowState`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CrossbowState {
    /// Empty; the mob may close the distance at full speed.
    Uncharged,
    /// Winding.
    Charging,
    /// Loaded, waiting out the pause before the shot.
    Charged,
    /// Loaded and out of patience.
    ReadyToAttack,
}

/// Winds a crossbow and shoots it.
///
/// Vanilla parity: `RangedCrossbowAttackGoal`. Vanilla reaches the mob through
/// the `CrossbowAttackMob` interface; Steel's goals take the mob-specific half
/// as function pointers, the way [`super::RangedBowAttackGoal`] takes its shot.
pub(crate) struct RangedCrossbowAttackGoal {
    /// Speed the mob closes the distance at while unloaded.
    speed_modifier: f64,
    /// Squared distance inside which the mob stops walking and shoots.
    attack_radius_sqr: f64,
    /// Where the crossbow is in its cycle.
    crossbow_state: CrossbowState,
    /// Ticks the target has been continuously visible, counted down when not.
    seen_time: i32,
    /// Ticks left of the pause between winding and shooting.
    attack_delay: i32,
    /// Ticks left before the mob repaths.
    update_path_delay: i32,
    /// Ticks spent winding so far.
    charge_time: i32,
    /// Sets the synced flag the client winds the model from.
    set_charging_crossbow: fn(&dyn PathfinderMob, bool),
    /// Shoots the loaded crossbow.
    perform_ranged_attack: fn(&dyn PathfinderMob, &SharedEntity),
}

impl RangedCrossbowAttackGoal {
    /// Creates the goal for one crossbow user.
    #[must_use]
    pub(crate) fn new(
        speed_modifier: f64,
        attack_radius: f32,
        set_charging_crossbow: fn(&dyn PathfinderMob, bool),
        perform_ranged_attack: fn(&dyn PathfinderMob, &SharedEntity),
    ) -> Self {
        Self {
            speed_modifier,
            attack_radius_sqr: f64::from(attack_radius) * f64::from(attack_radius),
            crossbow_state: CrossbowState::Uncharged,
            seen_time: 0,
            attack_delay: 0,
            update_path_delay: 0,
            charge_time: 0,
            set_charging_crossbow,
            perform_ranged_attack,
        }
    }

    /// Returns whether the mob still has a crossbow in hand.
    ///
    /// Vanilla parity: `isHoldingCrossbow`.
    fn is_holding_crossbow(mob: &dyn PathfinderMob) -> bool {
        mob.is_holding(&mut |item| item.is(&vanilla_items::CROSSBOW))
    }

    /// Returns whether the mob has a target worth shooting.
    ///
    /// Vanilla parity: `isValidTarget`.
    fn is_valid_target(mob: &dyn PathfinderMob) -> bool {
        mob.target().is_some_and(|target| {
            target
                .as_living_entity()
                .is_some_and(LivingEntity::is_alive)
        })
    }

    /// Returns whether the mob may run rather than walk.
    ///
    /// Vanilla parity: `canRun`. Only an empty crossbow lets a pillager sprint.
    const fn can_run(&self) -> bool {
        matches!(self.crossbow_state, CrossbowState::Uncharged)
    }

    /// Returns how long this mob's crossbow takes to wind.
    fn charge_duration(mob: &dyn PathfinderMob) -> i32 {
        let mut crossbow = ItemStack::empty();
        mob.with_equipment_slot(EquipmentSlot::MainHand, &mut |item| {
            if item.is(&vanilla_items::CROSSBOW) {
                crossbow = item.clone();
            }
        });
        if crossbow.is_empty() {
            mob.with_equipment_slot(EquipmentSlot::OffHand, &mut |item| {
                if item.is(&vanilla_items::CROSSBOW) {
                    crossbow = item.clone();
                }
            });
        }
        crossbow_charge_duration(&crossbow)
    }
}

impl Goal for RangedCrossbowAttackGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::MOVE | GoalControls::LOOK
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        Self::is_valid_target(mob) && Self::is_holding_crossbow(mob)
    }

    fn can_continue_to_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        Self::is_valid_target(mob)
            && (self.can_use(mob) || !mob.mob_base().navigation().lock().is_done())
            && Self::is_holding_crossbow(mob)
    }

    fn stop(&mut self, mob: &dyn PathfinderMob) {
        mob.set_aggressive(false);
        let _ = mob.set_target(None);
        self.seen_time = 0;
        if self.crossbow_state == CrossbowState::Charging {
            (self.set_charging_crossbow)(mob, false);
        }
        self.crossbow_state = CrossbowState::Uncharged;
        self.charge_time = 0;
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        let Some(target) = mob.target() else {
            return;
        };

        let has_line_of_sight = mob.has_line_of_sight_cached(target.as_ref());
        if has_line_of_sight != (self.seen_time > 0) {
            self.seen_time = 0;
        }
        if has_line_of_sight {
            self.seen_time += 1;
        } else {
            self.seen_time -= 1;
        }

        let distance_sqr = mob.position().distance_squared(target.position());
        let needs_to_move = (distance_sqr > self.attack_radius_sqr
            || self.seen_time < SEEN_TIME_BEFORE_STANDING)
            && self.attack_delay == 0;
        if needs_to_move {
            self.update_path_delay -= 1;
            if self.update_path_delay <= 0 {
                let speed = if self.can_run() {
                    self.speed_modifier
                } else {
                    self.speed_modifier * LOADED_SPEED_SCALE
                };
                mob.move_to_pos(target.position(), speed);
                self.update_path_delay =
                    rand::random_range(PATHFINDING_DELAY_MIN..=PATHFINDING_DELAY_MAX);
            }
        } else {
            self.update_path_delay = 0;
            mob.mob_base().navigation().lock().stop();
        }

        Mob::look_at(mob, target.as_ref(), LOOK_SPEED, LOOK_SPEED);

        match self.crossbow_state {
            CrossbowState::Uncharged => {
                if !needs_to_move {
                    self.crossbow_state = CrossbowState::Charging;
                    self.charge_time = 0;
                    (self.set_charging_crossbow)(mob, true);
                }
            }
            CrossbowState::Charging => {
                self.charge_time += 1;
                if self.charge_time >= Self::charge_duration(mob) {
                    self.crossbow_state = CrossbowState::Charged;
                    self.attack_delay =
                        ATTACK_DELAY_MIN + rand::random_range(0..ATTACK_DELAY_SPREAD);
                    (self.set_charging_crossbow)(mob, false);
                }
            }
            CrossbowState::Charged => {
                self.attack_delay -= 1;
                if self.attack_delay == 0 {
                    self.crossbow_state = CrossbowState::ReadyToAttack;
                }
            }
            CrossbowState::ReadyToAttack => {
                if has_line_of_sight {
                    (self.perform_ranged_attack)(mob, &target);
                    self.crossbow_state = CrossbowState::Uncharged;
                }
            }
        }
    }
}
