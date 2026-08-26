//! Breeze tests.

use std::sync::Arc;

use glam::DVec3;
use steel_registry::{
    init_vanilla_registry, vanilla_blocks, vanilla_damage_types, vanilla_entities,
};
use steel_utils::types::UpdateFlags;
use steel_utils::{BlockPos, ChunkPos};

use super::*;
use crate::behavior::init_behaviors;
use crate::entity::ai::brain::Activity;
use crate::entity::ai::brain::memory::{EntityMemory, Unit, memory_module_types};
use crate::entity::entities::{
    ArrowEntity, BreezeWindChargeEntity, IronGolemEntity, PigEntity, WindChargeEntity,
};
use crate::entity::next_entity_id;
use crate::test_support::{fresh_test_world, insert_ready_full_chunk};

/// The one spot the test world is guaranteed to be solid ground rather than a
/// column the fluid scan walks.
const SPAWN: DVec3 = DVec3::new(8.5, 64.0, 8.5);

fn breeze_world(key: &'static str) -> Arc<World> {
    init_vanilla_registry();
    init_behaviors();
    let world = fresh_test_world(key);
    insert_ready_full_chunk(&world, ChunkPos::new(0, 0));
    for x in 5..=12 {
        for z in 5..=12 {
            assert!(world.set_block(
                BlockPos::new(x, 63, z),
                vanilla_blocks::STONE.default_state(),
                UpdateFlags::UPDATE_NONE,
            ));
        }
    }
    world
}

fn spawn_breeze(world: &Arc<World>, position: DVec3) -> Arc<BreezeEntity> {
    let breeze = Arc::new(BreezeEntity::new(
        &vanilla_entities::BREEZE,
        next_entity_id(),
        position,
        Arc::downgrade(world),
    ));
    world
        .try_add_entity(Arc::clone(&breeze) as SharedEntity)
        .expect("the test chunk is loaded, so the breeze should attach");
    breeze
}

/// Vanilla's `Breeze.canAttack` is the whole reason a trial chamber full of
/// mobs does not turn into a brawl: a breeze takes a swing at a player or an
/// iron golem and ignores everything else alive.
#[test]
fn a_breeze_picks_a_fight_only_with_players_and_iron_golems() {
    let world = breeze_world("breeze_can_attack");
    let breeze = spawn_breeze(&world, SPAWN);

    let golem = IronGolemEntity::new(
        &vanilla_entities::IRON_GOLEM,
        next_entity_id(),
        DVec3::new(10.5, 64.0, 8.5),
        Arc::downgrade(&world),
    );
    let pig = PigEntity::new(
        &vanilla_entities::PIG,
        next_entity_id(),
        DVec3::new(10.5, 64.0, 8.5),
        Arc::downgrade(&world),
    );

    assert!(Mob::can_attack(breeze.as_ref(), &golem));
    assert!(
        !Mob::can_attack(breeze.as_ref(), &pig),
        "a breeze that attacks a pig would clear out its own trial chamber"
    );
}

/// Vanilla `Breeze.isInvulnerableTo`. Two breezes in one trial chamber stand in
/// each other's gusts constantly, and without this they would grind each other
/// down.
#[test]
fn a_breeze_shrugs_off_another_breezes_blow_but_not_a_pigs() {
    let world = breeze_world("breeze_invulnerable_to_breeze");
    let breeze = spawn_breeze(&world, SPAWN);
    let other = spawn_breeze(&world, DVec3::new(10.5, 64.0, 8.5));

    let pig: SharedEntity = Arc::new(PigEntity::new(
        &vanilla_entities::PIG,
        next_entity_id(),
        DVec3::new(6.5, 64.0, 8.5),
        Arc::downgrade(&world),
    ));
    world
        .try_add_entity(Arc::clone(&pig))
        .expect("pig should attach to the loaded test chunk");

    let from_breeze = DamageSource::environment(&vanilla_damage_types::MOB_ATTACK)
        .with_causing_entity(other.id());
    let from_pig =
        DamageSource::environment(&vanilla_damage_types::MOB_ATTACK).with_causing_entity(pig.id());

    assert!(breeze.is_invulnerable_to(&world, &from_breeze));
    assert!(
        !breeze.is_invulnerable_to(&world, &from_pig),
        "a breeze is only immune to another breeze, not to everything"
    );
}

/// Vanilla `Breeze.deflection`. A breeze is the one entity in
/// `#deflects_projectiles`, so an arrow bounces off it -- but its own
/// ammunition has to pass through, or a breeze standing in its own gust would
/// bat its charges out of the air.
#[test]
fn a_breeze_turns_an_arrow_back_and_lets_a_wind_charge_through() {
    let world = breeze_world("breeze_deflection");
    let breeze = spawn_breeze(&world, SPAWN);

    let arrow = ArrowEntity::new(
        &vanilla_entities::ARROW,
        next_entity_id(),
        SPAWN,
        Arc::downgrade(&world),
    );
    let charge = WindChargeEntity::new(
        &vanilla_entities::WIND_CHARGE,
        next_entity_id(),
        SPAWN,
        Arc::downgrade(&world),
    );
    let breeze_charge = BreezeWindChargeEntity::new(
        &vanilla_entities::BREEZE_WIND_CHARGE,
        next_entity_id(),
        SPAWN,
        Arc::downgrade(&world),
    );

    assert_eq!(
        breeze.deflection(&arrow),
        ProjectileDeflection::Reverse,
        "the breeze is the sole member of #deflects_projectiles"
    );
    assert_eq!(breeze.deflection(&charge), ProjectileDeflection::None);
    assert_eq!(
        breeze.deflection(&breeze_charge),
        ProjectileDeflection::None
    );
}

/// Vanilla `Breeze.getFiringYPosition`: a charge leaves from above the middle
/// of the hitbox, not from the breeze's feet, which is what lets it clear the
/// lip of a trial-chamber platform.
#[test]
fn a_wind_charge_leaves_above_the_middle_of_the_breeze() {
    init_vanilla_registry();
    let breeze = BreezeEntity::new(
        &vanilla_entities::BREEZE,
        next_entity_id(),
        SPAWN,
        Weak::<World>::new(),
    );

    let height = breeze.bounding_box().height();
    let expected = SPAWN.y + height * 0.5 + 0.3;

    assert!((breeze.firing_y_position() - expected).abs() < 1.0e-9);
    assert!(
        breeze.firing_y_position() > SPAWN.y + height * 0.5,
        "the charge leaves above the middle of the hitbox"
    );
}

/// The whole fight in one test: a breeze handed a target and a reason to shoot
/// runs its brain, inhales for fifteen ticks, and puts a wind charge in the
/// world. Nothing else in the tree exercises `Shoot`, and a breeze that never
/// fires is a breeze that does nothing at all.
#[test]
fn a_breeze_fires_a_wind_charge_at_what_it_is_fighting() {
    let world = breeze_world("breeze_shoots");
    let breeze = spawn_breeze(&world, SPAWN);

    let golem: SharedEntity = Arc::new(IronGolemEntity::new(
        &vanilla_entities::IRON_GOLEM,
        next_entity_id(),
        DVec3::new(11.5, 64.0, 8.5),
        Arc::downgrade(&world),
    ));
    world
        .try_add_entity(Arc::clone(&golem))
        .expect("golem should attach to the loaded test chunk");

    let brain = Mob::brain(breeze.as_ref()).expect("a breeze has a brain");
    brain.set_memory(
        memory_module_types::ATTACK_TARGET,
        EntityMemory::new(&golem),
    );
    brain.set_memory(memory_module_types::BREEZE_SHOOT, Unit);

    let mut fired = false;
    for _ in 0..40 {
        breeze.base_tick();
        breeze.tick();
        if charges_in_world(&world) > 0 {
            fired = true;
            break;
        }
    }

    assert!(
        brain.is_active(Activity::Fight),
        "a breeze with a target and no walk target is fighting"
    );
    assert!(
        fired,
        "the breeze inhaled but never let a wind charge go: running behaviors were {:?}",
        brain.running_behaviors()
    );
}

/// The inhale is not decoration: vanilla holds the charge for fifteen ticks
/// before it leaves, and a breeze that fired on the first tick would be a
/// different mob to fight.
#[test]
fn a_breeze_holds_its_charge_through_the_inhale() {
    let world = breeze_world("breeze_inhale");
    let breeze = spawn_breeze(&world, SPAWN);

    let golem: SharedEntity = Arc::new(IronGolemEntity::new(
        &vanilla_entities::IRON_GOLEM,
        next_entity_id(),
        DVec3::new(11.5, 64.0, 8.5),
        Arc::downgrade(&world),
    ));
    world
        .try_add_entity(Arc::clone(&golem))
        .expect("golem should attach to the loaded test chunk");

    let brain = Mob::brain(breeze.as_ref()).expect("a breeze has a brain");
    brain.set_memory(
        memory_module_types::ATTACK_TARGET,
        EntityMemory::new(&golem),
    );
    brain.set_memory(memory_module_types::BREEZE_SHOOT, Unit);

    for _ in 0..10 {
        breeze.base_tick();
        breeze.tick();
    }

    assert_eq!(
        charges_in_world(&world),
        0,
        "the charge left before the inhale was over"
    );
    assert!(
        brain.has_memory_value(memory_module_types::BREEZE_SHOOT_CHARGING.id()),
        "the breeze should still be charging ten ticks in"
    );
}

fn charges_in_world(world: &Arc<World>) -> usize {
    let area = steel_utils::WorldAabb::new(-64.0, 0.0, -64.0, 64.0, 128.0, 64.0);
    world
        .get_entities_in_aabb_matching(&area, |entity| {
            entity.entity_type() == &vanilla_entities::BREEZE_WIND_CHARGE
        })
        .len()
}
