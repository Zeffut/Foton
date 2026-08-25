//! The trading half of a villager, in a shape the trading screen can hold.
//!
//! Vanilla parity: the `Merchant` implementation of `AbstractVillager`, plus
//! the trade bookkeeping `Villager` keeps beside it -- experience, the level-up
//! timer, and who traded last.
//!
//! Why it is a separate object: [`crate::trading::open_trading_screen`] hands
//! the menu an `Arc<dyn Merchant>`, and a Steel entity has no way to produce an
//! `Arc` of itself. So the state a trade touches lives here, the entity owns
//! one of these, and the parts that genuinely need the mob -- is it alive, how
//! far away is it -- reach back through the world by entity id.

use std::sync::Weak;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};

use steel_registry::item_stack::ItemStack;
use steel_registry::sound_event::SoundEventRef;
use steel_registry::trading::{MerchantOffer, MerchantOffers};
use steel_registry::{sound_events, vanilla_mob_effects};
use steel_utils::locks::SyncMutex;
use uuid::Uuid;

use crate::entity::entities::ExperienceOrbEntity;
use crate::entity::{Entity as _, LivingEntity};
use crate::player::Player;
use crate::trading::Merchant;
use crate::world::World;

/// How long the villager waits before it acts on a pending level-up.
///
/// Vanilla parity: the `40` assigned to `updateMerchantTimer` in `rewardTradeXp`.
const LEVEL_UP_DELAY_TICKS: i32 = 40;

/// The trading state of one merchant mob.
pub struct MerchantState {
    world: Weak<World>,
    entity_id: i32,
    offers: SyncMutex<MerchantOffers>,
    /// Vanilla keeps `offers` null until something asks for it, so that a
    /// villager which never meets a player never rolls its trades. Steel keeps
    /// the list itself behind the `Merchant` seam, so the "not yet built" state
    /// is this flag.
    offers_built: AtomicBool,
    trading_player: SyncMutex<Option<Uuid>>,
    /// Mirrors `VillagerData.level` so the trading screen can draw its badge
    /// without reaching back through the world on every sync.
    level: AtomicI32,
    xp: AtomicI32,
    last_traded_player: SyncMutex<Option<Uuid>>,
    update_merchant_timer: AtomicI32,
    increase_profession_level_on_update: AtomicBool,
    /// Vanilla parity: `canRestock`, true only for a villager.
    can_restock: bool,
    /// Vanilla parity: `showProgressBar`, false for a wandering trader.
    show_progress_bar: bool,
}

impl MerchantState {
    /// The trading state of a villager, which levels up and restocks.
    #[must_use]
    pub const fn villager(entity_id: i32, world: Weak<World>) -> Self {
        Self::new(entity_id, world, true, true)
    }

    /// The trading state of a wandering trader, which does neither.
    #[must_use]
    pub const fn wandering_trader(entity_id: i32, world: Weak<World>) -> Self {
        Self::new(entity_id, world, false, false)
    }

    const fn new(
        entity_id: i32,
        world: Weak<World>,
        can_restock: bool,
        show_progress_bar: bool,
    ) -> Self {
        Self {
            world,
            entity_id,
            offers: SyncMutex::new(MerchantOffers::new()),
            offers_built: AtomicBool::new(false),
            trading_player: SyncMutex::new(None),
            level: AtomicI32::new(1),
            xp: AtomicI32::new(0),
            last_traded_player: SyncMutex::new(None),
            update_merchant_timer: AtomicI32::new(0),
            increase_profession_level_on_update: AtomicBool::new(false),
            can_restock,
            show_progress_bar,
        }
    }

    /// Whether the offer list has been rolled yet.
    ///
    /// Vanilla parity: `AbstractVillager.offers != null`.
    #[must_use]
    pub fn offers_built(&self) -> bool {
        self.offers_built.load(Ordering::Relaxed)
    }

    /// Marks the offer list as rolled, so it is not rolled again.
    pub fn mark_offers_built(&self) {
        self.offers_built.store(true, Ordering::Relaxed);
    }

    /// Throws away the rolled offers, so the next look rolls them afresh.
    ///
    /// Vanilla parity: the `this.offers = null` in `Villager.setVillagerData`,
    /// which is what makes a villager forget its trades when its profession
    /// changes -- and what makes breaking and replacing a workstation reroll
    /// them.
    pub fn clear_offers(&self) {
        self.offers.lock().clear();
        self.offers_built.store(false, Ordering::Relaxed);
    }

    /// Replaces the offer list wholesale, keeping it marked as built.
    ///
    /// Vanilla parity: `Villager.setOffers`, used by the cure to hand a villager
    /// back the trades it had as a zombie.
    pub fn set_offers(&self, offers: MerchantOffers) {
        *self.offers.lock() = offers;
        self.offers_built.store(true, Ordering::Relaxed);
    }

    /// The trading badge level, 1..=5.
    #[must_use]
    pub fn level(&self) -> i32 {
        self.level.load(Ordering::Relaxed)
    }

    /// Mirrors a change to `VillagerData.level` onto the trading screen.
    pub fn set_level(&self, level: i32) {
        self.level.store(level, Ordering::Relaxed);
    }

    /// Vanilla parity: `Villager.getVillagerXp`.
    #[must_use]
    pub fn xp(&self) -> i32 {
        self.xp.load(Ordering::Relaxed)
    }

    /// Vanilla parity: `Villager.setVillagerXp`.
    pub fn set_xp(&self, xp: i32) {
        self.xp.store(xp, Ordering::Relaxed);
    }

    /// The player this merchant traded with since the last tick, if any.
    ///
    /// Vanilla parity: `Villager.lastTradedPlayer`, which the villager's tick
    /// drains to raise that player's reputation and show happy particles.
    pub fn take_last_traded_player(&self) -> Option<Uuid> {
        self.last_traded_player.lock().take()
    }

    /// Counts down the level-up delay, reporting whether it just elapsed.
    ///
    /// Vanilla parity: the `updateMerchantTimer` branch of `customServerAiStep`.
    /// Returns `Some(true)` on the tick the timer runs out with a level-up
    /// pending, `Some(false)` if it ran out with nothing pending, `None` while
    /// it is still running or was never set.
    pub fn tick_level_up_timer(&self) -> Option<bool> {
        if self.update_merchant_timer.load(Ordering::Relaxed) <= 0 {
            return None;
        }
        let remaining = self.update_merchant_timer.fetch_sub(1, Ordering::Relaxed) - 1;
        if remaining > 0 {
            return None;
        }
        Some(
            self.increase_profession_level_on_update
                .swap(false, Ordering::Relaxed),
        )
    }

    /// Whether any offer has been used since the last restock.
    ///
    /// Vanilla parity: `Villager.needsToRestock`.
    #[must_use]
    pub fn needs_to_restock(&self) -> bool {
        self.offers.lock().iter().any(MerchantOffer::needs_restock)
    }

    /// Folds each offer's use count into its standing demand.
    ///
    /// Vanilla parity: `Villager.updateDemand`.
    pub fn update_demand(&self) {
        for offer in self.offers.lock().iter_mut() {
            offer.update_demand();
        }
    }

    /// Puts every offer back in stock.
    ///
    /// Vanilla parity: the `resetUses` loop of `Villager.restock`.
    pub fn reset_uses(&self) {
        for offer in self.offers.lock().iter_mut() {
            offer.reset_uses();
        }
    }

    /// Vanilla parity: `Villager.resetSpecialPrices`.
    pub fn reset_special_prices(&self) {
        for offer in self.offers.lock().iter_mut() {
            offer.reset_special_price_diff();
        }
    }

    /// Moves every price by this player's standing and their hero effect.
    ///
    /// Vanilla parity: `Villager.updateSpecialPrices`. Reputation scales with
    /// each trade's own multiplier, so a cure discounts an expensive trade more
    /// than a cheap one; the hero discount instead scales with the trade's base
    /// price and is never less than one item.
    pub fn update_special_prices(&self, player: &Player, reputation: i32) {
        self.apply_reputation_discount(reputation);
        if let Some(hero) =
            LivingEntity::mob_effect(player, vanilla_mob_effects::HERO_OF_THE_VILLAGE)
        {
            self.apply_hero_discount(hero.amplifier());
        }
    }

    /// Discounts every price by this player's standing with the merchant.
    ///
    /// Vanilla parity: the first half of `Villager.updateSpecialPrices`. The
    /// discount scales with each trade's own multiplier, so a cure takes more
    /// off an expensive trade than a cheap one.
    pub fn apply_reputation_discount(&self, reputation: i32) {
        if reputation == 0 {
            return;
        }
        for offer in self.offers.lock().iter_mut() {
            #[expect(
                clippy::cast_precision_loss,
                clippy::cast_possible_truncation,
                reason = "vanilla computes this product in float and takes Mth.floor"
            )]
            let discount = ((reputation as f32) * offer.price_multiplier()).floor() as i32;
            offer.add_to_special_price_diff(-discount);
        }
    }

    /// Discounts every price for a Hero of the Village.
    ///
    /// Vanilla parity: the second half of `Villager.updateSpecialPrices`. This
    /// one scales with the trade's base price instead, and is never less than
    /// one item off.
    pub fn apply_hero_discount(&self, amplifier: i32) {
        for offer in self.offers.lock().iter_mut() {
            let modifier = 0.3 + 0.0625 * f64::from(amplifier);
            #[expect(
                clippy::cast_possible_truncation,
                reason = "vanilla computes this product in double and takes Math.floor"
            )]
            let reduction = (modifier * f64::from(offer.base_cost_a().count())).floor() as i32;
            offer.add_to_special_price_diff(-reduction.max(1));
        }
    }

    fn villager_position(&self) -> Option<glam::DVec3> {
        let world = self.world.upgrade()?;
        let entity = world.get_entity_by_id(self.entity_id)?;
        Some(entity.position())
    }
}

impl Merchant for MerchantState {
    fn offers(&self) -> &SyncMutex<MerchantOffers> {
        &self.offers
    }

    fn trading_player(&self) -> Option<Uuid> {
        *self.trading_player.lock()
    }

    fn set_trading_player(&self, player: Option<Uuid>) {
        *self.trading_player.lock() = player;
    }

    /// Vanilla parity: `AbstractVillager.notifyTrade` followed by
    /// `Villager.rewardTradeXp`.
    fn notify_trade(&self, offer_index: usize) {
        let (xp, reward_exp) = {
            let mut offers = self.offers.lock();
            let Some(offer) = offers.get_mut(offer_index) else {
                return;
            };
            offer.increase_uses();
            (offer.xp(), offer.should_reward_exp())
        };

        self.xp.fetch_add(xp, Ordering::Relaxed);
        *self.last_traded_player.lock() = self.trading_player();

        // Vanilla parity: `3 + random.nextInt(4)`, plus five more when this
        // trade is the one that earns a level.
        let mut pop_xp = 3 + rand::random_range(0..4);
        if self.should_increase_level() {
            self.update_merchant_timer
                .store(LEVEL_UP_DELAY_TICKS, Ordering::Relaxed);
            self.increase_profession_level_on_update
                .store(true, Ordering::Relaxed);
            pop_xp += 5;
        }

        if !reward_exp {
            return;
        }
        let (Some(world), Some(position)) = (self.world.upgrade(), self.villager_position()) else {
            return;
        };
        ExperienceOrbEntity::award(&world, position + glam::DVec3::new(0.0, 0.5, 0.0), pop_xp);
    }

    fn notify_trade_updated(&self, _result: &ItemStack) {
        // Vanilla plays the villager's yes/no grunt here through
        // `AbstractVillager.notifyTradeUpdated`. The sound needs the mob's
        // ambient-sound timer, which lives on the entity, so the villager wires
        // this up itself rather than the merchant state doing it blind.
    }

    fn villager_xp(&self) -> i32 {
        self.xp()
    }

    fn merchant_level(&self) -> i32 {
        self.level()
    }

    fn show_progress_bar(&self) -> bool {
        self.show_progress_bar
    }

    fn notify_trade_sound(&self) -> SoundEventRef {
        &sound_events::ENTITY_VILLAGER_YES
    }

    fn can_restock(&self) -> bool {
        self.can_restock
    }

    /// Vanilla parity: `AbstractVillager.stillValid`.
    fn still_valid(&self, player: &Player) -> bool {
        if self.trading_player() != Some(player.uuid()) {
            return false;
        }
        let (Some(world), true) = (self.world.upgrade(), true) else {
            return false;
        };
        let Some(entity) = world.get_entity_by_id(self.entity_id) else {
            return false;
        };
        if entity.is_removed() {
            return false;
        }
        player.is_within_entity_interaction_range(entity.bounding_box(), 4.0)
    }
}

impl MerchantState {
    /// Whether the banked experience has reached this level's threshold.
    ///
    /// Vanilla parity: `Villager.shouldIncreaseLevel`.
    fn should_increase_level(&self) -> bool {
        let level = self.level();
        villager_data::can_level_up(level) && self.xp() >= villager_data::max_xp_per_level(level)
    }
}

/// The level thresholds a villager's experience is measured against.
///
/// Vanilla parity: `net.minecraft.world.entity.npc.villager.VillagerData`.
pub mod villager_data {
    /// Vanilla parity: `VillagerData.MIN_VILLAGER_LEVEL`.
    pub const MIN_LEVEL: i32 = 1;
    /// Vanilla parity: `VillagerData.MAX_VILLAGER_LEVEL`.
    pub const MAX_LEVEL: i32 = 5;

    /// Vanilla parity: `VillagerData.NEXT_LEVEL_XP_THRESHOLDS`.
    const NEXT_LEVEL_XP_THRESHOLDS: [i32; 5] = [0, 10, 70, 150, 250];

    /// Vanilla parity: `VillagerData.canLevelUp`.
    #[must_use]
    pub const fn can_level_up(level: i32) -> bool {
        level >= MIN_LEVEL && level < MAX_LEVEL
    }

    /// Vanilla parity: `VillagerData.getMinXpPerLevel`.
    #[must_use]
    pub fn min_xp_per_level(level: i32) -> i32 {
        if !can_level_up(level) {
            return 0;
        }
        NEXT_LEVEL_XP_THRESHOLDS
            .get((level - 1) as usize)
            .copied()
            .unwrap_or(0)
    }

    /// Vanilla parity: `VillagerData.getMaxXpPerLevel`.
    #[must_use]
    pub fn max_xp_per_level(level: i32) -> i32 {
        if !can_level_up(level) {
            return 0;
        }
        NEXT_LEVEL_XP_THRESHOLDS
            .get(level as usize)
            .copied()
            .unwrap_or(0)
    }
}
