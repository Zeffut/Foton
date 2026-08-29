//! The nautilus family, driven in a real world.
//!
//! Both mobs are brain-driven and both override `Entity::tick` for their dash
//! clock, so there are two separate ways for their AI to fall out of the tick.
//! `super::hostile_ai` covers the doors; these cover what happens once inside:
//! the cooldowns the core activity counts down, the charge the fight activity
//! is made of, the food tag that decides whether an untamed one can be tamed at
//! all, and the air a rider breathes.

use std::io::Cursor;

use foton_utils::types::UpdateFlags;
use simdnbt::borrow::read_compound;

use super::*;
use crate::entity::ai::brain::behavior::utils::remember;
use crate::entity::ai::brain::memory::memory_module_types;
use crate::entity::ai::brain::{Activity, Brain};
use crate::entity::entities::{NautilusEntity, PufferfishEntity, ZombieNautilusEntity};
use crate::entity::nautilus::AbstractNautilus;
use crate::entity::{AgeableMob, Animal, EntitySpawnReason, TamableAnimal, next_entity_id};
use crate::test_support::TestPlayerBuilder;

/// Where the nautilus swims, in blocks.
const HOME: BlockPos = BlockPos::new(8, 64, 8);
/// The same spot as a position; the pool is three blocks deep, so this floats.
const SPAWN: DVec3 = DVec3::new(8.5, 64.0, 8.5);

/// Vanilla parity: `AbstractNautilus.DASH_COOLDOWN_TICKS`.
const DASH_COOLDOWN_TICKS: i32 = 40;

/// A loaded chunk with a stone floor and a pool of water over it.
///
/// A nautilus out of water drowns, its brain's `RandomStroll.swim` finds
/// nowhere to go, and `findNearestValidAttackTarget` refuses outright, so every
/// test here needs real water rather than an air column.
fn nautilus_world(key: &'static str) -> Arc<World> {
    init_vanilla_registry();
    init_behaviors();
    let world = fresh_test_world(key);
    insert_ready_full_chunk(&world, ChunkPos::new(0, 0));

    for x in (HOME.x() - 3)..=(HOME.x() + 3) {
        for z in (HOME.z() - 3)..=(HOME.z() + 3) {
            assert!(world.set_block(
                BlockPos::new(x, HOME.y() - 1, z),
                vanilla_blocks::STONE.default_state(),
                UpdateFlags::UPDATE_NONE,
            ));
            for y in HOME.y()..=(HOME.y() + 2) {
                assert!(world.set_block(
                    BlockPos::new(x, y, z),
                    vanilla_blocks::WATER.default_state(),
                    UpdateFlags::UPDATE_NONE,
                ));
            }
        }
    }
    world
}

fn spawn_nautilus(world: &Arc<World>) -> Arc<NautilusEntity> {
    let nautilus = Arc::new(NautilusEntity::new(
        &vanilla_entities::NAUTILUS,
        next_entity_id(),
        SPAWN,
        Arc::downgrade(world),
    ));
    world
        .try_add_entity(Arc::clone(&nautilus) as SharedEntity)
        .expect("the test chunk is loaded, so the nautilus should attach");
    nautilus
}

fn spawn_zombie_nautilus(world: &Arc<World>) -> Arc<ZombieNautilusEntity> {
    let nautilus = Arc::new(ZombieNautilusEntity::new(
        &vanilla_entities::ZOMBIE_NAUTILUS,
        next_entity_id(),
        SPAWN,
        Arc::downgrade(world),
    ));
    world
        .try_add_entity(Arc::clone(&nautilus) as SharedEntity)
        .expect("the test chunk is loaded, so the zombie nautilus should attach");
    nautilus
}

fn run_ticks(entity: &impl Entity, ticks: i32) {
    for _ in 0..ticks {
        Entity::tick(entity);
    }
}

/// The whole brain is reachable from the mob's own tick.
///
/// `CHARGE_COOLDOWN_TICKS` is only counted down by the `CountDownCooldownTicks`
/// in the core activity, so a cooldown that moves proves the tick reached
/// `server_ai_step`, `custom_server_ai_step`, `Brain::tick`, the core activity
/// and the behavior inside it. Nothing else in the server writes that memory.
#[test]
fn a_nautilus_counts_its_charge_cooldown_down_from_its_own_tick() {
    let world = nautilus_world("nautilus_charge_cooldown");
    let nautilus = spawn_nautilus(&world);
    let brain = Mob::brain(nautilus.as_ref()).expect("a nautilus has a brain");
    brain.set_memory(memory_module_types::CHARGE_COOLDOWN_TICKS, 20);

    run_ticks(nautilus.as_ref(), 5);

    let remaining = brain
        .get_memory(memory_module_types::CHARGE_COOLDOWN_TICKS)
        .expect("five ticks should not have spent a twenty tick cooldown");
    assert!(
        remaining < 20,
        "the core activity's CountDownCooldownTicks never ran: the cooldown is still {remaining}"
    );
}

/// The same door, for the zombie nautilus, whose core activity is a different
/// list built by a different module.
#[test]
fn a_zombie_nautilus_counts_its_charge_cooldown_down_from_its_own_tick() {
    let world = nautilus_world("zombie_nautilus_charge_cooldown");
    let nautilus = spawn_zombie_nautilus(&world);
    let brain = Mob::brain(nautilus.as_ref()).expect("a zombie nautilus has a brain");
    brain.set_memory(memory_module_types::CHARGE_COOLDOWN_TICKS, 20);

    run_ticks(nautilus.as_ref(), 5);

    let remaining = brain
        .get_memory(memory_module_types::CHARGE_COOLDOWN_TICKS)
        .expect("five ticks should not have spent a twenty tick cooldown");
    assert!(
        remaining < 20,
        "the core activity's CountDownCooldownTicks never ran: the cooldown is still {remaining}"
    );
}

/// The fight activity is one behavior, and this is it.
///
/// A pufferfish is the whole of `#minecraft:nautilus_hostiles`, so it is what a
/// nautilus actually charges. Set as the attack target on top of the nautilus,
/// it is inside the bounding box `ChargeAttack.tick` sweeps, so the charge
/// connects on the tick the fight activity turns on: the fish takes the
/// nautilus's attack damage, the attack target is dropped and the cooldown is
/// armed. All three are `ChargeAttack`'s and nothing else's.
#[test]
fn a_nautilus_charges_the_target_its_brain_picked() {
    let world = nautilus_world("nautilus_charge_attack");
    let nautilus = spawn_nautilus(&world);
    let fish = Arc::new(PufferfishEntity::new(
        &vanilla_entities::PUFFERFISH,
        next_entity_id(),
        SPAWN,
        Arc::downgrade(&world),
    ));
    world
        .try_add_entity(Arc::clone(&fish) as SharedEntity)
        .expect("the test chunk is loaded, so the pufferfish should attach");

    let full_health = fish.get_health();
    let brain = Mob::brain(nautilus.as_ref()).expect("a nautilus has a brain");
    brain.set_memory(
        memory_module_types::ATTACK_TARGET,
        remember(&(Arc::clone(&fish) as SharedEntity)),
    );

    // The activity is switched after the brain ticks, so the first tick is what
    // turns FIGHT on and the second is what runs the charge in it.
    run_ticks(nautilus.as_ref(), 2);

    assert!(
        fish.get_health() < full_health,
        "the charge never landed: the pufferfish is still at {full_health} health"
    );
    assert_eq!(
        brain.get_memory(memory_module_types::CHARGE_COOLDOWN_TICKS),
        Some(80),
        "a landed charge arms NautilusAi.TIME_BETWEEN_ATTACKS"
    );
    assert!(
        !brain.has_memory_value(memory_module_types::ATTACK_TARGET.id()),
        "ChargeAttack.stop erases the attack target once the charge connects"
    );
}

/// A spawning nautilus gets its long fight cooldown, and still goes through the
/// shared ageable spawn.
///
/// Vanilla parity: `AbstractNautilus.finalizeSpawn`, which is `initMemories`
/// plus `super.finalizeSpawn` -- and that `super` is the one-in-five roll every
/// animal's calves come from. Returning the caller's group data instead would
/// have quietly taken baby nautiluses out of the game.
#[test]
fn a_spawning_nautilus_seeds_its_fight_cooldown_and_joins_its_cluster() {
    let world = nautilus_world("nautilus_finalize_spawn");
    let nautilus = spawn_nautilus(&world);
    let brain = Mob::brain(nautilus.as_ref()).expect("a nautilus has a brain");
    assert!(
        !brain.has_memory_value(memory_module_types::ATTACK_TARGET_COOLDOWN.id()),
        "nothing has seeded the cooldown yet"
    );

    let group_data =
        Mob::finalize_spawn(nautilus.as_ref(), &world, EntitySpawnReason::Natural, None);

    let cooldown = brain
        .get_memory(memory_module_types::ATTACK_TARGET_COOLDOWN)
        .expect("NautilusAi.initMemories seeds TIME_BETWEEN_NON_PLAYER_ATTACKS");
    assert!(
        (2400..=3600).contains(&cooldown),
        "the cooldown should be vanilla's UniformInt.of(2400, 3600), got {cooldown}"
    );
    assert!(
        group_data.is_some(),
        "AgeableMob.finalizeSpawn owns the cluster data, so skipping it also \
         skips the roll that makes a calf"
    );
}

/// An untamed adult nautilus is picky, and a tame one is not.
///
/// Vanilla parity: `AbstractNautilus.isFood`, which reads
/// `#minecraft:nautilus_taming_items` -- pufferfish only -- until the nautilus
/// is tamed, and `#minecraft:nautilus_food` afterwards. Getting the two the
/// wrong way round would make every fish a taming item.
#[test]
fn only_a_tame_nautilus_eats_anything_but_its_taming_items() {
    init_vanilla_registry();
    let nautilus = NautilusEntity::new(
        &vanilla_entities::NAUTILUS,
        next_entity_id(),
        DVec3::ZERO,
        Weak::new(),
    );
    let cod = ItemStack::new(&vanilla_items::COD);
    let pufferfish = ItemStack::new(&vanilla_items::PUFFERFISH);

    assert!(
        Animal::is_food(&nautilus, &pufferfish),
        "a pufferfish is in #minecraft:nautilus_taming_items"
    );
    assert!(
        !Animal::is_food(&nautilus, &cod),
        "an untamed adult nautilus reads the taming tag, which holds no cod"
    );

    nautilus.set_tame(true, false);

    assert!(
        Animal::is_food(&nautilus, &cod),
        "a tame nautilus reads #minecraft:nautilus_food, which holds every fish"
    );
}

/// Feeding an untamed nautilus spends one fish and comes back.
///
/// `AbstractNautilus.usePlayerItem` is what `Mob::use_player_item` is overridden
/// with, so the branch that is meant to be `Mob.usePlayerItem` cannot call it:
/// Rust has no `super`, and the call comes straight back. It did, and a
/// right-click with a pufferfish overflowed the stack and killed the server --
/// found by a real client, not by any of the tests above it.
#[test]
fn feeding_an_untamed_nautilus_spends_one_fish() {
    let world = nautilus_world("nautilus_feed");
    let nautilus = spawn_nautilus(&world);
    let player = TestPlayerBuilder::new(Arc::clone(&world), "Feeder", next_entity_id()).build();
    player.set_item_in_hand(
        InteractionHand::MainHand,
        ItemStack::with_count(&vanilla_items::PUFFERFISH, 4),
    );

    let result = Mob::mob_interact(nautilus.as_ref(), &player, InteractionHand::MainHand);

    assert!(
        result.consumes_action(),
        "an untamed nautilus takes its taming item"
    );
    assert_eq!(
        player.get_item_in_hand(InteractionHand::MainHand).count(),
        3,
        "feeding spends exactly one fish"
    );
}

/// A bucket of fish leaves the bucket behind.
///
/// Vanilla parity: the `ItemUtils.createFilledResult` branch of
/// `AbstractNautilus.usePlayerItem`, which is the only reason that override
/// exists at all.
#[test]
fn feeding_a_nautilus_a_bucket_of_fish_hands_the_bucket_back() {
    let world = nautilus_world("nautilus_feed_bucket");
    let nautilus = spawn_nautilus(&world);
    let player = TestPlayerBuilder::new(Arc::clone(&world), "Bucketeer", next_entity_id()).build();
    player.set_item_in_hand(
        InteractionHand::MainHand,
        ItemStack::new(&vanilla_items::PUFFERFISH_BUCKET),
    );

    let result = Mob::mob_interact(nautilus.as_ref(), &player, InteractionHand::MainHand);

    assert!(
        result.consumes_action(),
        "a bucket of pufferfish is in #minecraft:nautilus_taming_items"
    );
    assert!(
        player
            .get_item_in_hand(InteractionHand::MainHand)
            .is(&vanilla_items::WATER_BUCKET),
        "the emptied bucket comes back as a water bucket"
    );
}

/// A rider's charged jump arms the dash on the server.
///
/// The impulse itself is the controlling client's, but the flag, the cooldown
/// and the dash sound are the server's, and vanilla routes the cooldown through
/// `onSyncedDataUpdated` -- which Foton has no hook for, so
/// [`AbstractNautilus::set_dashing`] carries it instead. Without that, a rider
/// could dash every tick.
#[test]
fn a_riders_jump_arms_the_dash_cooldown() {
    init_vanilla_registry();
    let nautilus = NautilusEntity::new(
        &vanilla_entities::NAUTILUS,
        next_entity_id(),
        DVec3::ZERO,
        Weak::new(),
    );

    assert_eq!(nautilus.abstract_nautilus_base().dash_cooldown(), 0);
    Entity::handle_start_jump(&nautilus, 90);

    assert!(
        nautilus.is_dashing(),
        "handleStartJump raises the DASH flag"
    );
    assert_eq!(
        nautilus.abstract_nautilus_base().dash_cooldown(),
        DASH_COOLDOWN_TICKS,
        "the flag going up with the clock at zero is what starts the cooldown"
    );
}

/// The dash flag comes back down five ticks in, and the ready sound waits for
/// the whole cooldown.
///
/// Vanilla parity: `AbstractNautilus.tick`, whose `dashCooldown < 35` is the
/// forty-tick cooldown minus the five-tick minimum dash.
#[test]
fn the_dash_flag_clears_before_the_cooldown_does() {
    let world = nautilus_world("nautilus_dash_flag");
    let nautilus = spawn_nautilus(&world);
    Entity::handle_start_jump(nautilus.as_ref(), 90);

    run_ticks(nautilus.as_ref(), 5);
    assert!(
        nautilus.is_dashing(),
        "five ticks in, the dash is still at its minimum duration"
    );

    run_ticks(nautilus.as_ref(), 2);
    assert!(
        !nautilus.is_dashing(),
        "past the minimum duration the flag comes down"
    );
    assert!(
        nautilus.abstract_nautilus_base().dash_cooldown() > 0,
        "clearing the flag must not re-arm the cooldown it is counting down"
    );
}

/// A tame nautilus keeps a home to swim back to, and a saddle shrinks it.
///
/// Vanilla parity: `AbstractNautilus.checkRestriction`, which runs from
/// `customServerAiStep` -- so this is also a second witness that the step is
/// reached at all.
#[test]
fn a_tame_nautilus_takes_a_home_and_a_saddle_shrinks_it() {
    let world = nautilus_world("nautilus_restriction");
    let nautilus = spawn_nautilus(&world);
    nautilus.set_tame(true, false);

    run_ticks(nautilus.as_ref(), 1);

    assert!(nautilus.has_home(), "a tame nautilus restricts itself");
    assert_eq!(
        nautilus.home_radius(),
        32,
        "an adult with no saddle keeps LARGE_RESTRICTION_RADIUS"
    );

    nautilus.set_item_slot(
        EquipmentSlot::Saddle,
        ItemStack::new(&vanilla_items::SADDLE),
    );
    run_ticks(nautilus.as_ref(), 1);

    assert_eq!(
        nautilus.home_radius(),
        16,
        "a saddled nautilus keeps SMALL_RESTRICTION_RADIUS"
    );
}

/// A rider breathes.
///
/// Vanilla parity: `AbstractNautilus.applyEffects`, which is the whole reason
/// to ride one under water.
#[test]
fn a_nautilus_keeps_its_rider_breathing() {
    let world = nautilus_world("nautilus_rider_breath");
    let nautilus = spawn_nautilus(&world);
    let player = TestPlayerBuilder::new(Arc::clone(&world), "Diver", next_entity_id()).build();
    let rider: SharedEntity = Arc::clone(&player) as SharedEntity;
    let vehicle: SharedEntity = Arc::clone(&nautilus) as SharedEntity;
    assert!(
        start_riding_entities(&rider, &vehicle),
        "the player should be able to mount the nautilus"
    );

    run_ticks(nautilus.as_ref(), 1);

    let effect = player
        .mob_effect(vanilla_mob_effects::BREATH_OF_THE_NAUTILUS)
        .expect("a rider gets breath of the nautilus on the first tick");
    assert_eq!(effect.duration(), 60, "vanilla's EFFECT_DURATION");
}

/// A nautilus shrugs poison off.
///
/// Vanilla parity: `AbstractNautilus.canBeAffected`, the one effect it refuses.
#[test]
fn a_nautilus_cannot_be_poisoned() {
    init_vanilla_registry();
    let nautilus = NautilusEntity::new(
        &vanilla_entities::NAUTILUS,
        next_entity_id(),
        DVec3::ZERO,
        Weak::new(),
    );

    nautilus.add_mob_effect(MobEffectInstance::with_duration(
        vanilla_mob_effects::POISON,
        200,
        0,
    ));
    nautilus.add_mob_effect(MobEffectInstance::with_duration(
        vanilla_mob_effects::REGENERATION,
        200,
        0,
    ));

    assert!(
        nautilus.mob_effect(vanilla_mob_effects::POISON).is_none(),
        "poison never lands on a nautilus"
    );
    assert!(
        nautilus
            .mob_effect(vanilla_mob_effects::REGENERATION)
            .is_some(),
        "every other effect still does"
    );
}

/// A brain with no sensors and no behaviors is a brain that was never built.
#[test]
fn both_nautilus_brains_are_wired() {
    init_vanilla_registry();
    for brain in [
        Mob::brain(&NautilusEntity::new(
            &vanilla_entities::NAUTILUS,
            next_entity_id(),
            DVec3::ZERO,
            Weak::new(),
        ))
        .map(Brain::is_brain_dead),
        Mob::brain(&ZombieNautilusEntity::new(
            &vanilla_entities::ZOMBIE_NAUTILUS,
            next_entity_id(),
            DVec3::ZERO,
            Weak::new(),
        ))
        .map(Brain::is_brain_dead),
    ] {
        assert_eq!(brain, Some(false), "a nautilus brain has work to do");
    }
}

/// The idle activity is what a nautilus falls back to with nothing to fight.
#[test]
fn a_nautilus_with_nothing_to_fight_goes_idle() {
    let world = nautilus_world("nautilus_idle");
    let nautilus = spawn_nautilus(&world);

    run_ticks(nautilus.as_ref(), 2);

    let brain = Mob::brain(nautilus.as_ref()).expect("a nautilus has a brain");
    assert!(
        brain.is_active(Activity::Idle),
        "NautilusAi.updateActivity falls through FIGHT to IDLE"
    );
}

/// A zombie nautilus wears a variant, and it survives a save.
#[test]
fn a_zombie_nautilus_round_trips_its_variant() {
    init_vanilla_registry();
    let nautilus = ZombieNautilusEntity::new(
        &vanilla_entities::ZOMBIE_NAUTILUS,
        next_entity_id(),
        DVec3::ZERO,
        Weak::new(),
    );
    let Some(warm) = REGISTRY
        .zombie_nautilus_variants
        .iter()
        .map(|(_, variant)| variant)
        .find(|variant| variant.key.path != "temperate")
    else {
        panic!("the extractor should have produced more than one coral variant");
    };
    nautilus.set_variant(warm);

    let mut nbt = NbtCompound::new();
    nautilus.save_additional(&mut nbt);

    let reloaded = ZombieNautilusEntity::new(
        &vanilla_entities::ZOMBIE_NAUTILUS,
        next_entity_id(),
        DVec3::ZERO,
        Weak::new(),
    );
    let mut bytes = Vec::new();
    nbt.write(&mut bytes);
    let borrowed =
        read_compound(&mut Cursor::new(&bytes)).expect("the saved nautilus should reborrow");
    reloaded.load_additional((&borrowed).into());

    assert_eq!(
        reloaded.variant().key,
        warm.key,
        "the coral variant is saved under `variant` and read back"
    );
}

/// A zombie nautilus is born grown and stays that way.
///
/// Vanilla parity: `ZombieNautilus.canBeABaby`, which is what keeps `setBaby`
/// from producing a calf that no model exists for.
#[test]
fn a_zombie_nautilus_is_never_a_baby() {
    init_vanilla_registry();
    let nautilus = ZombieNautilusEntity::new(
        &vanilla_entities::ZOMBIE_NAUTILUS,
        next_entity_id(),
        DVec3::ZERO,
        Weak::new(),
    );

    nautilus.set_baby(true);

    assert!(
        !AgeableMob::is_baby(&nautilus),
        "a zombie nautilus has no calf form"
    );
}
