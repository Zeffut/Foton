use super::*;
use steel_registry::RegistryEntry as _;

fn test_persistent_end_crystal(pos: DVec3) -> PersistentEntity {
    PersistentEntity {
        entity_type: vanilla_entities::END_CRYSTAL.key.clone(),
        uuid: [9; 16],
        pos: [pos.x, pos.y, pos.z],
        motion: [0.0, 0.0, 0.0],
        rotation: [0.0, 0.0],
        fall_distance: 0.0,
        remaining_fire_ticks: 0,
        ticks_frozen: 0,
        is_in_powder_snow: false,
        was_in_powder_snow: false,
        has_visual_fire: false,
        on_ground: false,
        no_gravity: false,
        invulnerable: false,
        air_supply: DEFAULT_MAX_AIR_SUPPLY,
        portal_cooldown: 0,
        custom_name_nbt: Vec::new(),
        custom_name_visible: false,
        silent: false,
        glowing: false,
        tags: Vec::new(),
        custom_data_nbt: Vec::new(),
        nbt_data: Vec::new(),
        passengers: Vec::new(),
    }
}

#[test]
fn persistent_entity_load_clamps_position_like_vanilla() {
    init_globals_once();

    let persistent =
        test_persistent_end_crystal(DVec3::new(100_000_000.0, -100_000_000.0, -100_000_000.0));
    let Some(entity) =
        ChunkStorage::persistent_to_entity_at_level(&persistent, ChunkPos::new(0, 0), &Weak::new())
    else {
        panic!("entity should load with clamped position");
    };

    assert_eq!(
        entity.position(),
        DVec3::new(
            ENTITY_LOAD_MAX_HORIZONTAL_POSITION,
            -ENTITY_LOAD_MAX_VERTICAL_POSITION,
            -ENTITY_LOAD_MAX_HORIZONTAL_POSITION,
        )
    );
}

#[test]
fn persistent_entity_load_rejects_non_finite_rotation_like_vanilla() {
    init_globals_once();

    let mut persistent = test_persistent_end_crystal(DVec3::new(1.0, 2.0, 3.0));
    persistent.rotation = [f32::NAN, 0.0];

    assert!(
            ChunkStorage::persistent_to_entity_at_level(
                &persistent,
                ChunkPos::new(0, 0),
                &Weak::new(),
            )
            .is_none()
        );
}

#[test]
fn proto_block_entities_roundtrip_and_promote_to_full_chunk() {
    init_globals_once();

    let pos = ChunkPos::new(0, 0);
    let block_pos = BlockPos::new(3, 4, 5);
    let proto = Chunk::new(single_empty_section(), pos, 0, 16, Weak::new());
    let barrel = REGISTRY
        .blocks
        .get_default_state_id(&vanilla_blocks::BARREL);
    proto.set_block_state_for_generation(
        ChunkStatus::Features,
        block_pos,
        barrel,
        UpdateFlags::UPDATE_NONE,
    );
    proto.set_pending_block_entity(block_pos);

    assert!(proto.get_block_entity(block_pos).is_none());
    assert_eq!(proto.pending_block_entity_positions(), [block_pos]);

    let chunk = proto;
    let Some(prepared) =
        ChunkStorage::prepare_chunk_save(&chunk, ChunkStatus::Features, &[], false)
    else {
        panic!("dirty proto chunk should prepare for saving");
    };
    assert_eq!(prepared.persistent.block_entities.len(), 1);
    assert!(prepared.persistent.block_entities[0].entity_type.is_none());

    let loaded = ChunkStorage::persistent_to_chunk(
        &prepared.persistent,
        pos,
        ChunkStatus::Features,
        0,
        16,
        Weak::new(),
    );
    let loaded_proto = loaded.chunk;
    assert!(loaded_proto.get_block_entity(block_pos).is_none());
    assert_eq!(loaded_proto.pending_block_entity_positions(), [block_pos]);

    let full_ref = loaded_proto.promote_to_full().chunk;
    assert!(full_ref.get_block_entities().is_empty());
    assert_eq!(full_ref.pending_block_entity_positions(), [block_pos]);

    let Some(full_save) =
        ChunkStorage::prepare_chunk_save(&loaded_proto, ChunkStatus::Full, &[], true)
    else {
        panic!("forced full-chunk save should retain the pending marker");
    };
    assert_eq!(full_save.persistent.block_entities.len(), 1);
    assert!(full_save.persistent.block_entities[0].entity_type.is_none());
    let loaded = ChunkStorage::persistent_to_chunk(
        &full_save.persistent,
        pos,
        ChunkStatus::Full,
        0,
        16,
        Weak::new(),
    );
    let loaded_full = loaded.chunk;
    let loaded_full = FullChunkRef::from_full_context(&loaded_full);
    assert!(loaded_full.get_block_entities().is_empty());
    assert_eq!(loaded_full.pending_block_entity_positions(), [block_pos]);
    assert!(loaded_full.get_block_entity(block_pos).is_some());
    assert!(loaded_full.pending_block_entity_positions().is_empty());
}

#[test]
fn persistent_block_entity_with_invalid_live_state_is_rejected_before_construction() {
    init_globals_once();
    let persistent = PersistentBlockEntity {
        x: 1,
        y: 2,
        z: 3,
        entity_type: Some(vanilla_block_entity_types::BARREL.key.clone()),
        nbt_data: Vec::new(),
    };

    assert!(
        ChunkStorage::persistent_to_block_entity_at(
            &persistent,
            BlockPos::new(1, 2, 3),
            Weak::new(),
            vanilla_blocks::STONE.default_state(),
        )
        .is_none()
    );
}

#[test]
fn persistent_block_entity_with_malformed_nbt_is_dropped() {
    init_globals_once();
    let persistent = PersistentBlockEntity {
        x: 1,
        y: 2,
        z: 3,
        entity_type: Some(vanilla_block_entity_types::BARREL.key.clone()),
        nbt_data: vec![0xff],
    };

    assert!(
        ChunkStorage::persistent_to_block_entity_at(
            &persistent,
            BlockPos::new(1, 2, 3),
            Weak::new(),
            vanilla_blocks::BARREL.default_state(),
        )
        .is_none()
    );
}

#[test]
fn proto_entities_roundtrip_and_promote_to_full_chunk() {
    init_globals_once();

    let pos = ChunkPos::new(0, 0);
    let entity_pos = DVec3::new(5.5, 6.0, 7.5);
    let proto = Chunk::new(single_empty_section(), pos, 0, 16, Weak::new());
    let crystal = Arc::new(EndCrystalEntity::new(
        &vanilla_entities::END_CRYSTAL,
        next_entity_id(),
        entity_pos,
        Weak::new(),
    ));
    crystal.set_beam_target(Some(BlockPos::new(0, 64, 0)));
    crystal.set_invulnerable(true);
    crystal.set_fall_distance(3.75);
    crystal.set_no_gravity(true);
    crystal.set_air_supply(120);
    crystal.set_portal_cooldown(9);
    crystal.set_custom_name(Some(TextComponent::plain("End Test")));
    crystal.set_custom_name_visible(true);
    crystal.set_silent(true);
    crystal.set_glowing_tag(true);
    assert!(crystal.add_tag("steel:test".to_owned()));
    let mut custom_data = NbtCompound::new();
    custom_data.insert("marker", "roundtrip");
    crystal.set_custom_data(custom_data);
    proto.add_entity(crystal);

    let chunk = proto;
    let Some(prepared) =
        ChunkStorage::prepare_chunk_save(&chunk, ChunkStatus::Features, &[], false)
    else {
        panic!("dirty proto chunk should prepare for saving");
    };
    assert_eq!(prepared.persistent.entities.len(), 1);
    assert!((prepared.persistent.entities[0].fall_distance - 3.75).abs() <= f64::EPSILON);
    assert!(prepared.persistent.entities[0].no_gravity);
    assert!(prepared.persistent.entities[0].invulnerable);
    assert_eq!(prepared.persistent.entities[0].air_supply, 120);
    assert_eq!(prepared.persistent.entities[0].portal_cooldown, 9);
    assert!(prepared.persistent.entities[0].custom_name_visible);
    assert!(prepared.persistent.entities[0].silent);
    assert!(prepared.persistent.entities[0].glowing);
    assert_eq!(
        prepared.persistent.entities[0].tags,
        vec!["steel:test".to_owned()]
    );
    assert!(!prepared.persistent.entities[0].custom_name_nbt.is_empty());
    assert!(!prepared.persistent.entities[0].custom_data_nbt.is_empty());
    let custom_name_nbt = read_borrowed_compound(&mut Cursor::new(
        &prepared.persistent.entities[0].custom_name_nbt,
    ))
    .expect("saved custom name should be valid NBT");
    let custom_name_nbt = simdnbt::borrow::NbtCompound::from(&custom_name_nbt);
    assert_eq!(
        custom_name_nbt
            .string("CustomName")
            .map(|value| value.to_str().into_owned()),
        Some("End Test".to_owned())
    );

    let loaded = ChunkStorage::persistent_to_chunk(
        &prepared.persistent,
        pos,
        ChunkStatus::Features,
        0,
        16,
        Weak::new(),
    );
    assert!(loaded.pending_entities.is_empty());
    let loaded_proto = loaded.chunk;
    assert_eq!(loaded_proto.get_entities().len(), 1);

    let promoted = loaded_proto.promote_to_full();
    assert_eq!(promoted.pending_entities.len(), 1);
    assert!(promoted.pending_entities[0].is_no_gravity());
    assert!(promoted.pending_entities[0].is_invulnerable());
    assert_eq!(promoted.pending_entities[0].air_supply(), 120);
    assert_eq!(promoted.pending_entities[0].portal_cooldown(), 9);
    assert_eq!(
        promoted.pending_entities[0].custom_name(),
        Some(TextComponent::plain("End Test"))
    );
    assert!(promoted.pending_entities[0].is_custom_name_visible());
    assert!(promoted.pending_entities[0].is_silent());
    assert!(promoted.pending_entities[0].has_glowing_tag());
    assert_eq!(
        promoted.pending_entities[0].tags(),
        vec!["steel:test".to_owned()]
    );
    assert_eq!(
        promoted.pending_entities[0]
            .custom_data()
            .string("marker")
            .map(ToString::to_string),
        Some("roundtrip".to_owned())
    );
}

#[test]
fn prepared_save_reports_handled_runtime_entity_ids() {
    init_globals_once();

    let pos = ChunkPos::new(0, 0);
    let proto = Chunk::new(single_empty_section(), pos, 0, 16, Weak::new());
    let chunk = proto;
    let entity: SharedEntity = Arc::new(EndCrystalEntity::new(
        &vanilla_entities::END_CRYSTAL,
        next_entity_id(),
        DVec3::new(5.5, 6.0, 7.5),
        Weak::new(),
    ));

    let Some(prepared) = ChunkStorage::prepare_chunk_save(
        &chunk,
        ChunkStatus::Features,
        slice::from_ref(&entity),
        true,
    ) else {
        panic!("forced runtime entity save should prepare a chunk save");
    };

    assert_eq!(prepared.handled_runtime_entity_ids, vec![entity.id()]);
    assert_eq!(prepared.persistent.entities.len(), 1);
}

#[test]
fn full_chunk_load_defers_entities_to_world_registration() {
    init_globals_once();

    let pos = ChunkPos::new(0, 0);
    let proto = Chunk::new(single_empty_section(), pos, 0, 16, Weak::new());
    let chunk = proto;
    let entity: SharedEntity = Arc::new(EndCrystalEntity::new(
        &vanilla_entities::END_CRYSTAL,
        next_entity_id(),
        DVec3::new(5.5, 6.0, 7.5),
        Weak::new(),
    ));

    let Some(prepared) = ChunkStorage::prepare_chunk_save(
        &chunk,
        ChunkStatus::Features,
        slice::from_ref(&entity),
        true,
    ) else {
        panic!("forced runtime entity save should prepare a chunk save");
    };

    let loaded = ChunkStorage::persistent_to_chunk(
        &prepared.persistent,
        pos,
        ChunkStatus::Full,
        0,
        16,
        Weak::new(),
    );

    assert_eq!(loaded.status, ChunkStatus::Full);
    assert_eq!(loaded.pending_entities.len(), 1);
    assert_eq!(loaded.pending_entities[0].uuid(), entity.uuid());
}

#[test]
fn runtime_entity_passengers_save_nested_and_load_flattened_for_registration() {
    init_globals_once();

    let pos = ChunkPos::new(0, 0);
    let proto = Chunk::new(single_empty_section(), pos, 0, 16, Weak::new());
    let chunk = proto;
    let vehicle: SharedEntity = Arc::new(EndCrystalEntity::new(
        &vanilla_entities::END_CRYSTAL,
        next_entity_id(),
        DVec3::new(5.5, 6.0, 7.5),
        Weak::new(),
    ));
    let passenger: SharedEntity = Arc::new(EndCrystalEntity::new(
        &vanilla_entities::END_CRYSTAL,
        next_entity_id(),
        DVec3::new(5.5, 8.0, 7.5),
        Weak::new(),
    ));
    EntityBase::restore_passenger_relationship(&vehicle, &passenger);
    let vehicle_uuid = vehicle.uuid();
    let passenger_uuid = passenger.uuid();
    let entities = [Arc::clone(&vehicle), Arc::clone(&passenger)];

    let Some(prepared) =
        ChunkStorage::prepare_chunk_save(&chunk, ChunkStatus::Features, &entities, true)
    else {
        panic!("forced runtime entity save should prepare a chunk save");
    };

    assert_eq!(prepared.persistent.entities.len(), 1);
    assert_eq!(
        prepared.persistent.entities[0].uuid,
        *vehicle_uuid.as_bytes()
    );
    assert_eq!(prepared.persistent.entities[0].passengers.len(), 1);
    assert_eq!(
        prepared.persistent.entities[0].passengers[0].uuid,
        *passenger_uuid.as_bytes()
    );

    let loaded = ChunkStorage::persistent_to_chunk(
        &prepared.persistent,
        pos,
        ChunkStatus::Full,
        0,
        16,
        Weak::new(),
    );

    assert_eq!(loaded.status, ChunkStatus::Full);
    assert_eq!(loaded.pending_entities.len(), 2);
    let Some(loaded_passenger) = loaded
        .pending_entities
        .iter()
        .find(|entity| entity.uuid() == passenger_uuid)
    else {
        panic!("passenger should load into pending registration list");
    };
    let Some(loaded_vehicle) = loaded_passenger.vehicle() else {
        panic!("passenger should restore its vehicle relationship");
    };
    assert_eq!(loaded_vehicle.uuid(), vehicle_uuid);
    assert!(loaded_vehicle.has_passenger(loaded_passenger.as_ref()));
}

#[test]
fn runtime_entity_passengers_skip_non_serializable_entities_like_vanilla() {
    init_globals_once();

    let pos = ChunkPos::new(0, 0);
    let proto = Chunk::new(single_empty_section(), pos, 0, 16, Weak::new());
    let chunk = proto;
    let vehicle: SharedEntity = Arc::new(EndCrystalEntity::new(
        &vanilla_entities::END_CRYSTAL,
        next_entity_id(),
        DVec3::new(5.5, 6.0, 7.5),
        Weak::new(),
    ));
    let passenger: SharedEntity = Arc::new(RawEntity::new(
        next_entity_id(),
        DVec3::new(5.5, 8.0, 7.5),
        Weak::new(),
        &vanilla_entities::PLAYER,
    ));
    EntityBase::restore_passenger_relationship(&vehicle, &passenger);
    let vehicle_uuid = vehicle.uuid();

    let Some(prepared) = ChunkStorage::prepare_chunk_save(
        &chunk,
        ChunkStatus::Features,
        slice::from_ref(&vehicle),
        true,
    ) else {
        panic!("forced runtime entity save should prepare a chunk save");
    };

    assert_eq!(prepared.persistent.entities.len(), 1);
    assert_eq!(
        prepared.persistent.entities[0].uuid,
        *vehicle_uuid.as_bytes()
    );
    assert!(prepared.persistent.entities[0].passengers.is_empty());

    let loaded = ChunkStorage::persistent_to_chunk(
        &prepared.persistent,
        pos,
        ChunkStatus::Full,
        0,
        16,
        Weak::new(),
    );

    assert_eq!(loaded.status, ChunkStatus::Full);
    assert_eq!(loaded.pending_entities.len(), 1);
    assert_eq!(loaded.pending_entities[0].uuid(), vehicle_uuid);
}

#[test]
fn unimplemented_block_entities_preserve_nbt_through_proto_save_load() {
    init_globals_once();

    let pos = ChunkPos::new(0, 0);
    let block_pos = BlockPos::new(4, 4, 6);
    let proto = Chunk::new(single_empty_section(), pos, 0, 16, Weak::new());
    // A structure block on purpose: it is one of the types Steel still has no concrete
    // block entity for, so this exercises the raw NBT-preserving fallback. Repoint it if a
    // concrete structure-block entity ever lands.
    let structure_block = REGISTRY
        .blocks
        .get_default_state_id(&vanilla_blocks::TEST_BLOCK);
    proto.set_block_state_for_generation(
        ChunkStatus::Features,
        block_pos,
        structure_block,
        UpdateFlags::UPDATE_NONE,
    );

    let mut nbt = NbtCompound::new();
    nbt.insert("LootTable", "minecraft:chests/simple_dungeon");
    nbt.insert("LootTableSeed", 42_i64);
    let entity = BLOCK_ENTITIES.create_and_load_owned_or_raw(
        &vanilla_block_entity_types::TEST_BLOCK,
        proto.level_weak(),
        block_pos,
        structure_block,
        nbt,
    );
    assert!(proto.set_block_entity(entity));

    let chunk = proto;
    let Some(prepared) =
        ChunkStorage::prepare_chunk_save(&chunk, ChunkStatus::Features, &[], false)
    else {
        panic!("dirty proto chunk should prepare for saving");
    };
    assert_eq!(prepared.persistent.block_entities.len(), 1);

    let loaded = ChunkStorage::persistent_to_chunk(
        &prepared.persistent,
        pos,
        ChunkStatus::Features,
        0,
        16,
        Weak::new(),
    );
    let loaded_proto = loaded.chunk;
    let Some(loaded_entity) = loaded_proto.get_block_entity(block_pos) else {
        panic!("raw block entity should survive chunk load");
    };

    let mut saved = NbtCompound::new();
    assert_eq!(
        loaded_entity.get_type().id(),
        vanilla_block_entity_types::TEST_BLOCK.id()
    );
    loaded_entity.save_additional(&mut saved);

    assert_eq!(
        saved.string("LootTable").map(ToString::to_string),
        Some("minecraft:chests/simple_dungeon".to_owned())
    );
    assert_eq!(saved.long("LootTableSeed"), Some(42));
}

/// Position inside chunk (0, 0), so a saved entity loads back into it.
const LIVING_STATE_POS: DVec3 = DVec3::new(5.5, 6.0, 7.5);

/// The modifier id the attribute round trip looks for.
const TEST_SPEED_MODIFIER: Identifier = Identifier::new_static("steel", "test_speed");

fn living_state_round_trip(zombie: &SharedEntity) -> SharedEntity {
    let Some(persistent) = ChunkStorage::entity_tree_to_persistent(zombie) else {
        panic!("a live zombie should be saveable");
    };
    let Some(loaded) =
        ChunkStorage::persistent_to_entity_at_level(&persistent, ChunkPos::new(0, 0), &Weak::new())
    else {
        panic!("a saved zombie should load back");
    };
    loaded
}

fn test_zombie() -> SharedEntity {
    let Some(zombie) = ENTITIES.create(
        &vanilla_entities::ZOMBIE,
        next_entity_id(),
        LIVING_STATE_POS,
        Weak::new(),
    ) else {
        panic!("a zombie should have an entity factory");
    };
    zombie
}

/// The half of a save that belongs to every living entity rather than to a type.
///
/// The chunk saver used to write only `save_additional`, which is the *type's*
/// half, so health, potion effects, absorption and attribute modifiers reached
/// no chunk file and were read from none: every mob in the world came back at
/// full health with nothing on it. Saving one compound and loading it is what
/// proves the pair: a writer that writes nothing fails the loaded assertions,
/// and a reader that ignores the compound fails them too.
///
/// Every value here is deliberately not a zombie's default, and the untouched
/// control below is what says so -- a loader that hardcoded these numbers would
/// pass this test alone and fail that one.
#[test]
fn living_state_survives_a_chunk_save_and_load() {
    init_globals_once();

    let zombie = test_zombie();
    let Some(living) = zombie.as_living_entity() else {
        panic!("a zombie is a living entity");
    };
    living
        .attributes()
        .lock()
        .set_base_value(vanilla_attributes::MAX_HEALTH, 30.0);
    living.attributes().lock().set_modifier(
        vanilla_attributes::MOVEMENT_SPEED,
        AttributeModifier {
            id: TEST_SPEED_MODIFIER,
            amount: 0.5,
            operation: AttributeModifierOperation::AddValue,
        },
        true,
    );
    living.set_health(14.0);
    living.internal_set_absorption_amount(6.0);
    living.add_mob_effect(MobEffectInstance::with_duration(
        vanilla_mob_effects::STRENGTH,
        1200,
        3,
    ));

    let loaded = living_state_round_trip(&zombie);
    let Some(loaded) = loaded.as_living_entity() else {
        panic!("a loaded zombie is a living entity");
    };

    assert!((loaded.get_health() - 14.0).abs() <= f32::EPSILON);
    assert!((loaded.get_absorption_amount() - 6.0).abs() <= f32::EPSILON);
    let base_max_health = loaded
        .attributes()
        .lock()
        .get_base_value(vanilla_attributes::MAX_HEALTH);
    assert!(base_max_health.is_some_and(|base| (base - 30.0).abs() <= f64::EPSILON));
    assert!(
        loaded
            .attributes()
            .lock()
            .has_modifier(vanilla_attributes::MOVEMENT_SPEED, &TEST_SPEED_MODIFIER)
    );
    let Some(effect) = loaded.mob_effect(vanilla_mob_effects::STRENGTH) else {
        panic!("the strength effect should survive the save");
    };
    assert_eq!(effect.amplifier(), 3);
    assert_eq!(effect.duration(), 1200);
}

/// The control for the test above: an untouched mob keeps its own defaults.
///
/// Vanilla's `readAdditionalSaveData` defaults a missing `Health` to *full*
/// health rather than to zero, which is exactly the kind of full-looking
/// default that makes a broken loader look healthy. This says out loud what the
/// numbers above have to differ from.
#[test]
fn an_untouched_mob_round_trips_to_its_own_defaults() {
    init_globals_once();

    let zombie = test_zombie();
    let loaded = living_state_round_trip(&zombie);
    let Some(loaded) = loaded.as_living_entity() else {
        panic!("a loaded zombie is a living entity");
    };

    assert!((loaded.get_health() - 20.0).abs() <= f32::EPSILON);
    assert!(loaded.get_absorption_amount().abs() <= f32::EPSILON);
    assert!(loaded.active_mob_effects().is_empty());
    assert!(
        !loaded
            .attributes()
            .lock()
            .has_modifier(vanilla_attributes::MOVEMENT_SPEED, &TEST_SPEED_MODIFIER)
    );
}
