//! Reading one entity out of a vanilla save compound.
//!
//! Vanilla parity: the base-field half of `Entity.load`, which runs before
//! `readAdditionalSaveData` and owns every key an entity has regardless of its
//! type. Steel keeps those decoded fields in [`EntityBaseSaveData`] and friends
//! rather than handing the raw compound down, so this is where a vanilla
//! compound is turned into them.
//!
//! Two callers need it: structure templates, whose entities arrive as vanilla
//! compounds inside the template file, and `/summon` with an NBT argument.
//!
//! `Pos` is decoded by neither, and that is vanilla's own behaviour rather
//! than an omission. A template positions its entities from the
//! template-relative `pos` beside the compound, and `SummonCommand` follows
//! the load with a `snapTo` to the position the command named, which
//! overwrites whatever `Pos` had just set. The key is still stripped, so a
//! type's own reader never sees it.

use std::collections::BTreeSet;
use std::str::FromStr as _;

use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;
use steel_registry::{REGISTRY, RegistryExt as _, entity_type::EntityTypeRef};
use steel_utils::{Identifier, UuidExt as _};
use text_components::TextComponent;
use uuid::Uuid;

use super::{DEFAULT_MAX_AIR_SUPPLY, EntityBaseSaveData, EntityFireFreezeState, MAX_ENTITY_TAGS};

/// Base fields vanilla's `Entity.load` consumes before the type-specific data.
///
/// Removing them from the compound that goes on to `load_additional` keeps a
/// type's own reader from having to know about keys it does not own.
const BASE_FIELDS: [&str; 20] = [
    "id",
    "Pos",
    "Motion",
    "Rotation",
    "UUID",
    "fall_distance",
    "Fire",
    "Air",
    "OnGround",
    "NoGravity",
    "Invulnerable",
    "PortalCooldown",
    "CustomName",
    "CustomNameVisible",
    "Silent",
    "Glowing",
    "TicksFrozen",
    "HasVisualFire",
    "Tags",
    "data",
];

/// One entity decoded from a vanilla save compound.
pub(crate) struct EntityNbtLoad {
    /// The type named by `id`.
    pub(crate) entity_type: EntityTypeRef,
    /// `UUID`, when the compound carried one.
    pub(crate) uuid: Option<Uuid>,
    /// `Rotation`, as yaw and pitch.
    pub(crate) rotation: (f32, f32),
    /// `Motion`.
    pub(crate) velocity: DVec3,
    /// `fall_distance`.
    pub(crate) fall_distance: f64,
    /// `Fire`, `TicksFrozen` and `HasVisualFire`.
    pub(crate) fire_freeze: EntityFireFreezeState,
    /// `OnGround`.
    pub(crate) on_ground: bool,
    /// Everything else `Entity.load` reads.
    pub(crate) save_data: EntityBaseSaveData,
    /// What is left once every base field has been taken out, for the type's
    /// own `load_additional`.
    pub(crate) remainder: NbtCompound,
    /// `Passengers`, still encoded, for the caller to load and seat.
    pub(crate) passengers: Vec<NbtCompound>,
}

/// Resolves the entity type an `id` key names.
fn entity_type_from_nbt(nbt: &BorrowedNbtCompoundView<'_, '_>) -> Option<EntityTypeRef> {
    let id = nbt.string("id")?;
    let id = Identifier::from_str(id.to_str().as_ref()).ok()?;
    REGISTRY.entity_types.by_key(&id)
}

/// Decodes one vanilla entity compound.
///
/// Returns `None` when `id` is missing or names no registered entity type,
/// which is what vanilla's `EntityType.by(input)` failing amounts to.
pub(crate) fn read_entity_nbt(nbt: &BorrowedNbtCompoundView<'_, '_>) -> Option<EntityNbtLoad> {
    let entity_type = entity_type_from_nbt(nbt)?;

    let mut remainder = nbt.to_owned();
    for field in BASE_FIELDS {
        let _ = remainder.remove(field);
    }
    let passengers = nbt
        .list("Passengers")
        .and_then(|list| list.compounds())
        .map(|compounds| {
            compounds
                .clone()
                .into_iter()
                .map(|compound| compound.to_owned())
                .collect()
        })
        .unwrap_or_default();

    Some(EntityNbtLoad {
        entity_type,
        uuid: nbt
            .int_array("UUID")
            .and_then(|uuid| Uuid::from_int_array(&uuid)),
        rotation: read_rotation(nbt),
        velocity: read_vec3d(nbt, "Motion").unwrap_or(DVec3::ZERO),
        fall_distance: nbt.double("fall_distance").unwrap_or(0.0),
        fire_freeze: EntityFireFreezeState::from_parts(
            read_int(nbt, "Fire").unwrap_or(0),
            read_int(nbt, "TicksFrozen").unwrap_or(0),
            false,
            false,
            read_flag(nbt, "HasVisualFire"),
        ),
        on_ground: read_flag(nbt, "OnGround"),
        save_data: EntityBaseSaveData {
            air_supply: read_int(nbt, "Air").unwrap_or(DEFAULT_MAX_AIR_SUPPLY),
            portal_cooldown: read_int(nbt, "PortalCooldown").unwrap_or(0),
            no_gravity: read_flag(nbt, "NoGravity"),
            invulnerable: read_flag(nbt, "Invulnerable"),
            custom_name: read_custom_name(nbt),
            custom_name_visible: read_flag(nbt, "CustomNameVisible"),
            silent: read_flag(nbt, "Silent"),
            glowing: read_flag(nbt, "Glowing"),
            tags: read_tags(nbt),
            custom_data: nbt
                .compound("data")
                .map_or_else(NbtCompound::new, |compound| compound.to_owned()),
        },
        remainder,
        passengers,
    })
}

/// Reads `Rotation`, which vanilla stores as a two-float list.
fn read_rotation(nbt: &BorrowedNbtCompoundView<'_, '_>) -> (f32, f32) {
    let Some(rotation) = nbt.list("Rotation").and_then(|list| list.floats()) else {
        return (0.0, 0.0);
    };
    if rotation.len() < 2 {
        return (0.0, 0.0);
    }
    (rotation[0], rotation[1])
}

/// Reads a three-double list, or `None` when it is absent or the wrong shape.
fn read_vec3d(nbt: &BorrowedNbtCompoundView<'_, '_>, field: &str) -> Option<DVec3> {
    let values = nbt.list(field).and_then(|list| list.doubles())?;
    if values.len() < 3 {
        return None;
    }
    Some(DVec3::new(values[0], values[1], values[2]))
}

/// Reads an integer written as any of vanilla's narrower integer tags.
fn read_int(nbt: &BorrowedNbtCompoundView<'_, '_>, field: &str) -> Option<i32> {
    nbt.int(field)
        .or_else(|| nbt.short(field).map(i32::from))
        .or_else(|| nbt.byte(field).map(i32::from))
}

/// Reads a vanilla boolean, which is a byte that is not zero.
fn read_flag(nbt: &BorrowedNbtCompoundView<'_, '_>, field: &str) -> bool {
    nbt.byte(field).is_some_and(|value| value != 0)
}

/// Reads `CustomName`, which is a serialized text component.
fn read_custom_name(nbt: &BorrowedNbtCompoundView<'_, '_>) -> Option<TextComponent> {
    let tag = nbt.get("CustomName")?;
    TextComponent::from_nbt(&tag.to_owned())
}

/// Reads `Tags`, capped the way vanilla caps it.
fn read_tags(nbt: &BorrowedNbtCompoundView<'_, '_>) -> BTreeSet<String> {
    nbt.list("Tags")
        .and_then(|list| list.strings())
        .map(|tags| {
            tags.iter()
                .take(MAX_ENTITY_TAGS)
                .map(|tag| tag.to_str().into_owned())
                .collect()
        })
        .unwrap_or_default()
}
