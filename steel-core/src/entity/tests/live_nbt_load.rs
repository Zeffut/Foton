//! What a vanilla entity compound does to an entity that is already alive.
//!
//! This is the half of `Entity.load` the constructor path never needed:
//! `/execute store ... entity` and `/data modify entity` hand a whole compound
//! back to a mob that is standing in the world, and every field the commands
//! can see has to land on it.
//!
//! Two habits keep these honest. Every value below is deliberately not the one
//! a freshly built mob has, and the first assertion in each test says so, so a
//! reader that does nothing cannot pass by agreeing with the default. And the
//! compound is written to bytes and reborrowed rather than handed over
//! directly, so this goes through the same shape the command does.

use std::io::Cursor;
use std::sync::Weak;

use glam::DVec3;
use simdnbt::borrow::read_compound as read_borrowed_compound;
use simdnbt::owned::{NbtCompound, NbtList, NbtTag};
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::{init_vanilla_registry, vanilla_entities};
use steel_utils::UuidExt as _;
use text_components::TextComponent;
use uuid::Uuid;

use crate::entity::nbt_load::{load_live_entity, read_entity_nbt};
use crate::entity::{ENTITIES, EntityLoadRequest, SharedEntity, init_entities, next_entity_id};

/// Builds one live entity of `entity_type` with nothing loaded into it.
fn fresh(entity_type: EntityTypeRef) -> SharedEntity {
    init_vanilla_registry();
    init_entities();
    ENTITIES
        .create(entity_type, next_entity_id(), DVec3::ZERO, Weak::new())
        .unwrap_or_else(|| panic!("{} has no entity factory", entity_type.key))
}

/// Runs `load_live_entity` over the bytes `nbt` encodes to.
fn load(entity: &SharedEntity, nbt: &NbtCompound) {
    let mut bytes = Vec::new();
    nbt.write(&mut bytes);
    let borrowed = read_borrowed_compound(&mut Cursor::new(bytes.as_slice()))
        .unwrap_or_else(|error| panic!("test nbt should reborrow: {error}"));
    load_live_entity(entity.as_ref(), (&borrowed).into());
}

/// A compound carrying a non-default value for every base field.
fn every_base_field(custom_name: &TextComponent) -> NbtCompound {
    let mut nbt = NbtCompound::new();
    nbt.insert("Pos", NbtList::Double(vec![12.5, 70.25, -8.75]));
    nbt.insert("Motion", NbtList::Double(vec![0.25, -0.5, 0.75]));
    nbt.insert("Rotation", NbtList::Float(vec![45.0, 30.0]));
    nbt.insert("fall_distance", 6.5f64);
    nbt.insert("Fire", NbtTag::Short(37));
    nbt.insert("Air", NbtTag::Short(137));
    nbt.insert("OnGround", 1i8);
    nbt.insert("Invulnerable", 1i8);
    nbt.insert("PortalCooldown", 91);
    nbt.insert("CustomName", custom_name.to_codec_nbt());
    nbt.insert("CustomNameVisible", 1i8);
    nbt.insert("Silent", 1i8);
    nbt.insert("NoGravity", 1i8);
    nbt.insert("Glowing", 1i8);
    nbt.insert("TicksFrozen", 55);
    nbt.insert("HasVisualFire", 1i8);
    nbt.insert("Tags", NbtList::String(vec!["alpha".into(), "beta".into()]));
    let mut custom_data = NbtCompound::new();
    custom_data.insert("marker", 7);
    nbt.insert("data", NbtTag::Compound(custom_data));
    nbt
}

#[test]
#[expect(
    clippy::float_cmp,
    reason = "an exact value that survived an NBT round trip"
)]
fn a_live_entity_takes_every_base_field_the_compound_carries() {
    let entity = fresh(&vanilla_entities::PIG);
    let custom_name = TextComponent::from("Rocky");

    // Nothing below can pass by agreeing with a pig that was born that way.
    assert_eq!(entity.position(), DVec3::ZERO);
    assert_eq!(entity.velocity(), DVec3::ZERO);
    assert_eq!(entity.rotation(), (0.0, 0.0));
    assert_eq!(entity.fall_distance(), 0.0);
    assert_eq!(entity.remaining_fire_ticks(), 0);
    assert_ne!(entity.air_supply(), 137);
    assert!(!entity.on_ground());
    assert!(!entity.is_invulnerable());
    assert_eq!(entity.portal_cooldown(), 0);
    assert_eq!(entity.custom_name(), None);
    assert!(!entity.is_custom_name_visible());
    assert!(!entity.is_silent());
    assert!(!entity.is_no_gravity());
    assert!(!entity.has_glowing_tag());
    assert_eq!(entity.ticks_frozen(), 0);
    assert!(!entity.has_visual_fire());
    assert!(entity.tags().is_empty());
    assert!(entity.custom_data().is_empty());

    load(&entity, &every_base_field(&custom_name));

    assert_eq!(entity.position(), DVec3::new(12.5, 70.25, -8.75));
    assert_eq!(entity.velocity(), DVec3::new(0.25, -0.5, 0.75));
    assert_eq!(entity.rotation(), (45.0, 30.0));
    assert_eq!(entity.fall_distance(), 6.5);
    assert_eq!(entity.remaining_fire_ticks(), 37);
    assert_eq!(entity.air_supply(), 137);
    assert!(entity.on_ground());
    assert!(entity.is_invulnerable());
    assert_eq!(entity.portal_cooldown(), 91);
    assert_eq!(entity.custom_name(), Some(custom_name));
    assert!(entity.is_custom_name_visible());
    assert!(entity.is_silent());
    assert!(entity.is_no_gravity());
    assert!(entity.has_glowing_tag());
    assert_eq!(entity.ticks_frozen(), 55);
    assert!(entity.has_visual_fire());
    assert_eq!(entity.tags(), vec!["alpha".to_owned(), "beta".to_owned()]);
    assert_eq!(entity.custom_data().int("marker"), Some(7));
}

/// A field the compound leaves out goes back to its default rather than
/// keeping what the entity had. Vanilla resets all four unconditionally.
#[test]
fn a_live_load_clears_what_the_compound_leaves_out() {
    let entity = fresh(&vanilla_entities::PIG);
    load(&entity, &every_base_field(&TextComponent::from("Rocky")));
    assert!(entity.has_glowing_tag());
    assert!(!entity.tags().is_empty());

    load(&entity, &NbtCompound::new());

    assert_eq!(entity.custom_name(), None);
    assert!(!entity.has_glowing_tag());
    assert!(!entity.is_silent());
    assert!(entity.tags().is_empty());
    assert!(entity.custom_data().is_empty());
    assert_eq!(entity.air_supply(), entity.max_air_supply());
}

/// Vanilla reads `UUID` in `Entity.load` and `EntityDataAccessor.setData` puts
/// the old one straight back, because the world indexes a live entity by it.
#[test]
fn a_live_load_leaves_the_uuid_alone() {
    let entity = fresh(&vanilla_entities::PIG);
    let original = entity.uuid();
    let other = Uuid::from_u128(0x0123_4567_89ab_cdef_0123_4567_89ab_cdef);
    assert_ne!(original, other);

    let mut nbt = NbtCompound::new();
    nbt.insert("UUID", NbtTag::IntArray(other.to_int_array().to_vec()));
    load(&entity, &nbt);

    assert_eq!(entity.uuid(), original);
}

/// Vanilla throws away any velocity component over ten blocks a tick rather
/// than trusting it, and keeps the ones under.
#[test]
fn a_live_load_drops_a_velocity_vanilla_calls_impossible() {
    let entity = fresh(&vanilla_entities::PIG);
    let mut nbt = NbtCompound::new();
    nbt.insert("Motion", NbtList::Double(vec![10.5, -0.5, -12.0]));
    load(&entity, &nbt);

    assert_eq!(entity.velocity(), DVec3::new(0.0, -0.5, 0.0));
}

/// A compound whose position is not a number is refused whole. Vanilla writes
/// the position first and then throws a crash report, which leaves the entity
/// half-loaded; a command has no crash-report escape, so nothing is applied.
#[test]
fn a_live_load_refuses_a_compound_with_a_position_that_is_not_a_number() {
    let entity = fresh(&vanilla_entities::PIG);
    load(&entity, &every_base_field(&TextComponent::from("Rocky")));
    let air_before = entity.air_supply();
    let position_before = entity.position();

    let mut nbt = every_base_field(&TextComponent::from("Rocky"));
    while nbt.remove("Pos").is_some() {}
    nbt.insert("Pos", NbtList::Double(vec![f64::NAN, 70.0, 0.0]));
    while nbt.remove("Air").is_some() {}
    nbt.insert("Air", NbtTag::Short(11));
    load(&entity, &nbt);

    assert_eq!(entity.position(), position_before);
    assert_eq!(entity.air_supply(), air_before);
}

/// The shared living half and the type's own reader both run, in that order.
/// A slime reads its size on top of the health the living half restored, so
/// asking for both is what proves the whole chain ran rather than the base
/// fields alone.
#[test]
#[expect(
    clippy::float_cmp,
    reason = "an exact value that survived an NBT round trip"
)]
fn a_live_load_reaches_the_living_half_and_the_type_own_reader() {
    let entity = fresh(&vanilla_entities::SLIME);
    let Some(living) = entity.as_living_entity() else {
        panic!("a slime is a living entity");
    };
    assert_ne!(living.get_health(), 3.0);

    let mut before = NbtCompound::new();
    entity.save_additional(&mut before);
    assert_ne!(before.int("Size"), Some(3));

    let mut nbt = NbtCompound::new();
    nbt.insert("Health", 3.0f32);
    nbt.insert("Size", 3);
    load(&entity, &nbt);

    let Some(living) = entity.as_living_entity() else {
        panic!("a slime is a living entity");
    };
    assert_eq!(living.get_health(), 3.0);

    let mut after = NbtCompound::new();
    entity.save_additional(&mut after);
    assert_eq!(after.int("Size"), Some(3));
}

/// Builds an entity through the constructor load path, the one a chunk load
/// uses.
///
/// The round trip below needs a source `load_live_entity` never touched;
/// comparing two entities that both went through it would agree with itself
/// whatever either of them dropped. This mirrors `/summon`'s own decode, minus
/// the passengers.
fn loaded_the_way_a_chunk_load_does(
    entity_type: EntityTypeRef,
    position: DVec3,
    nbt: &NbtCompound,
) -> SharedEntity {
    init_vanilla_registry();
    init_entities();

    let mut tag = nbt.clone();
    while tag.remove("id").is_some() {}
    tag.insert("id", entity_type.key.to_string());

    let mut bytes = Vec::new();
    tag.write(&mut bytes);
    let borrowed = read_borrowed_compound(&mut Cursor::new(bytes.as_slice()))
        .unwrap_or_else(|error| panic!("test nbt should reborrow: {error}"));
    let Some(loaded) = read_entity_nbt(&(&borrowed).into()) else {
        panic!("{} should decode from a save compound", entity_type.key);
    };

    let mut remainder_bytes = Vec::new();
    loaded.remainder.write(&mut remainder_bytes);
    let remainder = read_borrowed_compound(&mut Cursor::new(remainder_bytes.as_slice()))
        .unwrap_or_else(|error| panic!("remainder should reborrow: {error}"));

    ENTITIES.create_and_load_or_raw(
        EntityLoadRequest {
            entity_type,
            position,
            uuid: loaded.uuid.unwrap_or_else(Uuid::new_v4),
            velocity: loaded.velocity,
            rotation: loaded.rotation,
            fall_distance: loaded.fall_distance,
            fire_freeze: loaded.fire_freeze,
            on_ground: loaded.on_ground,
            save_data: loaded.save_data,
            world: Weak::new(),
        },
        &remainder,
    )
}

/// The whole point of the exercise: a mob the chunk loader rebuilt and a mob a
/// command reloaded from that mob's own compound have to be the same mob.
///
/// Anything the two disagree about is a field a command can read off an entity
/// and cannot put back.
#[test]
fn a_live_load_rebuilds_what_the_chunk_loader_would_have_built() {
    let position = DVec3::new(12.5, 70.25, -8.75);
    let mut compound = every_base_field(&TextComponent::from("Rocky"));
    compound.insert("Health", 3.0f32);
    compound.insert("Size", 3);
    let source = loaded_the_way_a_chunk_load_does(&vanilla_entities::SLIME, position, &compound);

    let target = fresh(&vanilla_entities::SLIME);
    let expected = source.nbt_for_data_compare();
    load(&target, &expected);

    let mut expected = expected;
    let mut actual = target.nbt_for_data_compare();
    // The UUID is the one field a live load deliberately does not take.
    while expected.remove("UUID").is_some() {}
    while actual.remove("UUID").is_some() {}
    assert_eq!(actual, expected);
}
