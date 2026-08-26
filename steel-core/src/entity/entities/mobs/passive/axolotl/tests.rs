use std::sync::Weak;

use steel_registry::vanilla_blocks;
use steel_registry::{init_vanilla_registry, vanilla_entities};
use steel_utils::ChunkPos;
use steel_utils::types::UpdateFlags;

use super::*;
use crate::behavior::init_behaviors;
use crate::entity::ai::brain::memory::EntityMemory;
use crate::entity::next_entity_id;
use crate::test_support::{TestPlayerBuilder, fresh_test_world, insert_ready_full_chunk};

/// The block the test axolotls stand on and swim in.
const TEST_POS: BlockPos = BlockPos::new(8, 64, 8);
const TEST_POSITION: DVec3 = DVec3::new(8.5, 64.0, 8.5);

fn new_axolotl() -> AxolotlEntity {
    init_vanilla_registry();
    AxolotlEntity::new(
        &vanilla_entities::AXOLOTL,
        next_entity_id(),
        TEST_POSITION,
        Weak::new(),
    )
}

/// Builds a world with a loaded chunk under the test position.
fn axolotl_world(key: &'static str) -> Arc<World> {
    init_vanilla_registry();
    init_behaviors();
    let world = fresh_test_world(key);
    insert_ready_full_chunk(&world, ChunkPos::from_block_pos(TEST_POS));
    world
}

/// Adds an axolotl to `world` at the test position.
fn live_axolotl(world: &Arc<World>) -> Arc<AxolotlEntity> {
    let axolotl = Arc::new(AxolotlEntity::new(
        &vanilla_entities::AXOLOTL,
        next_entity_id(),
        TEST_POSITION,
        Arc::downgrade(world),
    ));
    world
        .try_add_entity(Arc::clone(&axolotl) as SharedEntity)
        .unwrap_or_else(|error| panic!("axolotl should enter the test world: {error:?}"));
    axolotl
}

/// Fills the test column with water and makes the axolotl notice.
fn submerge(world: &Arc<World>, axolotl: &AxolotlEntity) {
    for offset in 0..3 {
        assert!(world.set_block(
            BlockPos::new(TEST_POS.x(), TEST_POS.y() + offset, TEST_POS.z()),
            vanilla_blocks::WATER.default_state(),
            UpdateFlags::UPDATE_NONE
        ));
    }
    axolotl.refresh_fluid_contact();
    assert!(
        axolotl.is_in_water(),
        "the test axolotl should be standing in water"
    );
}

#[test]
fn an_axolotl_saves_and_reloads_its_color_and_where_it_came_from() {
    use std::io::Cursor;

    use simdnbt::borrow::read_compound as read_borrowed_compound;

    let axolotl = new_axolotl();
    axolotl.set_variant(AxolotlVariant::Blue);
    axolotl.set_from_bucket(true);

    let mut nbt = NbtCompound::new();
    axolotl.save_additional(&mut nbt);
    let mut bytes = Vec::new();
    nbt.write(&mut bytes);
    let borrowed = read_borrowed_compound(&mut Cursor::new(&bytes))
        .unwrap_or_else(|error| panic!("axolotl nbt should reborrow: {error}"));

    let reloaded = new_axolotl();
    reloaded.load_additional((&borrowed).into());

    assert_eq!(reloaded.variant(), AxolotlVariant::Blue);
    assert!(reloaded.from_bucket());
}

#[test]
fn an_axolotl_hurt_on_dry_land_never_plays_dead() {
    // The water test is the one vanilla condition that cannot be rolled around:
    // an axolotl out of the water has nothing to float in, so however unlucky
    // the roll is it must never start the act. Two hundred attempts is far past
    // the roughly one-in-nine chance a submerged one would take.
    let world = axolotl_world("axolotl_play_dead_dry");
    let axolotl = live_axolotl(&world);
    assert!(!axolotl.is_in_water());

    for _ in 0..200 {
        axolotl.set_health(axolotl.get_max_health());
        axolotl.hurt_server(&world, &attacker_damage(), 1.0);
        assert!(
            !axolotl
                .brain
                .has_memory_value(memory_module_types::PLAY_DEAD_TICKS.id()),
            "an axolotl on dry land started playing dead"
        );
    }
}

#[test]
fn an_axolotl_hurt_in_water_plays_dead_and_stops_being_worth_attacking() {
    // The roll is `nextInt(3) == 0` and then `nextInt(3) < damage`, so a single
    // hit is about one in nine. Two hundred hits make a run that never triggers
    // vanishingly unlikely, and a build that has lost the memory write never
    // triggers at all.
    let world = axolotl_world("axolotl_play_dead_wet");
    let axolotl = live_axolotl(&world);
    submerge(&world, &axolotl);

    let mut started = false;
    for _ in 0..200 {
        axolotl.set_health(axolotl.get_max_health());
        axolotl.hurt_server(&world, &attacker_damage(), 1.0);
        if axolotl
            .brain
            .has_memory_value(memory_module_types::PLAY_DEAD_TICKS.id())
        {
            started = true;
            break;
        }
    }
    assert!(
        started,
        "an axolotl hurt in water never started playing dead"
    );
    assert_eq!(
        axolotl
            .brain
            .get_memory(memory_module_types::PLAY_DEAD_TICKS),
        Some(TOTAL_PLAYDEAD_TIME)
    );

    // The act is not cosmetic: while it lasts, nothing will pick this axolotl
    // as a target.
    assert!(LivingEntity::can_be_seen_as_enemy(axolotl.as_ref()));
    axolotl.set_playing_dead(true);
    assert!(!LivingEntity::can_be_seen_as_enemy(axolotl.as_ref()));
}

#[test]
fn an_axolotl_stops_playing_dead_when_its_clock_runs_out() {
    // `ValidatePlayDead` lives in the core activity precisely so it keeps
    // counting while `PLAY_DEAD` holds every other activity out. Without it the
    // memory never decrements and the axolotl floats forever.
    let world = axolotl_world("axolotl_play_dead_clock");
    let axolotl = live_axolotl(&world);
    axolotl
        .brain
        .set_memory(memory_module_types::PLAY_DEAD_TICKS, 2);

    axolotl.custom_server_ai_step();
    assert!(
        axolotl.is_playing_dead(),
        "the synced flag should follow the memory"
    );
    assert_eq!(
        axolotl
            .brain
            .get_memory(memory_module_types::PLAY_DEAD_TICKS),
        Some(1),
        "the clock should count down once per tick"
    );

    for _ in 0..2 {
        axolotl.custom_server_ai_step();
    }

    assert!(
        !axolotl
            .brain
            .has_memory_value(memory_module_types::PLAY_DEAD_TICKS.id()),
        "the clock should be forgotten once it runs out"
    );
    assert!(!axolotl.is_playing_dead());
}

#[test]
fn a_player_who_finishes_an_axolotls_kill_gets_stacking_regeneration() {
    // Vanilla stacks the buff in duration, a hundred ticks at a time, and stops
    // stacking at two minutes. A shoal of axolotls is therefore worth no more
    // than a couple of them, which is the whole point of the cap.
    let world = axolotl_world("axolotl_regen_buff");
    let axolotl = live_axolotl(&world);
    let player =
        TestPlayerBuilder::new(Arc::clone(&world), "AxolotlFriend", next_entity_id()).build();
    player.set_mob_effect(vanilla_mob_effects::MINING_FATIGUE, 0);

    AxolotlEntity::apply_supporting_effects(axolotl.as_ref(), &player);
    assert_eq!(
        player
            .mob_effect(vanilla_mob_effects::REGENERATION)
            .map(|effect| effect.duration()),
        Some(REGEN_BUFF_BASE_DURATION)
    );
    assert!(
        !player.has_mob_effect(vanilla_mob_effects::MINING_FATIGUE),
        "helping an axolotl clears mining fatigue"
    );

    AxolotlEntity::apply_supporting_effects(axolotl.as_ref(), &player);
    assert_eq!(
        player
            .mob_effect(vanilla_mob_effects::REGENERATION)
            .map(|effect| effect.duration()),
        Some(REGEN_BUFF_BASE_DURATION * 2),
        "a second axolotl adds to the duration rather than restarting it"
    );

    // Already at the cap, so nothing more is owed.
    player.add_mob_effect(MobEffectInstance::with_duration(
        vanilla_mob_effects::REGENERATION,
        REGEN_BUFF_MAX_DURATION,
        0,
    ));
    AxolotlEntity::apply_supporting_effects(axolotl.as_ref(), &player);
    assert_eq!(
        player
            .mob_effect(vanilla_mob_effects::REGENERATION)
            .map(|effect| effect.duration()),
        Some(REGEN_BUFF_MAX_DURATION),
        "the buff stops stacking at two minutes"
    );
}

#[test]
fn a_bucketed_axolotl_comes_back_out_the_way_it_went_in() {
    // The color rides in the item component -- which is what stops two buckets
    // of different axolotls stacking -- and the age, the age lock and the
    // hunting cooldown ride in the entity data.
    let world = axolotl_world("axolotl_bucket");
    let axolotl = live_axolotl(&world);
    axolotl.set_variant(AxolotlVariant::Gold);
    axolotl.set_age(-1200);
    axolotl.set_age_locked(true);
    axolotl.set_health(5.0);
    axolotl
        .brain
        .set_memory_with_expiry(memory_module_types::HAS_HUNTING_COOLDOWN, true, 1234);

    let mut bucket = axolotl.bucket_item_stack();
    axolotl.save_to_bucket_tag(&mut bucket);

    assert_eq!(
        bucket.get(vanilla_components::AXOLOTL_VARIANT).copied(),
        Some(AxolotlVariant::Gold),
        "the color belongs to the bucket, not to the entity data"
    );

    let restored = live_axolotl(&world);
    read_bucket_entity_data(&bucket, |tag| restored.load_from_bucket_tag(tag));

    assert_eq!(restored.get_age(), -1200);
    assert!(restored.is_age_locked());
    assert!((restored.get_health() - 5.0).abs() < f32::EPSILON);
    assert_eq!(
        restored
            .brain
            .time_until_expiry(memory_module_types::HAS_HUNTING_COOLDOWN),
        1234,
        "a bucketed axolotl serves out the rest of its hunting cooldown"
    );
}

#[test]
fn an_axolotl_that_stops_fighting_leaves_the_fish_alone_for_two_minutes() {
    // Without this an axolotl clears a reef: the cooldown is the only thing
    // between one fight and the next, and only the fish are subject to it --
    // a drowned is still attacked on sight.
    let world = axolotl_world("axolotl_hunting_cooldown");
    let axolotl = live_axolotl(&world);
    let prey = live_axolotl(&world);

    axolotl.brain.set_memory(
        memory_module_types::ATTACK_TARGET,
        EntityMemory::new(&(prey as SharedEntity)),
    );
    axolotl_ai::update_activity(&axolotl.brain);
    assert!(
        !axolotl
            .brain
            .has_memory_value(memory_module_types::HAS_HUNTING_COOLDOWN.id()),
        "a fighting axolotl is not on cooldown yet"
    );

    axolotl
        .brain
        .erase_memory(memory_module_types::ATTACK_TARGET.id());
    axolotl_ai::update_activity(&axolotl.brain);

    assert_eq!(
        axolotl
            .brain
            .time_until_expiry(memory_module_types::HAS_HUNTING_COOLDOWN),
        2400,
        "leaving a fight puts an axolotl off hunting for two minutes"
    );
}

#[test]
fn the_third_axolotl_of_a_cluster_is_born_a_calf() {
    // Vanilla reads the group size itself instead of using the shared baby
    // roll, so the first two of a cluster are adults and everything after them
    // is a calf -- no randomness at all.
    let world = axolotl_world("axolotl_cluster");
    let mut group_data = None;
    let mut babies = Vec::new();

    for _ in 0..3 {
        let axolotl = live_axolotl(&world);
        group_data = axolotl.finalize_spawn(&world, EntitySpawnReason::Natural, group_data);
        babies.push(AgeableMob::is_baby(axolotl.as_ref()));
    }

    assert_eq!(babies, vec![false, false, true]);
}

#[test]
fn an_axolotl_out_of_a_bucket_keeps_the_color_the_bucket_carried() {
    // `finalizeSpawn` returns before it touches the variant when the reason is
    // `BUCKET`. Without that early return an axolotl let out of a bucket would
    // be recolored at random on the way out.
    let world = axolotl_world("axolotl_bucket_spawn");
    let axolotl = live_axolotl(&world);
    axolotl.set_variant(AxolotlVariant::Blue);

    let _ = axolotl.finalize_spawn(&world, EntitySpawnReason::Bucket, None);

    assert_eq!(axolotl.variant(), AxolotlVariant::Blue);
}

#[test]
fn an_axolotl_kept_out_of_the_water_spends_its_air_and_then_dries_out() {
    // The shared living air tick refills a mob's air on land; the axolotl's
    // `baseTick` reads the air before that happens and writes back one less,
    // which is what turns carrying one overland into a race.
    let world = axolotl_world("axolotl_air");
    let axolotl = live_axolotl(&world);
    assert!(!axolotl.is_in_water());

    axolotl.set_air_supply(100);
    axolotl.base_tick();
    assert_eq!(
        axolotl.air_supply(),
        99,
        "an axolotl on land loses air rather than regaining it"
    );

    axolotl.set_air_supply(-19);
    let health_before = axolotl.get_health();
    axolotl.base_tick();

    assert_eq!(axolotl.air_supply(), 0);
    assert!(
        axolotl.get_health() < health_before,
        "an axolotl with no air left takes drying-out damage"
    );
}

#[test]
fn an_axolotl_spawns_on_clay_and_nowhere_else() {
    // Clay is the whole rule: unlike every land animal the axolotl asks for no
    // light level at all, which is what lets it live in a pitch-black lush cave.
    let world = axolotl_world("axolotl_spawn_rules");

    assert!(world.set_block(
        TEST_POS.below(),
        vanilla_blocks::STONE.default_state(),
        UpdateFlags::UPDATE_NONE
    ));
    assert!(!AxolotlEntity::check_axolotl_spawn_rules(&world, TEST_POS));

    assert!(world.set_block(
        TEST_POS.below(),
        vanilla_blocks::CLAY.default_state(),
        UpdateFlags::UPDATE_NONE
    ));
    assert!(AxolotlEntity::check_axolotl_spawn_rules(&world, TEST_POS));
}

#[test]
fn an_axolotl_reaches_its_brain_and_survives_forty_ticks_in_a_live_world() {
    // An axolotl whose `server_ai_step` does not reach `Mob::mob_server_ai_step`
    // never ticks its brain at all, and the tick loop catches a lock-ordering
    // hang in the amphibious navigation the same way the frog's does.
    let world = axolotl_world("axolotl_ticks");
    assert!(world.set_block(
        TEST_POS.below(),
        vanilla_blocks::CLAY.default_state(),
        UpdateFlags::UPDATE_NONE
    ));
    let axolotl = live_axolotl(&world);

    axolotl.set_no_action_time(0);
    LivingEntity::server_ai_step(axolotl.as_ref());
    assert!(
        axolotl.no_action_time() > 0,
        "the axolotl's brain never ticks: `server_ai_step` does not reach \
         `Mob::mob_server_ai_step`"
    );

    for _ in 0..40 {
        axolotl.tick();
    }

    assert!(Entity::is_alive(axolotl.as_ref()));
}

/// A damage source with a real attacker behind it, which is one of the six
/// things vanilla's play-dead roll insists on.
fn attacker_damage() -> DamageSource {
    DamageSource::environment(&vanilla_damage_types::MOB_ATTACK).with_causing_entity(1)
}
