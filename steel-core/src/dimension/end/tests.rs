//! Tests for the End's fight.
//!
//! Every one of these enters through something the running server already
//! calls -- [`World::tick_game`], a hit landing on a crystal, a dragon being
//! killed -- rather than through the fight's own methods. The fight spent a
//! long time being the thing nothing called, and its podium longer still; a
//! test that reached in directly would not have noticed either.

use std::sync::Arc;

use glam::DVec3;
use steel_registry::blocks::BlockRef;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::feature::EndSpike;
use steel_registry::{vanilla_blocks, vanilla_damage_types, vanilla_entities};
use steel_utils::{BlockPos, ChunkPos, Direction, Downcast as _};

use super::EnderDragonFight;
use super::respawn_stage::DragonRespawnStage;
use crate::bootstrap::init_globals_once;
use crate::entity::damage::DamageSource;
use crate::entity::entities::mobs::bosses::ender_dragon::FIRST_KILL_DEATH_XP;
use crate::entity::entities::{EndCrystalEntity, EnderDragon, ExperienceOrbEntity};
use crate::entity::{Entity as _, LivingEntity as _, SharedEntity, next_entity_id};
use crate::test_support::{TestPlayerBuilder, fresh_end_test_world, insert_entity_ticking_chunk};
use crate::world::{LevelReader as _, World};
use crate::worldgen::feature::FeatureDecorationRunner;

/// How far out the fight's own checks reach, in chunks.
const ARENA_CHUNKS: i32 = 8;

/// Loads every chunk the arena checks touch.
///
/// `EnderDragonFight::is_arena_loaded` refuses to run the fight until they are
/// there, and the podium, the crystals and the dragon all need somewhere to go.
fn load_arena(world: &Arc<World>) {
    for x in -ARENA_CHUNKS..=ARENA_CHUNKS {
        for z in -ARENA_CHUNKS..=ARENA_CHUNKS {
            insert_entity_ticking_chunk(world, ChunkPos::new(x, z));
        }
    }
}

/// Builds an End with a player standing in it, ticks it once, and hands back
/// the dragon that appeared.
///
/// One tick is all it takes: the fight scans its state, builds the inactive
/// podium and puts a dragon in the sky.
fn started_end(key: &'static str) -> (Arc<World>, SharedEntity) {
    init_globals_once();
    let world = fresh_end_test_world(key);
    load_arena(&world);

    let player = TestPlayerBuilder::new(Arc::clone(&world), "Fighter", next_entity_id()).build();
    assert!(
        world.players.insert(Arc::clone(&player)),
        "the test player should join the End"
    );

    assert!(
        dragons(&world).is_empty(),
        "the End should start without a dragon"
    );
    world.tick_game(1, true);

    let mut dragons = dragons(&world);
    assert_eq!(
        dragons.len(),
        1,
        "one world tick of an occupied End should have produced a dragon"
    );
    (world, dragons.remove(0))
}

fn dragons(world: &Arc<World>) -> Vec<SharedEntity> {
    world
        .entity_manager()
        .get_accessible_entities()
        .into_iter()
        .filter(|entity| entity.downcast_ref::<EnderDragon>().is_some())
        .collect()
}

/// Places a crystal and returns it.
fn place_crystal(world: &Arc<World>, position: DVec3) -> Arc<EndCrystalEntity> {
    let crystal = Arc::new(EndCrystalEntity::new(
        &vanilla_entities::END_CRYSTAL,
        next_entity_id(),
        position,
        Arc::downgrade(world),
    ));
    world
        .try_add_entity(Arc::clone(&crystal) as SharedEntity)
        .expect("the crystal should spawn");
    crystal
}

/// The fight is the only thing that spawns a dragon on its own, and the world
/// tick is the only thing that runs the fight. This is the whole call path:
/// `World::tick_game` -> `EnderDragonFight::tick` -> `createNewDragon`.
#[test]
fn one_world_tick_of_an_occupied_end_puts_a_dragon_in_the_sky() {
    let (world, dragon_entity) = started_end("dragon_fight_spawns_from_tick");
    let fight = world
        .dragon_fight()
        .expect("an End-dimension world should carry a fight");
    let dragon = dragon_entity
        .downcast_ref::<EnderDragon>()
        .expect("the fight's entity should be a dragon");

    assert_eq!(
        fight.dragon_uuid(),
        Some(dragon.uuid()),
        "the fight should be following the dragon it just made"
    );
    assert!(
        dragon.has_fight(),
        "the dragon should know it belongs to a fight"
    );
    // Vanilla spawns it at `DRAGON_SPAWN_Y`, a hundred and twenty-eight above
    // the fight origin; it has already flown a tick by the time this reads it,
    // so the check is that it is up in the sky rather than down on the podium.
    assert!(
        dragon.position().y > 100.0,
        "the dragon should have appeared high over the arena, not at the origin"
    );
}

/// An End nobody is in runs nothing at all, which is what keeps an idle server
/// from holding the arena loaded and respawning dragons into an empty world.
#[test]
fn an_empty_end_never_grows_a_dragon() {
    init_globals_once();
    let world = fresh_end_test_world("dragon_fight_empty_end");
    load_arena(&world);

    for tick in 1..=3 {
        world.tick_game(tick, true);
    }

    assert!(
        dragons(&world).is_empty(),
        "an End nobody is in should not spawn a dragon"
    );
}

/// The crystal count is what the dragon's pathfinder branches on, and it was
/// hard zero before the fight existed. This drives the real route: a hit lands
/// on a crystal, the crystal tells the fight, the fight recounts the pillars,
/// and the dragon reads the answer back.
#[test]
fn breaking_a_pillar_crystal_recounts_the_pillars_for_the_dragon() {
    let (world, dragon_entity) = started_end("dragon_fight_crystal_count");
    let spikes = FeatureDecorationRunner::end_spikes_for_level(world.seed());
    assert!(spikes.len() >= 2, "the End should have ten pillars");

    let first = place_crystal(&world, spike_top(&spikes[0]));
    let _second = place_crystal(&world, spike_top(&spikes[1]));

    // An explosion source removes a crystal without setting off another blast,
    // which keeps the second pillar's crystal out of this.
    let landed = first.hurt(
        world.as_ref(),
        &DamageSource::environment(&vanilla_damage_types::EXPLOSION),
        1.0,
    );
    assert!(landed, "the crystal should have taken the hit");

    let dragon = dragon_entity
        .downcast_ref::<EnderDragon>()
        .expect("the fight's entity should be a dragon");
    assert_eq!(
        dragon.alive_crystals(),
        1,
        "the dragon should see the one pillar crystal still standing"
    );
}

/// `end_podium::place` had no caller at all, so the exit portal and the dragon
/// egg simply never existed. `/kill` on a dragon takes vanilla's short path
/// through `EnderDragon.kill`, which closes the fight out without the death
/// animation.
#[test]
fn killing_the_dragon_opens_the_exit_portal_and_leaves_the_egg() {
    let (world, dragon_entity) = started_end("dragon_fight_exit_portal");

    let portal_ring = BlockPos::new(1, exit_portal_y(&world), 0);
    assert_ne!(
        world.get_block_state(portal_ring).get_block(),
        &vanilla_blocks::END_PORTAL,
        "the portal should not be lit while the dragon is alive"
    );

    dragon_entity.kill(world.as_ref());

    assert_eq!(
        world.get_block_state(portal_ring).get_block(),
        &vanilla_blocks::END_PORTAL,
        "the exit portal should be lit once the dragon is dead"
    );
    assert!(
        column_holds(&world, BlockPos::ZERO, &vanilla_blocks::DRAGON_EGG),
        "the first dragon of a world should leave its egg on the podium"
    );
}

/// The twelve thousand of a first kill is the largest single experience award
/// in the game, and it is the fight -- not the dragon -- that knows a world has
/// never lost one before.
#[test]
fn the_first_dragon_of_a_world_pays_out_twelve_thousand() {
    let (world, dragon_entity) = started_end("dragon_fight_first_kill_experience");
    let dragon = dragon_entity
        .downcast_ref::<EnderDragon>()
        .expect("the fight's entity should be a dragon");

    // Vanilla pays out `floor(xpCount * 0.08)` on every fifth tick after the
    // hundred and fiftieth. One award is enough to tell the two totals apart:
    // nine hundred and sixty against forty.
    for _ in 0..155 {
        dragon.tick_death();
    }

    assert_eq!(
        experience_in(&world),
        (f64::from(FIRST_KILL_DEATH_XP) * 0.08).floor() as i32,
        "the first dragon of a world should be worth twelve thousand, not five hundred"
    );
}

/// Four crystals on the rim of a spent exit portal are the only way back to a
/// dragon, and they are the reason End crystals had to become destructible.
#[test]
fn four_crystals_on_a_spent_portal_start_the_respawn_ritual() {
    let (world, dragon_entity) = started_end("dragon_fight_respawn_ritual");
    dragon_entity.kill(world.as_ref());

    let fight = world.dragon_fight().expect("the End should carry a fight");
    assert!(
        fight.is_dragon_killed(),
        "the fight should have recorded the kill"
    );
    assert_eq!(
        fight.respawn_stage(),
        None,
        "no ritual should be running yet"
    );

    let center = BlockPos::new(0, exit_portal_y(&world), 0).above();
    for direction in Direction::HORIZONTAL {
        let pos = center.relative_n(direction, 3);
        place_crystal(
            &world,
            DVec3::new(
                f64::from(pos.x()) + 0.5,
                f64::from(pos.y()),
                f64::from(pos.z()) + 0.5,
            ),
        );
    }

    fight.try_respawn(&world);

    assert_eq!(
        fight.respawn_stage(),
        Some(DragonRespawnStage::Start),
        "four crystals on the rim should have started the ritual"
    );
}

/// The saved form is what a reloaded world is rebuilt from, and getting it
/// wrong means a second dragon in a world that already paid out its twelve
/// thousand.
#[test]
fn a_reloaded_fight_remembers_that_the_dragon_is_already_dead() {
    let (world, dragon_entity) = started_end("dragon_fight_persistence");
    dragon_entity.kill(world.as_ref());

    let saved = world
        .dragon_fight()
        .expect("the End should carry a fight")
        .to_persistent();
    let reloaded = EnderDragonFight::from_persistent(saved, world.seed());

    assert!(
        reloaded.is_dragon_killed(),
        "a reloaded fight should not put a second dragon in the sky"
    );
    assert!(
        reloaded.has_previously_killed_dragon(),
        "a reloaded fight should remember the twelve thousand is spent"
    );
}

fn spike_top(spike: &EndSpike) -> DVec3 {
    DVec3::new(
        f64::from(spike.center_x) + 0.5,
        f64::from(spike.height + 1),
        f64::from(spike.center_z) + 0.5,
    )
}

/// The Y the podium's portal socket sits at.
///
/// The fight drops from the podium column's surface until the bedrock ends and
/// then clamps to one above the world floor, which on an empty test End is the
/// floor itself.
fn exit_portal_y(world: &Arc<World>) -> i32 {
    world.get_min_y() + 1
}

fn column_holds(world: &Arc<World>, column: BlockPos, block: BlockRef) -> bool {
    let floor = world.get_min_y();
    (floor..floor + 16).any(|y| world.get_block_state(column.at_y(y)).get_block() == block)
}

fn experience_in(world: &Arc<World>) -> i32 {
    world
        .entity_manager()
        .get_accessible_entities()
        .iter()
        .filter_map(|entity| entity.downcast_ref::<ExperienceOrbEntity>())
        .map(ExperienceOrbEntity::value)
        .sum()
}
