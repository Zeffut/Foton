//! The villager, driven in a real world.
//!
//! These are the loop a player actually uses: a villager takes a job from a
//! workstation, rolls the trades that job sells, banks the experience a trade
//! pays, levels up, restocks, and remembers who cured it. Each of those steps
//! is where the whole thing silently stops being worth anything if it breaks --
//! a villager with no profession has no trades at all, and a villager that
//! rerolls its trades on load turns a server restart into a reroll button.

use std::io::Cursor;

use simdnbt::borrow::read_compound as read_borrowed_compound;
use steel_registry::blocks::properties::BedPart;
use steel_registry::{vanilla_villager_professions, vanilla_world_clocks};
use steel_utils::types::UpdateFlags;

use super::*;
use crate::behavior::{BlockStateBehaviorExt as _, init_behaviors};
use crate::block_entity::init_block_entities;
use crate::entity::ai::brain::Activity;
use crate::entity::ai::brain::memory::memory_module_types;
use crate::entity::ai::gossip::{GossipType, ReputationEventType};
use crate::entity::entities::{ItemEntity, VillagerEntity, ZombieEntity};
use crate::entity::{
    AgeableMob, InventoryCarrier as _, LivingEntity, Mob, MobEffectInstance, SharedEntity,
    next_entity_id,
};
use crate::inventory::container::Container as _;
use crate::poi::poi_storage::OccupationStatus;
use crate::test_support::{TestPlayerBuilder, fresh_test_world, insert_ready_full_chunk};
use crate::trading::Merchant as _;
use crate::world::World;

/// The one spot the test world is guaranteed to be solid ground rather than a
/// column the fluid scan walks.
const SPAWN: DVec3 = DVec3::new(8.5, 64.0, 8.5);
const STAND: BlockPos = BlockPos::new(8, 64, 8);

fn villager_world(key: &'static str) -> Arc<World> {
    init_vanilla_registry();
    init_behaviors();
    init_block_entities();
    let world = fresh_test_world(key);
    insert_ready_full_chunk(&world, ChunkPos::new(0, 0));

    // A villager in a void neither stands nor paths, and the job-site scan will
    // not claim what it cannot reach.
    for x in (STAND.x() - 4)..=(STAND.x() + 4) {
        for z in (STAND.z() - 4)..=(STAND.z() + 4) {
            assert!(world.set_block(
                BlockPos::new(x, STAND.y() - 1, z),
                vanilla_blocks::STONE.default_state(),
                UpdateFlags::UPDATE_NONE,
            ));
        }
    }
    world
}

fn spawn_villager(world: &Arc<World>) -> Arc<VillagerEntity> {
    let villager = Arc::new(VillagerEntity::new(
        &vanilla_entities::VILLAGER,
        next_entity_id(),
        SPAWN,
        Arc::downgrade(world),
    ));
    world
        .try_add_entity(Arc::clone(&villager) as SharedEntity)
        .expect("the test chunk is loaded, so the villager should attach");
    villager
}

/// How long to tick a villager that is expected to end up at a workstation.
///
/// `AcquirePoi` books its first scan up to twenty ticks out and re-books every
/// twenty-plus-jitter, and an idle villager strolls -- so the walk back to the
/// site it just claimed, at half speed, is the long pole rather than the scan.
const TICKS_TO_TAKE_A_JOB: i32 = 400;

/// Ticks the villager the way the server would, clock and all.
fn run_ticks(world: &Arc<World>, villager: &Arc<VillagerEntity>, ticks: i32) {
    for _ in 0..ticks {
        advance_time(world);
        villager.base_tick();
        villager.tick();
    }
}

/// Ticks until `reached` is true, or gives up after `ticks`.
///
/// A villager wanders while it waits, so a test that only looks at the end of a
/// fixed run can miss the moment it is watching for. This watches every tick
/// instead, which is what makes those assertions stable.
fn run_ticks_until(
    world: &Arc<World>,
    villager: &Arc<VillagerEntity>,
    ticks: i32,
    reached: impl Fn() -> bool,
) -> bool {
    for _ in 0..ticks {
        advance_time(world);
        villager.base_tick();
        villager.tick();
        if reached() {
            return true;
        }
    }
    false
}

/// Moves the world clock on by a tick.
///
/// The entity tests drive entities directly rather than running a whole world
/// tick, so nothing else advances game time -- and a villager that never sees
/// the clock move never runs its job-site scan.
fn advance_time(world: &Arc<World>) {
    let now = world.game_time();
    world.level_data.write().set_game_time(now + 1);
}

#[test]
fn a_villager_with_no_profession_has_nothing_to_sell() {
    let world = villager_world("villager_unemployed");
    let villager = spawn_villager(&world);

    assert_eq!(villager.profession().key.path, "none");
    assert!(
        villager.offers().is_empty(),
        "an unemployed villager offers nothing, which is why the job site matters"
    );
}

#[test]
fn claiming_a_workstation_gives_a_villager_its_profession_and_its_trades() {
    let world = villager_world("villager_takes_a_job");
    let villager = spawn_villager(&world);

    // A cartography table is the cartographer's POI, and the POI and the
    // profession share a key -- that is how the workstation names the trade.
    let table = BlockPos::new(STAND.x() + 1, STAND.y(), STAND.z());
    assert!(world.set_block(
        table,
        vanilla_blocks::CARTOGRAPHY_TABLE.default_state(),
        UpdateFlags::UPDATE_NONE,
    ));

    run_ticks(&world, &villager, TICKS_TO_TAKE_A_JOB);

    assert_eq!(
        villager.profession().key.path,
        "cartographer",
        "an unemployed villager beside a free cartography table takes the job"
    );
    assert_eq!(
        villager_job_site(&villager),
        Some(table),
        "and holds a ticket on it"
    );
    assert!(
        !villager.offers().is_empty(),
        "a cartographer has trades to sell"
    );
}

#[test]
fn two_villagers_cannot_claim_the_same_workstation() {
    let world = villager_world("villager_one_job_each");
    let first = spawn_villager(&world);
    let second = spawn_villager(&world);

    let table = BlockPos::new(STAND.x() + 1, STAND.y(), STAND.z());
    assert!(world.set_block(
        table,
        vanilla_blocks::CARTOGRAPHY_TABLE.default_state(),
        UpdateFlags::UPDATE_NONE,
    ));

    for _ in 0..TICKS_TO_TAKE_A_JOB {
        advance_time(&world);
        first.base_tick();
        first.tick();
        second.base_tick();
        second.tick();
    }

    let claims = [villager_job_site(&first), villager_job_site(&second)];
    assert_eq!(
        claims.iter().filter(|claim| **claim == Some(table)).count(),
        1,
        "a cartography table has one ticket, so exactly one villager gets the job"
    );
}

#[test]
fn a_villager_gives_its_workstation_back_when_it_dies() {
    let world = villager_world("villager_releases_job");
    let villager = spawn_villager(&world);

    let table = BlockPos::new(STAND.x() + 1, STAND.y(), STAND.z());
    assert!(world.set_block(
        table,
        vanilla_blocks::CARTOGRAPHY_TABLE.default_state(),
        UpdateFlags::UPDATE_NONE,
    ));
    run_ticks(&world, &villager, TICKS_TO_TAKE_A_JOB);
    assert_eq!(villager_job_site(&villager), Some(table));

    villager.release_all_pois();

    // Vanilla's `releasePoi` hands the ticket back and leaves the memory alone
    // -- every caller is a villager that is about to stop existing, so what
    // matters is that the workstation is free for somebody else.
    let free_again = world.poi_storage.lock().find_closest(
        &|_| true,
        &|pos| pos == table,
        STAND,
        8,
        OccupationStatus::Free,
    );
    assert_eq!(
        free_again,
        Some(table),
        "the table is claimable again, or a village slowly runs out of jobs"
    );
}

#[test]
fn a_traded_offer_runs_out_of_stock_and_a_restock_refills_it() {
    let world = villager_world("villager_restock");
    let villager = spawn_villager(&world);
    villager.set_profession(&vanilla_villager_professions::FARMER);

    let offers = villager.offers();
    assert!(!offers.is_empty(), "a farmer has trades");

    // Sell one of them out.
    let max_uses = villager.merchant().offers().lock()[0].max_uses();
    for _ in 0..max_uses {
        villager.merchant().notify_trade(0);
    }
    assert!(
        villager.merchant().offers().lock()[0].is_out_of_stock(),
        "using a trade to its limit takes it out of stock"
    );
    assert!(villager.merchant().needs_to_restock());

    villager.restock(0);

    assert!(
        !villager.merchant().offers().lock()[0].is_out_of_stock(),
        "a restock puts it back on the shelf"
    );
}

#[test]
fn a_villager_restocks_at_most_twice_a_day() {
    let world = villager_world("villager_restock_limit");
    let villager = spawn_villager(&world);
    villager.set_profession(&vanilla_villager_professions::FARMER);
    let _ = villager.offers();

    // One use is enough to want a restock.
    villager.merchant().notify_trade(0);

    // Vanilla parity: the first restock is free, the second needs 2400 ticks,
    // and there is no third until the day rolls over.
    assert!(villager.should_restock(100));
    villager.restock(100);

    villager.merchant().notify_trade(0);
    assert!(
        !villager.should_restock(200),
        "a second restock has to wait out the 2400-tick cooldown"
    );
    assert!(villager.should_restock(2600));
    villager.restock(2600);

    villager.merchant().notify_trade(0);
    assert!(
        !villager.should_restock(5200),
        "two restocks is the day's allowance"
    );
}

#[test]
fn banking_enough_experience_raises_the_trading_level_and_adds_trades() {
    let world = villager_world("villager_levels_up");
    let villager = spawn_villager(&world);
    villager.set_profession(&vanilla_villager_professions::FARMER);
    let first_level_offers = villager.offers().len();

    assert_eq!(villager.villager_level(), 1);

    // Level two needs ten experience; a farmer's first trades pay two apiece.
    villager.merchant().set_xp(0);
    for _ in 0..10 {
        villager.merchant().notify_trade(0);
    }

    // Vanilla waits forty ticks before it acts on the level-up.
    run_ticks(&world, &villager, 60);

    assert_eq!(
        villager.villager_level(),
        2,
        "ten experience is the level-two threshold"
    );
    assert!(
        villager.merchant().offers().lock().len() > first_level_offers,
        "leveling up adds the next tier's trades rather than replacing them"
    );
}

#[test]
fn curing_a_villager_leaves_a_discount_on_every_price() {
    let world = villager_world("villager_cure_discount");
    let villager = spawn_villager(&world);
    villager.set_profession(&vanilla_villager_professions::FARMER);
    let _ = villager.offers();

    let curer = Uuid::from_u128(7);
    let base: Vec<i32> = villager
        .merchant()
        .offers()
        .lock()
        .iter()
        .map(|offer| offer.cost_a().count())
        .collect();

    villager.on_reputation_event_from(ReputationEventType::ZombieVillagerCured, curer);
    assert_eq!(
        villager.player_reputation(curer),
        125,
        "a cure is twenty major-positive points and twenty-five minor ones"
    );

    // `updateSpecialPrices` is what turns reputation into a price, and it wants
    // the player who is opening the screen.
    villager
        .merchant()
        .apply_reputation_discount(villager.player_reputation(curer));

    let discounted: Vec<i32> = villager
        .merchant()
        .offers()
        .lock()
        .iter()
        .map(|offer| offer.cost_a().count())
        .collect();
    assert!(
        discounted
            .iter()
            .zip(&base)
            .any(|(after, before)| after < before),
        "curing a villager has to actually make its trades cheaper"
    );
    assert!(
        discounted.iter().all(|count| *count >= 1),
        "a discount never takes a price below one item"
    );
}

#[test]
fn a_villagers_trades_survive_a_save_and_load_rather_than_rerolling() {
    let world = villager_world("villager_offers_persist");
    let villager = spawn_villager(&world);
    villager.set_profession(&vanilla_villager_professions::FARMER);
    villager.set_level(3);
    let saved_offers = villager.offers();
    assert!(!saved_offers.is_empty());

    let mut nbt = NbtCompound::new();
    villager.save_additional(&mut nbt);

    let mut bytes = Vec::new();
    nbt.write(&mut bytes);
    let borrowed = read_borrowed_compound(&mut Cursor::new(bytes.as_slice()))
        .unwrap_or_else(|error| panic!("villager nbt should reborrow: {error}"));

    let restored = Arc::new(VillagerEntity::new(
        &vanilla_entities::VILLAGER,
        next_entity_id(),
        SPAWN,
        Arc::downgrade(&world),
    ));
    restored.load_additional((&borrowed).into());

    assert_eq!(restored.profession().key.path, "farmer");
    assert_eq!(restored.villager_level(), 3);
    assert_eq!(
        restored.merchant().offers().lock().clone(),
        saved_offers,
        "a restart must not reroll a villager's trades"
    );
}

#[test]
fn a_villagers_memory_of_a_player_survives_a_save() {
    let world = villager_world("villager_gossip_persists");
    let villager = spawn_villager(&world);
    let curer = Uuid::from_u128(11);
    villager.on_reputation_event_from(ReputationEventType::ZombieVillagerCured, curer);

    let mut nbt = NbtCompound::new();
    villager.save_additional(&mut nbt);
    let mut bytes = Vec::new();
    nbt.write(&mut bytes);
    let borrowed = read_borrowed_compound(&mut Cursor::new(bytes.as_slice()))
        .unwrap_or_else(|error| panic!("villager nbt should reborrow: {error}"));

    let restored = Arc::new(VillagerEntity::new(
        &vanilla_entities::VILLAGER,
        next_entity_id(),
        SPAWN,
        Arc::downgrade(&world),
    ));
    restored.load_additional((&borrowed).into());

    assert_eq!(
        restored.player_reputation(curer),
        125,
        "a cured village must still know you after a restart"
    );
    assert_eq!(
        restored
            .gossips()
            .reputation(curer, |kind| kind == GossipType::MajorPositive),
        100
    );
}

#[test]
fn changing_profession_throws_away_the_trades_of_the_old_one() {
    let world = villager_world("villager_reroll_on_profession_change");
    let villager = spawn_villager(&world);
    villager.set_profession(&vanilla_villager_professions::FARMER);
    let farmer_offers = villager.offers();
    assert!(!farmer_offers.is_empty());

    villager.set_profession(&vanilla_villager_professions::LIBRARIAN);

    let librarian_offers = villager.offers();
    assert!(!librarian_offers.is_empty());
    assert_ne!(
        librarian_offers, farmer_offers,
        "a villager that changed jobs sells the new job's goods"
    );
}

#[test]
fn a_baby_villager_will_not_take_a_job() {
    let world = villager_world("villager_baby");
    let villager = spawn_villager(&world);
    AgeableMob::set_age(&*villager, -24_000);
    assert!(AgeableMob::is_baby(&*villager));

    // A free workstation right beside it, which an adult would have claimed.
    let table = BlockPos::new(STAND.x() + 1, STAND.y(), STAND.z());
    assert!(world.set_block(
        table,
        vanilla_blocks::CARTOGRAPHY_TABLE.default_state(),
        UpdateFlags::UPDATE_NONE,
    ));

    run_ticks(&world, &villager, TICKS_TO_TAKE_A_JOB);

    assert_eq!(
        villager_job_site(&villager),
        None,
        "a baby leaves the workstation for the adults"
    );
    assert_eq!(villager.profession().key.path, "none");

    // And the table really was claimable, so the assertion above is about the
    // baby rather than about an unreachable table.
    let adult = spawn_villager(&world);
    run_ticks(&world, &adult, TICKS_TO_TAKE_A_JOB);
    assert_eq!(villager_job_site(&adult), Some(table));
}

/// Puts a bed at `head`, with its foot one block further east.
fn place_bed(world: &Arc<World>, head: BlockPos) {
    let bed = vanilla_blocks::WHITE_BED.default_state();
    // The head's `facing` points from the foot toward the head, so a foot to
    // the east makes a bed that faces west.
    assert!(
        world.set_block(
            head,
            bed.set_value(&BlockStateProperties::BED_PART, BedPart::Head)
                .set_value(
                    &BlockStateProperties::HORIZONTAL_FACING,
                    BlockDirection::West
                ),
            UpdateFlags::UPDATE_NONE,
        )
    );
    assert!(
        world.set_block(
            BlockPos::new(head.x() + 1, head.y(), head.z()),
            bed.set_value(&BlockStateProperties::BED_PART, BedPart::Foot)
                .set_value(
                    &BlockStateProperties::HORIZONTAL_FACING,
                    BlockDirection::West
                ),
            UpdateFlags::UPDATE_NONE,
        )
    );
}

/// Moves the overworld clock, which is what the `villager_schedule` timeline
/// samples -- game time alone does not change the hour of the day.
fn set_time_of_day(world: &Arc<World>, ticks: i64) {
    assert_eq!(
        world.set_clock_total_ticks(&vanilla_world_clocks::OVERWORLD, ticks),
        Some(()),
        "the overworld clock should exist in a test world"
    );
}

/// The container the rest of the villager's day hangs off.
///
/// `GoToWantedItem` walks a villager to a dropped stack, `makeBread` turns the
/// wheat in that stack into loaves and `TradeWithVillager` hands them on -- and
/// every one of them is inert unless the tick can actually put something in the
/// container. Three pieces have to agree for that: `canPickUpLoot`, the
/// `wantsToPickUp` override, and the `InventoryCarrier` seam that stows the
/// stack instead of equipping it.
///
/// This enters only through `villager.tick()`, the door the server tick uses.
#[test]
fn a_villager_stows_the_bread_it_walks_over() {
    let world = villager_world("villager_picks_up_bread");
    let villager = spawn_villager(&world);
    let dropped = world
        .spawn_item(SPAWN, ItemStack::with_count(&vanilla_items::BREAD, 3))
        .expect("the test chunk accepts an item entity");

    let taken = run_ticks_until(&world, &villager, 20, || dropped.is_removed());

    assert!(
        taken,
        "a villager standing on bread should have picked it up"
    );
    assert_eq!(
        villager.carried_inventory().lock().get_item(0).count(),
        3,
        "the bread has to land in the villager's own container, not its hands"
    );
    assert!(
        LivingEntity::get_item_by_slot(villager.as_ref(), EquipmentSlot::MainHand).is_empty(),
        "equipping the bread is exactly the failure the InventoryCarrier seam avoids"
    );
}

/// Vanilla parity: the `itemStack.is(ItemTags.VILLAGER_PICKS_UP)` half of
/// `Villager.wantsToPickUp`. Without it a villager with `canPickUpLoot` set
/// would hoover up whatever fell near it.
#[test]
fn a_villager_leaves_alone_what_it_does_not_collect() {
    let world = villager_world("villager_ignores_diamond");
    let villager = spawn_villager(&world);
    let dropped = world
        .spawn_item(SPAWN, ItemStack::new(&vanilla_items::DIAMOND))
        .expect("the test chunk accepts an item entity");

    run_ticks(&world, &villager, 20);

    assert!(
        !dropped.is_removed(),
        "a diamond is not a villager's business"
    );
    assert!(
        villager.carried_inventory().lock().is_empty(),
        "nothing outside VILLAGER_PICKS_UP should reach the container"
    );
}

/// Vanilla parity: the `profession().requestedItems().contains(item)` half of
/// `Villager.wantsToPickUp`. Bone meal is the one item that half decides on its
/// own -- it is not in `VILLAGER_PICKS_UP`, and only the farmer requests it.
#[test]
fn only_a_farmer_picks_up_bone_meal() {
    let world = villager_world("villager_bone_meal_is_the_farmers");
    let villager = spawn_villager(&world);
    let dropped = world
        .spawn_item(SPAWN, ItemStack::new(&vanilla_items::BONE_MEAL))
        .expect("the test chunk accepts an item entity");

    run_ticks(&world, &villager, 20);
    assert!(
        !dropped.is_removed(),
        "a villager with no profession requests nothing"
    );

    villager.set_profession(&vanilla_villager_professions::FARMER);
    let taken = run_ticks_until(&world, &villager, 20, || dropped.is_removed());

    assert!(taken, "a farmer requests bone meal and should take it");
}

/// The eight slots have to survive a save and load, or a villager that spent an
/// afternoon gathering wheat arrives at tomorrow empty-handed.
///
/// Vanilla parity: `AbstractVillager.addAdditionalSaveData`, which writes the
/// carried inventory under the shared `Inventory` tag.
#[test]
fn a_villager_carries_its_inventory_through_a_save_and_load() {
    let world = villager_world("villager_inventory_round_trip");
    let villager = spawn_villager(&world);
    villager
        .carried_inventory()
        .lock()
        .set_item(2, ItemStack::with_count(&vanilla_items::WHEAT, 7));

    let mut nbt = NbtCompound::new();
    villager.save_additional(&mut nbt);

    let mut bytes = Vec::new();
    nbt.write(&mut bytes);
    let borrowed = read_borrowed_compound(&mut Cursor::new(bytes.as_slice()))
        .unwrap_or_else(|error| panic!("villager nbt should reborrow: {error}"));

    let restored = Arc::new(VillagerEntity::new(
        &vanilla_entities::VILLAGER,
        next_entity_id(),
        SPAWN,
        Arc::downgrade(&world),
    ));
    restored.load_additional((&borrowed).into());

    let inventory = restored.carried_inventory().lock();
    let stack = inventory.get_item(0);
    assert!(
        stack.is(&vanilla_items::WHEAT) && stack.count() == 7,
        "the wheat should come back -- vanilla's `fromItemList` re-adds it, so it compacts to the first free slot"
    );
}

/// Vanilla parity: `SecondaryPoiSensor`, the only writer of
/// `SECONDARY_JOB_SITE`.
///
/// `HarvestFarmland` and the work package's `StrollToPoiList` both refuse to
/// start without that memory, so a farmer whose sensor never runs never works
/// its field -- and the memory is only ever written for the one profession that
/// registers a secondary POI at all.
///
/// This enters only through `villager.tick()`, which is what makes it fail if
/// the sensor is never added to the brain rather than merely written.
#[test]
fn only_a_farmer_notices_the_farmland_it_is_standing_on() {
    let world = villager_world("villager_secondary_job_site");
    let villager = spawn_villager(&world);
    for x in (STAND.x() - 4)..=(STAND.x() + 4) {
        for z in (STAND.z() - 4)..=(STAND.z() + 4) {
            assert!(world.set_block(
                BlockPos::new(x, STAND.y() - 1, z),
                vanilla_blocks::FARMLAND.default_state(),
                UpdateFlags::UPDATE_NONE,
            ));
        }
    }

    let brain = Mob::brain(villager.as_ref()).expect("a villager has a brain");
    // Two full scan rates, so the staggered first tick cannot be the reason.
    run_ticks(&world, &villager, 90);
    assert!(
        !brain.has_memory_value(memory_module_types::SECONDARY_JOB_SITE.id()),
        "farmland is the farmer's secondary POI, nobody else's"
    );

    // A profession only sticks while the villager holds the workstation that
    // grants it -- `ResetProfession` takes back a job with no job site.
    assert!(world.set_block(
        BlockPos::new(STAND.x() + 1, STAND.y(), STAND.z()),
        vanilla_blocks::COMPOSTER.default_state(),
        UpdateFlags::UPDATE_NONE,
    ));
    assert!(
        run_ticks_until(&world, &villager, TICKS_TO_TAKE_A_JOB, || {
            villager.profession().key.path == "farmer"
        }),
        "the composter should have made this villager a farmer"
    );

    let noticed = run_ticks_until(&world, &villager, 90, || {
        brain.has_memory_value(memory_module_types::SECONDARY_JOB_SITE.id())
    });

    assert!(noticed, "a farmer standing in a field should have seen it");
    let field = brain
        .get_memory(memory_module_types::SECONDARY_JOB_SITE)
        .expect("the memory was just asserted present");
    assert!(
        field
            .iter()
            .all(|pos| pos.dimension == world.key && pos.pos.y() == STAND.y() - 1),
        "the sensor should report the farmland it scanned, in this dimension"
    );
}

/// The five-by-five field the farming test plants and then watches.
const FIELD_RADIUS: i32 = 2;

/// Where the farming test puts the composter that makes the villager a farmer.
///
/// It has to be next to the villager: `AcquirePoi` only claims a workstation it
/// can path to, and the composter's own POI reach is one block.
const COMPOSTER: BlockPos = BlockPos::new(STAND.x() + 1, STAND.y(), STAND.z());

/// Every square of that field, at crop height.
///
/// The composter stands in the middle of it and is not farmland, so it is not
/// one of the squares the assertions watch -- leaving it in would make "some
/// square is no longer ripe wheat" true before the villager had done anything.
fn field() -> Vec<BlockPos> {
    let mut positions = Vec::new();
    for x in (STAND.x() - FIELD_RADIUS)..=(STAND.x() + FIELD_RADIUS) {
        for z in (STAND.z() - FIELD_RADIUS)..=(STAND.z() + FIELD_RADIUS) {
            let pos = BlockPos::new(x, STAND.y(), z);
            if pos != COMPOSTER {
                positions.push(pos);
            }
        }
    }
    positions
}

/// Lays farmland under the field and ripe wheat on top of it.
fn sow_a_ripe_field(world: &Arc<World>) {
    let ripe = vanilla_blocks::WHEAT
        .default_state()
        .set_value(&BlockStateProperties::AGE_7, 7);
    for pos in field() {
        assert!(world.set_block(
            pos.below(),
            vanilla_blocks::FARMLAND.default_state(),
            UpdateFlags::UPDATE_NONE,
        ));
        assert!(world.set_block(pos, ripe, UpdateFlags::UPDATE_NONE));
    }
}

/// The farming loop, end to end: a farmer walks into a ripe field, pulls the
/// wheat and puts a seed back in the ground.
///
/// Three separate pieces have to be right for this at once -- the
/// `SECONDARY_POIS` sensor `HarvestFarmland` is gated on, the `crop_is_max_age`
/// block seam that tells a ripe crop from a growing one, and the container the
/// seeds come out of. It enters only through `villager.tick()`.
#[test]
fn a_farmer_pulls_ripe_wheat_and_puts_a_seed_back_in_the_ground() {
    let world = villager_world("villager_harvests_farmland");
    let villager = spawn_villager(&world);
    sow_a_ripe_field(&world);
    assert!(world.set_block(
        COMPOSTER,
        vanilla_blocks::COMPOSTER.default_state(),
        UpdateFlags::UPDATE_NONE,
    ));
    villager
        .carried_inventory()
        .lock()
        .set_item(0, ItemStack::with_count(&vanilla_items::WHEAT_SEEDS, 8));

    // 2000..9000 is the WORK stretch of the schedule, and nothing here ticks
    // the day clock, so the villager stays in working hours for the whole run.
    set_time_of_day(&world, 3_000);
    assert!(
        run_ticks_until(&world, &villager, TICKS_TO_TAKE_A_JOB, || {
            villager.profession().key.path == "farmer"
        }),
        "the composter should have made this villager a farmer"
    );

    assert!(
        field()
            .iter()
            .all(|&pos| world.get_block_state(pos).crop_is_max_age() == Some(true)),
        "every square of the field starts as ripe wheat"
    );
    let harvested = || {
        field()
            .iter()
            .any(|&pos| world.get_block_state(pos).crop_is_max_age() != Some(true))
    };
    assert!(
        run_ticks_until(&world, &villager, 8_000, harvested),
        "a farmer standing in ripe wheat should have pulled some of it"
    );

    let replanted = || {
        field().iter().any(|&pos| {
            let state = world.get_block_state(pos);
            state.get_block().key == vanilla_blocks::WHEAT.key
                && state.get_value(&BlockStateProperties::AGE_7) == 0
        })
    };
    assert!(
        run_ticks_until(&world, &villager, 8_000, replanted),
        "the square it pulled should have been sown again from its own seeds"
    );
}

/// What the harvest is for: a farmer standing at its composter turns the wheat
/// it is carrying into bread, which is the only food a village breeds on.
///
/// Vanilla parity: `WorkAtComposter.makeBread`, reached through
/// `WorkAtPoi.useWorkstation`. This enters only through `villager.tick()`, so
/// it fails if the hook is never called as well as if the baking is wrong.
#[test]
fn a_farmer_at_its_composter_bakes_its_wheat_into_bread() {
    let world = villager_world("villager_makes_bread");
    let villager = spawn_villager(&world);
    assert!(world.set_block(
        BlockPos::new(STAND.x() + 1, STAND.y(), STAND.z()),
        vanilla_blocks::COMPOSTER.default_state(),
        UpdateFlags::UPDATE_NONE,
    ));

    set_time_of_day(&world, 3_000);
    assert!(
        run_ticks_until(&world, &villager, TICKS_TO_TAKE_A_JOB, || {
            villager.profession().key.path == "farmer"
        }),
        "the composter should have made this villager a farmer"
    );
    villager
        .carried_inventory()
        .lock()
        .set_item(0, ItemStack::with_count(&vanilla_items::WHEAT, 9));

    // `WorkAtPoi` only looks every three hundred ticks, and then on a coin flip.
    assert!(
        run_ticks_until(&world, &villager, 4_000, || {
            villager
                .carried_inventory()
                .lock()
                .count_item(&vanilla_items::BREAD)
                > 0
        }),
        "nine wheat at a composter is three loaves"
    );

    let inventory = villager.carried_inventory().lock();
    assert_eq!(
        inventory.count_item(&vanilla_items::BREAD),
        3,
        "vanilla bakes at most three loaves a visit, out of three wheat each"
    );
    assert_eq!(
        inventory.count_item(&vanilla_items::WHEAT),
        0,
        "and takes the nine wheat back out of the container"
    );
}

/// The bread a villager is carrying reaches the villager who is short of it.
///
/// Vanilla parity: `TradeWithVillager`, which is what stops a farmer hoarding a
/// harvest the rest of the village cannot eat. It rides on the container and on
/// the idle gate that picks an `INTERACTION_TARGET`, so this fails if either is
/// missing -- and it enters only through the two villagers' own ticks.
#[test]
fn a_villager_with_food_to_spare_throws_half_of_it_to_a_neighbour() {
    const CARRIED: i32 = 40;

    let world = villager_world("villager_shares_food");
    let giver = spawn_villager(&world);
    let taker = spawn_villager(&world);
    giver
        .carried_inventory()
        .lock()
        .set_item(0, ItemStack::with_count(&vanilla_items::BREAD, CARRIED));
    assert!(
        giver.has_excess_food() && taker.wants_more_food(),
        "the giver has more than it needs and the taker has nothing"
    );

    // 10..2000 is the IDLE stretch, the package the swap gate is in.
    set_time_of_day(&world, 1_000);
    let mut shared = false;
    for _ in 0..6_000 {
        advance_time(&world);
        giver.base_tick();
        giver.tick();
        taker.base_tick();
        taker.tick();
        if giver
            .carried_inventory()
            .lock()
            .count_item(&vanilla_items::BREAD)
            < CARRIED
        {
            shared = true;
            break;
        }
    }

    assert!(
        shared,
        "a villager with bread to spare should have shared it"
    );
    assert_eq!(
        giver
            .carried_inventory()
            .lock()
            .count_item(&vanilla_items::BREAD),
        CARRIED / 2,
        "vanilla throws half a stack that is more than half full"
    );
}

/// A villager gives a present back to the player who saved its village.
///
/// Vanilla parity: `GiveGiftToHero`. It rides on `NEAREST_VISIBLE_PLAYER`, on
/// the hero effect, and on the gift loot table its profession names -- and it
/// enters only through `villager.tick()`.
#[test]
fn a_villager_throws_a_gift_at_the_hero_of_the_village() {
    let world = villager_world("villager_gift");
    let villager = spawn_villager(&world);

    let hero = TestPlayerBuilder::new(Arc::clone(&world), "Hero", next_entity_id()).build();
    hero.try_set_position(SPAWN)
        .expect("the test chunk is loaded, so the hero can stand in it");
    assert!(world.players.insert(Arc::clone(&hero)));
    world
        .try_add_entity(Arc::clone(&hero) as SharedEntity)
        .expect("the test chunk is loaded, so the hero should attach");
    assert!(
        LivingEntity::add_mob_effect(
            hero.as_ref(),
            MobEffectInstance::with_duration(vanilla_mob_effects::HERO_OF_THE_VILLAGE, 20_000, 0),
        ),
        "the hero has to actually carry the effect the behavior looks for"
    );

    // 10..2000 is the IDLE stretch, one of the three packages the gift is in.
    set_time_of_day(&world, 1_000);
    let gift_near_the_villager = || {
        let around = WorldAabb::new(
            SPAWN.x - 8.0,
            SPAWN.y - 4.0,
            SPAWN.z - 8.0,
            SPAWN.x + 8.0,
            SPAWN.y + 4.0,
            SPAWN.z + 8.0,
        );
        !world
            .get_entities_in_aabb_matching(&around, |entity| {
                entity.downcast_ref::<ItemEntity>().is_some()
            })
            .is_empty()
    };

    // The countdown before a villager will offer a gift is six hundred ticks,
    // and it only runs down while a hero is in sight.
    assert!(
        run_ticks_until(&world, &villager, 4_000, gift_near_the_villager),
        "a villager that can see a hero of the village should have thrown it something"
    );
}

/// The activity the villager's own brain is currently in.
fn active_activity(villager: &Arc<VillagerEntity>) -> Option<Activity> {
    Mob::brain(villager.as_ref())?.active_non_core_activity()
}

/// The routing the whole day hangs off: the `villager_schedule` timeline names
/// an activity, and the brain switches to it.
///
/// This enters only through `villager.tick()`, so it fails if the brain is never
/// ticked, if the string-valued track is never sampled, or if
/// `UpdateActivityFromSchedule` is not actually in the packages.
///
/// The workstation and the bell are there because vanilla gates WORK on
/// `JOB_SITE` and MEET on `MEETING_POINT`: a villager with neither would fall
/// back to IDLE all day, which is correct and would prove nothing.
#[test]
fn the_clock_walks_a_villager_through_its_working_day() {
    let world = villager_world("villager_schedule_routes");
    let villager = spawn_villager(&world);
    assert!(world.set_block(
        BlockPos::new(STAND.x() + 1, STAND.y(), STAND.z()),
        vanilla_blocks::CARTOGRAPHY_TABLE.default_state(),
        UpdateFlags::UPDATE_NONE,
    ));
    assert!(world.set_block(
        BlockPos::new(STAND.x() - 1, STAND.y(), STAND.z()),
        vanilla_blocks::BELL.default_state(),
        UpdateFlags::UPDATE_NONE,
    ));
    run_ticks(&world, &villager, TICKS_TO_TAKE_A_JOB);
    assert_eq!(villager.profession().key.path, "cartographer");
    assert!(
        Mob::brain(villager.as_ref())
            .expect("a villager has a brain")
            .has_memory_value(memory_module_types::MEETING_POINT.id()),
        "the bell is the village's meeting point"
    );

    // 2000..9000 is the WORK stretch of `Timelines.VILLAGER_SCHEDULE`.
    set_time_of_day(&world, 3_000);
    run_ticks(&world, &villager, 60);
    assert_eq!(
        active_activity(&villager),
        Some(Activity::Work),
        "the schedule puts a villager to work in the morning"
    );

    // 9000..11000 is MEET.
    set_time_of_day(&world, 10_000);
    run_ticks(&world, &villager, 60);
    assert_eq!(active_activity(&villager), Some(Activity::Meet));

    // 12000 onward is REST.
    set_time_of_day(&world, 13_000);
    run_ticks(&world, &villager, 60);
    assert_eq!(
        active_activity(&villager),
        Some(Activity::Rest),
        "and sends it home at dusk"
    );
}

/// A villager with no workstation has no working hours to keep.
///
/// Vanilla gates the WORK activity on `JOB_SITE`, so the schedule naming WORK
/// is not enough on its own -- without this the whole village would clock in at
/// an imaginary bench.
#[test]
fn an_unemployed_villager_stays_idle_through_the_working_hours() {
    let world = villager_world("villager_no_work_without_a_site");
    let villager = spawn_villager(&world);

    set_time_of_day(&world, 3_000);
    run_ticks(&world, &villager, 60);

    assert_eq!(
        active_activity(&villager),
        Some(Activity::Idle),
        "WORK falls back to the default activity when there is no job site"
    );
}

/// A baby reads the other track of the same timeline, so the hour that puts an
/// adult to work puts a child at play.
#[test]
fn a_baby_villager_plays_the_hours_an_adult_works() {
    let world = villager_world("villager_baby_schedule");
    let villager = spawn_villager(&world);
    AgeableMob::set_age(&*villager, -24_000);
    assert!(AgeableMob::is_baby(&*villager));

    set_time_of_day(&world, 3_500);
    run_ticks(&world, &villager, 60);
    assert_eq!(
        active_activity(&villager),
        Some(Activity::Play),
        "3000..6000 is PLAY on the baby track and WORK on the adult one"
    );
}

/// The most visible thing a villager does with its day.
///
/// Everything between the server tick and the bed has to work for this to pass:
/// the timeline sample, the schedule routing, `AcquirePoi` claiming the bed,
/// the REST package reaching `SleepInBed`, and `WakeUp` in the core package
/// getting it out again when the hour turns.
#[test]
fn a_villager_sleeps_in_its_bed_at_night_and_gets_up_in_the_morning() {
    let world = villager_world("villager_sleeps");
    let villager = spawn_villager(&world);
    place_bed(&world, BlockPos::new(STAND.x() + 1, STAND.y(), STAND.z()));

    set_time_of_day(&world, 13_000);
    run_ticks(&world, &villager, 200);

    assert_eq!(
        villager_home(&villager),
        Some(BlockPos::new(STAND.x() + 1, STAND.y(), STAND.z())),
        "a villager claims the bed it is standing next to"
    );
    assert!(
        LivingEntity::is_sleeping(&*villager),
        "and is in it once the schedule says REST"
    );

    // 2000..9000 is WORK, so the villager has no business in bed.
    set_time_of_day(&world, 3_000);
    run_ticks(&world, &villager, 60);

    assert!(
        !LivingEntity::is_sleeping(&*villager),
        "and gets up when the schedule moves on"
    );
}

/// The bed this villager holds a POI ticket on.
fn villager_home(villager: &Arc<VillagerEntity>) -> Option<BlockPos> {
    Mob::brain(villager.as_ref())?
        .get_memory(memory_module_types::HOME)
        .map(|global| global.pos)
}

/// The courtship is 275 to 325 ticks once it starts, and the pair have to pick
/// each other out of the idle gate before that.
const TICKS_TO_RAISE_A_CHILD: i32 = 800;

/// Fills a villager's inventory with enough bread to be willing to breed.
///
/// Vanilla parity: `Villager.canBreed` adds the food level to the food points
/// in the inventory and wants twelve; bread is four points each.
fn feed_for_breeding(villager: &Arc<VillagerEntity>) {
    villager
        .carried_inventory()
        .lock()
        .set_item(0, ItemStack::with_count(&vanilla_items::BREAD, 3));
    assert!(
        villager.can_breed(),
        "three bread is the breeding threshold"
    );
}

/// Finds a baby villager near the village, which is what a birth leaves behind.
fn find_baby_villager(world: &Arc<World>) -> Option<SharedEntity> {
    let around_the_village = WorldAabb::new(
        f64::from(STAND.x() - 16),
        f64::from(STAND.y() - 8),
        f64::from(STAND.z() - 16),
        f64::from(STAND.x() + 16),
        f64::from(STAND.y() + 8),
        f64::from(STAND.z() + 16),
    );
    world
        .get_entities_in_aabb_matching(&around_the_village, |entity| {
            entity
                .downcast_ref::<VillagerEntity>()
                .is_some_and(AgeableMob::is_baby)
        })
        .into_iter()
        .next()
}

/// A village grows on its own, which it could not do at all before the brain.
///
/// This enters only through the two villagers' own ticks: the idle gate has to
/// pick a breeding partner, `VillagerMakeLove` has to court it, and the pair
/// have to take a bed for the child before it can be born.
///
/// Three beds for two villagers, because the two claim one each as their own
/// home the moment they see them -- a village only grows when it has a bed
/// spare, which is exactly what makes adding beds the way a player grows one.
#[test]
fn two_fed_villagers_with_a_spare_bed_have_a_child() {
    let world = villager_world("villager_breeding");
    let first = spawn_villager(&world);
    let second = spawn_villager(&world);
    feed_for_breeding(&first);
    feed_for_breeding(&second);
    let beds = [
        BlockPos::new(STAND.x() - 3, STAND.y(), STAND.z() - 2),
        BlockPos::new(STAND.x() - 3, STAND.y(), STAND.z()),
        BlockPos::new(STAND.x() - 3, STAND.y(), STAND.z() + 2),
    ];
    for bed in beds {
        place_bed(&world, bed);
    }

    // 10..2000 is the IDLE stretch, which is the package the breeding gate is in.
    set_time_of_day(&world, 1_000);
    for _ in 0..TICKS_TO_RAISE_A_CHILD {
        advance_time(&world);
        first.base_tick();
        first.tick();
        second.base_tick();
        second.tick();
    }

    let baby = find_baby_villager(&world).expect("the pair should have had a child");
    let baby = baby
        .downcast_ref::<VillagerEntity>()
        .expect("find_baby_villager only returns villagers");
    let child_home = Mob::brain(baby)
        .and_then(|brain| brain.get_memory(memory_module_types::HOME))
        .map(|home| home.pos);
    assert!(
        child_home.is_some_and(|home| beds.contains(&home)),
        "the bed the parents took for it is already the child's home, got {child_home:?}"
    );
    assert!(
        !first.can_breed() && !second.can_breed(),
        "both parents are on the six-thousand-tick cooldown a birth costs"
    );
}

/// The WORK package really reaches `WorkAtPoi`, which is the one behavior in it
/// that does anything a player would call working.
///
/// `LAST_WORKED_AT_POI` is the only mark it leaves that outlives the tick -- the
/// rest is a sound and a restock -- so it is what proves the package was
/// reached rather than merely registered.
#[test]
fn a_villager_at_its_workstation_puts_in_a_shift() {
    let world = villager_world("villager_works");
    let villager = spawn_villager(&world);

    let table = BlockPos::new(STAND.x() + 1, STAND.y(), STAND.z());
    assert!(world.set_block(
        table,
        vanilla_blocks::CARTOGRAPHY_TABLE.default_state(),
        UpdateFlags::UPDATE_NONE,
    ));

    // 2000..9000 is the WORK stretch of the schedule.
    set_time_of_day(&world, 3_000);
    run_ticks(&world, &villager, TICKS_TO_TAKE_A_JOB);
    assert_eq!(villager_job_site(&villager), Some(table));
    assert_eq!(
        active_activity(&villager),
        Some(Activity::Work),
        "the clock says these are working hours"
    );

    // `WorkAtPoi` only checks every three hundred ticks, and then only on a
    // coin flip.
    assert!(
        run_ticks_until(&world, &villager, 2_000, || {
            Mob::brain(villager.as_ref())
                .expect("a villager has a brain")
                .has_memory_value(memory_module_types::LAST_WORKED_AT_POI.id())
        }),
        "a villager standing at its own workstation during working hours works at it"
    );
}

/// Mining a villager's workstation out from under it puts it out of work.
///
/// Two behaviors in a row: `ValidateNearbyPoi` notices the block it remembers
/// is no longer that kind of point of interest, and `ResetProfession` fires a
/// villager that has lost its site and never earned anything at it.
#[test]
fn breaking_a_workstation_puts_its_villager_out_of_work() {
    let world = villager_world("villager_loses_job");
    let villager = spawn_villager(&world);

    let table = BlockPos::new(STAND.x() + 1, STAND.y(), STAND.z());
    assert!(world.set_block(
        table,
        vanilla_blocks::CARTOGRAPHY_TABLE.default_state(),
        UpdateFlags::UPDATE_NONE,
    ));
    run_ticks(&world, &villager, TICKS_TO_TAKE_A_JOB);
    assert_eq!(villager.profession().key.path, "cartographer");

    assert!(world.set_block(
        table,
        vanilla_blocks::AIR.default_state(),
        UpdateFlags::UPDATE_NONE,
    ));
    run_ticks(&world, &villager, 60);

    assert_eq!(
        villager_job_site(&villager),
        None,
        "the block it remembered is not a workstation any more"
    );
    assert_eq!(
        villager.profession().key.path,
        "none",
        "and a villager with no site and no experience goes back to unemployed"
    );
}

/// The panic path, from the sensor that sees the monster to the activity switch.
#[test]
fn a_villager_panics_when_a_zombie_comes_close() {
    let world = villager_world("villager_panics");
    let villager = spawn_villager(&world);

    // A zombie is frightening within eight blocks, per
    // `VillagerHostilesSensor.ACCEPTABLE_DISTANCE_FROM_HOSTILES`.
    let zombie = Arc::new(ZombieEntity::new(
        &vanilla_entities::ZOMBIE,
        next_entity_id(),
        DVec3::new(SPAWN.x + 3.0, SPAWN.y, SPAWN.z),
        Arc::downgrade(&world),
    ));
    world
        .try_add_entity(Arc::clone(&zombie) as SharedEntity)
        .expect("the test chunk is loaded");

    // Broad daylight, so the schedule would otherwise have it idling.
    set_time_of_day(&world, 3_000);
    assert!(
        run_ticks_until(&world, &villager, 400, || active_activity(&villager)
            == Some(Activity::Panic)),
        "a villager that can see a zombie drops what it was doing"
    );
}

#[test]
fn a_villagers_biome_variant_reaches_the_trades_that_are_gated_on_it() {
    let world = villager_world("villager_variant_gates_trades");
    let villager = spawn_villager(&world);

    // `villager_loot_variant` is what a trade's `merchant_predicate` reads.
    assert_eq!(
        LivingEntity::villager_loot_variant(&*villager).map(|key| key.path.as_ref()),
        Some("plains"),
        "a villager publishes its variant to the loot context"
    );
}
