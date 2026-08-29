use std::io::Cursor;
use std::sync::Weak;

use foton_registry::{init_vanilla_registry, vanilla_entities};
use simdnbt::borrow::read_compound as read_borrowed_compound;

use super::*;

fn phantom() -> PhantomEntity {
    PhantomEntity::new(
        &vanilla_entities::PHANTOM,
        1,
        DVec3::ZERO,
        Weak::<World>::new(),
    )
}

/// Vanilla parity: `Phantom.updatePhantomSizeInfo` widens the hitbox and raises
/// the attack damage together, and `setPhantomSize` clamps to `0..=64`.
#[test]
fn the_size_drives_both_the_hitbox_and_the_attack_damage() {
    init_vanilla_registry();
    let mob = phantom();
    let base_width = mob.dimensions_for_pose(EntityPose::Standing).width;

    mob.set_phantom_size(4);

    assert_eq!(mob.phantom_size(), 4);
    let scaled = mob.dimensions_for_pose(EntityPose::Standing).width;
    assert!((scaled - base_width * (1.0 + SIZE_SCALE_STEP * 4.0)).abs() < 1.0e-5);
    let damage = mob
        .attributes()
        .lock()
        .required_value(vanilla_attributes::ATTACK_DAMAGE);
    assert!((damage - (BASE_ATTACK_DAMAGE + 4.0)).abs() < 1.0e-9);
}

/// Vanilla parity: the `Mth.clamp(size, 0, 64)` of `setPhantomSize`.
#[test]
fn the_size_is_clamped_to_the_vanilla_range() {
    init_vanilla_registry();
    let mob = phantom();

    mob.set_phantom_size(-5);
    assert_eq!(mob.phantom_size(), 0);

    mob.set_phantom_size(1000);
    assert_eq!(mob.phantom_size(), MAX_SIZE);
}

/// The anchor and the size both have to survive a reload, or a saved phantom
/// forgets where it was circling and shrinks back to the smallest size.
#[test]
fn the_anchor_and_size_survive_a_save_and_load_round_trip() {
    init_vanilla_registry();
    let mob = phantom();
    *mob.anchor_point.lock() = Some(BlockPos::new(12, 96, -30));
    mob.set_phantom_size(3);

    let mut nbt = NbtCompound::new();
    mob.save_additional(&mut nbt);
    let mut bytes = Vec::new();
    nbt.write(&mut bytes);
    let borrowed = read_borrowed_compound(&mut Cursor::new(&bytes))
        .unwrap_or_else(|error| panic!("phantom save data should reborrow: {error}"));

    let loaded = phantom();
    loaded.load_additional((&borrowed).into());

    assert_eq!(loaded.anchor_point(), Some(BlockPos::new(12, 96, -30)));
    assert_eq!(loaded.phantom_size(), 3);
}

/// Vanilla parity: `PhantomCircleAroundAnchorGoal.selectNext` anchors to the
/// phantom's own block when it has no anchor yet, and steps fifteen degrees
/// around the circle each time.
#[test]
fn the_circle_goal_anchors_to_the_phantom_and_walks_around_it() {
    init_vanilla_registry();
    let mob = phantom();
    let mut goal = PhantomCircleAroundAnchorGoal::new();
    goal.distance = 10.0;
    goal.height = 0.0;
    goal.clockwise = 1.0;

    goal.select_next(&mob);
    let first = mob.move_target_point();
    assert_eq!(mob.anchor_point(), Some(mob.block_position()));

    goal.select_next(&mob);
    let second = mob.move_target_point();

    assert!(first != second, "the circle has to advance");
    let anchor = DVec3::new(0.0, 0.0, 0.0);
    let radius = |point: DVec3| (point - anchor).with_y(0.0).length();
    assert!((radius(first) - radius(second)).abs() < 1.0e-4);
}

/// Vanilla parity: `Phantom.PhantomBodyRotationControl.clientTick` pins both
/// rotations to the yaw the move control set, instead of easing the body around
/// after the head the way every other mob does.
#[test]
fn the_body_rotation_control_pins_the_head_and_body_to_the_yaw() {
    init_vanilla_registry();
    let mob = phantom();
    mob.set_rotation((90.0, 0.0));
    mob.set_y_body_rot(30.0);
    mob.set_y_head_rot(0.0);

    Mob::tick_body_rotation_control(&mob);

    assert!((mob.y_head_rot() - 30.0).abs() < f32::EPSILON);
    assert!((mob.y_body_rot() - 90.0).abs() < f32::EPSILON);
}
