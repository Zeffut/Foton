//! Showing trades off, and following the player who is buying.
//!
//! Vanilla parity: `ShowTradesToPlayer` and `LookAndFollowTradingPlayerSink`.

use steel_registry::equipment::EquipmentSlot;
use steel_registry::item_stack::ItemStack;
use steel_utils::types::InteractionHand;

use super::villager;
use crate::entity::ai::brain::behavior::{
    BrainContext, MemoryModuleId, MemoryStatus, TimedBehavior,
};
use crate::entity::ai::brain::memory::{WalkTarget, memory_module_types};
use crate::entity::ai::brain::position_tracker::PositionTracker;
use crate::entity::{Entity as _, LivingEntity, SharedEntity};
use crate::trading::Merchant as _;

/// Vanilla parity: `ShowTradesToPlayer.MAX_LOOK_TIME`.
const MAX_LOOK_TIME: i32 = 900;
/// Vanilla parity: `ShowTradesToPlayer.STARTING_LOOK_TIME`.
const STARTING_LOOK_TIME: i32 = 40;
/// How many ticks each offer stays in the villager's hand.
///
/// Vanilla parity: the `++this.cycleCounter >= 40` of `displayCyclingItems`.
const TICKS_PER_DISPLAYED_ITEM: i32 = 40;
/// Vanilla parity: the `body.distanceToSqr(target) <= 17.0`.
const MAX_SHOW_DISTANCE_SQR: f64 = 17.0;
/// Vanilla parity: the `setDropChance(MAINHAND, 0.085F)` a cleared hand gets.
const DEFAULT_MAIN_HAND_DROP_CHANCE: f32 = 0.085;

/// Waves the trades a player could afford in front of them.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.behavior.ShowTradesToPlayer`.
/// The villager holds up each offer the item in the player's hand pays for,
/// cycling every two seconds.
pub struct ShowTradesToPlayer {
    min_duration: i32,
    max_duration: i32,
    player_item_stack: Option<ItemStack>,
    display_items: Vec<ItemStack>,
    cycle_counter: i32,
    display_index: usize,
    look_time: i32,
}

impl ShowTradesToPlayer {
    /// Vanilla parity: `new ShowTradesToPlayer(int, int)`.
    #[must_use]
    pub const fn new(min_duration: i32, max_duration: i32) -> Self {
        Self {
            min_duration,
            max_duration,
            player_item_stack: None,
            display_items: Vec::new(),
            cycle_counter: 0,
            display_index: 0,
            look_time: 0,
        }
    }

    /// The player being shown to, if the memory still names a live one nearby.
    ///
    /// Vanilla parity: `ShowTradesToPlayer.checkExtraStartConditions`.
    fn audience(ctx: &BrainContext<'_>) -> Option<SharedEntity> {
        let target = ctx
            .brain()
            .get_memory(memory_module_types::INTERACTION_TARGET)?
            .get()?;
        let villager = villager(ctx)?;
        let living = target.as_living_entity()?;
        let is_player = target.as_player().is_some();
        let close_enough =
            target.position().distance_squared(villager.position()) <= MAX_SHOW_DISTANCE_SQR;
        (is_player
            && LivingEntity::is_alive(villager)
            && LivingEntity::is_alive(living)
            && !LivingEntity::is_baby(villager)
            && close_enough)
            .then_some(target)
    }

    /// Vanilla parity: `ShowTradesToPlayer.lookAtTarget`.
    fn look_at(ctx: &BrainContext<'_>, target: &SharedEntity) {
        ctx.brain().set_memory(
            memory_module_types::LOOK_TARGET,
            PositionTracker::of_entity(target, true),
        );
    }

    /// Vanilla parity: `ShowTradesToPlayer.findItemsToDisplay`.
    fn find_items_to_display(&mut self, ctx: &BrainContext<'_>, target: &SharedEntity) {
        let Some(player) = target.as_living_entity() else {
            return;
        };
        let held = player.get_item_in_hand(InteractionHand::MainHand);
        let changed = self
            .player_item_stack
            .as_ref()
            .is_none_or(|previous| !ItemStack::is_same_item(previous, &held));
        if !changed {
            return;
        }
        self.player_item_stack = Some(held.clone());
        self.display_items.clear();
        if held.is_empty() {
            return;
        }

        let Some(villager) = villager(ctx) else {
            return;
        };
        for offer in villager.offers().iter() {
            if offer.is_out_of_stock() {
                continue;
            }
            if ItemStack::is_same_item(&held, &offer.cost_a())
                || ItemStack::is_same_item(&held, &offer.cost_b())
            {
                self.display_items.push(offer.assemble());
            }
        }
        if let Some(first) = self.display_items.first().cloned() {
            self.look_time = MAX_LOOK_TIME;
            display_as_held_item(ctx, first);
        }
    }

    /// Vanilla parity: `ShowTradesToPlayer.displayCyclingItems`.
    fn display_cycling_items(&mut self, ctx: &BrainContext<'_>) {
        if self.display_items.len() < 2 {
            return;
        }
        self.cycle_counter += 1;
        if self.cycle_counter < TICKS_PER_DISPLAYED_ITEM {
            return;
        }
        self.display_index += 1;
        self.cycle_counter = 0;
        if self.display_index > self.display_items.len() - 1 {
            self.display_index = 0;
        }
        let stack = self.display_items[self.display_index].clone();
        display_as_held_item(ctx, stack);
    }
}

/// Vanilla parity: the `ImmutableMap.of(INTERACTION_TARGET, VALUE_PRESENT)`.
const INTERACTION_TARGET_PRESENT: &[(MemoryModuleId, MemoryStatus)] = &[(
    memory_module_types::INTERACTION_TARGET.id(),
    MemoryStatus::ValuePresent,
)];

impl TimedBehavior for ShowTradesToPlayer {
    fn entry_condition(&self) -> &[(MemoryModuleId, MemoryStatus)] {
        INTERACTION_TARGET_PRESENT
    }

    fn duration(&self) -> (i32, i32) {
        (self.min_duration, self.max_duration)
    }

    fn check_extra_start_conditions(&mut self, ctx: &BrainContext<'_>) -> bool {
        Self::audience(ctx).is_some()
    }

    fn can_still_use(&mut self, ctx: &BrainContext<'_>) -> bool {
        self.look_time > 0 && Self::audience(ctx).is_some()
    }

    fn start(&mut self, ctx: &BrainContext<'_>) {
        if let Some(target) = Self::audience(ctx) {
            Self::look_at(ctx, &target);
        }
        self.cycle_counter = 0;
        self.display_index = 0;
        self.look_time = STARTING_LOOK_TIME;
    }

    fn tick(&mut self, ctx: &BrainContext<'_>) {
        if let Some(target) = Self::audience(ctx) {
            Self::look_at(ctx, &target);
            self.find_items_to_display(ctx, &target);
        }
        if self.display_items.is_empty() {
            clear_held_item(ctx);
            self.look_time = self.look_time.min(STARTING_LOOK_TIME);
        } else {
            self.display_cycling_items(ctx);
        }
        self.look_time -= 1;
    }

    fn stop(&mut self, ctx: &BrainContext<'_>) {
        ctx.brain()
            .erase_memory(memory_module_types::INTERACTION_TARGET.id());
        clear_held_item(ctx);
        self.player_item_stack = None;
    }

    fn debug_name(&self) -> &'static str {
        "ShowTradesToPlayer"
    }
}

/// Vanilla parity: the private `ShowTradesToPlayer.clearHeldItem`.
fn clear_held_item(ctx: &BrainContext<'_>) {
    let mob = ctx.mob();
    mob.set_item_slot(EquipmentSlot::MainHand, ItemStack::empty());
    mob.set_drop_chance(EquipmentSlot::MainHand, DEFAULT_MAIN_HAND_DROP_CHANCE);
}

/// Vanilla parity: the private `ShowTradesToPlayer.displayAsHeldItem`.
fn display_as_held_item(ctx: &BrainContext<'_>, stack: ItemStack) {
    let mob = ctx.mob();
    mob.set_item_slot(EquipmentSlot::MainHand, stack);
    mob.set_drop_chance(EquipmentSlot::MainHand, 0.0);
}

/// Vanilla parity: the `body.distanceToSqr(tradingPlayer) <= 16.0`.
const MAX_FOLLOW_DISTANCE_SQR: f64 = 16.0;
/// Vanilla parity: the `new WalkTarget(..., speedModifier, 2)`.
const FOLLOW_CLOSE_ENOUGH_DIST: i32 = 2;

/// Keeps a villager facing and shuffling toward the player it is trading with.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.behavior.LookAndFollowTradingPlayerSink`.
pub struct LookAndFollowTradingPlayerSink {
    speed_modifier: f64,
}

impl LookAndFollowTradingPlayerSink {
    /// Vanilla parity: `new LookAndFollowTradingPlayerSink(float)`.
    #[must_use]
    pub const fn new(speed_modifier: f64) -> Self {
        Self { speed_modifier }
    }

    /// The player this villager has a trading screen open with, if it is close.
    fn customer(ctx: &BrainContext<'_>) -> Option<SharedEntity> {
        let villager = villager(ctx)?;
        if !LivingEntity::is_alive(villager) || villager.is_in_water() {
            return None;
        }
        let player = ctx
            .world()
            .get_entity_by_uuid(&villager.merchant().trading_player()?)?;
        (player.position().distance_squared(villager.position()) <= MAX_FOLLOW_DISTANCE_SQR)
            .then_some(player)
    }

    fn follow(&self, ctx: &BrainContext<'_>, player: &SharedEntity) {
        let brain = ctx.brain();
        brain.set_memory(
            memory_module_types::WALK_TARGET,
            WalkTarget::new(
                PositionTracker::of_entity(player, false),
                self.speed_modifier,
                FOLLOW_CLOSE_ENOUGH_DIST,
            ),
        );
        brain.set_memory(
            memory_module_types::LOOK_TARGET,
            PositionTracker::of_entity(player, true),
        );
    }
}

/// Vanilla parity: the `ImmutableMap` of registered walk and look targets.
const WALK_AND_LOOK_REGISTERED: &[(MemoryModuleId, MemoryStatus)] = &[
    (
        memory_module_types::WALK_TARGET.id(),
        MemoryStatus::Registered,
    ),
    (
        memory_module_types::LOOK_TARGET.id(),
        MemoryStatus::Registered,
    ),
];

impl TimedBehavior for LookAndFollowTradingPlayerSink {
    fn entry_condition(&self) -> &[(MemoryModuleId, MemoryStatus)] {
        WALK_AND_LOOK_REGISTERED
    }

    /// Vanilla parity: the `timedOut` override that returns `false`; the
    /// `Integer.MAX_VALUE` duration it is constructed with never matters.
    fn times_out(&self) -> bool {
        false
    }

    fn check_extra_start_conditions(&mut self, ctx: &BrainContext<'_>) -> bool {
        Self::customer(ctx).is_some()
    }

    fn can_still_use(&mut self, ctx: &BrainContext<'_>) -> bool {
        Self::customer(ctx).is_some()
    }

    fn start(&mut self, ctx: &BrainContext<'_>) {
        if let Some(player) = Self::customer(ctx) {
            self.follow(ctx, &player);
        }
    }

    fn tick(&mut self, ctx: &BrainContext<'_>) {
        if let Some(player) = Self::customer(ctx) {
            self.follow(ctx, &player);
        }
    }

    fn stop(&mut self, ctx: &BrainContext<'_>) {
        let brain = ctx.brain();
        brain.erase_memory(memory_module_types::WALK_TARGET.id());
        brain.erase_memory(memory_module_types::LOOK_TARGET.id());
    }

    fn debug_name(&self) -> &'static str {
        "LookAndFollowTradingPlayerSink"
    }
}
