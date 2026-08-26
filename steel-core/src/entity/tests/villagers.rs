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
use crate::behavior::init_behaviors;
use crate::block_entity::init_block_entities;
use crate::entity::ai::brain::Activity;
use crate::entity::ai::brain::memory::memory_module_types;
use crate::entity::ai::gossip::{GossipType, ReputationEventType};
use crate::entity::entities::VillagerEntity;
use crate::entity::{AgeableMob, LivingEntity, Mob, SharedEntity, next_entity_id};
use crate::poi::poi_storage::OccupationStatus;
use crate::test_support::{fresh_test_world, insert_ready_full_chunk};
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

/// Ticks the villager long enough for the jittered job-site scan to have run.
///
/// `AcquirePoi` books its first scan up to twenty ticks out and then re-books
/// every twenty-plus-jitter, so a hundred ticks is several scans' worth.
fn run_ticks(world: &Arc<World>, villager: &Arc<VillagerEntity>, ticks: i32) {
    for _ in 0..ticks {
        advance_time(world);
        villager.base_tick();
        villager.tick();
    }
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

    run_ticks(&world, &villager, 100);

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

    for _ in 0..100 {
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
    run_ticks(&world, &villager, 100);
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

    run_ticks(&world, &villager, 100);

    assert_eq!(
        villager_job_site(&villager),
        None,
        "a baby leaves the workstation for the adults"
    );
    assert_eq!(villager.profession().key.path, "none");

    // And the table really was claimable, so the assertion above is about the
    // baby rather than about an unreachable table.
    let adult = spawn_villager(&world);
    run_ticks(&world, &adult, 100);
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
#[test]
fn the_clock_walks_a_villager_through_its_working_day() {
    let world = villager_world("villager_schedule_routes");
    let villager = spawn_villager(&world);

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
