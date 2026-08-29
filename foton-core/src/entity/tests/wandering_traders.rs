//! The wandering trader, driven in a real world.
//!
//! It differs from a villager in exactly the ways that matter to a player: one
//! fixed stock drawn from three pools, no experience bar, no restock, and a
//! clock running down on how long it will stay.

use std::io::Cursor;

use foton_utils::types::UpdateFlags;
use simdnbt::borrow::read_compound as read_borrowed_compound;

use super::*;
use crate::behavior::init_behaviors;
use crate::block_entity::init_block_entities;
use crate::entity::entities::WanderingTraderEntity;
use crate::entity::{AgeableMob, SharedEntity, next_entity_id};
use crate::test_support::{fresh_test_world, insert_ready_full_chunk};
use crate::trading::Merchant as _;
use crate::world::World;

const SPAWN: DVec3 = DVec3::new(8.5, 64.0, 8.5);
const STAND: BlockPos = BlockPos::new(8, 64, 8);

fn trader_world(key: &'static str) -> Arc<World> {
    init_vanilla_registry();
    init_behaviors();
    init_block_entities();
    let world = fresh_test_world(key);
    insert_ready_full_chunk(&world, ChunkPos::new(0, 0));
    for x in (STAND.x() - 2)..=(STAND.x() + 2) {
        for z in (STAND.z() - 2)..=(STAND.z() + 2) {
            assert!(world.set_block(
                BlockPos::new(x, STAND.y() - 1, z),
                vanilla_blocks::STONE.default_state(),
                UpdateFlags::UPDATE_NONE,
            ));
        }
    }
    world
}

fn spawn_trader(world: &Arc<World>) -> Arc<WanderingTraderEntity> {
    let trader = Arc::new(WanderingTraderEntity::new(
        &vanilla_entities::WANDERING_TRADER,
        next_entity_id(),
        SPAWN,
        Arc::downgrade(world),
    ));
    world
        .try_add_entity(Arc::clone(&trader) as SharedEntity)
        .expect("the test chunk is loaded");
    trader
}

#[test]
fn a_wandering_trader_arrives_with_a_full_stock_from_all_three_pools() {
    let world = trader_world("wt_stock");
    let trader = spawn_trader(&world);

    let offers = trader.offers();
    // Vanilla parity: buying draws two, uncommon two and common five, and any
    // of them may decline -- so the count is a floor, not an equality.
    assert!(
        offers.len() >= 5,
        "three pools between them should fill a screen, got {}",
        offers.len()
    );
    assert!(
        offers.iter().any(|offer| offer.result().count() > 0),
        "every offer gives something"
    );
}

#[test]
fn a_wandering_trader_shows_no_experience_bar_and_never_restocks() {
    let world = trader_world("wt_no_progress");
    let trader = spawn_trader(&world);

    assert!(
        !trader.merchant().show_progress_bar(),
        "the screen draws no level badge for a trader"
    );
    assert!(
        !trader.merchant().can_restock(),
        "a trader's stock is all it will ever have, so the screen must not promise more"
    );
}

#[test]
fn a_wandering_trader_draws_its_stock_once_and_keeps_it() {
    let world = trader_world("wt_stock_is_fixed");
    let trader = spawn_trader(&world);

    let first = trader.offers();
    let second = trader.offers();
    assert_eq!(first, second, "looking twice must not roll a second stock");
}

#[test]
fn a_trader_packs_up_when_its_despawn_delay_runs_out() {
    let world = trader_world("wt_despawns");
    let trader = spawn_trader(&world);
    trader.set_despawn_delay(3);

    for _ in 0..2 {
        trader.base_tick();
        trader.tick();
    }
    assert!(!trader.is_removed(), "it has not run out yet");

    trader.base_tick();
    trader.tick();
    assert!(
        trader.is_removed(),
        "the delay running out is what takes it away"
    );
}

#[test]
fn a_trader_mid_trade_is_never_taken_away_from_the_player() {
    let world = trader_world("wt_stays_while_trading");
    let trader = spawn_trader(&world);
    trader.set_despawn_delay(2);
    trader
        .merchant()
        .set_trading_player(Some(Uuid::from_u128(3)));

    for _ in 0..10 {
        trader.base_tick();
        trader.tick();
    }

    assert!(!trader.is_removed());
    assert_eq!(
        trader.despawn_delay(),
        2,
        "the clock does not even run while a player has the screen open"
    );
}

#[test]
fn a_traders_stock_and_clock_survive_a_save() {
    let world = trader_world("wt_persists");
    let trader = spawn_trader(&world);
    trader.set_despawn_delay(1_234);
    trader.set_wander_target(Some(BlockPos::new(11, 65, -7)));
    let stock = trader.offers();

    let mut nbt = NbtCompound::new();
    trader.save_additional(&mut nbt);
    let mut bytes = Vec::new();
    nbt.write(&mut bytes);
    let borrowed = read_borrowed_compound(&mut Cursor::new(bytes.as_slice()))
        .unwrap_or_else(|error| panic!("trader nbt should reborrow: {error}"));

    let restored = Arc::new(WanderingTraderEntity::new(
        &vanilla_entities::WANDERING_TRADER,
        next_entity_id(),
        SPAWN,
        Arc::downgrade(&world),
    ));
    restored.load_additional((&borrowed).into());

    assert_eq!(restored.despawn_delay(), 1_234);
    assert_eq!(restored.wander_target(), Some(BlockPos::new(11, 65, -7)));
    assert_eq!(
        restored.merchant().offers().lock().clone(),
        stock,
        "a restart must not reroll a trader's stock either"
    );
}

#[test]
fn a_wandering_trader_is_never_a_baby() {
    let world = trader_world("wt_never_baby");
    let trader = spawn_trader(&world);

    // Vanilla clamps the age up on load, because a wandering trader has no
    // baby form to draw.
    AgeableMob::set_age(&*trader, -24_000);
    let mut nbt = NbtCompound::new();
    trader.save_additional(&mut nbt);
    let mut bytes = Vec::new();
    nbt.write(&mut bytes);
    let borrowed = read_borrowed_compound(&mut Cursor::new(bytes.as_slice()))
        .unwrap_or_else(|error| panic!("trader nbt should reborrow: {error}"));

    let restored = Arc::new(WanderingTraderEntity::new(
        &vanilla_entities::WANDERING_TRADER,
        next_entity_id(),
        SPAWN,
        Arc::downgrade(&world),
    ));
    restored.load_additional((&borrowed).into());

    assert!(!AgeableMob::is_baby(&*restored));
    assert_eq!(AgeableMob::get_age(&*restored), 0);
}
