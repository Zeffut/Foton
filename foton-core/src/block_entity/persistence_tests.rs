use std::{io::Cursor, sync::Weak};

use foton_registry::data_components::vanilla_components::CUSTOM_DATA;
use foton_registry::{init_vanilla_registry, vanilla_blocks};
use simdnbt::{borrow::read_compound, owned::NbtCompound};

use super::{
    BlockEntity, BlockEntityRegistry,
    entities::{BrewingStandBlockEntity, SpawnerBlockEntity},
};
use crate::chunk::full_chunk::client_block_entity_info;
use foton_utils::BlockPos;

fn load(entity: &dyn BlockEntity, nbt: &NbtCompound) {
    let mut bytes = Vec::new();
    nbt.write(&mut bytes);
    let borrowed = read_compound(&mut Cursor::new(&bytes)).expect("test NBT parses");
    entity.load_with_components(&borrowed);
}

fn stand() -> BrewingStandBlockEntity {
    init_vanilla_registry();
    BrewingStandBlockEntity::new(
        Weak::new(),
        BlockPos::new(3, 70, 4),
        vanilla_blocks::BREWING_STAND.default_state(),
    )
}

#[test]
fn brewing_pdc_roundtrips_without_losing_vanilla_fields_or_components() {
    let stand = stand();
    let mut nested = NbtCompound::new();
    nested.insert("inner", 17_i64);
    let mut pdc = NbtCompound::new();
    pdc.insert("plugin:byte", 7_i8);
    pdc.insert("plugin:nested", nested);
    let mut input = NbtCompound::new();
    input.insert("PublicBukkitValues", pdc.clone());
    input.insert("BrewTime", 23_i16);
    input.insert("Fuel", 4_i8);
    let mut components = NbtCompound::new();
    components.insert("minecraft:custom_name", "\"Potion stand\"");
    input.insert("components", components);
    load(&stand, &input);

    for saved in [
        stand.save_custom_only(),
        stand.save_without_metadata(),
        stand.save_with_full_metadata(),
    ] {
        assert_eq!(saved.compound("PublicBukkitValues"), Some(&pdc));
        assert_eq!(saved.short("BrewTime"), Some(23));
        assert_eq!(saved.byte("Fuel"), Some(4));
        assert_eq!(
            saved
                .iter()
                .filter(|(key, _)| key.to_str() == "PublicBukkitValues")
                .count(),
            1
        );
    }
    assert!(stand.save_custom_only().get("components").is_none());
    assert!(stand.save_without_metadata().get("components").is_some());
    assert!(stand.collect_components().get(CUSTOM_DATA).is_none());

    load(&stand, &NbtCompound::new());
    assert!(stand.save_custom_only().get("PublicBukkitValues").is_none());
    let mut wrong_type = NbtCompound::new();
    wrong_type.insert("PublicBukkitValues", 42_i32);
    load(&stand, &wrong_type);
    assert!(
        stand
            .save_without_metadata()
            .get("PublicBukkitValues")
            .is_none()
    );
}

#[test]
fn client_tag_omits_every_pdc_occurrence_and_retains_spawner_data() {
    init_vanilla_registry();
    let spawner = SpawnerBlockEntity::new(
        Weak::new(),
        BlockPos::new(8, 64, 8),
        vanilla_blocks::SPAWNER.default_state(),
    );
    let mut pdc = NbtCompound::new();
    pdc.insert("plugin:value", 1_i32);
    let mut input = NbtCompound::new();
    input.insert("PublicBukkitValues", pdc);
    let mut spawn_data = NbtCompound::new();
    spawn_data.insert("entity", NbtCompound::new());
    input.insert("SpawnData", spawn_data);
    load(&spawner, &input);
    assert!(
        spawner
            .get_update_tag()
            .expect("spawner tag")
            .contains("PublicBukkitValues")
    );
    let tag = spawner.get_client_update_tag().expect("spawner client tag");
    assert!(tag.contains("SpawnData"));
    assert!(!tag.contains("PublicBukkitValues"));
    let chunk_info = client_block_entity_info(&spawner);
    let chunk_tag = chunk_info.data.0.expect("spawner chunk tag");
    assert!(chunk_tag.contains("SpawnData"));
    assert!(!chunk_tag.contains("PublicBukkitValues"));
    let mut duplicate = NbtCompound::new();
    duplicate.insert("PublicBukkitValues", NbtCompound::new());
    duplicate.insert("PublicBukkitValues", NbtCompound::new());
    duplicate.insert("vanilla", 8_i32);
    super::sanitize_block_entity_client_nbt(&mut duplicate);
    assert_eq!(duplicate.int("vanilla"), Some(8));
    assert!(!duplicate.contains("PublicBukkitValues"));
}

#[test]
fn raw_fallback_keeps_pdc_unknown_fields_and_components_for_both_factory_inputs() {
    use foton_registry::vanilla_block_entity_types;

    init_vanilla_registry();
    let registry = BlockEntityRegistry::new();
    let mut pdc = NbtCompound::new();
    pdc.insert("plugin:long", 42_i64);
    let mut components = NbtCompound::new();
    components.insert("minecraft:custom_name", "\"Raw\"");
    let mut input = NbtCompound::new();
    input.insert("PublicBukkitValues", pdc.clone());
    input.insert("PublicBukkitValues", NbtCompound::new());
    input.insert("unknown", 31_i32);
    input.insert("components", components.clone());
    let mut bytes = Vec::new();
    input.write(&mut bytes);
    let borrowed = read_compound(&mut Cursor::new(&bytes)).expect("test NBT parses");
    let ty = &vanilla_block_entity_types::BARREL;
    let pos = BlockPos::new(3, 70, 4);
    let state = vanilla_blocks::BARREL.default_state();
    let entities = [
        registry.create_and_load_or_raw(ty, Weak::new(), pos, state, &borrowed),
        registry.create_and_load_owned_or_raw(ty, Weak::new(), pos, state, input),
    ];
    for entity in entities {
        let saved = entity.save_without_metadata();
        assert_eq!(saved.compound("PublicBukkitValues"), Some(&pdc));
        assert_eq!(saved.compound("components"), Some(&components));
        assert_eq!(saved.int("unknown"), Some(31));
        assert_eq!(
            saved
                .iter()
                .filter(|(key, _)| key.to_str() == "PublicBukkitValues")
                .count(),
            1
        );
    }
}

#[test]
fn falling_merge_replaces_incoming_pdc_and_preserves_it_when_absent() {
    use crate::entity::entities::objects::items::FallingBlockEntity;

    let stand = stand();
    let mut original_pdc = NbtCompound::new();
    original_pdc.insert("plugin:old", 2_i32);
    let mut initial = NbtCompound::new();
    initial.insert("PublicBukkitValues", original_pdc.clone());
    initial.insert("BrewTime", 9_i16);
    let mut components = NbtCompound::new();
    components.insert("minecraft:custom_name", "\"Carried\"");
    initial.insert("components", components.clone());
    load(&stand, &initial);

    let mut replacement_pdc = NbtCompound::new();
    replacement_pdc.insert("plugin:new", 5_i32);
    let mut incoming = NbtCompound::new();
    incoming.insert("PublicBukkitValues", replacement_pdc.clone());
    incoming.insert("Fuel", 7_i8);
    FallingBlockEntity::merge_block_entity_data_into(&stand, incoming);
    let saved = stand.save_custom_only();
    assert_eq!(saved.compound("PublicBukkitValues"), Some(&replacement_pdc));
    assert_eq!(saved.byte("Fuel"), Some(7));
    assert_eq!(saved.short("BrewTime"), Some(9));
    assert_eq!(
        stand.save_without_metadata().compound("components"),
        Some(&components)
    );

    let mut incoming_without_pdc = NbtCompound::new();
    incoming_without_pdc.insert("Fuel", 3_i8);
    FallingBlockEntity::merge_block_entity_data_into(&stand, incoming_without_pdc);
    let saved = stand.save_custom_only();
    assert_eq!(saved.compound("PublicBukkitValues"), Some(&replacement_pdc));
    assert_eq!(saved.byte("Fuel"), Some(3));
    assert_eq!(
        stand.save_without_metadata().compound("components"),
        Some(&components)
    );
}
