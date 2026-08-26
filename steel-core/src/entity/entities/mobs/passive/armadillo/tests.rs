//! Armadillo tests.

use steel_registry::{
    init_vanilla_registry, vanilla_blocks, vanilla_damage_types, vanilla_entities,
};
use steel_utils::ChunkPos;
use steel_utils::types::UpdateFlags;

use super::*;
use crate::behavior::init_behaviors;
use crate::entity::entities::ZombieEntity;
use crate::entity::{SharedEntity, next_entity_id};
use crate::test_support::{TestPlayerBuilder, fresh_test_world, insert_ready_full_chunk};

const TEST_POS: BlockPos = BlockPos::new(8, 64, 8);
const TEST_POSITION: DVec3 = DVec3::new(8.5, 64.0, 8.5);

fn armadillo_world(key: &'static str) -> Arc<World> {
    init_vanilla_registry();
    init_behaviors();
    let world = fresh_test_world(key);
    insert_ready_full_chunk(&world, ChunkPos::from_block_pos(TEST_POS));
    assert!(world.set_block(
        TEST_POS.below(),
        vanilla_blocks::COARSE_DIRT.default_state(),
        UpdateFlags::UPDATE_NONE
    ));
    world
}

fn live_armadillo(world: &Arc<World>) -> Arc<ArmadilloEntity> {
    let armadillo = Arc::new(ArmadilloEntity::new(
        &vanilla_entities::ARMADILLO,
        next_entity_id(),
        TEST_POSITION,
        Arc::downgrade(world),
    ));
    world
        .try_add_entity(Arc::clone(&armadillo) as SharedEntity)
        .unwrap_or_else(|error| panic!("armadillo should enter the test world: {error:?}"));
    armadillo
}

#[test]
fn an_armadillo_in_its_shell_takes_half_the_damage_less_one() {
    // Vanilla's `(damage - 1.0F) / 2.0F` is the whole point of balling up: a
    // four-point hit lands as one and a half.
    // Two armadillos rather than one hit twice: a second blow inside the
    // invulnerability window only lands if it is bigger than the first, and a
    // reduced one never is.
    let world = armadillo_world("armadillo_shell_damage");
    let unrolled = live_armadillo(&world);
    let rolled = live_armadillo(&world);
    let source = DamageSource::environment(&vanilla_damage_types::GENERIC);

    let full_health = unrolled.get_max_health();
    unrolled.hurt_server(&world, &source, 4.0);
    let unrolled_loss = full_health - unrolled.get_health();

    rolled.switch_to_state(ArmadilloState::Scared);
    assert!(rolled.is_scared());
    rolled.hurt_server(&world, &source, 4.0);
    let rolled_loss = full_health - rolled.get_health();

    assert!((unrolled_loss - 4.0).abs() < f32::EPSILON);
    assert!(
        (rolled_loss - 1.5).abs() < f32::EPSILON,
        "a four-point hit on a shell should land as one and a half, not {rolled_loss}"
    );
}

#[test]
fn being_hit_by_something_alive_balls_an_armadillo_up() {
    // The danger memory is what keeps it there; the ball-up itself is what a
    // player sees. Environmental damage does the opposite -- an armadillo
    // cannot flee a fire in a ball.
    let world = armadillo_world("armadillo_hit");
    let armadillo = live_armadillo(&world);
    let attacker = live_armadillo(&world);

    armadillo.actually_hurt(
        &world,
        &DamageSource::environment(&vanilla_damage_types::MOB_ATTACK)
            .with_causing_entity(attacker.id()),
        1.0,
    );

    assert_eq!(armadillo.state(), ArmadilloState::Rolling);
    assert!(
        armadillo
            .brain
            .has_memory_value(memory_module_types::DANGER_DETECTED_RECENTLY.id())
    );

    armadillo.actually_hurt(
        &world,
        &DamageSource::environment(&vanilla_damage_types::IN_FIRE),
        1.0,
    );
    assert_eq!(
        armadillo.state(),
        ArmadilloState::Idle,
        "burning unrolls an armadillo so it can run"
    );
}

#[test]
fn an_armadillo_is_frightened_by_the_undead_and_by_a_sprinting_player() {
    // Three things scare it and one does not: a player just walking past is
    // ignored, which is what makes sneaking up on one possible.
    let world = armadillo_world("armadillo_scares");
    let armadillo = live_armadillo(&world);

    let zombie = Arc::new(ZombieEntity::new(
        &vanilla_entities::ZOMBIE,
        next_entity_id(),
        TEST_POSITION,
        Arc::downgrade(&world),
    ));
    world
        .try_add_entity(Arc::clone(&zombie) as SharedEntity)
        .unwrap_or_else(|error| panic!("zombie should enter the test world: {error:?}"));
    assert!(armadillo.is_scared_by(zombie.as_ref()));

    let player = TestPlayerBuilder::new(Arc::clone(&world), "Sneaker", next_entity_id()).build();
    player
        .try_set_position(TEST_POSITION)
        .unwrap_or_else(|error| panic!("player should be placed: {error}"));
    assert!(
        !armadillo.is_scared_by(player.as_ref()),
        "a player standing still does not frighten an armadillo"
    );

    player.set_sprinting(true);
    assert!(armadillo.is_scared_by(player.as_ref()));
}

#[test]
fn distance_is_part_of_what_frightens_an_armadillo() {
    // Vanilla checks an inflated bounding box, not a radius: seven blocks
    // sideways and two up. A zombie further off than that is not a threat yet.
    let world = armadillo_world("armadillo_scare_distance");
    let armadillo = live_armadillo(&world);

    let zombie = Arc::new(ZombieEntity::new(
        &vanilla_entities::ZOMBIE,
        next_entity_id(),
        DVec3::new(TEST_POSITION.x + 20.0, TEST_POSITION.y, TEST_POSITION.z),
        Arc::downgrade(&world),
    ));

    assert!(!armadillo.is_scared_by(zombie.as_ref()));
}

#[test]
fn a_balled_up_armadillo_refuses_everything_but_a_brush() {
    // The shell is a real refusal: `FAIL` rather than `PASS`, so the item in
    // hand is not used on the block behind it either.
    let world = armadillo_world("armadillo_interact");
    let armadillo = live_armadillo(&world);
    let player = TestPlayerBuilder::new(Arc::clone(&world), "Brusher", next_entity_id()).build();
    player.inventory.lock().set_item_in_hand(
        InteractionHand::MainHand,
        ItemStack::new(&vanilla_items::SPIDER_EYE),
    );

    armadillo.switch_to_state(ArmadilloState::Scared);
    assert_eq!(
        armadillo.mob_interact(&player, InteractionHand::MainHand),
        InteractionResult::Fail
    );

    player.inventory.lock().set_item_in_hand(
        InteractionHand::MainHand,
        ItemStack::new(&vanilla_items::BRUSH),
    );
    let dropped_before = world
        .get_entities_in_aabb_matching(&armadillo.bounding_box().inflate(8.0), |entity| {
            entity.entity_type() == &vanilla_entities::ITEM
        })
        .len();

    assert_eq!(
        armadillo.mob_interact(&player, InteractionHand::MainHand),
        InteractionResult::Success,
        "a brush reaches an armadillo even in its shell"
    );

    let dropped_after = world
        .get_entities_in_aabb_matching(&armadillo.bounding_box().inflate(8.0), |entity| {
            entity.entity_type() == &vanilla_entities::ITEM
        })
        .len();
    assert_eq!(dropped_after, dropped_before + 1, "the brush takes a scute");
    assert!(
        player
            .inventory
            .lock()
            .get_item_in_hand(InteractionHand::MainHand)
            .get_damage_value()
            > 0,
        "brushing costs the brush durability"
    );
}

#[test]
fn a_baby_armadillo_has_no_scute_to_brush_off() {
    let world = armadillo_world("armadillo_baby_brush");
    let armadillo = live_armadillo(&world);
    armadillo.set_baby(true);

    assert!(!armadillo.brush_off_scute());
}

#[test]
fn the_shell_opens_and_closes_on_the_ticks_vanilla_says() {
    // `shouldHideInShell` is what the client draws from, and each state answers
    // differently: rolling hides after five ticks, unrolling stops hiding at
    // twenty-six, and scared always hides.
    assert!(!should_hide_in_shell(ArmadilloState::Idle, 100));
    assert!(!should_hide_in_shell(ArmadilloState::Rolling, 5));
    assert!(should_hide_in_shell(ArmadilloState::Rolling, 6));
    assert!(should_hide_in_shell(ArmadilloState::Scared, 0));
    assert!(should_hide_in_shell(ArmadilloState::Unrolling, 25));
    assert!(!should_hide_in_shell(ArmadilloState::Unrolling, 26));
}

#[test]
fn rolling_up_becomes_scared_once_the_animation_has_run() {
    // Vanilla's `shouldSwitchToScaredState` is the ten-tick roll animation; a
    // build that switched at once would skip the roll the client draws.
    let world = armadillo_world("armadillo_roll_timing");
    let armadillo = live_armadillo(&world);
    // The gate is what the brain reads, so the brain is switched off to read it directly.
    // Left on, it would answer correctly and then act: an armadillo with nothing to fear
    // balls up, peeks and unrolls again inside these eleven ticks, which is what the
    // world-driven test beside this one is for.
    Mob::set_no_ai(armadillo.as_ref(), true);

    armadillo.roll_up();
    assert_eq!(armadillo.state(), ArmadilloState::Rolling);
    assert!(!armadillo.should_switch_to_scared_state());

    for _ in 0..=armadillo_state_animation_duration(ArmadilloState::Rolling) {
        armadillo.tick();
    }

    assert!(armadillo.should_switch_to_scared_state());
}

#[test]
fn a_leashed_armadillo_cannot_stay_in_its_shell() {
    // `canStayRolledUp` is the sensor's ready test as well as the ball-up's
    // stop condition, so this is what stops an armadillo being carried around
    // rolled into a ball.
    let world = armadillo_world("armadillo_stay_rolled");
    let armadillo = live_armadillo(&world);
    assert!(armadillo.can_stay_rolled_up());

    let holder = live_armadillo(&world);
    assert!(armadillo.set_leashed_to(&(holder as SharedEntity)));

    assert!(!armadillo.can_stay_rolled_up());
}

#[test]
fn an_armadillo_saves_and_reloads_its_shell_and_its_scute_clock() {
    use std::io::Cursor;

    use simdnbt::borrow::read_compound as read_borrowed_compound;

    let world = armadillo_world("armadillo_save");
    let armadillo = live_armadillo(&world);
    armadillo.switch_to_state(ArmadilloState::Scared);
    *armadillo.scute_time.lock() = 1234;

    let mut nbt = NbtCompound::new();
    armadillo.save_additional(&mut nbt);
    let mut bytes = Vec::new();
    nbt.write(&mut bytes);
    let borrowed = read_borrowed_compound(&mut Cursor::new(&bytes))
        .unwrap_or_else(|error| panic!("armadillo nbt should reborrow: {error}"));

    let reloaded = live_armadillo(&world);
    reloaded.load_additional((&borrowed).into());

    assert_eq!(reloaded.state(), ArmadilloState::Scared);
    assert_eq!(*reloaded.scute_time.lock(), 1234);
}

#[test]
fn an_armadillo_reaches_its_brain_and_survives_forty_ticks_in_a_live_world() {
    // An armadillo whose `server_ai_step` does not reach
    // `Mob::mob_server_ai_step` never ticks its brain at all, and the tick loop
    // catches a lock-ordering hang in the navigation.
    let world = armadillo_world("armadillo_ticks");
    let armadillo = live_armadillo(&world);

    armadillo.set_no_action_time(0);
    LivingEntity::server_ai_step(armadillo.as_ref());
    assert!(
        armadillo.no_action_time() > 0,
        "the armadillo's brain never ticks: `server_ai_step` does not reach \
         `Mob::mob_server_ai_step`"
    );

    for _ in 0..40 {
        armadillo.tick();
    }

    assert!(Entity::is_alive(armadillo.as_ref()));
}
