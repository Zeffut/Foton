use std::sync::Weak;

use foton_registry::item_stack::ItemStack;
use foton_registry::{init_vanilla_registry, vanilla_attributes, vanilla_entities, vanilla_items};
use foton_utils::Identifier;
use glam::DVec3;

use super::*;
use crate::entity::entities::SlimeEntity;
use crate::entity::next_entity_id;
use crate::test_support::{fresh_test_world, insert_ready_full_chunk};

fn sulfur_cube() -> SulfurCubeEntity {
    init_vanilla_registry();
    SulfurCubeEntity::new(
        &vanilla_entities::SULFUR_CUBE,
        next_entity_id(),
        DVec3::new(8.5, 64.0, 8.5),
        Weak::new(),
    )
}

/// Puts `item_stack` in the body slot the way anything else would -- a hand, a
/// dispenser, a command -- and lets the equipment-change pass notice.
fn swallow(cube: &SulfurCubeEntity, item_stack: ItemStack) {
    cube.set_item_slot(EquipmentSlot::Body, item_stack);
    LivingEntity::detect_equipment_updates(cube);
}

/// Vanilla parity: `SulfurCube.setcubeMobHealth` is `4 * actualSize`, not the
/// `actualSize * actualSize` every other cube inherits. A grown sulfur cube has
/// eight health; a slime of the same size has four.
#[test]
#[expect(
    clippy::float_cmp,
    reason = "an attribute base value set from an exact literal"
)]
fn a_sulfur_cube_has_four_health_per_size_rather_than_its_size_squared() {
    let cube = sulfur_cube();

    cube.set_size(2, true);
    assert_eq!(cube.get_max_health(), 8.0);

    cube.set_size(1, true);
    assert_eq!(cube.get_max_health(), 4.0);
}

/// Vanilla parity: `SulfurCube.setSize` sets neither `ATTACK_DAMAGE` nor
/// `xpReward`, unlike the slime's and the magma cube's. It has no attack damage
/// attribute at all -- `createSulfurCubeAttributes` is `Mob.createMobAttributes`
/// plus a tempt range -- so a shared `apply_size` that set one would be writing
/// to an attribute vanilla leaves absent.
#[test]
fn sizing_a_sulfur_cube_gives_it_no_bite_and_no_reward() {
    let cube = sulfur_cube();
    cube.set_size(2, true);

    assert!(
        !cube
            .attributes()
            .lock()
            .has_attribute(vanilla_attributes::ATTACK_DAMAGE),
        "a sulfur cube has no attack damage attribute in vanilla"
    );
    assert_eq!(cube.xp_reward(), 0, "vanilla's setSize sets no xpReward");
}

/// The refactor that made the health a hook moved the slime's own two lines
/// into its `setSize`. Vanilla parity: `Slime.setSize`, which ends
/// `getAttribute(ATTACK_DAMAGE).setBaseValue(actualSize)` and
/// `this.xpReward = actualSize`.
#[test]
#[expect(
    clippy::float_cmp,
    reason = "an attribute base value set from an exact literal"
)]
fn a_slime_still_bites_and_pays_for_its_size() {
    init_vanilla_registry();
    let slime = SlimeEntity::new(
        &vanilla_entities::SLIME,
        next_entity_id(),
        DVec3::new(8.5, 64.0, 8.5),
        Weak::new(),
    );

    slime.set_cube_size(3, true);
    assert_eq!(
        slime
            .attributes()
            .lock()
            .required_value(vanilla_attributes::ATTACK_DAMAGE),
        3.0
    );
    assert_eq!(slime.xp_reward(), 3);
    assert_eq!(slime.get_max_health(), 9.0, "a slime is still size squared");
}

/// Vanilla parity: the `if (updateHealth && size == 1 && !isBaby())` of
/// `SulfurCube.setSize`. A sulfur cube's small form *is* its baby form, which is
/// why splitting one leaves two babies rather than two half-sized adults.
#[test]
fn sizing_a_sulfur_cube_down_to_one_makes_it_a_baby() {
    let cube = sulfur_cube();
    cube.set_size(2, true);
    assert!(!AgeableMob::is_baby(&cube));

    cube.set_size(1, true);
    assert!(AgeableMob::is_baby(&cube));
}

/// Vanilla parity: `SulfurCube.ageBoundaryReached`, which is what a fed baby
/// grows into.
#[test]
fn growing_up_makes_a_sulfur_cube_full_sized() {
    let cube = sulfur_cube();
    cube.set_size(1, true);
    assert!(AgeableMob::is_baby(&cube));

    cube.set_age(0);
    assert!(!AgeableMob::is_baby(&cube));
    assert_eq!(CubeLike::size(&cube), ADULT_SIZE);
}

/// Vanilla parity: `SulfurCube.getBaseExperienceReward`, `isBaby() ? 0 : 1 +
/// random.nextInt(2)`. The reward is not the size, which is why `setSize` sets
/// no `xpReward` for this cube to read.
#[test]
fn a_grown_sulfur_cube_is_worth_experience_and_a_baby_is_not() {
    let cube = sulfur_cube();
    cube.set_size(2, true);
    for _ in 0..16 {
        let reward = cube.base_experience_reward();
        assert!(
            (1..=2).contains(&reward),
            "a grown sulfur cube is worth one or two, got {reward}"
        );
    }

    cube.set_baby(true);
    assert_eq!(cube.base_experience_reward(), 0);
}

/// Vanilla parity: `SulfurCube.collectEquipmentChanges` applied to the
/// `explosive` archetype, whose item tag is `minecraft:tnt`. This is the whole
/// point of the archetype registry: the block in the body decides what the cube
/// is, and a cube that swallowed TNT is a walking charge.
#[test]
fn swallowing_tnt_makes_a_sulfur_cube_explosive() {
    let cube = sulfur_cube();
    cube.set_size(2, true);
    assert!(!cube.can_explode(), "an empty cube has no blast");

    swallow(&cube, ItemStack::new(&vanilla_items::TNT));
    assert!(cube.can_explode());
    assert_eq!(
        cube.state.lock().explosion.map(|explosion| explosion.fuse),
        Some(120),
        "the fuse comes from the explosive archetype's extracted data"
    );

    cube.shear();
    LivingEntity::detect_equipment_updates(&cube);
    assert!(!cube.can_explode(), "the blast left with the block");
}

/// Vanilla parity: the `removeAllGoals` / `registerGoals` pair of
/// `SulfurCube.collectEquipmentChanges`. A loaded cube does not steer itself --
/// that is what makes it something you kick rather than something that chases
/// you -- and it gets its goals back the moment the block comes out.
#[test]
fn swallowing_a_block_takes_the_cubes_goals_away_and_shearing_gives_them_back() {
    let cube = sulfur_cube();
    cube.set_size(2, true);
    let goals_when_empty = cube.mob_base().goal_selector().lock().goal_count();
    assert!(goals_when_empty > 0);

    swallow(&cube, ItemStack::new(&vanilla_items::TNT));
    assert_eq!(cube.mob_base().goal_selector().lock().goal_count(), 0);

    cube.shear();
    LivingEntity::detect_equipment_updates(&cube);
    assert_eq!(
        cube.mob_base().goal_selector().lock().goal_count(),
        goals_when_empty
    );
}

/// Vanilla parity: the `pickupTimer = 100` at the end of `SulfurCube.shear`,
/// without which a sheared cube swallows the block it just dropped on the next
/// tick and shears become useless.
#[test]
fn shearing_stops_the_cube_swallowing_the_block_straight_back() {
    let world = fresh_test_world("sulfur_cube_shear_pickup_timer");
    insert_ready_full_chunk(&world, ChunkPos::new(0, 0));

    let cube = sulfur_cube();
    cube.set_size(2, true);
    swallow(&cube, ItemStack::new(&vanilla_items::TNT));

    cube.shear();
    assert!(!cube.has_body_item());
    assert_eq!(cube.state.lock().pickup_timer, PICKUP_TIMER_DURATION);

    let dropped: SharedEntity = world
        .spawn_item(
            DVec3::new(8.5, 64.0, 8.5),
            ItemStack::new(&vanilla_items::TNT),
        )
        .expect("the test world accepts an item entity");
    Mob::pick_up_item(&cube, &world, &dropped);
    assert!(
        !cube.has_body_item(),
        "a freshly sheared cube swallowed the block it had just dropped"
    );

    cube.state.lock().pickup_timer = 0;
    Mob::pick_up_item(&cube, &world, &dropped);
    assert!(
        cube.has_body_item(),
        "once the timer runs out the cube swallows again"
    );
}

/// Vanilla parity: `SulfurCube.customServerAiStep`, reached through
/// `LivingEntity::server_ai_step` -- the door the tick itself uses. Without the
/// override the shear cooldown never counts down and a sheared cube stays
/// deaf to dropped blocks forever.
#[test]
fn the_shear_cooldown_counts_down_while_the_cube_ticks() {
    let cube = sulfur_cube();
    cube.set_size(2, true);
    cube.state.lock().pickup_timer = 3;

    LivingEntity::server_ai_step(&cube);
    assert_eq!(cube.state.lock().pickup_timer, 2);

    for _ in 0..4 {
        LivingEntity::server_ai_step(&cube);
    }
    assert_eq!(
        cube.state.lock().pickup_timer,
        0,
        "the timer ran past zero instead of stopping there"
    );
}

/// Vanilla parity: the two `matchingArchetypes` loops of
/// `SulfurCube.collectEquipmentChanges`, which take the previous block's
/// modifiers off before they put the new block's on. Leaving them on would let
/// a player stack every archetype by feeding a cube one block after another.
#[test]
fn an_archetypes_attribute_modifiers_come_off_when_its_block_leaves() {
    let cube = sulfur_cube();
    cube.set_size(2, true);
    let modifier = Identifier::vanilla_static("explosive_add_knockback_resistance");
    let has_modifier = |cube: &SulfurCubeEntity| {
        cube.attributes()
            .lock()
            .has_modifier(vanilla_attributes::KNOCKBACK_RESISTANCE, &modifier)
    };

    swallow(&cube, ItemStack::new(&vanilla_items::TNT));
    assert!(has_modifier(&cube));

    swallow(&cube, ItemStack::new(&vanilla_items::MAGMA_BLOCK));
    assert!(
        !has_modifier(&cube),
        "the explosive archetype's modifier survived the block leaving"
    );
}

/// Vanilla parity: `SulfurCube.collectEquipmentChanges` accumulating
/// `contactDamage`, applied to the `hot` archetype, whose tag is
/// `minecraft:magma_block`. A cube with a magma block in it burns what it
/// touches even though `isDealsDamage` is flatly `false`.
#[test]
fn a_cube_that_swallowed_a_magma_block_burns_what_it_touches() {
    let cube = sulfur_cube();
    cube.set_size(2, true);
    assert!(cube.state.lock().contact_damages.is_empty());
    assert!(!CubeLike::deals_damage(&cube), "no sulfur cube ever bites");

    swallow(&cube, ItemStack::new(&vanilla_items::MAGMA_BLOCK));
    assert_eq!(cube.state.lock().contact_damages.len(), 1);
}

/// Vanilla parity: `SulfurCube.canHoldItem` and `isSwallowableItem`, which is
/// `#minecraft:sulfur_cube_swallowable` -- a slime ball feeds a baby but is not
/// something a cube swallows.
#[test]
fn a_sulfur_cube_swallows_only_what_the_tag_allows() {
    let cube = sulfur_cube();
    cube.set_size(2, true);

    assert!(cube.can_hold_item(&ItemStack::new(&vanilla_items::TNT)));
    assert!(!cube.can_hold_item(&ItemStack::new(&vanilla_items::SLIME_BALL)));

    cube.set_baby(true);
    assert!(
        !cube.can_hold_item(&ItemStack::new(&vanilla_items::TNT)),
        "a baby swallows nothing"
    );
}

/// Vanilla parity: `SulfurCube.saveToBucketTag`, which copies the swallowed
/// block into the bucket's own `minecraft:sulfur_cube_content`. That component
/// is what stops two buckets of differently-fed cubes from stacking.
#[test]
fn a_bucketed_sulfur_cube_carries_what_it_swallowed() {
    let cube = sulfur_cube();
    cube.set_size(2, true);
    swallow(&cube, ItemStack::new(&vanilla_items::TNT));

    let mut bucket = cube.bucket_item_stack();
    cube.save_to_bucket_tag(&mut bucket);

    let content = bucket
        .get(vanilla_components::SULFUR_CUBE_CONTENT)
        .expect("a fed cube writes its block into the bucket");
    assert_eq!(
        content.absorbed_block_item_stack().item().key,
        vanilla_items::TNT.key
    );
}

/// Vanilla parity: `SulfurCube.canBePickedUpWithBucket`, an empty bucket rather
/// than the water bucket every fish wants.
#[test]
fn a_sulfur_cube_goes_in_an_empty_bucket_not_a_water_one() {
    let cube = sulfur_cube();

    assert!(cube.can_be_picked_up_with_bucket(&ItemStack::new(&vanilla_items::BUCKET)));
    assert!(!cube.can_be_picked_up_with_bucket(&ItemStack::new(&vanilla_items::WATER_BUCKET)));
}

/// Vanilla parity: `SulfurCube.getSplitCount`, which is `isPrimed() ? 0 : 2`.
/// A cube that goes off leaves nothing behind, which is what stops an explosive
/// cube from multiplying every time it detonates.
#[test]
fn a_primed_sulfur_cube_leaves_no_children() {
    let cube = sulfur_cube();
    cube.set_size(2, true);
    swallow(&cube, ItemStack::new(&vanilla_items::TNT));
    assert!(!cube.is_primed());

    cube.state.lock().fuse = 40;
    assert!(cube.is_primed());
    assert!(
        !cube.can_explode(),
        "a burning fuse cannot be lit a second time"
    );
}

/// Vanilla parity: `SulfurCube.getFluidJumpThreshold`, `getBbHeight() * 0.2`.
/// A grown cube is a block tall at size two, so it counts as swimming at a
/// fifth of that -- which is what lets a buoyant one ride a shallow pool.
#[test]
#[expect(
    clippy::float_cmp,
    reason = "a threshold computed from an exact hitbox height"
)]
fn a_sulfur_cube_counts_as_swimming_at_a_fifth_of_its_height() {
    let cube = sulfur_cube();

    cube.set_size(2, true);
    let grown_height = f64::from(cube.dimensions_for_pose(cube.pose()).height);
    assert_eq!(cube.get_fluid_jump_threshold(), grown_height * 0.2);

    cube.set_size(1, true);
    assert!(
        cube.get_fluid_jump_threshold() < grown_height * 0.2,
        "a baby sits lower in the water than a grown one"
    );
}

/// Vanilla parity: the `buoyant` flag of the archetype registry, read by
/// `SulfurCube.travelInFluid`. It is the difference between a cube you can
/// float across a lake and one that drops to the bottom.
#[test]
fn only_a_buoyant_block_makes_a_cube_float() {
    let cube = sulfur_cube();
    cube.set_size(2, true);
    assert!(!cube.state.lock().floats_in_liquids);

    // `explosive` is buoyant; `fast_sliding` is not.
    swallow(&cube, ItemStack::new(&vanilla_items::TNT));
    assert!(cube.state.lock().floats_in_liquids);

    swallow(&cube, ItemStack::new(&vanilla_items::BLUE_ICE));
    assert!(!cube.state.lock().floats_in_liquids);
}

/// Vanilla parity: `SulfurCube.getSquishSound`, whose grown form answers
/// `SULFUR_CUBE_BOUNCE` while something is swallowed and `SULFUR_CUBE_SQUISH`
/// otherwise. It is the audible half of the cube becoming a ball.
#[test]
fn a_loaded_sulfur_cube_bounces_where_an_empty_one_squishes() {
    let cube = sulfur_cube();
    cube.set_size(2, true);
    assert_eq!(
        cube.squish_sound().key,
        sound_events::ENTITY_SULFUR_CUBE_SQUISH.key
    );

    swallow(&cube, ItemStack::new(&vanilla_items::TNT));
    assert_eq!(
        cube.squish_sound().key,
        sound_events::ENTITY_SULFUR_CUBE_BOUNCE.key
    );
}

/// Vanilla parity: `SulfurCube.SulfurCubeMobMoveControl.tick`, which runs the
/// shared body only when nothing is swallowed. Without this a loaded cube would
/// still hop away under its own power.
#[test]
fn a_loaded_sulfur_cube_does_not_steer_itself() {
    let cube = sulfur_cube();
    cube.set_size(2, true);
    cube.cube_state().lock().wanted_movement = Some(1.0);
    cube.set_rotation((0.0, 0.0));

    swallow(&cube, ItemStack::new(&vanilla_items::TNT));
    Mob::tick_move_control(&cube);
    assert_eq!(
        cube.cube_state().lock().wanted_movement,
        Some(1.0),
        "the move control consumed the request while a block was swallowed"
    );
}
