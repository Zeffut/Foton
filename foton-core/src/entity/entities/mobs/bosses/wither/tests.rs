//! The wither's arrival, its armor, and its bar.

use std::sync::Weak;

use foton_registry::{
    init_vanilla_registry, vanilla_blocks, vanilla_damage_types, vanilla_mob_effects,
};
use foton_utils::ChunkPos;

use super::*;
use crate::behavior::init_behaviors;
use crate::chunk::player_chunk_view::PlayerChunkView;
use crate::entity::entities::ArrowEntity;
use crate::entity::{init_entities, next_entity_id};
use crate::test_support::{
    BossBarViewer, OP_ADD, OP_REMOVE, OP_UPDATE_PROGRESS, fresh_test_world, insert_ready_full_chunk,
};

/// The only spawn coordinate the test world is happy with.
const SPAWN: DVec3 = DVec3::new(8.5, 64.0, 8.5);

fn detached_wither() -> WitherBoss {
    init_vanilla_registry();
    WitherBoss::new(
        &vanilla_entities::WITHER,
        next_entity_id(),
        SPAWN,
        Weak::<World>::new(),
    )
}

fn prepared_world(key: &'static str) -> Arc<World> {
    init_vanilla_registry();
    init_behaviors();
    init_entities();
    let world = fresh_test_world(key);
    insert_ready_full_chunk(&world, ChunkPos::new(0, 0));
    world
}

fn world_wither(world: &Arc<World>) -> Arc<WitherBoss> {
    Arc::new(WitherBoss::new(
        &vanilla_entities::WITHER,
        next_entity_id(),
        SPAWN,
        Arc::downgrade(world),
    ))
}

/// Vanilla parity: the `xpReward = 50` of the constructor. Thirteen hostiles
/// once shipped worth nothing at all, so this is pinned rather than assumed.
#[test]
fn a_wither_is_worth_fifty_experience() {
    let wither = detached_wither();

    assert_eq!(wither.xp_reward(), 50);
}

/// `LivingEntity::server_ai_step` does nothing unless a mob routes it into
/// `Mob::mob_server_ai_step`, and that call is the only path to the goal
/// selector. Fifteen hostiles once registered full goal sets and ticked none.
#[test]
fn a_wither_runs_its_goals() {
    let wither = detached_wither();
    wither.set_no_action_time(0);

    LivingEntity::server_ai_step(&wither);

    assert!(
        wither.no_action_time() > 0,
        "this mob's goals never tick: `server_ai_step` does not reach \
         `Mob::mob_server_ai_step`"
    );
}

/// Vanilla parity: `WitherBoss.makeInvulnerable`, which the summoning skull
/// calls. The health drop to a third is what makes the arrival survivable.
#[test]
fn a_summoned_wither_arrives_at_a_third_health_with_an_empty_bar() {
    let wither = detached_wither();
    let max_health = wither.get_max_health();
    assert!((wither.get_health() - max_health).abs() < f32::EPSILON);

    wither.make_invulnerable();

    assert_eq!(wither.invulnerable_ticks(), INVULNERABLE_TICKS);
    assert!((wither.boss_event().progress() - 0.0).abs() < f32::EPSILON);
    assert!((wither.get_health() - max_health / 3.0).abs() < 1.0e-4);
}

/// Vanilla parity: the `setProgress(1.0F - newCount / 220.0F)` of the
/// invulnerable branch, which is the only thing that fills the bar during the
/// arrival.
#[test]
fn the_arrival_counts_down_and_fills_the_bar_as_it_goes() {
    let world = prepared_world("wither_arrival");
    let wither = world_wither(&world);
    wither.make_invulnerable();

    wither.custom_server_ai_step();

    assert_eq!(wither.invulnerable_ticks(), INVULNERABLE_TICKS - 1);
    let expected = 1.0 - (INVULNERABLE_TICKS - 1) as f32 / INVULNERABLE_TICKS as f32;
    assert!((wither.boss_event().progress() - expected).abs() < 1.0e-6);
}

/// Vanilla parity: the `this.heal(10.0F)` every tenth tick of the arrival,
/// which is why a wither that is left alone comes out of its shell at full
/// health.
#[test]
fn the_arrival_heals_the_wither_back_up() {
    let world = prepared_world("wither_arrival_heal");
    let wither = world_wither(&world);
    wither.make_invulnerable();
    let start = wither.get_health();

    // `tickCount` starts at zero, so the first tick is a healing one.
    wither.custom_server_ai_step();

    assert!(
        wither.get_health() > start,
        "the arrival must heal the wither"
    );
}

/// Vanilla parity: `WitherBoss.isPowered`, the halfway point where the fight
/// changes.
#[test]
fn a_wither_is_powered_at_and_below_half_health() {
    let wither = detached_wither();
    let max_health = wither.get_max_health();

    wither.set_health(max_health / 2.0 + 1.0);
    assert!(!wither.is_powered());

    wither.set_health(max_health / 2.0);
    assert!(wither.is_powered());
}

/// Vanilla parity: `WitherBoss.canDestroy`, which is what stops a wither
/// chewing through bedrock.
#[test]
fn a_wither_eats_stone_but_not_the_blocks_in_the_immune_tag() {
    init_vanilla_registry();

    assert!(can_destroy(vanilla_blocks::STONE.default_state()));
    assert!(!can_destroy(vanilla_blocks::BEDROCK.default_state()));
    assert!(!can_destroy(vanilla_blocks::AIR.default_state()));
}

/// Vanilla parity: `WitherBoss.addEffect` returns false for everything, so a
/// splash potion of harming does nothing to one.
#[test]
fn a_wither_takes_no_mob_effect_at_all() {
    init_vanilla_registry();
    let wither = detached_wither();

    let applied = wither.add_mob_effect(MobEffectInstance::with_duration(
        vanilla_mob_effects::WITHER,
        200,
        0,
    ));
    assert!(!applied);
    let applied = wither.add_mob_effect(MobEffectInstance::with_duration(
        vanilla_mob_effects::POISON,
        200,
        0,
    ));

    assert!(!applied);
    assert!(wither.active_mob_effects().is_empty());
}

/// Vanilla parity: the head offsets of `getHeadX`/`getHeadY`/`getHeadZ`. Head
/// zero is the body itself; the side heads sit `1.3` out and lower down. These
/// are the muzzle positions the skulls leave from, so getting them wrong aims
/// the whole fight somewhere else.
#[test]
fn the_middle_head_sits_on_the_body_and_the_side_heads_lean_out() {
    let wither = detached_wither();
    wither.set_y_body_rot(0.0);

    let middle = wither.head_position(0);
    assert!((middle.x - SPAWN.x).abs() < 1.0e-9);
    assert!((middle.z - SPAWN.z).abs() < 1.0e-9);
    assert!((middle.y - (SPAWN.y + 3.0)).abs() < 1.0e-6);

    let first = wither.head_position(1);
    let second = wither.head_position(2);
    assert!((first.y - (SPAWN.y + 2.2)).abs() < 1.0e-6);
    assert!(
        (first.x - (SPAWN.x + 1.3)).abs() < 1.0e-6,
        "head one leans out along +X at a body rotation of zero"
    );
    assert!(
        (second.x - (SPAWN.x - 1.3)).abs() < 1.0e-6,
        "head two leans out the opposite way"
    );
}

/// Vanilla parity: the `isPowered()` branch of `hurtServer`. An arrow is the
/// obvious way to fight the first half and useless in the second, and getting
/// this backwards makes the whole second phase trivial.
#[test]
fn a_powered_wither_shrugs_off_arrows_that_hurt_it_before() {
    let world = prepared_world("wither_powered_arrows");
    let wither = world_wither(&world);
    let entity: SharedEntity = Arc::clone(&wither) as SharedEntity;
    world
        .try_add_entity(entity)
        .expect("the wither should enter the world");

    let arrow = Arc::new(ArrowEntity::new(
        &vanilla_entities::ARROW,
        next_entity_id(),
        SPAWN,
        Arc::downgrade(&world),
    ));
    let arrow_entity: SharedEntity = arrow;
    world
        .try_add_entity(Arc::clone(&arrow_entity))
        .expect("the arrow should enter the world");

    let source = DamageSource::environment(&vanilla_damage_types::ARROW)
        .with_direct_entity(arrow_entity.id());

    let max_health = wither.get_max_health();
    wither.set_health(max_health);
    assert!(
        wither.hurt_server(&world, &source, 5.0),
        "an arrow must land while the wither is above half health"
    );

    wither.set_health(max_health / 4.0);
    assert!(
        !wither.hurt_server(&world, &source, 5.0),
        "a powered wither is immune to arrows"
    );
}

/// Vanilla parity: the `getInvulnerableTicks() > 0` guard of `hurtServer`. The
/// arriving wither cannot be killed before it hatches.
#[test]
fn an_arriving_wither_cannot_be_hurt() {
    let world = prepared_world("wither_arrival_invulnerable");
    let wither = world_wither(&world);
    let entity: SharedEntity = Arc::clone(&wither) as SharedEntity;
    world
        .try_add_entity(entity)
        .expect("the wither should enter the world");
    wither.make_invulnerable();
    let health = wither.get_health();

    let source = DamageSource::environment(&vanilla_damage_types::GENERIC);
    let hurt = wither.hurt_server(&world, &source, 10.0);

    assert!(!hurt);
    assert!((wither.get_health() - health).abs() < f32::EPSILON);
}

/// Vanilla parity: `hurtServer` arms `destroyBlocksTick` on every hit, which is
/// what makes a wither eat the room it was hit in one second later.
#[test]
fn hitting_a_wither_arms_the_block_destruction_timer() {
    let world = prepared_world("wither_destroy_timer");
    let wither = world_wither(&world);
    let entity: SharedEntity = Arc::clone(&wither) as SharedEntity;
    world
        .try_add_entity(entity)
        .expect("the wither should enter the world");
    assert_eq!(*wither.destroy_blocks_tick.lock(), 0);

    let source = DamageSource::environment(&vanilla_damage_types::GENERIC);
    wither.hurt_server(&world, &source, 1.0);

    assert_eq!(*wither.destroy_blocks_tick.lock(), DESTROY_BLOCKS_DELAY);
    assert!(
        wither
            .idle_head_updates
            .lock()
            .iter()
            .all(|idle| *idle == IDLE_UPDATES_PER_HIT),
        "a hit must hurry both side heads along"
    );
}

/// The case a naive boss-bar port gets wrong. The bar is not broadcast to the
/// world: it follows the tracker, so it must appear when a player starts
/// tracking the wither and come down the moment they stop. A bar that lingers
/// on a client that walked away is the classic symptom.
#[test]
fn the_bar_follows_the_tracker_onto_and_off_a_players_screen() {
    let world = prepared_world("wither_bar_tracking");
    let wither = world_wither(&world);
    let entity: SharedEntity = Arc::clone(&wither) as SharedEntity;
    let viewer = BossBarViewer::new(&world, "Watcher", next_entity_id());

    let tracker = world.entity_tracker();
    tracker.add(&entity, |_| Vec::new(), |_| None);
    assert!(
        viewer.take_boss_operations().is_empty(),
        "a player who is not tracking the wither yet has no bar"
    );

    let near = PlayerChunkView::new(ChunkPos::new(0, 0), 8);
    tracker.update_player(&viewer.player, &near, |_| true);
    assert_eq!(
        viewer.take_boss_operations(),
        vec![OP_ADD],
        "the bar must arrive with the wither"
    );

    // Updates now reach the viewer.
    wither.boss_event().set_progress(0.5);
    assert_eq!(viewer.take_boss_operations(), vec![OP_UPDATE_PROGRESS]);

    // The player walks out of range: the tracker drops the pairing.
    viewer
        .player
        .base()
        .set_position_local(DVec3::new(4000.5, 64.0, 4000.5));
    let far = PlayerChunkView::new(ChunkPos::new(250, 250), 8);
    tracker.update_player(&viewer.player, &far, |_| true);
    assert_eq!(
        viewer.take_boss_operations(),
        vec![OP_REMOVE],
        "the bar must come down when the player stops tracking the wither"
    );

    wither.boss_event().set_progress(0.25);
    assert!(
        viewer.take_boss_operations().is_empty(),
        "a bar must never linger on a client that walked away"
    );

    // And it comes back when they walk in again.
    viewer.player.base().set_position_local(SPAWN);
    tracker.update_player(&viewer.player, &near, |_| true);
    assert_eq!(
        viewer.take_boss_operations(),
        vec![OP_ADD],
        "the bar must reappear for a player who walks back in"
    );
}

/// The other half of the same rule: when the wither itself goes, every bar goes
/// with it.
#[test]
fn removing_the_wither_takes_its_bar_off_every_screen() {
    let world = prepared_world("wither_bar_removal");
    let wither = world_wither(&world);
    let entity: SharedEntity = Arc::clone(&wither) as SharedEntity;
    let viewer = BossBarViewer::new(&world, "Watcher", next_entity_id());
    let viewer_id = viewer.player.id();
    let viewer_player = Arc::clone(&viewer.player);

    let tracker = world.entity_tracker();
    tracker.add(&entity, |_| Vec::new(), |_| None);
    let near = PlayerChunkView::new(ChunkPos::new(0, 0), 8);
    tracker.update_player(&viewer.player, &near, |_| true);
    viewer.take_boss_operations();

    tracker.remove(entity.id(), |id| {
        (id == viewer_id).then(|| Arc::clone(&viewer_player))
    });

    assert_eq!(viewer.take_boss_operations(), vec![OP_REMOVE]);
    assert!(!wither.boss_event().has_players());
}
