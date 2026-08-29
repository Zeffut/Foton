use std::sync::Weak;

use foton_registry::{init_vanilla_registry, vanilla_blocks, vanilla_entities};
use foton_utils::BlockStateId;

use super::*;
use crate::entity::next_entity_id;

struct CarrotPatch {
    soil: BlockStateId,
    crop: BlockStateId,
}

impl LevelReader for CarrotPatch {
    fn get_block_state(&self, pos: BlockPos) -> BlockStateId {
        if pos.y() == 64 { self.soil } else { self.crop }
    }

    fn raw_brightness(&self, _pos: BlockPos, _sky_darkening: u8) -> u8 {
        15
    }

    fn min_y(&self) -> i32 {
        -64
    }

    fn height(&self) -> i32 {
        384
    }
}

fn rabbit() -> RabbitEntity {
    init_vanilla_registry();
    RabbitEntity::new(
        &vanilla_entities::RABBIT,
        next_entity_id(),
        DVec3::new(8.5, 64.0, 8.5),
        Weak::new(),
    )
}

#[test]
fn the_killer_bunny_keeps_its_sparse_id_across_a_save() {
    // Vanilla numbers the variants 0..=5 and then jumps to 99 for the evil one,
    // so a plain index would quietly turn a killer bunny into a brown rabbit.
    assert_eq!(RabbitVariant::Evil.id(), 99);
    assert_eq!(RabbitVariant::by_id(99), RabbitVariant::Evil);
    assert_eq!(RabbitVariant::by_id(42), RabbitVariant::Brown);
    assert_eq!(RabbitVariant::by_id(5), RabbitVariant::Salt);
}

#[test]
fn becoming_a_killer_bunny_arms_the_rabbit_and_going_back_disarms_it() {
    let rabbit = rabbit();
    let plain_attack_damage = rabbit
        .attributes()
        .lock()
        .get_value(vanilla_attributes::ATTACK_DAMAGE);

    rabbit.set_variant(RabbitVariant::Evil);

    assert_eq!(rabbit.variant(), RabbitVariant::Evil);
    assert!(rabbit.custom_name().is_some());
    assert!(rabbit.attributes().lock().has_modifier(
        vanilla_attributes::ATTACK_DAMAGE,
        &EVIL_ATTACK_POWER_MODIFIER
    ));
    assert!(
        (rabbit
            .attributes()
            .lock()
            .required_value(vanilla_attributes::ARMOR)
            - EVIL_ARMOR_VALUE)
            .abs()
            < f64::EPSILON
    );

    rabbit.set_variant(RabbitVariant::Brown);

    assert!(!rabbit.attributes().lock().has_modifier(
        vanilla_attributes::ATTACK_DAMAGE,
        &EVIL_ATTACK_POWER_MODIFIER
    ));
    assert_eq!(
        rabbit
            .attributes()
            .lock()
            .get_value(vanilla_attributes::ATTACK_DAMAGE)
            .map(f64::to_bits),
        plain_attack_damage.map(f64::to_bits)
    );
}

#[test]
fn a_rabbit_in_water_hops_at_the_fixed_swim_speed_whatever_it_was_asked_for() {
    // Vanilla's `RabbitMoveControl.setWantedPosition` overwrites the speed with
    // 1.5 in water, and only a positive speed is remembered for the next hop.
    let rabbit = rabbit();

    rabbit.set_wanted_position(DVec3::new(8.5, 64.0, 12.5), 0.0);
    assert!(rabbit.state.lock().next_jump_speed.abs() < f64::EPSILON);

    rabbit.set_wanted_position(DVec3::new(8.5, 64.0, 12.5), FLEE_SPEED_MOD);
    assert!((rabbit.state.lock().next_jump_speed - FLEE_SPEED_MOD).abs() < f64::EPSILON);

    rabbit.set_wanted_position(DVec3::new(8.5, 64.0, 12.5), 0.0);
    assert!(
        (rabbit.state.lock().next_jump_speed - FLEE_SPEED_MOD).abs() < f64::EPSILON,
        "a zero speed must not overwrite the remembered hop speed"
    );
}

#[test]
fn a_fleeing_rabbit_gets_back_on_its_feet_faster_than_a_strolling_one() {
    let rabbit = rabbit();

    rabbit.set_wanted_position(DVec3::new(8.5, 64.0, 12.5), STROLL_SPEED_MOD);
    rabbit.set_landing_delay();
    assert_eq!(rabbit.state.lock().jump_delay_ticks, JUMP_DELAY_TICKS);

    rabbit.set_wanted_position(DVec3::new(8.5, 64.0, 12.5), FLEE_SPEED_MOD);
    rabbit.set_landing_delay();
    assert_eq!(rabbit.state.lock().jump_delay_ticks, PANIC_JUMP_DELAY_TICKS);
}

#[test]
fn only_a_fully_grown_carrot_on_farmland_is_worth_raiding() {
    init_vanilla_registry();
    let pos = BlockPos::new(8, 64, 8);
    let wants_to_raid = AtomicBool::new(true);
    let can_raid = AtomicBool::new(false);

    let unripe = CarrotPatch {
        soil: vanilla_blocks::FARMLAND.default_state(),
        crop: vanilla_blocks::CARROTS.default_state(),
    };
    assert!(!is_valid_carrot_target(
        &unripe,
        pos,
        &wants_to_raid,
        &can_raid
    ));
    assert!(!can_raid.load(Ordering::Relaxed));

    let ripe = CarrotPatch {
        soil: vanilla_blocks::FARMLAND.default_state(),
        crop: vanilla_blocks::CARROTS
            .default_state()
            .set_value(CARROT_AGE, CARROT_MAX_AGE),
    };
    assert!(is_valid_carrot_target(
        &ripe,
        pos,
        &wants_to_raid,
        &can_raid
    ));
    assert!(can_raid.load(Ordering::Relaxed));

    // Vanilla only latches one raid at a time: with `canRaid` already set the
    // rabbit walks to the carrot it found instead of retargeting every tick.
    assert!(!is_valid_carrot_target(
        &ripe,
        pos,
        &wants_to_raid,
        &can_raid
    ));

    can_raid.store(false, Ordering::Relaxed);
    let stone_below = CarrotPatch {
        soil: vanilla_blocks::STONE.default_state(),
        crop: vanilla_blocks::CARROTS
            .default_state()
            .set_value(CARROT_AGE, CARROT_MAX_AGE),
    };
    assert!(!is_valid_carrot_target(
        &stone_below,
        pos,
        &wants_to_raid,
        &can_raid
    ));
}

#[test]
fn a_rabbit_that_has_just_eaten_stops_wanting_carrots() {
    let rabbit = rabbit();

    assert!(rabbit.wants_more_food());

    rabbit.state.lock().more_carrot_ticks = MORE_CARROTS_DELAY;

    assert!(!rabbit.wants_more_food());
}
