//! The warden, driven in a live world.
//!
//! Everything the warden does is downstream of its anger, and the anger is
//! downstream of a vibration. These run both halves for real rather than poking
//! memories directly, because the wiring between the vibration listener, the
//! anger bookkeeping and the brain's activity order is where a warden can be
//! fully implemented and still stand still.

use std::io::Cursor;
use std::sync::Arc;

use glam::DVec3;
use simdnbt::borrow::{NbtCompound as NbtCompoundView, read_compound as read_borrowed_compound};
use simdnbt::owned::NbtCompound;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::blocks::properties::BlockStateProperties;
use steel_registry::vanilla_blocks;
use steel_registry::{init_vanilla_registry, vanilla_entities, vanilla_game_events};
use steel_utils::types::UpdateFlags;
use steel_utils::{BlockPos, ChunkPos, Downcast as _, WorldAabb};

use super::anger::{AngerLevel, AngerManagement};
use super::entity::WardenEntity;
use super::warden_ai;
use crate::behavior::init_behaviors;
use crate::block_entity::entities::SculkShriekerBlockEntity;
use crate::block_entity::init_block_entities;
use crate::entity::ai::brain::memory::{Unit, memory_module_types};
use crate::entity::ai::brain::{Activity, Brain};
use crate::entity::init_entities;
use crate::entity::{Entity, LivingEntity, SharedEntity, next_entity_id};
use crate::player::ResetReason;
use crate::test_support::{TestPlayerBuilder, fresh_test_world, insert_ready_full_chunk};
use crate::world::World;
use crate::world::game_event::GameEventContext;

/// The one spot the test world is guaranteed to be solid ground rather than a
/// column the fluid scan walks.
const SPAWN: DVec3 = DVec3::new(8.5, 64.0, 8.5);
const STAND: BlockPos = BlockPos::new(8, 64, 8);

fn warden_world(key: &'static str) -> Arc<World> {
    init_vanilla_registry();
    init_behaviors();
    init_block_entities();
    init_entities();
    let world = fresh_test_world(key);
    for x in -1..=1 {
        for z in -1..=1 {
            insert_ready_full_chunk(&world, ChunkPos::new(x, z));
        }
    }
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

fn spawn_warden(world: &Arc<World>) -> Arc<WardenEntity> {
    let warden = Arc::new(WardenEntity::new(
        &vanilla_entities::WARDEN,
        next_entity_id(),
        SPAWN,
        Arc::downgrade(world),
    ));
    world
        .try_add_entity(Arc::clone(&warden) as SharedEntity)
        .expect("the test chunk is loaded, so the warden should attach");
    // Vanilla's `finalizeSpawn` gives every warden twenty minutes before it may dig away
    // again; without it the digging activity outranks everything a test wants to watch.
    warden.brain_ref().set_memory_with_expiry(
        memory_module_types::DIG_COOLDOWN,
        Unit,
        warden_ai::DIGGING_COOLDOWN.into(),
    );
    warden
}

/// Runs `ticks` server ticks, advancing the world clock with them.
///
/// The clock matters: a brain behavior's duration is measured against `game_time`, so a
/// world whose clock never moves is a world where the roar never ends.
fn run_ticks(world: &Arc<World>, warden: &Arc<WardenEntity>, ticks: i32) {
    for _ in 0..ticks {
        advance_game_time(world, 1);
        // The world's entity tick advances this before anything else, and the warden reads
        // it: its anger decays once every twenty ticks, and a tick count stuck at zero
        // would decay it every tick instead.
        warden.advance_tick_count();
        warden.base_tick();
        Entity::tick(warden.as_ref());
        LivingEntity::server_ai_step(warden.as_ref());
    }
}

fn advance_game_time(world: &Arc<World>, ticks: i64) {
    let now = world.game_time();
    world.level_data.write().set_game_time(now + ticks);
}

fn brain(warden: &Arc<WardenEntity>) -> &Brain {
    warden.brain_ref()
}

/// A warden with a grudge worth roaring about drops everything else and roars, and the
/// roar is what promotes the suspect to an attack target. Nothing short of running the
/// brain proves the activity order -- the digging activity is tried before the roar, and a
/// roar target is the only thing that stops it.
#[test]
fn an_angry_warden_roars_at_its_suspect_and_then_hunts_it() {
    let world = warden_world("warden_roars_then_hunts");
    let warden = spawn_warden(&world);
    let player = TestPlayerBuilder::new(Arc::clone(&world), "suspect", next_entity_id()).build();
    world
        .try_add_entity(Arc::clone(&player) as SharedEntity)
        .expect("the test chunk is loaded, so the player should attach");

    warden.increase_anger_at_by(
        Some(player.as_ref()),
        AngerLevel::Angry.minimum_anger(),
        false,
    );
    assert_eq!(warden.anger_level(), AngerLevel::Angry);
    assert!(
        warden.entity_angry_at().is_some(),
        "an angry warden names the suspect it is angry at"
    );

    run_ticks(&world, &warden, 2);
    assert!(
        brain(&warden).is_active(Activity::Roar),
        "a warden with a roar target roars before it digs or wanders"
    );

    // The roar lasts eighty-four ticks and ends by naming the attack target.
    run_ticks(&world, &warden, warden_ai::ROAR_DURATION + 2);
    assert!(
        brain(&warden).has_memory_value(memory_module_types::ATTACK_TARGET.id()),
        "the roar ends by promoting the roar target to the attack target"
    );
    assert!(
        brain(&warden).is_active(Activity::Fight),
        "a warden with an attack target is fighting"
    );
}

/// A warden hears through a vibration like a sculk sensor does, and what it does with what
/// it hears is get angry. This is the whole reason the warden could not exist before the
/// vibration layer.
#[test]
fn a_warden_gets_angry_at_what_it_hears() {
    let world = warden_world("warden_hears_a_step");
    let warden = spawn_warden(&world);
    let player = TestPlayerBuilder::new(Arc::clone(&world), "walker", next_entity_id()).build();
    world
        .try_add_entity(Arc::clone(&player) as SharedEntity)
        .expect("the test chunk is loaded, so the player should attach");

    // One tick to attach the listener to the chunk it stands in.
    run_ticks(&world, &warden, 1);
    let step_pos = BlockPos::new(STAND.x() + 3, STAND.y(), STAND.z());
    world.game_event(
        &vanilla_game_events::STEP,
        step_pos,
        &GameEventContext::new(Some(player.as_ref()), None),
    );

    // The vibration is selected on the following tick and then travels three blocks.
    run_ticks(&world, &warden, 8);

    assert!(
        brain(&warden).has_memory_value(memory_module_types::VIBRATION_COOLDOWN.id()),
        "a warden that took a vibration is deaf for the next two seconds"
    );
    // Vanilla `Warden.DEFAULT_ANGER`: one heard vibration is worth thirty-five points,
    // which is five short of agitated -- two steps is what it takes to stir a warden.
    assert_eq!(
        warden.anger_at(player.as_ref()),
        35,
        "hearing a step should have made the warden angry at whoever took it"
    );
    assert!(
        brain(&warden).has_memory_value(memory_module_types::DISTURBANCE_LOCATION.id()),
        "a warden that is not yet angry goes to look at what it heard"
    );
}

/// Four shrieks summon a warden, and not one shriek sooner. The count lives on the player,
/// so this is also the check that the shrieker reads it from there rather than from itself.
#[test]
fn four_shrieks_summon_a_warden() {
    let world = warden_world("warden_summoned_by_shrieks");
    let shrieker_pos = BlockPos::new(STAND.x(), STAND.y(), STAND.z());
    assert!(
        world.set_block(
            shrieker_pos,
            vanilla_blocks::SCULK_SHRIEKER
                .default_state()
                .set_value(&BlockStateProperties::CAN_SUMMON, true),
            UpdateFlags::UPDATE_ALL,
        )
    );
    let player =
        TestPlayerBuilder::new(Arc::clone(&world), "shrieked_at", next_entity_id()).build();
    player
        .try_set_position(DVec3::new(
            f64::from(STAND.x()) + 0.5,
            f64::from(STAND.y()) + 1.0,
            f64::from(STAND.z()) + 0.5,
        ))
        .expect("the test chunk is loaded");
    // A shrieker asks the world for the players near it, and that index is the one a
    // joining player is put into -- `try_add_entity` alone would leave it empty.
    assert!(world.add_player(Arc::clone(&player), ResetReason::InitialJoin));

    let shrieker = |world: &Arc<World>| {
        world
            .get_block_entity(shrieker_pos)
            .and_then(|block_entity| {
                block_entity
                    .downcast_ref::<SculkShriekerBlockEntity>()
                    .map(SculkShriekerBlockEntity::warning_level)
            })
            .expect("placing a shrieker creates its block entity")
    };

    for expected in 1..=4 {
        // The block state has to be quiet again before the next shriek, and the player has
        // to be out of the ten-second cooldown between warnings. Ending the shriek by hand
        // stands in for the ninety-tick scheduled tick that would end it in a live world.
        let state = world.get_block_state(shrieker_pos);
        if state.get_value(&BlockStateProperties::SHRIEKING) {
            assert!(world.set_block(
                shrieker_pos,
                state.set_value(&BlockStateProperties::SHRIEKING, false),
                UpdateFlags::UPDATE_ALL,
            ));
        }
        with_shrieker(&world, shrieker_pos, |block_entity| {
            block_entity.try_shriek(&world, &player);
        });
        assert_eq!(
            shrieker(&world),
            expected,
            "each allowed shriek should move the player one step closer to a warden"
        );
        for _ in 0..210 {
            player.tick();
        }
    }

    assert!(
        wardens_in(&world).is_empty(),
        "the warden arrives with the answer to the fourth shriek, not with the shriek"
    );
    with_shrieker(&world, shrieker_pos, |block_entity| {
        block_entity.try_respond(&world);
    });
    assert_eq!(
        wardens_in(&world).len(),
        1,
        "a shrieker answering its fourth warning summons exactly one warden"
    );
}

fn with_shrieker(
    world: &Arc<World>,
    pos: BlockPos,
    action: impl FnOnce(&SculkShriekerBlockEntity),
) {
    let block_entity = world
        .get_block_entity(pos)
        .expect("placing a shrieker creates its block entity");
    let shrieker = block_entity
        .downcast_ref::<SculkShriekerBlockEntity>()
        .expect("the sculk shrieker block entity is the one that was created");
    action(shrieker);
}

fn wardens_in(world: &Arc<World>) -> Vec<SharedEntity> {
    world.get_entities_in_aabb_matching(
        &WorldAabb::of_size(
            DVec3::new(
                f64::from(STAND.x()),
                f64::from(STAND.y()),
                f64::from(STAND.z()),
            ),
            64.0,
            64.0,
            64.0,
        ),
        |entity| entity.entity_type() == &vanilla_entities::WARDEN,
    )
}

/// Anger decays a point a second, which is what makes standing still work as a way to
/// survive a warden. A grudge that never faded would make the deep dark unplayable.
#[test]
fn anger_decays_and_is_capped() {
    let world = warden_world("warden_anger_decays");
    let player = TestPlayerBuilder::new(Arc::clone(&world), "suspect", next_entity_id()).build();
    world
        .try_add_entity(Arc::clone(&player) as SharedEntity)
        .expect("the test chunk is loaded, so the player should attach");

    let mut anger = AngerManagement::default();
    assert_eq!(anger.increase_anger(player.as_ref(), 35), 35);
    assert_eq!(
        anger.increase_anger(player.as_ref(), 500),
        150,
        "vanilla clamps a single suspect's anger at 150"
    );

    let always_valid = |_: &dyn Entity| true;
    anger.tick(&world, &always_valid);
    assert_eq!(
        anger.anger_at(player.as_ref()),
        149,
        "every tick of the anger manager sheds one point"
    );
    assert_eq!(anger.active_anger(None), 149);
    assert_eq!(
        anger.active_entity(&always_valid).map(|entity| entity.id()),
        Some(player.id())
    );
}

/// A warden that unloaded mid-grudge remembers it. The suspects are stored by UUID, which
/// is the only handle that survives the entity being gone.
#[test]
fn a_grudge_survives_a_save_and_load() {
    let world = warden_world("warden_anger_saves");
    let _warden = spawn_warden(&world);
    let player = TestPlayerBuilder::new(Arc::clone(&world), "suspect", next_entity_id()).build();
    world
        .try_add_entity(Arc::clone(&player) as SharedEntity)
        .expect("the test chunk is loaded, so the player should attach");

    let mut anger = AngerManagement::default();
    anger.increase_anger(player.as_ref(), 90);
    let mut saved = NbtCompound::new();
    anger.save(&mut saved);

    let mut bytes = Vec::new();
    saved.write(&mut bytes);
    let borrowed =
        read_borrowed_compound(&mut Cursor::new(bytes.as_slice())).expect("test NBT reborrows");
    let view: NbtCompoundView<'_, '_> = (&borrowed).into();
    let mut loaded = AngerManagement::load(&view);

    // The suspect is only a UUID until the manager next looks the level over.
    loaded.tick(&world, &|_| true);
    loaded.tick(&world, &|_| true);
    loaded.tick(&world, &|_| true);
    assert!(
        loaded.anger_at(player.as_ref()) >= AngerLevel::Angry.minimum_anger(),
        "the warden should still be angry at the player it remembered"
    );
}
