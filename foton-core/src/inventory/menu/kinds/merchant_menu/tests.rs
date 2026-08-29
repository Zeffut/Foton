//! Tests for the trading screen.
//!
//! These drive the menu the way a client does -- carry a stack, click a slot,
//! click a trade, take the result -- so the price arithmetic, the payment
//! bookkeeping and the merchant's own accounting are all exercised through the
//! same path a real trade takes.

use std::sync::Arc;
use std::sync::atomic::{AtomicI32, AtomicUsize, Ordering};

use foton_registry::sound_event::SoundEventRef;
use foton_registry::trading::{ItemCost, MerchantOffer, MerchantOffers};
use foton_registry::{
    REGISTRY, RegistryExt as _, init_vanilla_registry, item_stack::ItemStack, items::ItemRef,
    sound_events, vanilla_items,
};
use foton_utils::locks::SyncMutex;
use foton_utils::{ChunkPos, Downcast as _};
use glam::DVec3;
use uuid::Uuid;

use super::{MerchantKind, merchant_menu};
use crate::behavior::init_behaviors;
use crate::entity::{Entity as _, next_entity_id};
use crate::inventory::click::{Click, MouseButton};
use crate::inventory::lock::ContainerId;
use crate::inventory::menu::Menu;
use crate::player::Player;
use crate::test_support::{TestPlayerBuilder, fresh_test_world, insert_ready_full_chunk};
use crate::trading::Merchant;
use crate::world::World;
use foton_utils::BlockPos;

const PAYMENT_A: usize = 0;
const PAYMENT_B: usize = 1;
const RESULT_SLOT: usize = 2;

/// A merchant with no mob behind it, so the menu can be tested on its own.
///
/// It records what the menu told it, which is the only way to see that a trade
/// was actually banked rather than just visually removed from the screen.
struct TestMerchant {
    offers: SyncMutex<MerchantOffers>,
    trading_player: SyncMutex<Option<Uuid>>,
    xp: SyncMutex<i32>,
    trades_notified: AtomicUsize,
    updates_notified: AtomicUsize,
    level: AtomicI32,
}

impl TestMerchant {
    fn new(offers: Vec<MerchantOffer>) -> Arc<Self> {
        Arc::new(Self {
            offers: SyncMutex::new(offers.into()),
            trading_player: SyncMutex::new(None),
            xp: SyncMutex::new(0),
            trades_notified: AtomicUsize::new(0),
            updates_notified: AtomicUsize::new(0),
            level: AtomicI32::new(1),
        })
    }

    fn trades_notified(&self) -> usize {
        self.trades_notified.load(Ordering::Relaxed)
    }

    fn offer_uses(&self, index: usize) -> i32 {
        self.offers.lock()[index].uses()
    }

    fn updates_notified(&self) -> usize {
        self.updates_notified.load(Ordering::Relaxed)
    }
}

impl Merchant for TestMerchant {
    fn offers(&self) -> &SyncMutex<MerchantOffers> {
        &self.offers
    }

    fn trading_player(&self) -> Option<Uuid> {
        *self.trading_player.lock()
    }

    fn set_trading_player(&self, player: Option<Uuid>) {
        *self.trading_player.lock() = player;
    }

    /// Mirrors the shape of `AbstractVillager.notifyTrade`: raise the use count
    /// on the merchant's own offer, then bank the experience.
    fn notify_trade(&self, offer_index: usize) {
        let xp = {
            let mut offers = self.offers.lock();
            offers[offer_index].increase_uses();
            offers[offer_index].xp()
        };
        *self.xp.lock() += xp;
        self.trades_notified.fetch_add(1, Ordering::Relaxed);
    }

    fn notify_trade_updated(&self, _result: &ItemStack) {
        self.updates_notified.fetch_add(1, Ordering::Relaxed);
    }

    fn villager_xp(&self) -> i32 {
        *self.xp.lock()
    }

    fn merchant_level(&self) -> i32 {
        self.level.load(Ordering::Relaxed)
    }

    fn show_progress_bar(&self) -> bool {
        true
    }

    fn notify_trade_sound(&self) -> SoundEventRef {
        &sound_events::ENTITY_VILLAGER_YES
    }

    fn still_valid(&self, _player: &Player) -> bool {
        true
    }
}

fn cost(item: ItemRef, count: i32) -> ItemCost {
    ItemCost::new(item, count)
}

fn stack(item: ItemRef, count: i32) -> ItemStack {
    ItemStack::with_count(item, count)
}

/// "20 wheat for an emerald", the farmer's first trade.
fn wheat_for_emerald() -> MerchantOffer {
    MerchantOffer::new(
        cost(&vanilla_items::WHEAT, 20),
        None,
        stack(&vanilla_items::EMERALD, 1),
        16,
        2,
        0.05,
    )
}

/// "five emeralds and three wheat for a loaf", a two-cost trade.
fn bread_for_emeralds_and_wheat() -> MerchantOffer {
    MerchantOffer::new(
        cost(&vanilla_items::EMERALD, 5),
        Some(cost(&vanilla_items::WHEAT, 3)),
        stack(&vanilla_items::BREAD, 1),
        12,
        1,
        0.05,
    )
}

fn trading_world(name: &'static str) -> (Arc<World>, Arc<Player>) {
    init_vanilla_registry();
    init_behaviors();
    let world = fresh_test_world(name);
    let pos = BlockPos::new(8, 64, 8);
    insert_ready_full_chunk(&world, ChunkPos::from_block_pos(pos));
    let player = TestPlayerBuilder::new(Arc::clone(&world), "Customer", next_entity_id()).build();
    player.base().set_position_local(DVec3::new(8.5, 64.0, 8.5));
    (world, player)
}

/// Opens a trading screen against `merchant` and hands back the live menu.
fn open_menu(player: &Arc<Player>, merchant: &Arc<TestMerchant>) -> Menu {
    let merchant: Arc<dyn Merchant> = Arc::clone(merchant) as Arc<dyn Merchant>;
    let mut menu = merchant_menu(player.inventory.clone(), 1, merchant);
    // `on_open` is what the real open path runs after the screen packet; the
    // menu is otherwise inert, with no result and no trading player set.
    menu.on_open(player);
    menu
}

fn container_ids(menu: &Menu) -> (ContainerId, ContainerId) {
    let kind = menu
        .kind()
        .downcast_ref::<MerchantKind>()
        .expect("the merchant builder makes a merchant menu");
    (kind.payment_id_for_tests(), kind.result_id_for_tests())
}

/// Puts `stack` into a payment slot by carrying it and clicking, the way a
/// client does.
fn pay(menu: &mut Menu, player: &Arc<Player>, slot: usize, stack: ItemStack) {
    *menu.behavior_mut().carried_mut() = stack;
    menu.clicked(
        Click::Pickup {
            slot,
            button: MouseButton::Left,
        },
        player,
    );
}

/// Picks a slot's contents up onto the cursor, the way a client does.
fn pickup(menu: &mut Menu, player: &Arc<Player>, slot: usize) {
    *menu.behavior_mut().carried_mut() = ItemStack::empty();
    menu.clicked(
        Click::Pickup {
            slot,
            button: MouseButton::Left,
        },
        player,
    );
}

/// Takes whatever is in the result slot onto the cursor.
fn take_result(menu: &mut Menu, player: &Arc<Player>) {
    pickup(menu, player, RESULT_SLOT);
}

fn item_in(menu: &Menu, container: ContainerId, slot: usize) -> ItemStack {
    menu.behavior()
        .lock_all_containers()
        .get(container)
        .expect("container is registered with the menu")
        .get_item(slot)
        .clone()
}

fn result_of(menu: &Menu) -> ItemStack {
    let (_, result) = container_ids(menu);
    item_in(menu, result, 0)
}

#[test]
fn opening_the_screen_claims_the_merchant() {
    let (_world, player) = trading_world("merchant_open");
    let merchant = TestMerchant::new(vec![wheat_for_emerald()]);

    let mut menu = open_menu(&player, &merchant);
    assert_eq!(merchant.trading_player(), Some(player.uuid()));

    menu.removed(&player);
    assert_eq!(
        merchant.trading_player(),
        None,
        "closing must release the merchant for the next customer"
    );
}

#[test]
fn paying_the_price_fills_the_result_slot() {
    let (_world, player) = trading_world("merchant_pays");
    let merchant = TestMerchant::new(vec![wheat_for_emerald()]);
    let mut menu = open_menu(&player, &merchant);

    assert!(result_of(&menu).is_empty());

    pay(
        &mut menu,
        &player,
        PAYMENT_A,
        stack(&vanilla_items::WHEAT, 20),
    );

    let result = result_of(&menu);
    assert!(result.is(&vanilla_items::EMERALD));
    assert_eq!(result.count(), 1);
}

#[test]
fn an_underpayment_leaves_the_result_empty() {
    let (_world, player) = trading_world("merchant_underpays");
    let merchant = TestMerchant::new(vec![wheat_for_emerald()]);
    let mut menu = open_menu(&player, &merchant);

    pay(
        &mut menu,
        &player,
        PAYMENT_A,
        stack(&vanilla_items::WHEAT, 19),
    );

    assert!(result_of(&menu).is_empty());
}

#[test]
fn a_lone_second_payment_still_pays_for_a_one_cost_trade() {
    // Vanilla parity: `updateSellItem` slides a lone slot-2 payment into first
    // position, so dropping the price into either slot works.
    let (_world, player) = trading_world("merchant_slot_b");
    let merchant = TestMerchant::new(vec![wheat_for_emerald()]);
    let mut menu = open_menu(&player, &merchant);

    pay(
        &mut menu,
        &player,
        PAYMENT_B,
        stack(&vanilla_items::WHEAT, 20),
    );

    assert!(result_of(&menu).is(&vanilla_items::EMERALD));
}

#[test]
fn taking_the_result_spends_the_payment_and_banks_the_trade() {
    let (_world, player) = trading_world("merchant_takes");
    let merchant = TestMerchant::new(vec![wheat_for_emerald()]);
    let mut menu = open_menu(&player, &merchant);
    let (payment, _) = container_ids(&menu);

    pay(
        &mut menu,
        &player,
        PAYMENT_A,
        stack(&vanilla_items::WHEAT, 25),
    );
    take_result(&mut menu, &player);

    assert!(menu.behavior().carried().is(&vanilla_items::EMERALD));
    assert_eq!(
        item_in(&menu, payment, 0).count(),
        5,
        "exactly the price comes out of the payment"
    );
    assert_eq!(merchant.trades_notified(), 1);
    assert_eq!(merchant.offer_uses(0), 1);
    assert_eq!(merchant.villager_xp(), 2, "the merchant banks the trade xp");
}

#[test]
fn a_two_cost_trade_spends_both_payments() {
    let (_world, player) = trading_world("merchant_two_costs");
    let merchant = TestMerchant::new(vec![bread_for_emeralds_and_wheat()]);
    let mut menu = open_menu(&player, &merchant);
    let (payment, _) = container_ids(&menu);

    pay(
        &mut menu,
        &player,
        PAYMENT_A,
        stack(&vanilla_items::EMERALD, 7),
    );
    pay(
        &mut menu,
        &player,
        PAYMENT_B,
        stack(&vanilla_items::WHEAT, 5),
    );
    assert!(result_of(&menu).is(&vanilla_items::BREAD));

    take_result(&mut menu, &player);

    assert_eq!(item_in(&menu, payment, 0).count(), 2);
    assert_eq!(item_in(&menu, payment, 1).count(), 2);
    assert_eq!(merchant.trades_notified(), 1);
}

#[test]
fn a_two_cost_trade_pays_with_its_costs_in_either_slot() {
    // Vanilla parity: `updateSellItem` tries the payment both ways round, so
    // dropping the emeralds and the wheat into the "wrong" slots still trades.
    let (_world, player) = trading_world("merchant_swapped");
    let merchant = TestMerchant::new(vec![bread_for_emeralds_and_wheat()]);
    let mut menu = open_menu(&player, &merchant);

    pay(
        &mut menu,
        &player,
        PAYMENT_A,
        stack(&vanilla_items::WHEAT, 3),
    );
    pay(
        &mut menu,
        &player,
        PAYMENT_B,
        stack(&vanilla_items::EMERALD, 5),
    );

    assert!(
        result_of(&menu).is(&vanilla_items::BREAD),
        "the costs are in the opposite slots, which vanilla still accepts"
    );

    take_result(&mut menu, &player);
    assert_eq!(merchant.trades_notified(), 1);
}

#[test]
fn clearing_the_payment_does_not_make_the_merchant_grunt() {
    // Vanilla parity: `updateSellItem` only reaches `notifyTradeUpdated` when a
    // payment is present. Taking your own items back is not an offer.
    let (_world, player) = trading_world("merchant_quiet");
    let merchant = TestMerchant::new(vec![wheat_for_emerald()]);
    let mut menu = open_menu(&player, &merchant);

    pay(
        &mut menu,
        &player,
        PAYMENT_A,
        stack(&vanilla_items::WHEAT, 20),
    );
    let after_paying = merchant.updates_notified();
    assert!(after_paying > 0, "offering something is worth a grunt");

    pickup(&mut menu, &player, PAYMENT_A);
    assert!(result_of(&menu).is_empty());
    assert_eq!(
        merchant.updates_notified(),
        after_paying,
        "emptying the slots must be silent"
    );
}

#[test]
fn an_out_of_stock_trade_offers_nothing() {
    let (_world, player) = trading_world("merchant_out_of_stock");
    let mut offer = wheat_for_emerald();
    offer.set_to_out_of_stock();
    let merchant = TestMerchant::new(vec![offer]);
    let mut menu = open_menu(&player, &merchant);

    pay(
        &mut menu,
        &player,
        PAYMENT_A,
        stack(&vanilla_items::WHEAT, 20),
    );

    assert!(
        result_of(&menu).is_empty(),
        "a sold-out trade must not be buyable"
    );
    assert_eq!(merchant.trades_notified(), 0);
}

#[test]
fn selecting_a_trade_picks_between_two_wanting_the_same_item() {
    let bread = MerchantOffer::new(
        cost(&vanilla_items::EMERALD, 1),
        None,
        stack(&vanilla_items::BREAD, 1),
        16,
        1,
        0.05,
    );
    let carrots = MerchantOffer::new(
        cost(&vanilla_items::EMERALD, 1),
        None,
        stack(&vanilla_items::CARROT, 1),
        16,
        1,
        0.05,
    );
    let (_world, player) = trading_world("merchant_selects");
    let merchant = TestMerchant::new(vec![bread, carrots]);
    let mut menu = open_menu(&player, &merchant);

    pay(
        &mut menu,
        &player,
        PAYMENT_A,
        stack(&vanilla_items::EMERALD, 1),
    );
    assert!(
        result_of(&menu).is(&vanilla_items::BREAD),
        "with no selection the scan finds the first payable trade"
    );

    menu.select_trade(&player, 1);
    assert!(
        result_of(&menu).is(&vanilla_items::CARROT),
        "clicking the second trade must switch the result to it"
    );

    take_result(&mut menu, &player);
    assert_eq!(
        merchant.offer_uses(1),
        1,
        "the selected trade is the one sold"
    );
    assert_eq!(merchant.offer_uses(0), 0);
}

#[test]
fn the_price_rises_with_demand_through_the_whole_screen() {
    // The menu must read the merchant's live offer, not a snapshot taken when
    // the screen opened, or a price that moved between trades would not stick.
    let (_world, player) = trading_world("merchant_demand");
    let merchant = TestMerchant::new(vec![wheat_for_emerald()]);
    let mut menu = open_menu(&player, &merchant);

    pay(
        &mut menu,
        &player,
        PAYMENT_A,
        stack(&vanilla_items::WHEAT, 20),
    );
    assert!(result_of(&menu).is(&vanilla_items::EMERALD));

    // A day's trading, then a restock. update_demand is
    // `demand + uses - (maxUses - uses)`, so out of 16 uses it takes 10 sales
    // before demand goes positive at all: 0 + 10 - 6 = 4. That is then
    // floor(20 * 4 * 0.05) = 4 more wheat on the price.
    for _ in 0..10 {
        merchant.offers.lock()[0].increase_uses();
    }
    merchant.offers.lock()[0].update_demand();
    merchant.offers.lock()[0].reset_uses();
    assert_eq!(merchant.offers.lock()[0].demand(), 4);

    // Any slot touch is what makes the menu recompute; lift the payment out
    // and put it straight back.
    pickup(&mut menu, &player, PAYMENT_A);
    pay(
        &mut menu,
        &player,
        PAYMENT_A,
        stack(&vanilla_items::WHEAT, 20),
    );
    assert!(
        result_of(&menu).is_empty(),
        "20 wheat no longer covers a price that moved to 24"
    );

    pickup(&mut menu, &player, PAYMENT_A);

    pay(
        &mut menu,
        &player,
        PAYMENT_A,
        stack(&vanilla_items::WHEAT, 24),
    );
    assert!(result_of(&menu).is(&vanilla_items::EMERALD));
}

#[test]
fn closing_the_screen_hands_the_payment_back() {
    let (_world, player) = trading_world("merchant_returns");
    let merchant = TestMerchant::new(vec![wheat_for_emerald()]);
    let mut menu = open_menu(&player, &merchant);

    pay(
        &mut menu,
        &player,
        PAYMENT_A,
        stack(&vanilla_items::WHEAT, 5),
    );
    menu.removed(&player);

    let held = player
        .inventory
        .lock()
        .get_items()
        .iter()
        .filter(|stack| stack.is(&vanilla_items::WHEAT))
        .map(ItemStack::count)
        .sum::<i32>();
    assert_eq!(held, 5, "an abandoned payment must not be eaten");
}

#[test]
fn an_unemployed_merchant_opens_a_blank_screen() {
    // A villager with no profession has no offers, and vanilla sends no offers
    // packet at all rather than an empty one.
    let (_world, player) = trading_world("merchant_blank");
    let merchant = TestMerchant::new(Vec::new());
    let mut menu = open_menu(&player, &merchant);

    pay(
        &mut menu,
        &player,
        PAYMENT_A,
        stack(&vanilla_items::WHEAT, 64),
    );

    assert!(result_of(&menu).is_empty());
    assert_eq!(merchant.trades_notified(), 0);
}

#[test]
fn the_extracted_registry_carries_the_merchant_menu_type() {
    // If this fails everything above is building a menu the client cannot open.
    init_vanilla_registry();
    assert!(
        REGISTRY
            .menu_types
            .by_key(&foton_utils::Identifier::vanilla_static("merchant"))
            .is_some()
    );
}
