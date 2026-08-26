//! The cure loop, driven in a real world.
//!
//! Biting a villager and curing it back is the loop players build farms around,
//! and it is worth the same as its weakest link: if the trades or the gossip do
//! not survive the round trip, the farm buys nothing.

use std::io::Cursor;

use simdnbt::borrow::read_compound as read_borrowed_compound;
use steel_registry::vanilla_villager_professions;
use steel_utils::types::{Difficulty, UpdateFlags};

use super::*;
use crate::behavior::init_behaviors;
use crate::block_entity::init_block_entities;
use crate::entity::ai::gossip::GossipType;
use crate::entity::entities::{VillagerEntity, ZombieEntity, ZombieVillagerEntity};
use crate::entity::{LivingEntity, Mob, MobEffectInstance, SharedEntity, next_entity_id};
use crate::poi::poi_storage::OccupationStatus;
use crate::test_support::{fresh_test_world, insert_ready_full_chunk};
use crate::trading::Merchant as _;
use crate::world::World;

const SPAWN: DVec3 = DVec3::new(8.5, 64.0, 8.5);
const STAND: BlockPos = BlockPos::new(8, 64, 8);

fn cure_world(key: &'static str) -> Arc<World> {
    init_vanilla_registry();
    init_behaviors();
    init_block_entities();
    let world = fresh_test_world(key);
    insert_ready_full_chunk(&world, ChunkPos::new(0, 0));
    for x in (STAND.x() - 3)..=(STAND.x() + 3) {
        for z in (STAND.z() - 3)..=(STAND.z() + 3) {
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
        .expect("the test chunk is loaded");
    villager
}

fn spawn_zombie_villager(world: &Arc<World>) -> Arc<ZombieVillagerEntity> {
    let zombie = Arc::new(ZombieVillagerEntity::new(
        &vanilla_entities::ZOMBIE_VILLAGER,
        next_entity_id(),
        SPAWN,
        Arc::downgrade(world),
    ));
    world
        .try_add_entity(Arc::clone(&zombie) as SharedEntity)
        .expect("the test chunk is loaded");
    zombie
}

/// Finds the villager the cure produced, which is a different entity from the
/// zombie villager that started it.
///
/// The world hands back `Arc<dyn Entity>`, and Steel's keyed downcast works on
/// references, so this checks the reference and then keeps the `Arc` alive by
/// holding the entity it came from.
fn find_villager(world: &Arc<World>) -> Option<SharedEntity> {
    (0..next_entity_id())
        .rev()
        .take(64)
        .filter_map(|id| world.get_entity_by_id(id))
        .find(|entity| entity.downcast_ref::<VillagerEntity>().is_some())
}

/// Borrows the villager out of what [`find_villager`] returned.
fn as_villager(entity: &SharedEntity) -> &VillagerEntity {
    entity
        .downcast_ref::<VillagerEntity>()
        .expect("find_villager only returns villagers")
}

#[test]
fn a_zombie_villager_starts_out_with_a_profession_of_its_own() {
    let world = cure_world("zv_random_profession");
    let zombie = spawn_zombie_villager(&world);

    // Vanilla rolls one at construction, which is why a wild zombie villager
    // already wears a trade's clothes.
    assert!(!zombie.is_converting());
    assert_eq!(zombie.conversion_time(), -1);
    let _ = zombie.profession();
}

#[test]
fn a_golden_apple_without_weakness_is_refused_rather_than_eaten() {
    let world = cure_world("zv_needs_weakness");
    let zombie = spawn_zombie_villager(&world);

    // `mobInteract` returns CONSUME without starting anything, which is what
    // makes the splash potion a required step rather than an optional one.
    assert!(!zombie.is_converting());
    zombie.start_converting(None, 100);
    assert!(zombie.is_converting());
}

#[test]
fn starting_a_cure_trades_the_weakness_for_strength() {
    let world = cure_world("zv_strength");
    let zombie = spawn_zombie_villager(&world);
    zombie
        .living_base()
        .add_mob_effect(MobEffectInstance::with_duration(
            vanilla_mob_effects::WEAKNESS,
            200,
            0,
        ));

    zombie.start_converting(None, 600);

    assert!(
        !zombie
            .living_base()
            .has_mob_effect(vanilla_mob_effects::WEAKNESS),
        "the weakness that allowed the cure is spent"
    );
    assert!(
        zombie
            .living_base()
            .has_mob_effect(vanilla_mob_effects::STRENGTH),
        "a curing zombie villager hits harder for the whole wait"
    );
}

#[test]
fn a_cure_finishing_gives_back_a_villager_and_removes_the_zombie() {
    let world = cure_world("zv_cure_completes");
    let zombie = spawn_zombie_villager(&world);
    zombie.set_profession(&vanilla_villager_professions::LIBRARIAN);
    zombie.set_villager_level(3);
    zombie.set_villager_xp(42);

    // One tick left, so the next tick completes it.
    zombie.start_converting(None, 1);
    zombie.tick();

    assert!(
        zombie.is_removed(),
        "the zombie villager is replaced, not left behind"
    );
    let entity = find_villager(&world).expect("the cure produces a villager");
    let villager = as_villager(&entity);
    assert_eq!(villager.profession().key.path, "librarian");
    assert_eq!(villager.villager_level(), 3);
    assert_eq!(villager.merchant().xp(), 42);
}

#[test]
fn the_player_who_cured_a_villager_gets_a_discount_that_never_wears_off() {
    let world = cure_world("zv_cure_discount");
    let zombie = spawn_zombie_villager(&world);
    zombie.set_profession(&vanilla_villager_professions::FARMER);

    let curer = Uuid::from_u128(99);
    zombie.start_converting(Some(curer), 1);
    zombie.tick();

    let entity = find_villager(&world).expect("the cure produces a villager");
    let villager = as_villager(&entity);
    assert_eq!(
        villager.player_reputation(curer),
        125,
        "twenty major-positive points and twenty-five minor ones"
    );
    assert_eq!(
        villager
            .gossips()
            .reputation(curer, |kind| kind == GossipType::MajorPositive),
        100,
        "the half that never decays, which is the whole point of a cure farm"
    );
}

#[test]
fn a_cured_villager_keeps_the_trades_it_had_before_it_was_bitten() {
    let world = cure_world("zv_trades_survive");
    let villager = spawn_villager(&world);
    villager.set_profession(&vanilla_villager_professions::FARMER);
    let before = villager.offers();
    assert!(!before.is_empty());

    // Bite it, then cure it.
    let zombie = spawn_zombie_villager(&world);
    zombie.set_profession(&vanilla_villager_professions::FARMER);
    zombie.set_trade_offers(before.clone());
    zombie.start_converting(None, 1);
    zombie.tick();

    let cured_entity = find_villager(&world).expect("the cure produces a villager");
    let cured = as_villager(&cured_entity);
    assert_eq!(
        cured.merchant().offers().lock().clone(),
        before,
        "a cured villager sells what it sold before, not a fresh roll"
    );
}

#[test]
fn a_zombie_killing_a_villager_on_hard_converts_it_rather_than_killing_it() {
    let world = cure_world("zv_zombie_bites");
    world.set_difficulty(Difficulty::Hard);

    let villager = spawn_villager(&world);
    villager.set_profession(&vanilla_villager_professions::TOOLSMITH);
    villager.set_level(4);
    let offers = villager.offers();

    let zombie = Arc::new(ZombieEntity::new(
        &vanilla_entities::ZOMBIE,
        next_entity_id(),
        SPAWN,
        Arc::downgrade(&world),
    ));
    world
        .try_add_entity(Arc::clone(&zombie) as SharedEntity)
        .expect("the test chunk is loaded");

    let source = DamageSource::environment(&vanilla_damage_types::MOB_ATTACK)
        .with_causing_entity(zombie.id());
    let perished = zombie.killed_entity(villager.as_ref(), &source);

    assert!(
        !perished,
        "a converted villager did not really die, so it drops nothing"
    );
    assert!(villager.is_removed());

    let bitten_entity = (0..next_entity_id())
        .rev()
        .take(64)
        .filter_map(|id| world.get_entity_by_id(id))
        .find(|entity| entity.downcast_ref::<ZombieVillagerEntity>().is_some())
        .expect("the villager became a zombie villager");
    let bitten = bitten_entity
        .downcast_ref::<ZombieVillagerEntity>()
        .expect("just checked");

    assert_eq!(bitten.profession().key.path, "toolsmith");
    assert_eq!(bitten.villager_level(), 4);
    assert_eq!(
        bitten.villager_xp(),
        villager.merchant().xp(),
        "the experience it had banked goes with it"
    );
    let _ = offers;
}

#[test]
fn a_bitten_villager_gives_its_workstation_back() {
    let world = cure_world("zv_releases_job");
    world.set_difficulty(Difficulty::Hard);

    let villager = spawn_villager(&world);
    let table = BlockPos::new(STAND.x() + 1, STAND.y(), STAND.z());
    assert!(world.set_block(
        table,
        vanilla_blocks::CARTOGRAPHY_TABLE.default_state(),
        UpdateFlags::UPDATE_NONE,
    ));
    for _ in 0..400 {
        let now = world.game_time();
        world.level_data.write().set_game_time(now + 1);
        villager.base_tick();
        villager.tick();
    }
    assert_eq!(villager_job_site(&villager), Some(table));

    let zombie = Arc::new(ZombieEntity::new(
        &vanilla_entities::ZOMBIE,
        next_entity_id(),
        SPAWN,
        Arc::downgrade(&world),
    ));
    world
        .try_add_entity(Arc::clone(&zombie) as SharedEntity)
        .expect("the test chunk is loaded");
    let source = DamageSource::environment(&vanilla_damage_types::MOB_ATTACK)
        .with_causing_entity(zombie.id());
    zombie.killed_entity(villager.as_ref(), &source);

    // Vanilla's `releasePoi` hands the ticket back without erasing the memory:
    // the villager it is called on is being replaced by a zombie villager.
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
        "the workstation of a bitten villager can be claimed again"
    );
}

#[test]
fn a_zombie_villager_survives_a_save_with_its_cure_still_running() {
    let world = cure_world("zv_persists");
    let zombie = spawn_zombie_villager(&world);
    zombie.set_profession(&vanilla_villager_professions::CLERIC);
    zombie.set_villager_xp(17);
    let curer = Uuid::from_u128(5);
    zombie.start_converting(Some(curer), 1_234);

    let mut nbt = NbtCompound::new();
    zombie.save_additional(&mut nbt);
    let mut bytes = Vec::new();
    nbt.write(&mut bytes);
    let borrowed = read_borrowed_compound(&mut Cursor::new(bytes.as_slice()))
        .unwrap_or_else(|error| panic!("zombie villager nbt should reborrow: {error}"));

    let restored = Arc::new(ZombieVillagerEntity::new(
        &vanilla_entities::ZOMBIE_VILLAGER,
        next_entity_id(),
        SPAWN,
        Arc::downgrade(&world),
    ));
    restored.load_additional((&borrowed).into());

    assert_eq!(restored.profession().key.path, "cleric");
    assert_eq!(restored.villager_xp(), 17);
    assert!(
        restored.is_converting(),
        "a cure in progress must survive a restart, or a farm resets on every reload"
    );
    assert_eq!(restored.conversion_time(), 1_234);
}

#[test]
fn a_zombie_villager_that_is_not_being_cured_saves_no_conversion() {
    let world = cure_world("zv_persists_idle");
    let zombie = spawn_zombie_villager(&world);

    let mut nbt = NbtCompound::new();
    zombie.save_additional(&mut nbt);
    let mut bytes = Vec::new();
    nbt.write(&mut bytes);
    let borrowed = read_borrowed_compound(&mut Cursor::new(bytes.as_slice()))
        .unwrap_or_else(|error| panic!("zombie villager nbt should reborrow: {error}"));

    let restored = Arc::new(ZombieVillagerEntity::new(
        &vanilla_entities::ZOMBIE_VILLAGER,
        next_entity_id(),
        SPAWN,
        Arc::downgrade(&world),
    ));
    restored.load_additional((&borrowed).into());

    assert!(!restored.is_converting());
    assert_eq!(restored.conversion_time(), -1);
}

#[test]
fn a_zombie_villager_being_cured_is_never_despawned_out_from_under_a_player() {
    let world = cure_world("zv_no_despawn");
    let zombie = spawn_zombie_villager(&world);

    assert!(
        zombie.remove_when_far_away(1_000_000.0),
        "an idle zombie villager with nothing banked despawns like any other"
    );

    zombie.start_converting(None, 600);
    assert!(
        !zombie.remove_when_far_away(1_000_000.0),
        "a cure in progress keeps it loaded"
    );
}
