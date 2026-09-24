//! Entity state a plugin reads and writes through `org.bukkit.entity`.

use std::ffi::c_void;
use std::ptr::null_mut;

use foton_core::entity::SharedEntity;
use foton_core::entity::damage::DamageSource;
use foton_core::entity::entities::decoration::MannequinEntity;
use foton_core::entity::entities::mobs::hostile::ShulkerEntity;
use foton_core::entity::entities::objects::display_ui::{GlowItemFrameEntity, ItemFrameEntity};
use foton_core::entity::entities::objects::items::ItemEntity;
use foton_core::entity::entities::objects::projectiles::FireworkRocketEntity;
use foton_core::entity::entities::objects::vehicles::{ChestBoatEntity, ChestRaftEntity};
use foton_core::inventory::container::{Container as _, SimpleContainer};
use foton_registry::DyeColor;
use foton_registry::resolvable_profile::{
    PartialProfile, PlayerSkinPatch, ProfileProperty, ResolvableProfile, ResolvableProfileContents,
    StoredGameProfile,
};
use foton_registry::vanilla_damage_types;
use foton_utils::Direction;
use foton_utils::Downcast as _;
use jni::JNIEnv;
use jni::objects::{JClass, JObjectArray, JString};
use jni::sys::{jboolean, jdouble, jdoubleArray, jfloat, jint, jobjectArray, jstring};
use uuid::Uuid;

use super::support::{component, doubles, entity, method, text};
use super::{
    describe_slot, equipment_slot_from_index, parse_slot, read_string_array, string_array, to_java,
};

/// Runs `f` on the container an entity holds: a mob's carried inventory
/// (villager, allay, piglin) or a chest boat's chest.
fn with_container<R>(
    entity: &SharedEntity,
    f: impl FnOnce(&mut SimpleContainer) -> R,
) -> Option<R> {
    if let Some(carrier) = entity.as_inventory_carrier() {
        return Some(f(&mut carrier.carried_inventory().lock()));
    }
    if let Some(boat) = entity.as_ref().downcast_ref::<ChestBoatEntity>() {
        return Some(f(&mut boat.container().lock()));
    }
    if let Some(raft) = entity.as_ref().downcast_ref::<ChestRaftEntity>() {
        return Some(f(&mut raft.container().lock()));
    }
    None
}

/// The number of slots the entity's container has, or -1 when it has none.
extern "system" fn entity_container_size(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jint {
    entity(&mut env, &uuid)
        .and_then(|(_, entity)| with_container(&entity, |container| container.get_container_size()))
        .and_then(|size| i32::try_from(size).ok())
        .unwrap_or(-1)
}

extern "system" fn entity_container_slot(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    slot: jint,
) -> jstring {
    let described = usize::try_from(slot).ok().and_then(|slot| {
        let (_, entity) = entity(&mut env, &uuid)?;
        with_container(&entity, |container| {
            (slot < container.get_container_size()).then(|| describe_slot(container.get_item(slot)))
        })
        .flatten()
    });
    to_java(&mut env, described)
}

extern "system" fn set_entity_container_slot(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    slot: jint,
    item: JString<'_>,
) {
    let Some(stack) = text(&mut env, &item).and_then(|value| parse_slot(&value)) else {
        return;
    };
    let Ok(slot) = usize::try_from(slot) else {
        return;
    };
    let Some((_, entity)) = entity(&mut env, &uuid) else {
        return;
    };
    with_container(&entity, |container| {
        if slot < container.get_container_size() {
            container.set_item(slot, stack);
            container.set_changed();
        }
    });
}

/// The entity's scoreboard tags, the ones `/tag` lists and saves with it.
extern "system" fn entity_tags(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jobjectArray {
    let Some((_, entity)) = entity(&mut env, &uuid) else {
        return null_mut();
    };
    string_array(&mut env, &entity.tags())
}

/// Adds a scoreboard tag; `false` when it was already there or the entity
/// holds vanilla's maximum.
extern "system" fn add_entity_tag(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    tag: JString<'_>,
) -> jboolean {
    let Some(tag) = text(&mut env, &tag) else {
        return 0;
    };
    let Some((_, entity)) = entity(&mut env, &uuid) else {
        return 0;
    };
    jboolean::from(entity.add_tag(tag))
}

extern "system" fn remove_entity_tag(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    tag: JString<'_>,
) -> jboolean {
    let Some(tag) = text(&mut env, &tag) else {
        return 0;
    };
    let Some((_, entity)) = entity(&mut env, &uuid) else {
        return 0;
    };
    jboolean::from(entity.remove_tag(&tag))
}

/// Sets the custom name from a component's JSON; null clears it.
extern "system" fn set_entity_custom_name_component(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    json: JString<'_>,
) {
    let name = component(&mut env, &json);
    if name.is_none() && !json.is_null() {
        return;
    }
    let Some((_, entity)) = entity(&mut env, &uuid) else {
        return;
    };
    entity.set_custom_name(name);
}

/// Bukkit's gravity is vanilla's `NoGravity` tag, inverted.
extern "system" fn entity_gravity(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jboolean {
    entity(&mut env, &uuid).map_or(1, |(_, entity)| jboolean::from(!entity.is_no_gravity()))
}

extern "system" fn set_entity_gravity(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    gravity: jboolean,
) {
    if let Some((_, entity)) = entity(&mut env, &uuid) {
        entity.set_no_gravity(gravity == 0);
    }
}

extern "system" fn entity_silent(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jboolean {
    entity(&mut env, &uuid).map_or(0, |(_, entity)| jboolean::from(entity.is_silent()))
}

extern "system" fn set_entity_silent(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    silent: jboolean,
) {
    if let Some((_, entity)) = entity(&mut env, &uuid) {
        entity.set_silent(silent != 0);
    }
}

/// CraftEntity.setRotation: the body and, for a living entity, the head turn
/// together. The Java side has already normalised both angles.
extern "system" fn set_entity_rotation(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    yaw: jfloat,
    pitch: jfloat,
) {
    let Some((_, entity)) = entity(&mut env, &uuid) else {
        return;
    };
    entity.set_rotation((yaw, pitch));
    if let Some(living) = entity.as_living_entity() {
        living.set_y_head_rot(yaw);
    }
}

/// The entity's pose as vanilla's network id, which Bukkit's `Pose` ordinal is.
extern "system" fn entity_pose(mut env: JNIEnv<'_>, _class: JClass<'_>, uuid: JString<'_>) -> jint {
    entity(&mut env, &uuid).map_or(0, |(_, entity)| entity.pose() as jint)
}

/// `LivingEntity.damage`, through the entity's own `hurt`: armour, effects,
/// invulnerability frames and death all apply. The damage type is Paper's
/// choice -- a player's attack, a mob's, or generic damage.
extern "system" fn damage_entity(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    amount: jdouble,
    source: JString<'_>,
) {
    let Some((world, entity)) = entity(&mut env, &uuid) else {
        return;
    };
    if entity.as_living_entity().is_none() {
        return;
    }
    let attacker = super::support::entity(&mut env, &source).map(|(_, attacker)| attacker);
    let damage = match &attacker {
        Some(attacker) if attacker.as_player().is_some() => {
            DamageSource::environment(&vanilla_damage_types::PLAYER_ATTACK)
        }
        Some(attacker) if attacker.as_living_entity().is_some() => {
            DamageSource::environment(&vanilla_damage_types::MOB_ATTACK)
        }
        _ => DamageSource::environment(&vanilla_damage_types::GENERIC),
    };
    let damage = match &attacker {
        Some(attacker) => damage
            .with_causing_entity(attacker.id())
            .with_direct_entity(attacker.id()),
        None => damage,
    };
    let _ = entity.hurt(&world, &damage, amount as f32);
}

/// Whether a mob thinks at all, vanilla's `NoAI` inverted. An entity that is
/// not a mob has no AI to switch.
extern "system" fn entity_has_ai(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jboolean {
    entity(&mut env, &uuid)
        .and_then(|(_, entity)| entity.as_mob().map(|mob| jboolean::from(!mob.is_no_ai())))
        .unwrap_or(0)
}

extern "system" fn set_entity_ai(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    ai: jboolean,
) {
    if let Some((_, entity)) = entity(&mut env, &uuid)
        && let Some(mob) = entity.as_mob()
    {
        mob.set_no_ai(ai == 0);
    }
}

extern "system" fn mob_aware(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jboolean {
    entity(&mut env, &uuid)
        .and_then(|(_, entity)| entity.as_mob().map(|mob| jboolean::from(mob.is_aware())))
        .unwrap_or(0)
}

extern "system" fn set_mob_aware(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    aware: jboolean,
) {
    if let Some((_, entity)) = entity(&mut env, &uuid)
        && let Some(mob) = entity.as_mob()
    {
        mob.set_aware(aware != 0);
    }
}

extern "system" fn entity_collidable(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jboolean {
    entity(&mut env, &uuid)
        .and_then(|(_, entity)| {
            entity
                .as_living_entity()
                .map(|living| jboolean::from(living.living_base().collides()))
        })
        .unwrap_or(0)
}

extern "system" fn set_entity_collidable(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    collidable: jboolean,
) {
    if let Some((_, entity)) = entity(&mut env, &uuid)
        && let Some(living) = entity.as_living_entity()
    {
        living.living_base().set_collides(collidable != 0);
    }
}

/// Reads or writes a dropped item, if `uuid` still names one.
fn with_item<T>(
    env: &mut JNIEnv<'_>,
    uuid: &JString<'_>,
    read: impl FnOnce(&ItemEntity) -> T,
) -> Option<T> {
    let (_, entity) = entity(env, uuid)?;
    entity.as_ref().downcast_ref::<ItemEntity>().map(read)
}

extern "system" fn item_thrower(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jstring {
    let thrower = with_item(&mut env, &uuid, ItemEntity::get_thrower).flatten();
    to_java(&mut env, thrower.map(|id| id.to_string()))
}

extern "system" fn item_owner(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jstring {
    let owner = with_item(&mut env, &uuid, ItemEntity::get_owner).flatten();
    to_java(&mut env, owner.map(|id| id.to_string()))
}

/// The only player allowed to pick the item up, vanilla's `Owner`; null frees it.
extern "system" fn set_item_owner(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    owner: JString<'_>,
) {
    let owner = text(&mut env, &owner).and_then(|value| Uuid::parse_str(&value).ok());
    with_item(&mut env, &uuid, |item| item.set_owner(owner));
}

extern "system" fn item_pickup_delay(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jint {
    with_item(&mut env, &uuid, ItemEntity::get_pickup_delay).unwrap_or(0)
}

/// CraftItem caps the delay at `Short.MAX_VALUE`, the width vanilla saves it in.
extern "system" fn set_item_pickup_delay(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    delay: jint,
) {
    let delay = delay.min(i32::from(i16::MAX));
    with_item(&mut env, &uuid, |item| item.set_pickup_delay(delay));
}

/// Puts an item in a frame, with or without the placing sound.
extern "system" fn set_item_frame_item(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    item: JString<'_>,
    play_sound: jboolean,
) {
    let Some(stack) = text(&mut env, &item).and_then(|value| parse_slot(&value)) else {
        return;
    };
    let Some((_, entity)) = entity(&mut env, &uuid) else {
        return;
    };
    let sound = play_sound != 0 && !stack.is_empty();
    if let Some(frame) = entity.as_ref().downcast_ref::<ItemFrameEntity>() {
        frame.set_item(stack);
        if sound {
            frame.play_add_item_sound();
        }
    } else if let Some(frame) = entity.as_ref().downcast_ref::<GlowItemFrameEntity>() {
        frame.set_item(stack);
        if sound {
            frame.play_add_item_sound();
        }
    }
}

/// The living entity a firework rocket is propelling, such as a gliding player.
extern "system" fn firework_attached_to(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jstring {
    let attached = entity(&mut env, &uuid).and_then(|(world, entity)| {
        let rocket = entity.as_ref().downcast_ref::<FireworkRocketEntity>()?;
        rocket
            .attached_entity(&world)
            .map(|attached| attached.uuid().to_string())
    });
    to_java(&mut env, attached)
}

/// The item in one equipment slot, by Bukkit `EquipmentSlot` ordinal -- the
/// same slots vanilla's `EquipmentSlot` has, in the same order.
extern "system" fn entity_equipment_item(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    slot: jint,
) -> jstring {
    let described = equipment_slot_from_index(slot).and_then(|slot| {
        let (_, entity) = entity(&mut env, &uuid)?;
        Some(describe_slot(
            &entity.as_living_entity()?.get_item_by_slot(slot),
        ))
    });
    to_java(&mut env, described)
}

extern "system" fn set_entity_equipment_item(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    slot: jint,
    item: JString<'_>,
) {
    let Some(slot) = equipment_slot_from_index(slot) else {
        return;
    };
    let Some(stack) = text(&mut env, &item).and_then(|value| parse_slot(&value)) else {
        return;
    };
    if let Some((_, entity)) = entity(&mut env, &uuid)
        && let Some(living) = entity.as_living_entity()
    {
        living.set_item_slot(slot, stack);
    }
}

/// A mannequin's profile as `[id, name, (property name, value, signature)...]`,
/// empty strings standing for absent values.
extern "system" fn mannequin_profile(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jobjectArray {
    let Some((_, entity)) = entity(&mut env, &uuid) else {
        return null_mut();
    };
    let Some(mannequin) = entity.as_ref().downcast_ref::<MannequinEntity>() else {
        return null_mut();
    };
    let profile = mannequin.profile();
    let (id, name, properties) = match profile.contents() {
        ResolvableProfileContents::DynamicName(name) => (None, Some(name.clone()), &[][..]),
        ResolvableProfileContents::DynamicId(id) => (Some(*id), None, &[][..]),
        ResolvableProfileContents::StaticFull(profile) => (
            Some(profile.id()),
            Some(profile.name().to_owned()),
            profile.properties(),
        ),
        ResolvableProfileContents::StaticPartial(profile) => (
            profile.id(),
            profile.name().map(ToOwned::to_owned),
            profile.properties(),
        ),
    };
    let mut values = vec![
        id.map(|id| id.to_string()).unwrap_or_default(),
        name.unwrap_or_default(),
    ];
    for property in properties {
        values.push(property.name().to_owned());
        values.push(property.value().to_owned());
        values.push(property.signature().unwrap_or_default().to_owned());
    }
    string_array(&mut env, &values)
}

/// A full profile when both id and name are given, a partial one otherwise --
/// the split vanilla's profile component makes when it reads one.
extern "system" fn set_mannequin_profile(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    profile_id: JString<'_>,
    name: JString<'_>,
    properties: JObjectArray<'_>,
) {
    let id = text(&mut env, &profile_id).and_then(|value| Uuid::parse_str(&value).ok());
    let name = text(&mut env, &name);
    let Some(raw) = read_string_array(&mut env, &properties) else {
        return;
    };
    let Some(properties) = raw
        .chunks_exact(3)
        .map(|property| {
            let signature = (!property[2].is_empty()).then(|| property[2].clone());
            ProfileProperty::new(property[0].clone(), property[1].clone(), signature).ok()
        })
        .collect::<Option<Vec<_>>>()
    else {
        return;
    };
    let profile = match (id, name) {
        (Some(id), Some(name)) => StoredGameProfile::new(id, name, properties)
            .ok()
            .map(|profile| ResolvableProfile::static_full(profile, PlayerSkinPatch::default())),
        (id, name) => PartialProfile::new(name, id, properties)
            .ok()
            .map(|profile| ResolvableProfile::static_partial(profile, PlayerSkinPatch::default())),
    };
    let Some(profile) = profile else {
        return;
    };
    if let Some((_, entity)) = entity(&mut env, &uuid)
        && let Some(mannequin) = entity.as_ref().downcast_ref::<MannequinEntity>()
    {
        mannequin.set_profile(profile);
    }
}

extern "system" fn mannequin_immovable(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jboolean {
    entity(&mut env, &uuid)
        .and_then(|(_, entity)| {
            let mannequin = entity.as_ref().downcast_ref::<MannequinEntity>()?;
            Some(jboolean::from(mannequin.is_immovable()))
        })
        .unwrap_or(0)
}

extern "system" fn set_mannequin_immovable(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    immovable: jboolean,
) {
    if let Some((_, entity)) = entity(&mut env, &uuid)
        && let Some(mannequin) = entity.as_ref().downcast_ref::<MannequinEntity>()
    {
        mannequin.set_immovable(immovable != 0);
    }
}

/// The line under the name, from a component's JSON; null hides it.
extern "system" fn set_mannequin_description(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    json: JString<'_>,
) {
    let description = component(&mut env, &json);
    if description.is_none() && !json.is_null() {
        return;
    }
    let Some((_, entity)) = entity(&mut env, &uuid) else {
        return;
    };
    let Some(mannequin) = entity.as_ref().downcast_ref::<MannequinEntity>() else {
        return;
    };
    match description {
        Some(description) => {
            mannequin.set_description(description);
            mannequin.set_hide_description(false);
        }
        None => mannequin.set_hide_description(true),
    }
}

fn with_shulker<R>(
    env: &mut JNIEnv<'_>,
    uuid: &JString<'_>,
    f: impl FnOnce(&ShulkerEntity) -> R,
) -> Option<R> {
    let (_, entity) = entity(env, uuid)?;
    entity.as_ref().downcast_ref::<ShulkerEntity>().map(f)
}

/// `{raw peek (0-100), 0, dye colour id or -1}`.
extern "system" fn shulker_state(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jdoubleArray {
    let state = with_shulker(&mut env, &uuid, |shulker| {
        [
            f64::from(shulker.raw_peek_amount()),
            0.0,
            f64::from(shulker.color().map_or(-1, |color| color.id())),
        ]
    });
    doubles(&mut env, state.as_ref().map(<[f64; 3]>::as_slice))
}

/// The face the shulker clings to, by Bukkit `BlockFace` name.
extern "system" fn shulker_attached_face(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jstring {
    let face = with_shulker(&mut env, &uuid, |shulker| {
        match shulker.attach_face() {
            Direction::Down => "DOWN",
            Direction::Up => "UP",
            Direction::North => "NORTH",
            Direction::South => "SOUTH",
            Direction::West => "WEST",
            Direction::East => "EAST",
        }
        .to_owned()
    });
    to_java(&mut env, face)
}

extern "system" fn set_shulker_attached_face(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    face: JString<'_>,
) {
    let direction = match text(&mut env, &face).as_deref() {
        Some("DOWN") => Direction::Down,
        Some("UP") => Direction::Up,
        Some("NORTH") => Direction::North,
        Some("SOUTH") => Direction::South,
        Some("WEST") => Direction::West,
        Some("EAST") => Direction::East,
        _ => return,
    };
    with_shulker(&mut env, &uuid, |shulker| {
        shulker.set_attach_face(direction)
    });
}

/// Opens or closes the shell, with vanilla's armour change and sound.
extern "system" fn set_shulker_peek(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    peek: jint,
) {
    with_shulker(&mut env, &uuid, |shulker| {
        shulker.set_raw_peek_amount(peek.clamp(0, 100))
    });
}

/// A dye colour id, or -1 for the undyed shell.
extern "system" fn set_shulker_color(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    color: jint,
) {
    let color = DyeColor::VALUES
        .get(usize::try_from(color).unwrap_or(usize::MAX))
        .copied();
    with_shulker(&mut env, &uuid, |shulker| shulker.set_color(color));
}

pub(super) fn bindings() -> Vec<jni::NativeMethod> {
    vec![
        method(
            "entityContainerSize",
            "(Ljava/lang/String;)I",
            entity_container_size as *mut c_void,
        ),
        method(
            "entityContainerSlot",
            "(Ljava/lang/String;I)Ljava/lang/String;",
            entity_container_slot as *mut c_void,
        ),
        method(
            "setEntityContainerSlot",
            "(Ljava/lang/String;ILjava/lang/String;)V",
            set_entity_container_slot as *mut c_void,
        ),
        method(
            "mannequinProfile",
            "(Ljava/lang/String;)[Ljava/lang/String;",
            mannequin_profile as *mut c_void,
        ),
        method(
            "setMannequinProfile",
            "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;[Ljava/lang/String;)V",
            set_mannequin_profile as *mut c_void,
        ),
        method(
            "mannequinImmovable",
            "(Ljava/lang/String;)Z",
            mannequin_immovable as *mut c_void,
        ),
        method(
            "setMannequinImmovable",
            "(Ljava/lang/String;Z)V",
            set_mannequin_immovable as *mut c_void,
        ),
        method(
            "setMannequinDescription",
            "(Ljava/lang/String;Ljava/lang/String;)V",
            set_mannequin_description as *mut c_void,
        ),
        method(
            "shulkerState",
            "(Ljava/lang/String;)[D",
            shulker_state as *mut c_void,
        ),
        method(
            "shulkerAttachedFace",
            "(Ljava/lang/String;)Ljava/lang/String;",
            shulker_attached_face as *mut c_void,
        ),
        method(
            "setShulkerAttachedFace",
            "(Ljava/lang/String;Ljava/lang/String;)V",
            set_shulker_attached_face as *mut c_void,
        ),
        method(
            "setShulkerPeek",
            "(Ljava/lang/String;I)V",
            set_shulker_peek as *mut c_void,
        ),
        method(
            "setShulkerColor",
            "(Ljava/lang/String;I)V",
            set_shulker_color as *mut c_void,
        ),
        method(
            "entityTags",
            "(Ljava/lang/String;)[Ljava/lang/String;",
            entity_tags as *mut c_void,
        ),
        method(
            "addEntityTag",
            "(Ljava/lang/String;Ljava/lang/String;)Z",
            add_entity_tag as *mut c_void,
        ),
        method(
            "removeEntityTag",
            "(Ljava/lang/String;Ljava/lang/String;)Z",
            remove_entity_tag as *mut c_void,
        ),
        method(
            "setEntityCustomNameComponent",
            "(Ljava/lang/String;Ljava/lang/String;)V",
            set_entity_custom_name_component as *mut c_void,
        ),
        method(
            "entityGravity",
            "(Ljava/lang/String;)Z",
            entity_gravity as *mut c_void,
        ),
        method(
            "setEntityGravity",
            "(Ljava/lang/String;Z)V",
            set_entity_gravity as *mut c_void,
        ),
        method(
            "entitySilent",
            "(Ljava/lang/String;)Z",
            entity_silent as *mut c_void,
        ),
        method(
            "setEntitySilent",
            "(Ljava/lang/String;Z)V",
            set_entity_silent as *mut c_void,
        ),
        method(
            "setEntityRotation",
            "(Ljava/lang/String;FF)V",
            set_entity_rotation as *mut c_void,
        ),
        method(
            "entityPose",
            "(Ljava/lang/String;)I",
            entity_pose as *mut c_void,
        ),
        method(
            "damageEntity",
            "(Ljava/lang/String;DLjava/lang/String;)V",
            damage_entity as *mut c_void,
        ),
        method(
            "entityHasAi",
            "(Ljava/lang/String;)Z",
            entity_has_ai as *mut c_void,
        ),
        method(
            "setEntityAi",
            "(Ljava/lang/String;Z)V",
            set_entity_ai as *mut c_void,
        ),
        method(
            "mobAware",
            "(Ljava/lang/String;)Z",
            mob_aware as *mut c_void,
        ),
        method(
            "setMobAware",
            "(Ljava/lang/String;Z)V",
            set_mob_aware as *mut c_void,
        ),
        method(
            "entityCollidable",
            "(Ljava/lang/String;)Z",
            entity_collidable as *mut c_void,
        ),
        method(
            "setEntityCollidable",
            "(Ljava/lang/String;Z)V",
            set_entity_collidable as *mut c_void,
        ),
        method(
            "itemThrower",
            "(Ljava/lang/String;)Ljava/lang/String;",
            item_thrower as *mut c_void,
        ),
        method(
            "itemOwner",
            "(Ljava/lang/String;)Ljava/lang/String;",
            item_owner as *mut c_void,
        ),
        method(
            "setItemOwner",
            "(Ljava/lang/String;Ljava/lang/String;)V",
            set_item_owner as *mut c_void,
        ),
        method(
            "itemPickupDelay",
            "(Ljava/lang/String;)I",
            item_pickup_delay as *mut c_void,
        ),
        method(
            "setItemPickupDelay",
            "(Ljava/lang/String;I)V",
            set_item_pickup_delay as *mut c_void,
        ),
        method(
            "setItemFrameItem",
            "(Ljava/lang/String;Ljava/lang/String;Z)V",
            set_item_frame_item as *mut c_void,
        ),
        method(
            "fireworkAttachedTo",
            "(Ljava/lang/String;)Ljava/lang/String;",
            firework_attached_to as *mut c_void,
        ),
        method(
            "entityEquipmentItem",
            "(Ljava/lang/String;I)Ljava/lang/String;",
            entity_equipment_item as *mut c_void,
        ),
        method(
            "setEntityEquipmentItem",
            "(Ljava/lang/String;ILjava/lang/String;)V",
            set_entity_equipment_item as *mut c_void,
        ),
    ]
}
