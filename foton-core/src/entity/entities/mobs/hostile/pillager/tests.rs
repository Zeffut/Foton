//! Pillager behavior worth pinning.

use std::sync::Weak;

use foton_registry::{init_vanilla_registry, vanilla_entities, vanilla_items};
use glam::DVec3;

use super::*;
use crate::entity::raider::{is_ominous_banner, ominous_banner};
use crate::entity::{entity_loot_ref, next_entity_id};
use foton_registry::loot_table::RaiderStatus;

const TEST_POSITION: DVec3 = DVec3::new(8.5, 64.0, 8.5);

fn pillager() -> PillagerEntity {
    init_vanilla_registry();
    PillagerEntity::new(
        &vanilla_entities::PILLAGER,
        next_entity_id(),
        TEST_POSITION,
        Weak::new(),
    )
}

/// The arm pose is the one thing a player reads off a pillager, and its four
/// branches are ordered: winding beats holding, holding beats swinging. A
/// reordering would leave a pillager visually reloading while it shoots.
#[test]
fn a_pillager_winding_its_crossbow_outranks_every_other_arm_pose() {
    let pillager = pillager();
    pillager.living_base().equipment().lock().set(
        EquipmentSlot::MainHand,
        ItemStack::new(&vanilla_items::CROSSBOW),
    );
    pillager.set_aggressive(true);

    assert_eq!(pillager.arm_pose(), IllagerArmPose::CrossbowHold);

    pillager.set_charging_crossbow(true);

    assert_eq!(pillager.arm_pose(), IllagerArmPose::CrossbowCharge);
}

/// A disarmed pillager drops to `NEUTRAL`, not to the folded arms every other
/// illager idles in.
#[test]
fn a_pillager_without_a_crossbow_stands_neutral_rather_than_crossed() {
    let pillager = pillager();

    assert_eq!(pillager.arm_pose(), IllagerArmPose::Neutral);
}

/// A captain is both halves at once: vanilla's `isCaptain` needs the banner
/// and the patrol leadership, and either alone is a different mob.
#[test]
fn a_captain_needs_both_the_banner_and_the_patrol_leadership() {
    let pillager = pillager();
    pillager.set_patrol_leader(true);

    assert!(!pillager.is_captain(), "leadership alone is not a captain");

    pillager
        .living_base()
        .equipment()
        .lock()
        .set(EquipmentSlot::Head, ominous_banner());

    assert!(pillager.is_captain());
}

/// The ominous banner is identified by its eight pattern layers, so a plain
/// white banner on a patrol leader's head must not read as the real thing.
#[test]
fn a_plain_white_banner_is_not_the_ominous_banner() {
    init_vanilla_registry();

    assert!(is_ominous_banner(&ominous_banner()));
    assert!(!is_ominous_banner(&ItemStack::new(
        &vanilla_items::WHITE_BANNER
    )));
}

/// A raider's idle clock runs at double speed, which is what makes it stop
/// being recruitable after two minutes rather than four.
#[test]
fn a_pillager_counts_its_idle_time_twice_per_tick() {
    let pillager = pillager();
    pillager.set_no_action_time(0);

    LivingEntity::server_ai_step(&pillager);

    assert_eq!(pillager.no_action_time(), 2);
}

/// The loot context has to see the captain, not just the mob: `entities/pillager`
/// hangs the ominous bottle off `minecraft:type_specific/raider`, and nothing
/// else on a pillager tells those two apart.
#[test]
fn a_captains_loot_reference_reports_the_raider_status() {
    let pillager = pillager();

    assert_eq!(
        entity_loot_ref(&pillager).raider,
        Some(RaiderStatus {
            has_raid: false,
            is_captain: false,
        })
    );

    pillager.set_patrol_leader(true);
    pillager
        .living_base()
        .equipment()
        .lock()
        .set(EquipmentSlot::Head, ominous_banner());

    assert_eq!(
        entity_loot_ref(&pillager).raider,
        Some(RaiderStatus {
            has_raid: false,
            is_captain: true,
        })
    );
}

/// Vanilla parity: `Pillager.pickUpItem`, which reaches for a banner and for
/// nothing else.
///
/// A pillager is born with `canPickUpLoot` set, so it is the one mob the shared
/// `Mob.pickUpItem` body would otherwise have turned into a scavenger. The
/// banner is the control: it proves the refusal below is the item test rather
/// than the pickup never running at all.
#[test]
fn a_pillager_reaches_for_a_banner_and_leaves_the_sword() {
    use crate::test_support::{fresh_test_world, insert_ready_full_chunk};
    use foton_utils::ChunkPos;
    use std::sync::Arc;

    init_vanilla_registry();
    let world = fresh_test_world("pillager_picks_up_banners_only");
    insert_ready_full_chunk(&world, ChunkPos::new(0, 0));
    let pillager = Arc::new(PillagerEntity::new(
        &vanilla_entities::PILLAGER,
        next_entity_id(),
        TEST_POSITION,
        Arc::downgrade(&world),
    ));
    world
        .try_add_entity(Arc::clone(&pillager) as SharedEntity)
        .expect("the test chunk is loaded, so the pillager should attach");

    let sword = world
        .spawn_item(TEST_POSITION, ItemStack::new(&vanilla_items::IRON_SWORD))
        .expect("the test chunk accepts an item entity");
    Mob::pick_up_item(pillager.as_ref(), &world, &(sword.clone() as SharedEntity));
    assert!(
        !sword.is_removed(),
        "a pillager has no use for a sword on the ground"
    );

    let banner = world
        .spawn_item(TEST_POSITION, ItemStack::new(&vanilla_items::WHITE_BANNER))
        .expect("the test chunk accepts an item entity");
    Mob::pick_up_item(pillager.as_ref(), &world, &(banner.clone() as SharedEntity));
    assert!(
        banner.is_removed(),
        "a banner is the one thing a pillager does reach for"
    );
}
