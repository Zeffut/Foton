//! The dragon's hitboxes, the block of IDs they need, and where a hit goes.

use std::sync::Weak;

use steel_registry::{init_vanilla_registry, vanilla_damage_types};
use steel_utils::ChunkPos;

use super::*;
use crate::behavior::init_behaviors;
use crate::entity::{SharedEntity, init_entities, next_entity_id};
use crate::test_support::{fresh_test_world, insert_ready_full_chunk};

/// The only spawn coordinate the test world is happy with.
const SPAWN: DVec3 = DVec3::new(8.5, 64.0, 8.5);

fn detached_dragon() -> EnderDragon {
    init_vanilla_registry();
    EnderDragon::new(
        &vanilla_entities::ENDER_DRAGON,
        next_entity_id(),
        SPAWN,
        Weak::<World>::new(),
    )
}

fn prepared_world(key: &'static str) -> Arc<World> {
    init_vanilla_registry();
    init_behaviors();
    init_entities();
    let world = fresh_test_world(key);
    insert_ready_full_chunk(&world, ChunkPos::new(0, 0));
    world
}

fn world_dragon(world: &Arc<World>) -> Arc<EnderDragon> {
    Arc::new(EnderDragon::new(
        &vanilla_entities::ENDER_DRAGON,
        next_entity_id(),
        SPAWN,
        Arc::downgrade(world),
    ))
}

fn accepted_hit() -> DamageSource {
    // A hit the dragon accepts. `EnderDragon.hurt` swallows anything that is
    // neither dealt by a player nor tagged ALWAYS_HURTS_ENDER_DRAGONS, and that
    // tag is `#is_explosion` -- which is how a destroyed crystal hurts it.
    DamageSource::environment(&vanilla_damage_types::EXPLOSION)
}

/// A hit from something that is neither a player nor an explosion.
fn swallowed_hit() -> DamageSource {
    DamageSource::environment(&vanilla_damage_types::CACTUS)
}

/// The client rebuilds the dragon's eight hitboxes from arithmetic on the
/// dragon's own ID and nothing else, so this block is a protocol requirement
/// rather than a convenience.
#[test]
fn a_dragons_hitboxes_take_the_eight_ids_immediately_after_its_own() {
    let dragon = detached_dragon();
    let id = dragon.id();

    for part in DragonPartIndex::ORDER {
        assert_eq!(
            dragon.part(part).id(),
            id + 1 + part.slot() as i32,
            "hitbox {part:?} is not at the offset the client derives"
        );
    }
}

/// Two dragons built back to back must not overlap, which is the whole reason
/// the ID block is reserved in one go rather than drawn nine times.
#[test]
fn two_dragons_built_in_a_row_never_share_a_hitbox_id() {
    let first = detached_dragon();
    let second = detached_dragon();

    let last_of_first = first.part(DragonPartIndex::Wing2).id();
    assert!(
        second.id() > last_of_first,
        "second dragon at {} overlaps the first dragon's hitboxes ending at {last_of_first}",
        second.id()
    );
}

#[test]
fn every_hitbox_points_back_at_the_dragon_that_owns_it() {
    let dragon = detached_dragon();

    for part in dragon.sub_entities() {
        assert_eq!(part.parent_id(), dragon.id());
    }
}

/// The routing the parts exist for: a hit addressed to a hitbox has to reach
/// the dragon behind it. A part is not a living entity, so the trait default
/// would refuse the hit outright and the damage would evaporate.
#[test]
fn a_hit_on_a_hitbox_reaches_the_dragon_behind_it() {
    let world = prepared_world("dragon_part_hit_reaches_parent");
    let dragon = world_dragon(&world);
    world
        .try_add_entity(dragon.clone() as SharedEntity)
        .expect("dragon should spawn");

    let before = dragon.get_health();
    let landed = dragon.head().hurt(world.as_ref(), &accepted_hit(), 20.0);

    assert!(landed, "the hit on the head was refused");
    assert!(
        dragon.get_health() < before,
        "the dragon kept all {before} health after being hit on the head"
    );
}

/// Vanilla parity: `EnderDragon.hurt` cuts everything that is not the head to
/// `damage / 4 + min(damage, 1)`. Hitting the dragon in the face is the whole
/// tactic of the fight, so the split is the point of having parts at all.
#[test]
fn a_wing_takes_a_quarter_of_what_the_head_takes() {
    let world = prepared_world("dragon_part_damage_split");

    let head_loss = {
        let dragon = world_dragon(&world);
        world
            .try_add_entity(dragon.clone() as SharedEntity)
            .expect("dragon should spawn");
        let before = dragon.get_health();
        dragon.head().hurt(world.as_ref(), &accepted_hit(), 40.0);
        before - dragon.get_health()
    };

    let wing_loss = {
        let dragon = world_dragon(&world);
        world
            .try_add_entity(dragon.clone() as SharedEntity)
            .expect("dragon should spawn");
        let before = dragon.get_health();
        dragon
            .part(DragonPartIndex::Wing1)
            .hurt(world.as_ref(), &accepted_hit(), 40.0);
        before - dragon.get_health()
    };

    assert!(head_loss > 0.0, "the head hit did nothing");
    assert!(wing_loss > 0.0, "the wing hit did nothing");
    assert!(
        (wing_loss - (head_loss / 4.0 + 1.0)).abs() < 0.001,
        "a wing took {wing_loss} where a quarter of the head's {head_loss} plus one was expected"
    );
}

/// A hitbox ID is never a live entity ID, so the plain lookup every packet
/// handler used before this must miss it -- and the part lookup must not.
#[test]
fn a_hitbox_id_resolves_only_through_the_part_lookup() {
    let world = prepared_world("dragon_part_lookup");
    let dragon = world_dragon(&world);
    world
        .try_add_entity(dragon.clone() as SharedEntity)
        .expect("dragon should spawn");
    let head_id = dragon.head().id();

    assert!(
        world.get_accessible_entity_by_id(head_id).is_none(),
        "a hitbox should not be a live entity"
    );
    let found = world
        .get_accessible_entity_or_part_by_id(head_id)
        .expect("the part lookup should find the head");
    assert_eq!(found.id(), head_id);
}

/// Vanilla parity: `EnderDragon.isPickable` is false while every part's is
/// true. It is what makes the client aim at a hitbox rather than at the body.
#[test]
fn only_the_hitboxes_are_pickable_not_the_dragon() {
    let dragon = detached_dragon();

    assert!(!dragon.is_pickable());
    for part in dragon.sub_entities() {
        assert!(part.is_pickable(), "a hitbox was not pickable");
    }
}

/// Vanilla parity: the `Attributes.MAX_HEALTH, 200.0` of `createAttributes`,
/// and the constructor's `setHealth(getMaxHealth())`.
#[test]
fn a_dragon_arrives_at_two_hundred_health() {
    let dragon = detached_dragon();

    assert!((dragon.get_max_health() - MAX_HEALTH).abs() < 0.001);
    assert!((dragon.get_health() - MAX_HEALTH).abs() < 0.001);
}

/// Vanilla parity: `EnderDragon` never sets `xpReward`; it awards the orbs
/// itself in `tickDeath`. A mob experience reward here would double-pay.
#[test]
fn a_dragon_has_no_mob_experience_reward_of_its_own() {
    let dragon = detached_dragon();

    assert_eq!(dragon.xp_reward(), 0);
}

/// Vanilla parity: the synced `DATA_PHASE` default is `HOVERING`, and the
/// client switches its own phase on it.
#[test]
fn a_new_dragon_is_hovering_and_says_so_on_the_wire() {
    let dragon = detached_dragon();

    assert_eq!(
        dragon.phase_manager().current_phase(),
        EnderDragonPhase::Hovering
    );
    assert_eq!(
        *dragon.entity_data.lock().phase.get(),
        EnderDragonPhase::Hovering.id()
    );
}

/// Vanilla parity: the `source.getEntity() instanceof Player ||
/// source.is(ALWAYS_HURTS_ENDER_DRAGONS)` guard of `EnderDragon.hurt`, which is
/// why a dragon cannot be killed by fire or a falling anvil. It still reports
/// the hit as landed, so the caller does not go looking for another target.
#[test]
fn a_hit_that_is_neither_a_player_nor_an_explosion_is_swallowed_but_reported_as_landed() {
    let world = prepared_world("dragon_swallows_foreign_damage");
    let dragon = world_dragon(&world);
    world
        .try_add_entity(dragon.clone() as SharedEntity)
        .expect("dragon should spawn");

    let before = dragon.get_health();
    let landed = dragon.head().hurt(world.as_ref(), &swallowed_hit(), 40.0);

    assert!(landed, "the hit should still report as landed");
    assert!(
        (dragon.get_health() - before).abs() < 0.001,
        "a cactus took {} health off the dragon",
        before - dragon.get_health()
    );
}

/// Vanilla parity: `EnderDragon.hurtServer` routes a hit that arrives on the
/// dragon itself -- a command, a potion, a fall -- through the body hitbox, so
/// it takes the same quarter damage a body hit takes. Everything else in this
/// file comes in through a part; this is the other door, and `/damage` is what
/// walks through it.
#[test]
fn a_hit_addressed_to_the_dragon_itself_goes_through_the_body_hitbox() {
    let world = prepared_world("dragon_direct_hurt_routes_through_body");
    let dragon = world_dragon(&world);
    let entity: SharedEntity = dragon.clone();
    world
        .try_add_entity(entity.clone())
        .expect("dragon should spawn");

    let before = dragon.get_health();
    let landed = entity.hurt(world.as_ref(), &accepted_hit(), 40.0);
    let taken = before - dragon.get_health();

    assert!(landed, "the hit on the dragon was refused");
    assert!(
        (taken - (40.0 / 4.0 + 1.0)).abs() < 0.001,
        "the dragon took {taken} where a body hitbox's eleven was expected"
    );
}

/// Hovering counts as sitting, so a beating accumulates towards a takeoff.
/// Vanilla parity: the `sittingDamageReceived > 0.25F * getMaxHealth()` of
/// `EnderDragon.hurt`, which is what stops a landed dragon being a free kill.
#[test]
fn beating_a_sitting_dragon_past_a_quarter_of_its_health_makes_it_take_off() {
    let world = prepared_world("dragon_sitting_damage_takes_off");
    let dragon = world_dragon(&world);
    world
        .try_add_entity(dragon.clone() as SharedEntity)
        .expect("dragon should spawn");
    assert!(
        dragon.phase_manager().current_instance().is_sitting(),
        "a new dragon should be hovering, which counts as sitting"
    );

    // Each hit clears the damage cooldown first, the way twenty ticks apart
    // would in the world.
    for _ in 0..6 {
        dragon.living_base().set_invulnerable_time(0);
        dragon
            .part(DragonPartIndex::Body)
            .hurt(world.as_ref(), &accepted_hit(), 40.0);
    }

    assert_eq!(
        dragon.phase_manager().current_phase(),
        EnderDragonPhase::Takeoff
    );
}
