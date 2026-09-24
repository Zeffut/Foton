//! Entity state a plugin reads and writes through `org.bukkit.entity`.

use std::ffi::c_void;
use std::ptr::null_mut;

use foton_core::entity::damage::DamageSource;
use foton_core::entity::entities::objects::display_ui::{GlowItemFrameEntity, ItemFrameEntity};
use foton_core::entity::entities::objects::items::ItemEntity;
use foton_core::entity::entities::objects::projectiles::FireworkRocketEntity;
use foton_core::inventory::container::Container as _;
use foton_registry::vanilla_damage_types;
use foton_utils::Downcast as _;
use jni::JNIEnv;
use jni::objects::{JClass, JString};
use jni::sys::{jboolean, jdouble, jfloat, jint, jobjectArray, jstring};
use uuid::Uuid;

use super::support::{component, entity, method, text};
use super::{describe_slot, equipment_slot_from_index, parse_slot, string_array, to_java};

/// The number of slots a mob carries, or -1 when it carries none.
extern "system" fn carried_inventory_size(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jint {
    let Some((_, entity)) = entity(&mut env, &uuid) else {
        return -1;
    };
    entity.as_inventory_carrier().map_or(-1, |carrier| {
        i32::try_from(carrier.carried_inventory().lock().get_container_size()).unwrap_or(-1)
    })
}

extern "system" fn carried_inventory_slot(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    slot: jint,
) -> jstring {
    let Some((_, entity)) = entity(&mut env, &uuid) else {
        return null_mut();
    };
    let described = entity.as_inventory_carrier().and_then(|carrier| {
        let inventory = carrier.carried_inventory().lock();
        let slot = usize::try_from(slot).ok()?;
        (slot < inventory.get_container_size()).then(|| describe_slot(inventory.get_item(slot)))
    });
    to_java(&mut env, described)
}

extern "system" fn set_carried_inventory_slot(
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
    let Some(carrier) = entity.as_inventory_carrier() else {
        return;
    };
    let mut inventory = carrier.carried_inventory().lock();
    if slot < inventory.get_container_size() {
        inventory.set_item(slot, stack);
    }
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

pub(super) fn bindings() -> Vec<jni::NativeMethod> {
    vec![
        method(
            "carriedInventorySize",
            "(Ljava/lang/String;)I",
            carried_inventory_size as *mut c_void,
        ),
        method(
            "carriedInventorySlot",
            "(Ljava/lang/String;I)Ljava/lang/String;",
            carried_inventory_slot as *mut c_void,
        ),
        method(
            "setCarriedInventorySlot",
            "(Ljava/lang/String;ILjava/lang/String;)V",
            set_carried_inventory_slot as *mut c_void,
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
