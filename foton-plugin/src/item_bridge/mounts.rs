//! Owning mount-equipment transfers, including full-batch admission before callbacks.

use foton_core::entity::{
    Entity, LivingEntity,
    entities::mobs::passive::{HorseEntity, NautilusEntity, ZombieNautilusEntity},
};
use foton_registry::equipment::EquipmentSlot;
use foton_utils::Downcast as _;
use jni::{
    JNIEnv,
    objects::{JClass, JObject, JObjectArray, JString},
    sys::{jint, jobject},
};

use super::{ItemBridgeError, entity::identity, mutation, store, transfer};

fn mount(entity: &dyn Entity, horse_only: bool) -> Option<&dyn LivingEntity> {
    if let Some(horse) = entity.downcast_ref::<HorseEntity>() {
        return Some(horse);
    }
    if horse_only {
        return None;
    }
    if let Some(nautilus) = entity.downcast_ref::<NautilusEntity>() {
        return Some(nautilus);
    }
    entity
        .downcast_ref::<ZombieNautilusEntity>()
        .map(|entity| entity as &dyn LivingEntity)
}

fn slot(value: jint) -> Result<EquipmentSlot, ItemBridgeError> {
    match value {
        0 => Ok(EquipmentSlot::Saddle),
        1 => Ok(EquipmentSlot::Body),
        _ => Err(ItemBridgeError::InvalidEdit(
            "mount inventory slot out of range",
        )),
    }
}

fn read(
    env: &mut JNIEnv<'_>,
    uuid: &JString<'_>,
    index: jint,
    horse_only: bool,
) -> Result<jobject, ItemBridgeError> {
    let slot = slot(index)?;
    let id = identity(env, uuid)?;
    let Some((_, entity)) = crate::natives::entity_by_uuid(&id) else {
        return Ok(std::ptr::null_mut());
    };
    let Some(mount) = mount(entity.as_ref(), horse_only) else {
        return Ok(std::ptr::null_mut());
    };
    let store = store()?;
    let mut capture = Ok(None);
    mount.with_equipment_slot(slot, &mut |item| {
        if !item.is_empty() {
            capture = store
                .capture(item)
                .map(Some)
                .map_err(ItemBridgeError::native_state);
        }
    });
    match capture? {
        Some(capture) => transfer::from_capture(env, capture).map(JObject::into_raw),
        None => Ok(std::ptr::null_mut()),
    }
}

fn write(
    env: &mut JNIEnv<'_>,
    uuid: &JString<'_>,
    index: jint,
    value: &JObject<'_>,
    horse_only: bool,
) -> Result<(), ItemBridgeError> {
    let candidate = mutation::materialize(env, value)?;
    let slot = slot(index)?;
    let id = identity(env, uuid)?;
    let Some((_, entity)) = crate::natives::entity_by_uuid(&id) else {
        return Ok(());
    };
    if let Some(mount) = mount(entity.as_ref(), horse_only) {
        candidate.commit(|stack| mount.set_item_slot(slot, stack));
    }
    Ok(())
}

pub(crate) extern "system" fn get(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    index: jint,
) -> jobject {
    match read(&mut env, &uuid, index, false) {
        Ok(value) => value,
        Err(error) => {
            error.throw_java(&mut env);
            std::ptr::null_mut()
        }
    }
}

pub(crate) extern "system" fn get_horse(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    index: jint,
) -> jobject {
    match read(&mut env, &uuid, index, true) {
        Ok(value) => value,
        Err(error) => {
            error.throw_java(&mut env);
            std::ptr::null_mut()
        }
    }
}

pub(crate) extern "system" fn set(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    index: jint,
    value: JObject<'_>,
) {
    if let Err(error) = write(&mut env, &uuid, index, &value, false) {
        error.throw_java(&mut env);
    }
}

pub(crate) extern "system" fn set_horse(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    index: jint,
    value: JObject<'_>,
) {
    if let Err(error) = write(&mut env, &uuid, index, &value, true) {
        error.throw_java(&mut env);
    }
}

pub(crate) extern "system" fn set_contents(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    values: JObjectArray<'_>,
) {
    let result = (|| -> Result<(), ItemBridgeError> {
        if env.get_array_length(&values)? != 2 {
            return Err(ItemBridgeError::InvalidEdit(
                "mount batch must contain two slots",
            ));
        }
        let mut candidates = Vec::with_capacity(2);
        for index in 0..2 {
            let value = env.get_object_array_element(&values, index)?;
            candidates.push((slot(index)?, mutation::materialize(&mut env, &value)?));
        }
        let id = identity(&mut env, &uuid)?;
        let Some((_, entity)) = crate::natives::entity_by_uuid(&id) else {
            return Ok(());
        };
        if let Some(mount) = mount(entity.as_ref(), false) {
            // These concrete mounts use LivingEntity's owned equipment setter:
            // no callbacks, and one guard makes the admitted batch visible together.
            let mut equipment = mount.living_base().equipment().lock();
            for (slot, candidate) in candidates {
                candidate.commit(|stack| {
                    equipment.set(slot, stack);
                });
            }
        }
        Ok(())
    })();
    if let Err(error) = result {
        error.throw_java(&mut env);
    }
}
