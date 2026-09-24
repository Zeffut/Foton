//! Entity state a plugin reads and writes through `org.bukkit.entity`.

use std::ffi::c_void;

use foton_core::inventory::container::Container as _;
use jni::JNIEnv;
use jni::objects::{JClass, JString};
use jni::sys::{jint, jstring};

use super::support::{entity, method, text};
use super::{describe_slot, parse_slot, to_java};

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
        return std::ptr::null_mut();
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
    ]
}
