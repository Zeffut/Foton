use std::sync::Weak;

use steel_registry::{init_vanilla_registry, vanilla_blocks, vanilla_entities};
use steel_utils::ChunkPos;
use steel_utils::types::UpdateFlags;

use super::*;
use crate::behavior::init_behaviors;
use crate::entity::ai::brain::memory::memory_module_types;
use crate::entity::entities::MagmaCubeEntity;
use crate::entity::next_entity_id;
use crate::test_support::{fresh_test_world, insert_ready_full_chunk};

fn new_frog() -> FrogEntity {
    init_vanilla_registry();
    FrogEntity::new(
        &vanilla_entities::FROG,
        next_entity_id(),
        DVec3::new(8.5, 64.0, 8.5),
        Weak::new(),
    )
}

#[test]
fn a_frog_only_swallows_a_cube_mob_of_size_one() {
    // Vanilla's `canEat` is what stops a frog eating a full-size magma cube, and
    // the size-one branch is exactly the one that leaves a froglight behind.
    init_vanilla_registry();
    let small = MagmaCubeEntity::new(
        &vanilla_entities::MAGMA_CUBE,
        next_entity_id(),
        DVec3::new(8.5, 64.0, 8.5),
        Weak::new(),
    );
    small.set_cube_size(1, true);
    let large = MagmaCubeEntity::new(
        &vanilla_entities::MAGMA_CUBE,
        next_entity_id(),
        DVec3::new(8.5, 64.0, 8.5),
        Weak::new(),
    );
    large.set_cube_size(2, true);

    assert!(FrogEntity::can_eat(&small));
    assert!(!FrogEntity::can_eat(&large));
}

#[test]
fn a_frog_spawns_on_mud_where_a_plain_animal_would_not() {
    // Mud is the discriminating block: it is in `#minecraft:frogs_spawnable_on`
    // and not in the `#minecraft:animals_spawnable_on` every other animal reads,
    // so a frog wired to the shared tag would refuse here.
    init_vanilla_registry();
    init_behaviors();
    let world = fresh_test_world("frog_spawn_rules");
    let pos = BlockPos::new(8, 64, 8);
    insert_ready_full_chunk(&world, ChunkPos::from_block_pos(pos));

    assert!(world.set_block(
        pos.below(),
        vanilla_blocks::STONE.default_state(),
        UpdateFlags::UPDATE_NONE
    ));
    assert!(!FrogEntity::check_frog_spawn_rules(&world, pos));

    assert!(world.set_block(
        pos.below(),
        vanilla_blocks::MUD.default_state(),
        UpdateFlags::UPDATE_NONE
    ));
    assert!(FrogEntity::check_frog_spawn_rules(&world, pos));
}

#[test]
fn a_frog_remembers_the_variant_it_is_saved_with() {
    use std::io::Cursor;

    use simdnbt::borrow::read_compound as read_borrowed_compound;

    let frog = new_frog();
    frog.set_variant(&vanilla_frog_variants::COLD);

    let mut nbt = NbtCompound::new();
    frog.save_additional(&mut nbt);
    let mut bytes = Vec::new();
    nbt.write(&mut bytes);
    let borrowed = read_borrowed_compound(&mut Cursor::new(&bytes))
        .unwrap_or_else(|error| panic!("frog nbt should reborrow: {error}"));

    let reloaded = new_frog();
    reloaded.load_additional((&borrowed).into());

    assert_eq!(reloaded.variant().key, vanilla_frog_variants::COLD.key);
}

#[test]
fn a_frog_tracks_and_forgets_its_tongue_target() {
    // The tongue target is a synced entity id, and `ShootTongue.stop` clears it;
    // a frog that kept it would keep its mouth open forever on the client.
    init_vanilla_registry();
    init_behaviors();
    let world = fresh_test_world("frog_tongue_target");
    let pos = BlockPos::new(8, 64, 8);
    insert_ready_full_chunk(&world, ChunkPos::from_block_pos(pos));

    let prey: SharedEntity = Arc::new(MagmaCubeEntity::new(
        &vanilla_entities::MAGMA_CUBE,
        next_entity_id(),
        DVec3::new(8.5, 64.0, 8.5),
        Arc::downgrade(&world),
    ));
    world
        .try_add_entity(Arc::clone(&prey))
        .unwrap_or_else(|error| panic!("prey should enter the test world: {error:?}"));

    let frog = Arc::new(FrogEntity::new(
        &vanilla_entities::FROG,
        next_entity_id(),
        DVec3::new(8.5, 64.0, 8.5),
        Arc::downgrade(&world),
    ));
    world
        .try_add_entity(Arc::clone(&frog) as SharedEntity)
        .unwrap_or_else(|error| panic!("frog should enter the test world: {error:?}"));

    assert!(frog.tongue_target().is_none());

    frog.set_tongue_target(&prey);
    assert_eq!(
        frog.tongue_target().map(|target| target.id()),
        Some(prey.id())
    );

    frog.erase_tongue_target();
    assert!(frog.tongue_target().is_none());
}

#[test]
fn breeding_leaves_a_frog_pregnant_rather_than_with_a_tadpole() {
    // Vanilla's `Frog.spawnChildFromBreeding` produces no child: the frogspawn
    // it lays later is the whole next generation. A frog that came out of this
    // with a baby would skip the block, and the loop with it.
    init_vanilla_registry();
    init_behaviors();
    let world = fresh_test_world("frog_breeding");
    let pos = BlockPos::new(8, 64, 8);
    insert_ready_full_chunk(&world, ChunkPos::from_block_pos(pos));

    let frog = Arc::new(FrogEntity::new(
        &vanilla_entities::FROG,
        next_entity_id(),
        DVec3::new(8.5, 64.0, 8.5),
        Arc::downgrade(&world),
    ));
    let partner = Arc::new(FrogEntity::new(
        &vanilla_entities::FROG,
        next_entity_id(),
        DVec3::new(8.5, 64.0, 8.5),
        Arc::downgrade(&world),
    ));
    for entity in [Arc::clone(&frog), Arc::clone(&partner)] {
        world
            .try_add_entity(entity as SharedEntity)
            .unwrap_or_else(|error| panic!("frog should enter the test world: {error:?}"));
    }

    // Only the frogs are counted: `finalizeSpawnChildFromBreeding` also drops
    // the breeding experience, and those orbs are not a child.
    let search = frog.bounding_box().inflate(8.0);
    let count_frogs = || {
        world
            .get_entities_in_aabb_matching(&search, |entity| {
                entity.entity_type() == &vanilla_entities::FROG
            })
            .len()
    };
    let before = count_frogs();
    Animal::spawn_child_from_breeding(frog.as_ref(), &world, partner.as_ref());

    assert_eq!(count_frogs(), before, "a bred frog spawns no child");
    assert!(
        frog.brain
            .has_memory_value(memory_module_types::IS_PREGNANT.id()),
        "a bred frog carries IS_PREGNANT until it finds water"
    );
}

#[test]
fn a_frog_reaches_its_brain_and_survives_forty_ticks_in_a_live_world() {
    // A frog whose `server_ai_step` does not reach `Mob::mob_server_ai_step`
    // never ticks its brain at all, and the tick loop catches a lock-ordering
    // hang in the amphibious navigation the same way the bee's does.
    init_vanilla_registry();
    init_behaviors();
    let world = fresh_test_world("frog_ticks");
    let pos = BlockPos::new(8, 64, 8);
    insert_ready_full_chunk(&world, ChunkPos::from_block_pos(pos));
    assert!(world.set_block(
        pos.below(),
        vanilla_blocks::GRASS_BLOCK.default_state(),
        UpdateFlags::UPDATE_NONE
    ));

    let frog = Arc::new(FrogEntity::new(
        &vanilla_entities::FROG,
        next_entity_id(),
        DVec3::new(8.5, 64.0, 8.5),
        Arc::downgrade(&world),
    ));
    world
        .try_add_entity(Arc::clone(&frog) as SharedEntity)
        .unwrap_or_else(|error| panic!("frog should enter the test world: {error:?}"));

    frog.set_no_action_time(0);
    LivingEntity::server_ai_step(frog.as_ref());
    assert!(
        frog.no_action_time() > 0,
        "the frog's brain never ticks: `server_ai_step` does not reach \
         `Mob::mob_server_ai_step`"
    );

    for _ in 0..40 {
        frog.tick();
    }

    assert!(Entity::is_alive(frog.as_ref()));
}
