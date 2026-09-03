//! Reading one entity out of a vanilla save compound.
//!
//! Vanilla parity: the base-field half of `Entity.load`, which runs before
//! `readAdditionalSaveData` and owns every key an entity has regardless of its
//! type. Foton keeps those decoded fields in [`EntityBaseSaveData`] and friends
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

use foton_registry::{REGISTRY, RegistryExt as _, entity_type::EntityTypeRef};
use foton_utils::{Identifier, UuidExt as _};
use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;
use text_components::TextComponent;
use uuid::Uuid;

use super::{
    DEFAULT_MAX_AIR_SUPPLY, Entity, EntityBaseSaveData, EntityFireFreezeState, MAX_ENTITY_TAGS,
};

/// Vanilla's horizontal position clamp in `Entity.load`.
const MAX_LOADED_HORIZONTAL_POSITION: f64 = 3.000_051_2E7;

/// Vanilla's vertical position clamp in `Entity.load`.
const MAX_LOADED_VERTICAL_POSITION: f64 = 2.0E7;

/// Vanilla's per-axis velocity cutoff in `Entity.load`.
const MAX_LOADED_MOTION: f64 = 10.0;

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
            persistent: !nbt.contains("PersistenceRequired")
                || read_flag(nbt, "PersistenceRequired"),
        },
        remainder,
        passengers,
    })
}

/// Applies a vanilla entity compound to an entity that is already in a world.
///
/// Vanilla parity: `Entity.load`, which is what `EntityDataAccessor.setData`
/// runs behind `/data modify entity` and `/execute store ... entity`. Three
/// differences, all forced by the entity being alive rather than under
/// construction:
///
/// - `UUID` is not read. Vanilla reads it here and then `setData` puts the old
///   one straight back, because the world indexes a live entity by it. Foton
///   fixes the UUID at construction, so the key is ignored for the same
///   observable result.
/// - The finite checks run *before* anything is written. Vanilla writes the
///   position and rotation first and throws a crash report if they turn out
///   not to be finite, which leaves the entity half-loaded. A command has no
///   crash-report escape, so a bad compound is refused whole instead.
/// - `Passengers` is not read. Vanilla's recursive passenger load lives in
///   `EntityType.loadEntityRecursive`, not in `Entity.load`, and neither the
///   data command nor `execute store` reaches it.
///
/// The type's own reader sees the whole compound, the way vanilla's
/// `readAdditionalSaveData` does. Only [`read_entity_nbt`] splits the base
/// fields off, because there they have already been consumed into an
/// `EntityBaseLoad`.
pub(crate) fn load_live_entity(entity: &dyn Entity, nbt: BorrowedNbtCompoundView<'_, '_>) {
    let position = read_vec3d(&nbt, "Pos").unwrap_or(DVec3::ZERO);
    let position = DVec3::new(
        position.x.clamp(
            -MAX_LOADED_HORIZONTAL_POSITION,
            MAX_LOADED_HORIZONTAL_POSITION,
        ),
        position
            .y
            .clamp(-MAX_LOADED_VERTICAL_POSITION, MAX_LOADED_VERTICAL_POSITION),
        position.z.clamp(
            -MAX_LOADED_HORIZONTAL_POSITION,
            MAX_LOADED_HORIZONTAL_POSITION,
        ),
    );
    let rotation = read_rotation(&nbt);
    if !position.is_finite() || !rotation.0.is_finite() || !rotation.1.is_finite() {
        tracing::warn!(
            entity = %entity.entity_type().key,
            "refused an entity compound with a non-finite position or rotation"
        );
        return;
    }

    let base = entity.base();
    let velocity = read_vec3d(&nbt, "Motion").unwrap_or(DVec3::ZERO);
    entity.set_velocity(DVec3::new(
        clamp_loaded_motion(velocity.x),
        clamp_loaded_motion(velocity.y),
        clamp_loaded_motion(velocity.z),
    ));
    entity.mark_velocity_sync();

    if let Err(error) = entity.try_set_position(position) {
        // Vanilla's `setPosRaw` cannot fail. Foton's commits through the world
        // entity manager, and a rejected move must not take the rest of the
        // compound down with it.
        tracing::warn!(%error, "entity load could not commit the loaded position");
    }
    entity.set_rotation(rotation);
    base.set_old_position_to_current();
    base.set_old_rotation_to_current();

    base.set_fall_distance(nbt.double("fall_distance").unwrap_or(0.0));
    // Vanilla assigns `remainingFireTicks` directly here rather than going
    // through `setRemainingFireTicks`, so the player cap does not apply.
    base.set_remaining_fire_ticks(read_int(&nbt, "Fire").unwrap_or(0));
    entity.set_air_supply(read_int(&nbt, "Air").unwrap_or_else(|| entity.max_air_supply()));
    base.set_on_ground(read_flag(&nbt, "OnGround"));
    base.set_invulnerable(read_flag(&nbt, "Invulnerable"));
    base.set_portal_cooldown(read_int(&nbt, "PortalCooldown").unwrap_or(0));

    // `set_custom_name` is the virtual one: the wither renames its boss bar
    // from it and a vindicator becomes Johnny.
    entity.set_custom_name(read_custom_name(&nbt));
    entity.set_custom_name_visible(read_flag(&nbt, "CustomNameVisible"));
    entity.set_silent(read_flag(&nbt, "Silent"));
    entity.set_no_gravity(read_flag(&nbt, "NoGravity"));
    entity.set_glowing_tag(read_flag(&nbt, "Glowing"));
    base.set_ticks_frozen(read_int(&nbt, "TicksFrozen").unwrap_or(0));
    base.set_visual_fire(read_flag(&nbt, "HasVisualFire"));
    entity.set_custom_data(
        nbt.compound("data")
            .map_or_else(NbtCompound::new, |compound| compound.to_owned()),
    );
    base.set_tags(read_tags(&nbt));

    load_entity_save_data(entity, nbt);
}

/// Runs the save-data half of an entity load and resynchronizes the base
/// fields it may have moved.
///
/// Vanilla parity: `Entity.load`'s `readAdditionalSaveData` call, which
/// dispatches through the `LivingEntity` override before reaching the type's
/// own. Foton keeps the two halves as separate methods, so the order is spelt
/// out here instead. Shared by the load factories, which have already restored
/// the base fields through the constructor, and by [`load_live_entity`].
pub(crate) fn load_entity_save_data(entity: &dyn Entity, nbt: BorrowedNbtCompoundView<'_, '_>) {
    let yaw = entity.rotation().0;
    if let Some(living) = entity.as_living_entity() {
        living.set_y_head_rot(yaw);
        living.set_y_body_rot(yaw);
        living.load_living(nbt);
    }

    entity.load_additional(nbt);
    entity.sync_base_entity_data();
}

/// Drops a velocity component vanilla considers too large to have been saved.
fn clamp_loaded_motion(component: f64) -> f64 {
    if component.abs() > MAX_LOADED_MOTION {
        0.0
    } else {
        component
    }
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
