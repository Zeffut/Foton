//! The behaviors only a piglin runs.
//!
//! Vanilla parity: the six small classes that sit beside `PiglinAi` in
//! `net.minecraft.world.entity.monster.piglin` -- `StartAdmiringItemIfSeen`,
//! `StopAdmiringIfItemTooFarAway`, `StopAdmiringIfTiredOfTryingToReachItem`,
//! `StopHoldingItemIfNoLongerAdmiring`, `RememberIfHoglinWasKilled` and
//! `StartHuntingHoglin`.

use foton_registry::data_components::vanilla_components::BLOCKS_ATTACKS;
use foton_registry::vanilla_entities;
use foton_utils::Downcast as _;
use foton_utils::types::InteractionHand;

use crate::entity::ai::brain::behavior::{BrainContext, Trigger, utils};
use crate::entity::ai::brain::memory::{MemoryModuleId, memory_module_types};
use crate::entity::entities::ItemEntity;

use super::entity::PiglinEntity;
use super::piglin_ai;
use crate::entity::{LivingEntity, Mob};

/// Reaches for the piglin behind a brain context.
///
/// Every behavior here is registered on a piglin's own brain, so the downcast
/// only fails if one is put on some other mob -- in which case doing nothing is
/// the right answer.
fn piglin<'a>(ctx: &'a BrainContext<'a>) -> Option<&'a PiglinEntity> {
    ctx.mob().downcast_ref::<PiglinEntity>()
}

/// Starts the admiring clock when a piece of gold is in sight.
///
/// Vanilla parity: `StartAdmiringItemIfSeen`.
pub struct StartAdmiringItemIfSeen {
    admire_duration: i64,
}

impl StartAdmiringItemIfSeen {
    /// Vanilla parity: `StartAdmiringItemIfSeen.create`.
    #[must_use]
    pub const fn new(admire_duration: i64) -> Self {
        Self { admire_duration }
    }
}

impl Trigger for StartAdmiringItemIfSeen {
    fn required_memories(&self) -> Vec<MemoryModuleId> {
        vec![
            memory_module_types::NEAREST_VISIBLE_WANTED_ITEM.id(),
            memory_module_types::ADMIRING_ITEM.id(),
            memory_module_types::ADMIRING_DISABLED.id(),
            memory_module_types::DISABLE_WALK_TO_ADMIRE_ITEM.id(),
        ]
    }

    fn trigger(&mut self, ctx: &BrainContext<'_>) -> bool {
        let brain = ctx.brain();
        if brain.has_memory_value(memory_module_types::ADMIRING_ITEM.id())
            || brain.has_memory_value(memory_module_types::ADMIRING_DISABLED.id())
            || brain.has_memory_value(memory_module_types::DISABLE_WALK_TO_ADMIRE_ITEM.id())
        {
            return false;
        }
        let Some(item_entity) = brain
            .get_memory(memory_module_types::NEAREST_VISIBLE_WANTED_ITEM)
            .and_then(|memory| memory.get())
        else {
            return false;
        };
        let Some(item_entity) = item_entity.downcast_ref::<ItemEntity>() else {
            return false;
        };
        if !piglin_ai::is_loved_item(&item_entity.get_item()) {
            return false;
        }

        brain.set_memory_with_expiry(
            memory_module_types::ADMIRING_ITEM,
            true,
            self.admire_duration,
        );
        true
    }

    fn debug_name(&self) -> &'static str {
        "StartAdmiringItemIfSeen"
    }
}

/// Gives up admiring once the item is out of reach.
///
/// Vanilla parity: `StopAdmiringIfItemTooFarAway`.
pub struct StopAdmiringIfItemTooFarAway {
    max_distance_to_item: f64,
}

impl StopAdmiringIfItemTooFarAway {
    /// Vanilla parity: `StopAdmiringIfItemTooFarAway.create`.
    #[must_use]
    pub fn new(max_distance_to_item: i32) -> Self {
        Self {
            max_distance_to_item: f64::from(max_distance_to_item),
        }
    }
}

impl Trigger for StopAdmiringIfItemTooFarAway {
    fn required_memories(&self) -> Vec<MemoryModuleId> {
        vec![
            memory_module_types::ADMIRING_ITEM.id(),
            memory_module_types::NEAREST_VISIBLE_WANTED_ITEM.id(),
        ]
    }

    fn trigger(&mut self, ctx: &BrainContext<'_>) -> bool {
        let brain = ctx.brain();
        if !brain.has_memory_value(memory_module_types::ADMIRING_ITEM.id()) {
            return false;
        }
        // A piglin already holding its prize is admiring that, not the ground.
        if !ctx
            .mob()
            .get_item_in_hand(InteractionHand::OffHand)
            .is_empty()
        {
            return false;
        }

        let still_near = brain
            .get_memory(memory_module_types::NEAREST_VISIBLE_WANTED_ITEM)
            .and_then(|memory| memory.get())
            .is_some_and(|item| {
                item.position().distance_squared(ctx.mob().position())
                    < self.max_distance_to_item * self.max_distance_to_item
            });
        if still_near {
            return false;
        }

        brain.erase_memory(memory_module_types::ADMIRING_ITEM.id());
        true
    }

    fn debug_name(&self) -> &'static str {
        "StopAdmiringIfItemTooFarAway"
    }
}

/// Gives up on an item it has spent too long failing to reach.
///
/// Vanilla parity: `StopAdmiringIfTiredOfTryingToReachItem`. The
/// `DISABLE_WALK_TO_ADMIRE_ITEM` it sets is what stops a piglin standing
/// against a wall forever, staring at gold on the other side.
pub struct StopAdmiringIfTiredOfTryingToReachItem {
    max_time_to_reach_item: i32,
    disable_time: i64,
}

impl StopAdmiringIfTiredOfTryingToReachItem {
    /// Vanilla parity: `StopAdmiringIfTiredOfTryingToReachItem.create`.
    #[must_use]
    pub const fn new(max_time_to_reach_item: i32, disable_time: i64) -> Self {
        Self {
            max_time_to_reach_item,
            disable_time,
        }
    }
}

impl Trigger for StopAdmiringIfTiredOfTryingToReachItem {
    fn required_memories(&self) -> Vec<MemoryModuleId> {
        vec![
            memory_module_types::ADMIRING_ITEM.id(),
            memory_module_types::NEAREST_VISIBLE_WANTED_ITEM.id(),
            memory_module_types::TIME_TRYING_TO_REACH_ADMIRE_ITEM.id(),
            memory_module_types::DISABLE_WALK_TO_ADMIRE_ITEM.id(),
        ]
    }

    fn trigger(&mut self, ctx: &BrainContext<'_>) -> bool {
        let brain = ctx.brain();
        if !brain.has_memory_value(memory_module_types::ADMIRING_ITEM.id())
            || !brain.has_memory_value(memory_module_types::NEAREST_VISIBLE_WANTED_ITEM.id())
        {
            return false;
        }
        if !ctx
            .mob()
            .get_item_in_hand(InteractionHand::OffHand)
            .is_empty()
        {
            return false;
        }

        let Some(time_trying) =
            brain.get_memory(memory_module_types::TIME_TRYING_TO_REACH_ADMIRE_ITEM)
        else {
            brain.set_memory(memory_module_types::TIME_TRYING_TO_REACH_ADMIRE_ITEM, 0);
            return true;
        };

        if time_trying > self.max_time_to_reach_item {
            brain.erase_memory(memory_module_types::ADMIRING_ITEM.id());
            brain.erase_memory(memory_module_types::TIME_TRYING_TO_REACH_ADMIRE_ITEM.id());
            brain.set_memory_with_expiry(
                memory_module_types::DISABLE_WALK_TO_ADMIRE_ITEM,
                true,
                self.disable_time,
            );
        } else {
            brain.set_memory(
                memory_module_types::TIME_TRYING_TO_REACH_ADMIRE_ITEM,
                time_trying + 1,
            );
        }
        true
    }

    fn debug_name(&self) -> &'static str {
        "StopAdmiringIfTiredOfTryingToReachItem"
    }
}

/// Puts the admired item away -- or barters it -- once admiring is over.
///
/// Vanilla parity: `StopHoldingItemIfNoLongerAdmiring`. This is the behavior
/// that actually trades: a gold ingot in the off hand at the moment admiring
/// expires becomes a barter roll.
pub struct StopHoldingItemIfNoLongerAdmiring;

impl Trigger for StopHoldingItemIfNoLongerAdmiring {
    fn required_memories(&self) -> Vec<MemoryModuleId> {
        vec![memory_module_types::ADMIRING_ITEM.id()]
    }

    fn trigger(&mut self, ctx: &BrainContext<'_>) -> bool {
        if ctx
            .brain()
            .has_memory_value(memory_module_types::ADMIRING_ITEM.id())
        {
            return false;
        }
        let Some(piglin) = piglin(ctx) else {
            return false;
        };
        let offhand = piglin.get_item_in_hand(InteractionHand::OffHand);
        // Vanilla exempts anything that blocks attacks -- a shield taken off a
        // player is kept up rather than put away.
        if offhand.is_empty() || offhand.get(BLOCKS_ATTACKS).is_some() {
            return false;
        }

        piglin_ai::stop_holding_off_hand_item(piglin, true);
        true
    }

    fn debug_name(&self) -> &'static str {
        "StopHoldingItemIfNoLongerAdmiring"
    }
}

/// Notes a successful hunt so the pack leaves the rest of the stable alone.
///
/// Vanilla parity: `RememberIfHoglinWasKilled`.
pub struct RememberIfHoglinWasKilled;

impl Trigger for RememberIfHoglinWasKilled {
    fn required_memories(&self) -> Vec<MemoryModuleId> {
        vec![
            memory_module_types::ATTACK_TARGET.id(),
            memory_module_types::HUNTED_RECENTLY.id(),
        ]
    }

    fn trigger(&mut self, ctx: &BrainContext<'_>) -> bool {
        let brain = ctx.brain();
        let Some(target) = brain
            .get_memory(memory_module_types::ATTACK_TARGET)
            .and_then(|memory| memory.get())
        else {
            return false;
        };
        let is_dying_hoglin = utils::is_of_type(target.as_ref(), &vanilla_entities::HOGLIN)
            && target
                .as_living_entity()
                .is_some_and(LivingEntity::is_dead_or_dying);
        if is_dying_hoglin {
            brain.set_memory_with_expiry(
                memory_module_types::HUNTED_RECENTLY,
                true,
                piglin_ai::sample_time_between_hunts(),
            );
        }
        true
    }

    fn debug_name(&self) -> &'static str {
        "RememberIfHoglinWasKilled"
    }
}

/// Starts a hunt, and takes the neighbors along.
///
/// Vanilla parity: `StartHuntingHoglin`. The check that no visible adult has
/// hunted recently is what makes a bastion's piglins hunt as a group and then
/// leave the hoglins alone for a while.
pub struct StartHuntingHoglin;

impl Trigger for StartHuntingHoglin {
    fn required_memories(&self) -> Vec<MemoryModuleId> {
        vec![
            memory_module_types::NEAREST_VISIBLE_HUNTABLE_HOGLIN.id(),
            memory_module_types::ANGRY_AT.id(),
            memory_module_types::HUNTED_RECENTLY.id(),
            memory_module_types::NEAREST_VISIBLE_ADULT_PIGLINS.id(),
        ]
    }

    fn trigger(&mut self, ctx: &BrainContext<'_>) -> bool {
        let brain = ctx.brain();
        if brain.has_memory_value(memory_module_types::ANGRY_AT.id())
            || brain.has_memory_value(memory_module_types::HUNTED_RECENTLY.id())
        {
            return false;
        }
        let Some(target) = brain
            .get_memory(memory_module_types::NEAREST_VISIBLE_HUNTABLE_HOGLIN)
            .and_then(|memory| memory.get())
        else {
            return false;
        };
        if ctx.mob().is_baby() {
            return false;
        }

        let neighbors: Vec<_> = brain
            .get_memory(memory_module_types::NEAREST_VISIBLE_ADULT_PIGLINS)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|remembered| remembered.get())
            .collect();
        let any_hunted_recently = neighbors.iter().any(|neighbor| {
            neighbor
                .as_mob()
                .and_then(Mob::brain)
                .is_some_and(|neighbor_brain| {
                    neighbor_brain.has_memory_value(memory_module_types::HUNTED_RECENTLY.id())
                })
        });
        if any_hunted_recently {
            return false;
        }

        piglin_ai::set_anger_target(ctx.world(), brain, ctx.mob(), &target);
        piglin_ai::dont_kill_any_more_hoglins_for_a_while(brain);
        piglin_ai::broadcast_anger_target(ctx.world(), brain, ctx.mob(), &target);
        for neighbor in &neighbors {
            if let Some(neighbor_brain) = neighbor.as_mob().and_then(Mob::brain) {
                piglin_ai::dont_kill_any_more_hoglins_for_a_while(neighbor_brain);
            }
        }
        true
    }

    fn debug_name(&self) -> &'static str {
        "StartHuntingHoglin"
    }
}
