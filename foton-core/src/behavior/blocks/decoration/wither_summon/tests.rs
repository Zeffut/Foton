//! Building the wither out of blocks.

use foton_registry::init_vanilla_registry;
use foton_registry::item_stack::ItemStack;
use foton_registry::vanilla_items;
use foton_utils::types::{InteractionHand, UpdateFlags};
use foton_utils::{ChunkPos, WorldAabb};

use super::*;
use crate::behavior::block::BlockBehavior as _;
use crate::behavior::blocks::decoration::WitherSkullBlock;
use crate::behavior::context::{PlacementOrientation, PlacementSource};
use crate::behavior::init_behaviors;
use crate::block_entity::init_block_entities;
use crate::entity::entities::mobs::bosses::INVULNERABLE_TICKS;
use crate::entity::{LivingEntity as _, SharedEntity, init_entities};
use crate::test_support::{fresh_test_world, insert_ready_full_chunk};

/// The bottom soul sand of the T, and the block the wither ends up standing on.
const FOOT: BlockPos = BlockPos::new(8, 64, 8);

fn prepared_world(key: &'static str) -> Arc<World> {
    init_vanilla_registry();
    init_behaviors();
    init_block_entities();
    init_entities();
    let world = fresh_test_world(key);
    insert_ready_full_chunk(&world, ChunkPos::new(0, 0));
    world
}

/// Lays the soul sand T, and the skulls on top of it if asked.
///
/// The shape is vanilla's `"^^^", "###", "~#~"` read bottom up: one sand, then
/// three, then three skulls.
fn build_frame(world: &Arc<World>, skulls: usize) {
    let sand = vanilla_blocks::SOUL_SAND.default_state();
    world.set_block(FOOT, sand, UpdateFlags::UPDATE_ALL);
    for x in -1..=1 {
        world.set_block(FOOT.offset(x, 1, 0), sand, UpdateFlags::UPDATE_ALL);
    }
    let skull = vanilla_blocks::WITHER_SKELETON_SKULL.default_state();
    for x in (-1..=1).take(skulls) {
        world.set_block(FOOT.offset(x, 2, 0), skull, UpdateFlags::UPDATE_ALL);
    }
}

/// Puts the last skull down through the block behavior, the way a player does.
fn place_last_skull(world: &Arc<World>, pos: BlockPos) {
    let behavior = WitherSkullBlock::new(&vanilla_blocks::WITHER_SKELETON_SKULL);
    let state = vanilla_blocks::WITHER_SKELETON_SKULL.default_state();
    world.set_block(pos, state, UpdateFlags::UPDATE_ALL);
    let mut item = ItemStack::new(&vanilla_items::WITHER_SKELETON_SKULL);
    let source = PlacementSource::direct(
        None,
        InteractionHand::MainHand,
        &mut item,
        PlacementOrientation::Player {
            rotation: 0.0,
            pitch: 0.0,
        },
        false,
    );
    behavior.set_placed_by(state, world, pos, &source);
}

fn withers_in(world: &Arc<World>) -> Vec<SharedEntity> {
    let search = WorldAabb::new(0.0, 56.0, 0.0, 16.0, 80.0, 16.0);
    world.get_entities_in_aabb_matching(&search, |entity| {
        entity.entity_type() == &vanilla_entities::WITHER
    })
}

/// The whole summon: the third skull completes the pattern, the frame is eaten,
/// and what comes out is already counting down rather than a plain mob.
#[test]
fn the_last_skull_on_a_soul_sand_t_summons_an_arriving_wither_and_eats_the_frame() {
    let world = prepared_world("wither_summon_full");
    build_frame(&world, 2);

    place_last_skull(&world, FOOT.offset(1, 2, 0));

    let withers = withers_in(&world);
    assert_eq!(withers.len(), 1, "one wither, built from the frame");
    let wither = withers[0]
        .downcast_ref::<WitherBoss>()
        .expect("the summoned entity should be a wither");
    assert_eq!(
        wither.invulnerable_ticks(),
        INVULNERABLE_TICKS,
        "a summoned wither arrives invulnerable"
    );
    assert!(
        (wither.get_health() - wither.get_max_health() / 3.0).abs() < 1.0e-4,
        "a summoned wither arrives at a third health"
    );

    for consumed in [
        FOOT,
        FOOT.offset(-1, 1, 0),
        FOOT.offset(0, 1, 0),
        FOOT.offset(1, 1, 0),
        FOOT.offset(-1, 2, 0),
        FOOT.offset(0, 2, 0),
        FOOT.offset(1, 2, 0),
    ] {
        assert!(
            world.get_block_state(consumed).is_air(),
            "{consumed:?} should have been eaten by the summon"
        );
    }
}

/// A frame one skull short is just a pile of soul sand.
#[test]
fn a_frame_missing_a_skull_keeps_its_blocks_and_summons_nothing() {
    let world = prepared_world("wither_summon_incomplete");
    build_frame(&world, 1);

    place_last_skull(&world, FOOT.offset(0, 2, 0));

    assert!(withers_in(&world).is_empty());
    assert_eq!(
        world.get_block_state(FOOT).get_block(),
        &vanilla_blocks::SOUL_SAND,
        "an unfinished frame keeps its blocks"
    );
}

/// Vanilla parity: `checkSpawn` refuses on peaceful, which is what stops a
/// wither appearing in a world that cannot have hostiles.
#[test]
fn a_finished_frame_summons_nothing_on_peaceful() {
    let world = prepared_world("wither_summon_peaceful");
    world.set_difficulty(Difficulty::Peaceful);
    build_frame(&world, 2);

    place_last_skull(&world, FOOT.offset(1, 2, 0));

    assert!(withers_in(&world).is_empty());
    assert_eq!(
        world.get_block_state(FOOT).get_block(),
        &vanilla_blocks::SOUL_SAND,
        "a refused summon leaves the frame standing"
    );
}
