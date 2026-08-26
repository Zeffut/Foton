//! Panda tests.

use std::sync::Weak;

use steel_registry::{init_vanilla_registry, vanilla_blocks, vanilla_damage_types};
use steel_utils::ChunkPos;
use steel_utils::types::UpdateFlags;

use super::*;
use crate::behavior::init_behaviors;
use crate::entity::next_entity_id;
use crate::test_support::{TestPlayerBuilder, fresh_test_world, insert_ready_full_chunk};

const TEST_POS: BlockPos = BlockPos::new(8, 64, 8);
const TEST_POSITION: DVec3 = DVec3::new(8.5, 64.0, 8.5);

fn new_panda() -> PandaEntity {
    init_vanilla_registry();
    PandaEntity::new(
        &vanilla_entities::PANDA,
        next_entity_id(),
        TEST_POSITION,
        Weak::new(),
    )
}

fn panda_world(key: &'static str) -> Arc<World> {
    init_vanilla_registry();
    init_behaviors();
    let world = fresh_test_world(key);
    insert_ready_full_chunk(&world, ChunkPos::from_block_pos(TEST_POS));
    assert!(world.set_block(
        TEST_POS.below(),
        vanilla_blocks::GRASS_BLOCK.default_state(),
        UpdateFlags::UPDATE_NONE
    ));
    world
}

/// Forces the storm on or off.
///
/// `is_thundering` reads the two lerped weather levels rather than the saved
/// flag, and those take two hundred ticks to climb; the test sets them itself.
fn set_thundering(world: &Arc<World>, thundering: bool) {
    let mut weather = world.weather.lock();
    let level = if thundering { 1.0 } else { 0.0 };
    weather.rain_level = level;
    weather.thunder_level = level;
}

fn live_panda(world: &Arc<World>) -> Arc<PandaEntity> {
    let panda = Arc::new(PandaEntity::new(
        &vanilla_entities::PANDA,
        next_entity_id(),
        TEST_POSITION,
        Arc::downgrade(world),
    ));
    world
        .try_add_entity(Arc::clone(&panda) as SharedEntity)
        .unwrap_or_else(|error| panic!("panda should enter the test world: {error:?}"));
    panda
}

#[test]
fn a_recessive_gene_only_shows_when_a_panda_has_two_of_it() {
    // This is the whole reason a brown panda is rare: one brown gene shows
    // nothing at all, and the pair has to meet a generation later.
    assert_eq!(
        PandaGene::variant_from_genes(PandaGene::Brown, PandaGene::Normal),
        PandaGene::Normal
    );
    assert_eq!(
        PandaGene::variant_from_genes(PandaGene::Brown, PandaGene::Brown),
        PandaGene::Brown
    );
    assert_eq!(
        PandaGene::variant_from_genes(PandaGene::Weak, PandaGene::Brown),
        PandaGene::Normal
    );

    // A dominant gene shows whatever is hiding behind it.
    assert_eq!(
        PandaGene::variant_from_genes(PandaGene::Lazy, PandaGene::Brown),
        PandaGene::Lazy
    );
}

#[test]
fn the_gene_roll_matches_the_weights_vanilla_uses() {
    // Every one of the sixteen rolls, so a table that drifted would be caught
    // rather than averaged away. Weak is five of them, which is why it is the
    // gene a wild panda most often hides.
    let rolled: Vec<PandaGene> = (0..16).map(PandaGene::from_roll).collect();

    assert_eq!(rolled[0], PandaGene::Lazy);
    assert_eq!(rolled[1], PandaGene::Worried);
    assert_eq!(rolled[2], PandaGene::Playful);
    assert_eq!(rolled[4], PandaGene::Aggressive);
    assert_eq!(
        rolled
            .iter()
            .filter(|gene| **gene == PandaGene::Weak)
            .count(),
        5
    );
    assert_eq!(
        rolled
            .iter()
            .filter(|gene| **gene == PandaGene::Brown)
            .count(),
        2
    );
    assert_eq!(
        rolled
            .iter()
            .filter(|gene| **gene == PandaGene::Normal)
            .count(),
        5
    );
}

#[test]
fn a_weak_panda_has_less_health_and_a_lazy_one_moves_slower() {
    // The only two genes that change a number rather than a behaviour.
    init_vanilla_registry();
    let ordinary = new_panda();
    let full_health = ordinary.get_max_health();
    let full_speed = ordinary
        .attributes()
        .lock()
        .get_value(vanilla_attributes::MOVEMENT_SPEED)
        .unwrap_or_default();

    let weak = new_panda();
    weak.set_main_gene(PandaGene::Weak);
    weak.set_hidden_gene(PandaGene::Weak);
    weak.set_attributes();
    assert!(weak.get_max_health() < full_health);
    assert!((f64::from(weak.get_max_health()) - WEAK_MAX_HEALTH).abs() < f64::EPSILON);

    let lazy = new_panda();
    lazy.set_main_gene(PandaGene::Lazy);
    lazy.set_hidden_gene(PandaGene::Lazy);
    lazy.set_attributes();
    let lazy_speed = lazy
        .attributes()
        .lock()
        .get_value(vanilla_attributes::MOVEMENT_SPEED)
        .unwrap_or_default();
    assert!(lazy_speed < full_speed);
    assert!((lazy_speed - LAZY_MOVEMENT_SPEED).abs() < f64::EPSILON);
}

#[test]
fn a_panda_saves_and_reloads_both_of_its_genes() {
    use std::io::Cursor;

    use simdnbt::borrow::read_compound as read_borrowed_compound;

    let panda = new_panda();
    panda.set_main_gene(PandaGene::Aggressive);
    panda.set_hidden_gene(PandaGene::Brown);

    let mut nbt = NbtCompound::new();
    panda.save_additional(&mut nbt);
    let mut bytes = Vec::new();
    nbt.write(&mut bytes);
    let borrowed = read_borrowed_compound(&mut Cursor::new(&bytes))
        .unwrap_or_else(|error| panic!("panda nbt should reborrow: {error}"));

    let reloaded = new_panda();
    reloaded.load_additional((&borrowed).into());

    assert_eq!(reloaded.main_gene(), PandaGene::Aggressive);
    assert_eq!(
        reloaded.hidden_gene(),
        PandaGene::Brown,
        "the hidden gene has to survive the save or brown pandas never appear"
    );
    assert_eq!(reloaded.variant(), PandaGene::Aggressive);
}

#[test]
fn a_panda_doing_anything_else_will_not_start_a_second_thing() {
    // `canPerformAction` is the gate every panda goal but panic and breeding
    // passes through; without it a panda could roll while it eats.
    let panda = new_panda();
    assert!(panda.can_perform_action());

    for start in [
        PandaEntity::set_on_back as fn(&PandaEntity, bool),
        PandaEntity::sit,
        PandaEntity::roll,
        PandaEntity::eat,
    ] {
        start(&panda, true);
        assert!(
            !panda.can_perform_action(),
            "a busy panda should refuse to start something else"
        );
        start(&panda, false);
        assert!(panda.can_perform_action());
    }
}

#[test]
fn a_worried_panda_sits_out_a_thunderstorm() {
    // A scared panda cannot act at all: this is what makes a worried one
    // useless during a storm and normal again the moment it passes.
    let world = panda_world("panda_thunder");
    let panda = live_panda(&world);
    panda.set_main_gene(PandaGene::Worried);
    panda.set_hidden_gene(PandaGene::Worried);

    assert!(!panda.is_scared());
    set_thundering(&world, true);

    assert!(panda.is_scared());
    assert!(!panda.can_perform_action());

    panda.tick();
    assert!(panda.is_sitting(), "a worried panda sits out the storm");

    set_thundering(&world, false);
    panda.tick();
    assert!(!panda.is_sitting());
}

#[test]
fn a_panda_stands_up_the_moment_it_is_hurt() {
    // Vanilla's `hurtServer` sits the panda up before the damage is even
    // applied, so nothing can attack one while it is eating and expect it to
    // stay put.
    let world = panda_world("panda_hurt");
    let panda = live_panda(&world);
    panda.sit(true);
    assert!(panda.is_sitting());

    panda.hurt_server(
        &world,
        &DamageSource::environment(&vanilla_damage_types::GENERIC),
        1.0,
    );

    assert!(!panda.is_sitting());
}

#[test]
fn a_sneeze_runs_its_course_and_startles_the_pandas_around_it() {
    // The pre-sneeze sound, the twenty ticks, and the hop every grown panda
    // within ten blocks takes. A cub does not hop, and neither does a panda in
    // the middle of something else.
    let world = panda_world("panda_sneeze");
    let sneezer = live_panda(&world);
    let neighbour = live_panda(&world);
    // Vanilla only startles a panda that is standing on something: one in
    // mid-air is already going somewhere.
    neighbour.set_on_ground(true);

    sneezer.sneeze(true);
    assert!(sneezer.is_sneezing());

    for _ in 0..=SNEEZE_DURATION {
        sneezer.tick();
    }

    assert!(!sneezer.is_sneezing(), "a sneeze lasts twenty ticks");
    assert_eq!(sneezer.sneeze_counter(), 0);
    assert!(
        neighbour.velocity().y > 0.0,
        "a sneeze startles the pandas around it into a hop"
    );
}

#[test]
fn a_rolling_panda_launches_itself_and_stops_after_thirty_two_ticks() {
    // The roll is a shove on the first tick and three bounces after it; a panda
    // whose counter never ran out would roll forever.
    let world = panda_world("panda_roll");
    let panda = live_panda(&world);
    panda.set_on_ground(true);
    panda.roll(true);

    panda.tick();
    assert!(
        panda.velocity().length_squared() > 0.0,
        "the first roll tick launches the panda"
    );

    for _ in 0..TOTAL_ROLL_STEPS {
        panda.tick();
    }

    assert!(!panda.is_rolling(), "a roll ends after thirty-two steps");
}

#[test]
fn feeding_a_panda_that_is_angry_at_you_buys_you_off() {
    // `gotBamboo` is what makes the hurt-by goal drop its target: a panda you
    // have fed stops chasing you even though you hit it.
    let world = panda_world("panda_bribe");
    let panda = live_panda(&world);
    let victim = live_panda(&world);

    assert!(!panda.got_bamboo());
    panda.set_target(Some(&(victim as SharedEntity)));

    let player =
        TestPlayerBuilder::new(Arc::clone(&world), "PandaFriend", next_entity_id()).build();
    player.inventory.lock().set_item_in_hand(
        InteractionHand::MainHand,
        ItemStack::new(&vanilla_items::BAMBOO),
    );

    panda.mob_interact(&player, InteractionHand::MainHand);

    assert!(
        panda.got_bamboo(),
        "a fed panda remembers the bribe until it loses its target"
    );
}

#[test]
fn a_panda_reaches_its_goals_and_survives_forty_ticks_in_a_live_world() {
    // A panda whose `server_ai_step` does not reach `Mob::mob_server_ai_step`
    // never runs a goal at all, and the tick loop catches a lock-ordering hang
    // in the navigation the sit goal drives.
    let world = panda_world("panda_ticks");
    let panda = live_panda(&world);

    panda.set_no_action_time(0);
    LivingEntity::server_ai_step(panda.as_ref());
    assert!(
        panda.no_action_time() > 0,
        "the panda's goals never run: `server_ai_step` does not reach \
         `Mob::mob_server_ai_step`"
    );

    for _ in 0..40 {
        panda.tick();
    }

    assert!(Entity::is_alive(panda.as_ref()));
}
