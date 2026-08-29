use std::io::Cursor;
use std::sync::Weak;

use foton_registry::init_vanilla_registry;
use simdnbt::borrow::read_compound as read_borrowed_compound;

use super::*;

fn shulker() -> ShulkerEntity {
    ShulkerEntity::new(
        &vanilla_entities::SHULKER,
        1,
        DVec3::ZERO,
        Weak::<World>::new(),
    )
}

/// Vanilla parity: `Shulker.setRawPeekAmount` adds the twenty-point armor
/// bonus on close and takes it off on open. It is the whole reason a closed
/// shulker is worth waiting out.
#[test]
fn closing_the_lid_is_what_gives_a_shulker_its_armor() {
    init_vanilla_registry();
    let mob = shulker();
    let armor = || {
        mob.attributes()
            .lock()
            .required_value(vanilla_attributes::ARMOR)
    };
    let bare = armor();

    mob.set_raw_peek_amount(0);
    assert!(mob.is_closed());
    assert!((armor() - (bare + COVERED_ARMOR_BONUS)).abs() < 1.0e-9);

    mob.set_raw_peek_amount(ATTACK_PEEK);
    assert!(!mob.is_closed());
    assert!((armor() - bare).abs() < 1.0e-9);
}

/// Vanilla parity: `Shulker.updatePeekAmount` moves the visible lid a
/// twentieth at a time rather than snapping it, and stops exactly on target.
#[test]
fn the_lid_swings_a_twentieth_of_the_way_each_tick() {
    init_vanilla_registry();
    let mob = shulker();
    mob.entity_data.lock().peek.set(10);

    assert!(mob.update_peek_amount());
    assert!((*mob.current_peek_amount.lock() - PEEK_PER_TICK).abs() < 1.0e-6);

    assert!(mob.update_peek_amount());
    assert!((*mob.current_peek_amount.lock() - 0.1).abs() < 1.0e-6);

    assert!(!mob.update_peek_amount());
}

/// Vanilla parity: `Shulker.getDefaultDimensions`. A floor-mounted shulker
/// grows as its lid opens; one on a wall keeps its cube.
#[test]
fn only_a_floor_mounted_shulker_grows_as_it_opens() {
    init_vanilla_registry();
    let mob = shulker();
    let closed_height = mob.dimensions_for_pose(EntityPose::Standing).height;
    *mob.current_peek_amount.lock() = 0.5;

    mob.set_attach_face(Direction::Down);
    let open_height = mob.dimensions_for_pose(EntityPose::Standing).height;
    assert!((open_height - closed_height * 1.5).abs() < 1.0e-5);

    mob.set_attach_face(Direction::North);
    let wall_height = mob.dimensions_for_pose(EntityPose::Standing).height;
    assert!((wall_height - closed_height).abs() < f32::EPSILON);
}

/// Vanilla parity: `Shulker.getProgressAabb`. A shut shulker is a plain unit
/// cube; a fully open one reaches a whole block further out of the face it is
/// stuck to, which is exactly the clearance `canStayAt` demands.
#[test]
fn a_fully_open_shulker_reaches_one_block_out_of_its_face() {
    let closed = progress_aabb(1.0, Direction::Up, 0.0, DVec3::ZERO);
    assert!((closed.min_y() - 0.0).abs() < 1.0e-9);
    assert!((closed.max_y() - 1.0).abs() < 1.0e-9);
    assert!((closed.width() - 1.0).abs() < 1.0e-9);

    let open = progress_aabb(1.0, Direction::Up, 1.0, DVec3::ZERO);
    assert!((open.min_y() - 0.0).abs() < 1.0e-9);
    assert!((open.max_y() - 2.0).abs() < 1.0e-9);
    assert!((open.width() - 1.0).abs() < 1.0e-9);

    // The lid opens along the face, so a wall-mounted shulker grows sideways
    // and keeps its height.
    let sideways = progress_aabb(1.0, Direction::North, 1.0, DVec3::ZERO);
    assert!((sideways.min_z() - -1.5).abs() < 1.0e-9);
    assert!((sideways.max_z() - 0.5).abs() < 1.0e-9);
    assert!((sideways.height() - 1.0).abs() < 1.0e-9);
}

/// Vanilla parity: `Shulker.getDeltaMovement` and `setDeltaMovement`. Nothing
/// may give a shulker velocity, or gravity would pull it off its wall.
#[test]
fn a_shulker_can_never_be_given_velocity() {
    init_vanilla_registry();
    let mob = shulker();

    mob.set_velocity(DVec3::new(1.0, 2.0, 3.0));

    assert_eq!(mob.velocity(), DVec3::ZERO);
}

/// Vanilla parity: `Shulker.getColor` treats sixteen as "no color", and the
/// attach face, peek and color all have to survive a reload.
#[test]
fn the_face_peek_and_color_survive_a_save_and_load_round_trip() {
    init_vanilla_registry();
    let mob = shulker();
    assert_eq!(mob.color(), None);

    mob.set_attach_face(Direction::West);
    mob.entity_data.lock().peek.set(30);
    mob.set_color(Some(DyeColor::Magenta));

    let mut nbt = NbtCompound::new();
    mob.save_additional(&mut nbt);
    let mut bytes = Vec::new();
    nbt.write(&mut bytes);
    let borrowed = read_borrowed_compound(&mut Cursor::new(&bytes))
        .unwrap_or_else(|error| panic!("shulker save data should reborrow: {error}"));

    let loaded = shulker();
    loaded.load_additional((&borrowed).into());

    assert_eq!(loaded.attach_face(), Direction::West);
    assert_eq!(loaded.raw_peek_amount(), 30);
    assert_eq!(loaded.color(), Some(DyeColor::Magenta));
}

/// The legacy direction ids are what vanilla writes into `AttachFace`, so a
/// wrong table would silently rotate every saved shulker.
#[test]
fn the_legacy_direction_ids_round_trip() {
    for direction in Direction::ALL {
        assert_eq!(
            direction_from_legacy_id(legacy_direction_id(direction)),
            Some(direction)
        );
    }
}

/// Vanilla parity: `ShulkerPeekGoal` sets no goal flags, so a peeking shulker
/// can still be looked at and targeted by the goals beside it.
#[test]
fn the_peek_goal_holds_no_control() {
    let goal = ShulkerPeekGoal::new();

    assert_eq!(goal.controls(), GoalControls::EMPTY);
}
