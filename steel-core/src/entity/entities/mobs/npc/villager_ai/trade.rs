//! Trading: showing wares to a player, and handing goods to another villager.
//!
//! Vanilla parity: `ShowTradesToPlayer`, `LookAndFollowTradingPlayerSink` and
//! `TradeWithVillager`.

use steel_registry::equipment::EquipmentSlot;
use steel_registry::item_stack::ItemStack;
use steel_registry::items::ItemRef;
use steel_registry::{vanilla_entities, vanilla_items, vanilla_villager_professions};
use steel_utils::Downcast as _;
use steel_utils::types::InteractionHand;

use super::villager;
use crate::entity::ai::brain::behavior::{
    BrainContext, MemoryModuleId, MemoryStatus, TimedBehavior, utils,
};
use crate::entity::ai::brain::memory::{WalkTarget, memory_module_types};
use crate::entity::ai::brain::position_tracker::PositionTracker;
use crate::entity::entities::mobs::npc::VillagerEntity;
use crate::entity::{Entity as _, InventoryCarrier as _, LivingEntity, SharedEntity};
use crate::inventory::container::Container as _;
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

/// Vanilla parity: the `0.5F` and `2` of the `lockGazeAndWalkToEachOther` calls.
const SWAP_SPEED_MODIFIER: f64 = 0.5;
const SWAP_CLOSE_ENOUGH_DIST: i32 = 2;
/// Vanilla parity: the `distanceToSqr(target) > 5.0` that holds the exchange
/// until the pair have actually reached each other.
const SWAP_DISTANCE_SQR: f64 = 5.0;
/// Vanilla parity: the `24` of `throwHalfStack`, the count above which a
/// villager gives away everything past two dozen rather than half.
const KEEP_AT_MOST: i32 = 24;

/// Vanilla parity: the `ImmutableMap` handed to `TradeWithVillager`'s `super(...)`.
const SWAP_ENTRY_CONDITION: &[(MemoryModuleId, MemoryStatus)] = &[
    (
        memory_module_types::INTERACTION_TARGET.id(),
        MemoryStatus::ValuePresent,
    ),
    (
        memory_module_types::NEAREST_VISIBLE_LIVING_ENTITIES.id(),
        MemoryStatus::ValuePresent,
    ),
];

/// Two villagers meet and hand each other what the other one needs.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.behavior.TradeWithVillager`.
/// Despite the name nothing is bought: each villager throws the other half a
/// stack of food, of a farmer's wheat, or of whatever the other's profession
/// asks for and its own does not.
pub struct TradeWithVillager {
    /// Vanilla parity: the `Set<Item> trades` the behavior works out on start.
    trades: Vec<ItemRef>,
}

impl TradeWithVillager {
    /// Vanilla parity: `new TradeWithVillager()`.
    #[must_use]
    pub const fn new() -> Self {
        Self { trades: Vec::new() }
    }

    /// Vanilla parity: `TradeWithVillager.figureOutWhatIAmWillingToTrade` --
    /// what the other one collects and this one does not.
    fn figure_out_what_i_am_willing_to_trade(
        body: &VillagerEntity,
        target: &VillagerEntity,
    ) -> Vec<ItemRef> {
        let mine = body.profession().requested_items();
        target
            .profession()
            .requested_items()
            .iter()
            .filter(|wanted| !mine.iter().any(|own| own.key == wanted.key))
            .copied()
            .collect()
    }

    /// Vanilla parity: the private static `TradeWithVillager.throwHalfStack`.
    fn throw_half_stack(body: &VillagerEntity, items: &[ItemRef], target: &SharedEntity) {
        let to_throw = {
            let mut inventory = body.carried_inventory().lock();
            let mut to_throw = ItemStack::empty();
            for slot in 0..inventory.get_container_size() {
                let stack = inventory.get_item(slot);
                if stack.is_empty() || !items.iter().any(|item| item.key == stack.item().key) {
                    continue;
                }
                let count = if stack.count() > stack.max_stack_size() / 2 {
                    stack.count() / 2
                } else if stack.count() > KEEP_AT_MOST {
                    stack.count() - KEEP_AT_MOST
                } else {
                    continue;
                };
                to_throw = ItemStack::with_count(stack.item(), count);
                inventory.get_item_mut(slot).shrink(count);
                break;
            }
            to_throw
        };
        if !to_throw.is_empty() {
            utils::throw_item(body, to_throw, target.position());
        }
    }
}

impl Default for TradeWithVillager {
    fn default() -> Self {
        Self::new()
    }
}

/// The villager this one is facing, if the memory still names a live one.
fn interaction_target(ctx: &BrainContext<'_>) -> Option<SharedEntity> {
    ctx.brain()
        .get_memory(memory_module_types::INTERACTION_TARGET)?
        .get()
}

impl TimedBehavior for TradeWithVillager {
    fn entry_condition(&self) -> &[(MemoryModuleId, MemoryStatus)] {
        SWAP_ENTRY_CONDITION
    }

    /// Vanilla parity: `TradeWithVillager.checkExtraStartConditions`.
    fn check_extra_start_conditions(&mut self, ctx: &BrainContext<'_>) -> bool {
        utils::target_is_valid(
            ctx.brain(),
            memory_module_types::INTERACTION_TARGET,
            &vanilla_entities::VILLAGER,
        )
    }

    fn can_still_use(&mut self, ctx: &BrainContext<'_>) -> bool {
        self.check_extra_start_conditions(ctx)
    }

    /// Vanilla parity: `TradeWithVillager.start`.
    fn start(&mut self, ctx: &BrainContext<'_>) {
        let (Some(body_entity), Some(target_entity)) = (
            ctx.world().get_entity_by_id(ctx.mob().id()),
            interaction_target(ctx),
        ) else {
            return;
        };
        utils::lock_gaze_and_walk_to_each_other(
            &body_entity,
            &target_entity,
            SWAP_SPEED_MODIFIER,
            SWAP_CLOSE_ENOUGH_DIST,
        );
        let (Some(body), Some(target)) = (
            villager(ctx),
            target_entity.downcast_ref::<VillagerEntity>(),
        ) else {
            return;
        };
        self.trades = Self::figure_out_what_i_am_willing_to_trade(body, target);
    }

    /// Vanilla parity: `TradeWithVillager.tick`.
    fn tick(&mut self, ctx: &BrainContext<'_>) {
        let (Some(body_entity), Some(target_entity)) = (
            ctx.world().get_entity_by_id(ctx.mob().id()),
            interaction_target(ctx),
        ) else {
            return;
        };
        if body_entity
            .position()
            .distance_squared(target_entity.position())
            > SWAP_DISTANCE_SQR
        {
            return;
        }
        utils::lock_gaze_and_walk_to_each_other(
            &body_entity,
            &target_entity,
            SWAP_SPEED_MODIFIER,
            SWAP_CLOSE_ENOUGH_DIST,
        );

        let (Some(body), Some(target)) = (
            villager(ctx),
            target_entity.downcast_ref::<VillagerEntity>(),
        ) else {
            return;
        };
        body.gossip_with(ctx.world(), target, ctx.game_time());

        let is_farmer = body.profession().key == vanilla_villager_professions::FARMER.key;
        if body.has_excess_food() && (is_farmer || target.wants_more_food()) {
            Self::throw_half_stack(body, &VillagerEntity::food_items(), &target_entity);
        }

        let spare_wheat = body
            .carried_inventory()
            .lock()
            .count_item(&vanilla_items::WHEAT)
            > ItemStack::new(&vanilla_items::WHEAT).max_stack_size() / 2;
        if is_farmer && spare_wheat {
            Self::throw_half_stack(body, &[&vanilla_items::WHEAT], &target_entity);
        }

        if !self.trades.is_empty() && body.carried_inventory().lock().has_any_of(&self.trades) {
            Self::throw_half_stack(body, &self.trades.clone(), &target_entity);
        }
    }

    /// Vanilla parity: `TradeWithVillager.stop`.
    fn stop(&mut self, ctx: &BrainContext<'_>) {
        ctx.brain()
            .erase_memory(memory_module_types::INTERACTION_TARGET.id());
    }

    fn debug_name(&self) -> &'static str {
        "TradeWithVillager"
    }
}
