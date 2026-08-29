//! What an enderman does with the block it is holding.
//!
//! The dice in front of both goals -- one attempt in ten to take, one in a
//! thousand to leave -- are what make them unpleasant to drive from a unit
//! test. These call the goal bodies directly and leave "the goals are wired
//! into the selector at all" to `dev/enderman-block-test.sh`, which watches a
//! real enderman strip a real field.

use std::io::Cursor;
use std::sync::Arc;

use foton_registry::init_vanilla_registry;
use foton_utils::ChunkPos;
use simdnbt::borrow::read_compound as read_borrowed_compound;

use super::*;
use crate::behavior::init_behaviors;
use crate::entity::entities::ItemEntity;
use crate::entity::next_entity_id;
use crate::test_support::{fresh_test_world, insert_ready_full_chunk};

/// The one spot the arenas below are built around.
const SPAWN: DVec3 = DVec3::new(8.5, 64.0, 8.5);
const STAND: BlockPos = BlockPos::new(8, 64, 8);

fn enderman_world(key: &'static str) -> Arc<World> {
    init_vanilla_registry();
    init_behaviors();
    let world = fresh_test_world(key);
    insert_ready_full_chunk(&world, ChunkPos::new(0, 0));
    world
}

fn set(world: &Arc<World>, pos: BlockPos, state: BlockStateId) {
    assert!(
        world.set_block(pos, state, UpdateFlags::UPDATE_NONE),
        "the test chunk should accept a block at {pos:?}"
    );
}

fn spawn_enderman(world: &Arc<World>) -> Arc<EndermanEntity> {
    let enderman = Arc::new(EndermanEntity::new(
        &vanilla_entities::ENDERMAN,
        next_entity_id(),
        SPAWN,
        Arc::downgrade(world),
    ));
    world
        .try_add_entity(Arc::clone(&enderman) as SharedEntity)
        .expect("the test chunk is loaded, so the enderman should attach");
    enderman
}

fn saved_nbt(enderman: &EndermanEntity) -> NbtCompound {
    let mut nbt = NbtCompound::new();
    Entity::save_additional(enderman, &mut nbt);
    nbt
}

fn load_into(enderman: &EndermanEntity, nbt: &NbtCompound) {
    let mut bytes = Vec::new();
    nbt.write(&mut bytes);
    let borrowed = read_borrowed_compound(&mut Cursor::new(&bytes))
        .unwrap_or_else(|error| panic!("test nbt should reborrow: {error}"));
    Entity::load_additional(enderman, (&borrowed).into());
}

/// Vanilla parity: the `carriedBlockState` of `addAdditionalSaveData`. Without
/// it an enderman that carried a block through a chunk unload comes back
/// empty-handed -- and, because `requiresCustomPersistence` reads the same
/// field, would have been eligible to despawn in the meantime.
#[test]
fn a_carried_block_survives_a_save_and_load() {
    let world = enderman_world("enderman_carry_roundtrip");
    let enderman = spawn_enderman(&world);
    let carried = vanilla_blocks::PUMPKIN.default_state();
    enderman.set_carried_block(Some(carried));

    let nbt = saved_nbt(&enderman);
    let reloaded = spawn_enderman(&world);
    load_into(&reloaded, &nbt);

    assert_eq!(reloaded.carried_block(), Some(carried));
    assert!(
        Mob::requires_custom_persistence(reloaded.as_ref()),
        "an enderman holding a block must not be allowed to despawn"
    );
}

/// Vanilla parity: the `.filter(blockState -> !blockState.isAir())` of
/// `readAdditionalSaveData`. A saved air state means "carrying nothing".
#[test]
fn a_saved_air_state_is_not_a_carried_block() {
    let world = enderman_world("enderman_carry_air");
    let enderman = spawn_enderman(&world);

    let mut nbt = NbtCompound::new();
    nbt.insert(
        TAG_CARRIED_BLOCK_STATE,
        NbtTag::Compound(block_state_nbt::save(vanilla_blocks::AIR.default_state())),
    );
    load_into(&enderman, &nbt);

    assert_eq!(enderman.carried_block(), None);
}

/// Vanilla parity: `EndermanTakeBlockGoal.tick`. The block leaves the world and
/// arrives in the enderman's hands as its *default* state, which is why an
/// enderman never carries a half-grown or oddly-rotated one.
#[test]
fn the_take_goal_lifts_a_holdable_block_out_of_the_world() {
    let world = enderman_world("enderman_take_block");
    // A shell of grass around a one-block slot, so the enderman stands in the
    // open with something holdable on every side and cannot walk off.
    for x in (STAND.x() - 2)..=(STAND.x() + 2) {
        for z in (STAND.z() - 2)..=(STAND.z() + 2) {
            set(
                &world,
                BlockPos::new(x, STAND.y() - 1, z),
                vanilla_blocks::STONE.default_state(),
            );
            for y in STAND.y()..=(STAND.y() + 2) {
                if x == STAND.x() && z == STAND.z() {
                    continue;
                }
                set(
                    &world,
                    BlockPos::new(x, y, z),
                    vanilla_blocks::GRASS_BLOCK.default_state(),
                );
            }
        }
    }
    let enderman = spawn_enderman(&world);

    let mut goal = EndermanTakeBlockGoal;
    for _ in 0..300 {
        Goal::tick(&mut goal, enderman.as_ref());
        if enderman.carried_block().is_some() {
            break;
        }
    }

    assert_eq!(
        enderman.carried_block(),
        Some(vanilla_blocks::GRASS_BLOCK.default_state()),
        "the enderman should be holding the grass block it reached for"
    );
    let holes = (STAND.x() - 2..=STAND.x() + 2)
        .flat_map(|x| (STAND.z() - 2..=STAND.z() + 2).map(move |z| (x, z)))
        .flat_map(|(x, z)| (STAND.y()..=STAND.y() + 2).map(move |y| BlockPos::new(x, y, z)))
        .filter(|pos| {
            !(pos.x() == STAND.x() && pos.z() == STAND.z()) && world.get_block_state(*pos).is_air()
        })
        .count();
    assert_eq!(
        holes, 1,
        "exactly the block it took should be missing from the shell"
    );
}

/// Vanilla parity: `EndermanLeaveBlockGoal.tick`, which is the half that makes
/// the take permanent: the block reappears somewhere else and the enderman's
/// hands come up empty.
#[test]
fn the_leave_goal_puts_the_block_down_and_lets_go() {
    let world = enderman_world("enderman_leave_block");
    // Bare stone floor and open air above it: every cell the goal can pick at
    // the enderman's own feet level is a legal placement, and every cell a
    // block higher is not, because nothing would hold it up.
    for x in (STAND.x() - 2)..=(STAND.x() + 2) {
        for z in (STAND.z() - 2)..=(STAND.z() + 2) {
            set(
                &world,
                BlockPos::new(x, STAND.y() - 1, z),
                vanilla_blocks::STONE.default_state(),
            );
        }
    }
    let enderman = spawn_enderman(&world);
    let carried = vanilla_blocks::PUMPKIN.default_state();
    enderman.set_carried_block(Some(carried));

    let mut goal = EndermanLeaveBlockGoal;
    for _ in 0..200 {
        Goal::tick(&mut goal, enderman.as_ref());
        if enderman.carried_block().is_none() {
            break;
        }
    }

    assert_eq!(
        enderman.carried_block(),
        None,
        "the enderman should have let the block go"
    );
    let placed = (STAND.x() - 1..=STAND.x() + 1)
        .flat_map(|x| (STAND.z() - 1..=STAND.z() + 1).map(move |z| BlockPos::new(x, STAND.y(), z)))
        .filter(|pos| world.get_block_state(*pos) == carried)
        .collect::<Vec<_>>();
    assert_eq!(
        placed.len(),
        1,
        "the block it let go of has to turn up in the world"
    );
}

/// Vanilla parity: the `level.getEntities(this.enderman, unitCube).isEmpty()`
/// of `EndermanLeaveBlockGoal.canPlaceBlock` -- the rule that stops an enderman
/// setting a block down inside somebody.
///
/// Vanilla passes itself as the exclusion, so the cell the enderman is standing
/// in is *not* refused; only a cell somebody else occupies is. The empty cell
/// is the control: it differs from the occupied one in nothing else.
#[test]
fn a_block_is_never_left_in_a_cell_someone_else_is_standing_in() {
    let world = enderman_world("enderman_leave_occupied");
    for x in (STAND.x() - 1)..=(STAND.x() + 1) {
        for z in (STAND.z() - 1)..=(STAND.z() + 1) {
            set(
                &world,
                BlockPos::new(x, STAND.y() - 1, z),
                vanilla_blocks::STONE.default_state(),
            );
        }
    }
    let enderman = spawn_enderman(&world);
    let carried = vanilla_blocks::PUMPKIN.default_state();

    let occupied = BlockPos::new(STAND.x() + 1, STAND.y(), STAND.z());
    let empty = BlockPos::new(STAND.x() - 1, STAND.y(), STAND.z());
    let bystander = Arc::new(EndermanEntity::new(
        &vanilla_entities::ENDERMAN,
        next_entity_id(),
        DVec3::new(
            f64::from(occupied.x()) + 0.5,
            f64::from(occupied.y()),
            f64::from(occupied.z()) + 0.5,
        ),
        Arc::downgrade(&world),
    ));
    world
        .try_add_entity(bystander as SharedEntity)
        .expect("the test chunk is loaded, so the bystander should attach");

    assert!(
        can_place_block(&world, enderman.as_ref(), empty, carried, empty.below()),
        "an empty cell beside the enderman is a legal placement, or the check below proves nothing"
    );
    assert!(
        !can_place_block(
            &world,
            enderman.as_ref(),
            occupied,
            carried,
            occupied.below()
        ),
        "a cell another entity is standing in must be refused"
    );
}

/// Vanilla parity: `EnderMan.dropCustomDeathLoot`, whose fake diamond axe is
/// enchanted from `minecraft:enderman_loot_drop`.
///
/// A grass block is the witness precisely because the enchantment decides the
/// answer: silk touch drops the grass block, a bare axe drops dirt.
#[test]
fn an_enderman_killed_holding_a_grass_block_drops_the_grass_block() {
    let world = enderman_world("enderman_death_drop");
    set(
        &world,
        BlockPos::new(STAND.x(), STAND.y() - 1, STAND.z()),
        vanilla_blocks::STONE.default_state(),
    );
    let enderman = spawn_enderman(&world);
    enderman.set_carried_block(Some(vanilla_blocks::GRASS_BLOCK.default_state()));

    LivingEntity::drop_custom_death_loot(
        enderman.as_ref(),
        &DamageSource::environment(&vanilla_damage_types::GENERIC_KILL),
        false,
    );

    let dropped = world
        .get_entities_in_aabb_matching(
            &WorldAabb::new(0.0, 60.0, 0.0, 16.0, 70.0, 16.0),
            |entity| entity.downcast_ref::<ItemEntity>().is_some(),
        )
        .into_iter()
        .filter_map(|entity| {
            entity
                .downcast_ref::<ItemEntity>()
                .map(ItemEntity::get_item)
        })
        .collect::<Vec<_>>();

    assert_eq!(dropped.len(), 1, "the carried block owes exactly one drop");
    assert!(
        dropped[0].is(&vanilla_items::GRASS_BLOCK),
        "a grass block dropping dirt means the fake tool lost its silk touch"
    );
}

/// Vanilla parity: the `NearestAttackableTargetGoal<>(this, Endermite.class,
/// true, false)` at target priority 3. The module carried a TODO saying the
/// endermite did not exist; it does, and this is the goal that makes one worth
/// running from.
///
/// This comes in through `Entity::tick` rather than the goal, because the
/// question is whether the target selector reaches it at all.
#[test]
fn an_enderman_hunts_a_nearby_endermite() {
    use crate::entity::entities::EndermiteEntity;

    let world = enderman_world("enderman_hunts_endermite");
    set(
        &world,
        BlockPos::new(STAND.x(), STAND.y() - 1, STAND.z()),
        vanilla_blocks::STONE.default_state(),
    );
    let enderman = spawn_enderman(&world);
    let endermite = Arc::new(EndermiteEntity::new(
        &vanilla_entities::ENDERMITE,
        next_entity_id(),
        SPAWN + DVec3::new(2.0, 0.0, 0.0),
        Arc::downgrade(&world),
    ));
    world
        .try_add_entity(Arc::clone(&endermite) as SharedEntity)
        .expect("the test chunk is loaded, so the endermite should attach");

    let mut hunted = false;
    for _ in 0..100 {
        Entity::tick(enderman.as_ref());
        if let Some(target) = Mob::target(enderman.as_ref()) {
            hunted = target.entity_type() == &vanilla_entities::ENDERMITE;
            break;
        }
    }

    assert!(
        hunted,
        "an enderman standing next to an endermite should have taken it as a target"
    );
}
