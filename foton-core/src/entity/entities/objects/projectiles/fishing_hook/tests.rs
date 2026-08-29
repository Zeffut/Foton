use std::ops::RangeInclusive;

use foton_registry::init_vanilla_registry;
use foton_registry::loot_table::{
    EntityPredicate, EntityRef, LootCondition, LootContext, LootContextEntity,
};
use foton_utils::types::UpdateFlags;
use foton_utils::{ChunkPos, WorldAabb};

use super::*;
use crate::behavior::init_behaviors;
use crate::player::Player;
use crate::test_support::{TestPlayerBuilder, fresh_test_world, insert_ready_full_chunk};

/// The one spot the test world is guaranteed to be solid ground rather than a
/// column the fluid scan walks.
const SPAWN: DVec3 = DVec3::new(8.5, 64.0, 8.5);

fn fishing_world(key: &'static str) -> Arc<World> {
    init_vanilla_registry();
    init_behaviors();
    let world = fresh_test_world(key);
    insert_ready_full_chunk(&world, ChunkPos::new(0, 0));
    world
}

/// Fills `y_range` of the 5x5 open-water scan area around x/z 8 with water.
fn fill_pool(world: &Arc<World>, y_range: RangeInclusive<i32>, half_width: i32) {
    let water = vanilla_blocks::WATER.default_state();
    for y in y_range {
        for x in (8 - half_width)..=(8 + half_width) {
            for z in (8 - half_width)..=(8 + half_width) {
                assert!(world.set_block(BlockPos::new(x, y, z), water, UpdateFlags::UPDATE_ALL));
            }
        }
    }
}

fn cast_hook(world: &Arc<World>, position: DVec3, owner: &Arc<Player>) -> Arc<FishingHookEntity> {
    let hook = Arc::new(FishingHookEntity::new(
        &vanilla_entities::FISHING_BOBBER,
        next_entity_id(),
        position,
        Arc::downgrade(world),
    ));
    let owner_entity: SharedEntity = Arc::clone(owner) as SharedEntity;
    hook.set_owner_entity(Some(&owner_entity));
    let entity: SharedEntity = Arc::clone(&hook) as SharedEntity;
    world
        .try_add_entity(entity)
        .expect("the test chunk is loaded, so the hook should attach");
    hook
}

fn rod_holder(world: &Arc<World>, name: &'static str, position: DVec3) -> Arc<Player> {
    let player = TestPlayerBuilder::new(Arc::clone(world), name, next_entity_id()).build();
    *player
        .inventory
        .lock()
        .get_item_in_hand_mut(InteractionHand::MainHand) =
        ItemStack::new(&vanilla_items::FISHING_ROD);
    player
        .try_set_position(position)
        .expect("the test chunk is loaded, so the player should move");
    player
}

#[test]
fn a_pool_deep_and_wide_enough_is_open_water() {
    let world = fishing_world("fishing_open_water_pool");
    // The bobber sits in the top water block, so the scan sees water at y-1 and
    // y, then air at y+1 and y+2.
    fill_pool(&world, 62..=63, 2);

    assert!(
        FishingHookEntity::calculate_open_water(&world, BlockPos::new(8, 63, 8)),
        "a five-by-five pool two blocks deep with open sky is vanilla open water"
    );
}

#[test]
fn a_pool_narrower_than_the_scan_area_is_not_open_water() {
    let world = fishing_world("fishing_open_water_narrow");
    // Three blocks across, so the outer ring of the five-by-five scan is air
    // inside a layer that is otherwise water.
    fill_pool(&world, 62..=63, 1);

    assert!(
        !FishingHookEntity::calculate_open_water(&world, BlockPos::new(8, 63, 8)),
        "a mixed layer is INVALID, which is what stops treasure fishing in a puddle"
    );
}

#[test]
fn a_roof_over_the_pool_closes_the_open_water() {
    let world = fishing_world("fishing_open_water_roof");
    fill_pool(&world, 62..=63, 2);
    let stone = vanilla_blocks::STONE.default_state();
    for x in 6..=10 {
        for z in 6..=10 {
            assert!(world.set_block(BlockPos::new(x, 65, z), stone, UpdateFlags::UPDATE_ALL));
        }
    }

    assert!(
        !FishingHookEntity::calculate_open_water(&world, BlockPos::new(8, 63, 8)),
        "a solid layer above the water is INVALID, so fishing under an overhang \
         cannot reach treasure"
    );
}

#[test]
fn a_hook_that_reaches_water_starts_bobbing() {
    let world = fishing_world("fishing_hook_starts_bobbing");
    fill_pool(&world, 62..=63, 4);
    let player = rod_holder(&world, "Angler", DVec3::new(8.5, 64.0, 4.5));
    let hook = cast_hook(&world, DVec3::new(8.5, 66.0, 8.5), &player);
    hook.set_velocity(DVec3::new(0.0, -0.5, 0.0));

    for _ in 0..40 {
        hook.set_old_position_to_current();
        hook.advance_tick_count();
        hook.tick();
        if hook.state.lock().current_state == FishHookState::Bobbing {
            break;
        }
    }

    assert_eq!(
        hook.state.lock().current_state,
        FishHookState::Bobbing,
        "a hook dropped into water must switch out of FLYING"
    );
    assert!(
        !hook.is_removed(),
        "the caster still holds a rod and is well inside the leash"
    );
}

/// The whole catch, end to end: the loot roll, the item it produces, and the
/// experience that comes with it. Each of those layers looks fine on its own.
#[test]
fn reeling_in_a_nibbling_hook_drops_a_catch_and_experience() {
    let world = fishing_world("fishing_hook_catch");
    fill_pool(&world, 62..=63, 4);
    let player = rod_holder(&world, "Reeler", DVec3::new(8.5, 64.0, 6.5));
    let hook = cast_hook(&world, DVec3::new(8.5, 63.5, 8.5), &player);
    hook.state.lock().nibble = 20;

    let rod = ItemStack::new(&vanilla_items::FISHING_ROD);
    assert_eq!(
        hook.retrieve(&rod),
        1,
        "reeling a bite in costs the rod one durability"
    );
    assert!(
        hook.is_removed(),
        "the bobber is spent once it is reeled in"
    );
    assert!(
        player.fishing_hook().is_none(),
        "the caster must be free to cast again"
    );

    let area = WorldAabb::new(0.0, 55.0, 0.0, 16.0, 75.0, 16.0);
    let spawned = world.get_entities_in_aabb(&area);
    assert!(
        spawned
            .iter()
            .any(|entity| entity.entity_type() == &vanilla_entities::ITEM),
        "a bite must drop whatever gameplay/fishing rolled"
    );
    assert!(
        spawned
            .iter()
            .any(|entity| entity.entity_type() == &vanilla_entities::EXPERIENCE_ORB),
        "vanilla awards experience for every item a catch produces"
    );
}

/// Reeling a hooked entity in drags it toward the caster and costs the rod the
/// item-entity rate rather than the living-entity one.
#[test]
fn reeling_a_hooked_item_drags_it_toward_the_caster() {
    let world = fishing_world("fishing_hook_pull");
    let player = rod_holder(&world, "Puller", DVec3::new(8.5, 64.0, 2.5));
    let hook = cast_hook(&world, DVec3::new(8.5, 64.0, 10.5), &player);

    let catch: SharedEntity = Arc::new(ItemEntity::with_item_and_velocity(
        &vanilla_entities::ITEM,
        next_entity_id(),
        DVec3::new(8.5, 64.0, 10.5),
        ItemStack::new(&vanilla_items::STICK),
        DVec3::ZERO,
        Arc::downgrade(&world),
    ));
    world
        .try_add_entity(Arc::clone(&catch))
        .expect("the test chunk is loaded, so the item should attach");
    hook.set_hooked_entity(Some(&catch));

    let rod = ItemStack::new(&vanilla_items::FISHING_ROD);
    assert_eq!(
        hook.retrieve(&rod),
        3,
        "vanilla charges three durability for an item entity and five for anything else"
    );
    assert!(
        catch.velocity().z < 0.0,
        "the catch has to be pulled back toward the caster, who stands at lower z"
    );
}

#[test]
fn a_hook_with_no_owner_discards_itself() {
    let world = fishing_world("fishing_hook_no_owner");
    let hook = Arc::new(FishingHookEntity::new(
        &vanilla_entities::FISHING_BOBBER,
        next_entity_id(),
        SPAWN,
        Arc::downgrade(&world),
    ));
    let entity: SharedEntity = Arc::clone(&hook) as SharedEntity;
    world
        .try_add_entity(entity)
        .expect("the test chunk is loaded, so the hook should attach");

    hook.tick();

    assert!(hook.is_removed(), "an ownerless bobber has nothing to reel");
}

#[test]
fn casting_points_the_hook_along_the_caster_look_direction() {
    let world = fishing_world("fishing_hook_cast");
    let player = rod_holder(&world, "Caster", SPAWN);
    // Yaw 0 faces +Z in Minecraft, and a flat pitch keeps the throw horizontal.
    player.set_rotation((0.0, 0.0));

    let hook = FishingHookEntity::new(
        &vanilla_entities::FISHING_BOBBER,
        next_entity_id(),
        DVec3::ZERO,
        Arc::downgrade(&world),
    );
    hook.cast_from(&player, 3, 60);

    assert!(
        hook.velocity().z > 0.0,
        "a bobber thrown while facing +Z has to travel +Z, not back at the caster"
    );
    let state = hook.state.lock();
    assert_eq!(state.luck, 3);
    assert_eq!(state.lure_speed, 60);
}

/// The treasure entry of `gameplay/fishing` is gated on
/// `minecraft:type_specific/fishing_hook.in_open_water`. Rolling the real table
/// is the only thing that proves the predicate reaches the interpreter: an
/// unmodeled predicate key fails silently by never matching, which reads exactly
/// like bad luck.
#[test]
fn treasure_is_only_reachable_from_open_water() {
    const ROLLS: usize = 400;

    init_vanilla_registry();

    let treasure_only = [
        "name_tag",
        "saddle",
        "nautilus_shell",
        "bow",
        "book",
        "enchanted_book",
    ];
    let is_treasure = |items: &[ItemStack]| {
        items
            .iter()
            .any(|item| treasure_only.contains(&item.item().key.path.as_ref()))
    };

    let mut rng = rand::rng();
    let mut open_water_treasure = 0;
    let mut closed_water_treasure = 0;
    for open in [true, false] {
        for _ in 0..ROLLS {
            let mut context = LootContext::new(&mut rng).with_this_entity(EntityRef {
                in_open_water: Some(open),
                ..EntityRef::default()
            });
            let items = vanilla_loot_tables::GAMEPLAY_FISHING.get_random_items(&mut context);
            if is_treasure(&items) {
                if open {
                    open_water_treasure += 1;
                } else {
                    closed_water_treasure += 1;
                }
            }
        }
    }

    assert_eq!(
        closed_water_treasure, 0,
        "treasure must be unreachable without open water"
    );
    assert!(
        open_water_treasure > 0,
        "treasure must be reachable from open water; {ROLLS} rolls at a 5% weight \
         produced none, so the open-water predicate is not being read"
    );
}

/// A predicate that asks about open water must fail against anything that is
/// not a fishing hook, the way vanilla `FishingHookPredicate.matches` does.
#[test]
fn the_open_water_predicate_rejects_a_non_hook() {
    init_vanilla_registry();

    let predicate = EntityPredicate {
        in_open_water: Some(true),
        ..EntityPredicate::ANY
    };
    let mut rng = rand::rng();
    let mut context = LootContext::new(&mut rng).with_this_entity(EntityRef::default());
    let condition = LootCondition::EntityProperties {
        entity: LootContextEntity::This,
        predicate,
    };

    assert!(!condition.test(&mut context));
}
