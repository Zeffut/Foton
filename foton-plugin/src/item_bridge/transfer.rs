//! Owning JNI transfers. Never send a bare token without its Java owner.

use crate::natives as bridge_native_bridge;
use foton_registry::data_components::vanilla_components as bridge_vanilla_components;
#[cfg(test)]
use foton_registry::item_stack::ItemStack;
use jni::JNIEnv;
use jni::objects::{JClass, JObject, JString, JValue};
use uuid::Uuid;

use super::{ItemBridgeError, LeaseKey, snapshot::Capture, store};

#[cfg(test)]
pub(crate) fn capture<'local>(
    env: &mut JNIEnv<'local>,
    stack: &ItemStack,
) -> Result<JObject<'local>, ItemBridgeError> {
    let capture = store()?
        .capture(stack)
        .map_err(ItemBridgeError::native_state)?;
    from_capture(env, capture)
}

/// Core locks must be released before constructing the Java owner.
pub(crate) fn from_capture<'local>(
    env: &mut JNIEnv<'local>,
    capture: Capture<'_>,
) -> Result<JObject<'local>, ItemBridgeError> {
    if let Some(data) = capture.stack().get(bridge_vanilla_components::CUSTOM_DATA) {
        super::preflight::check_java_nbt(data.as_compound())
            .map_err(ItemBridgeError::native_state)?;
    }
    let projection = bridge_native_bridge::describe_slot(capture.stack());
    if projection == bridge_native_bridge::SLOT_METADATA_LIMIT_ERROR {
        return Err(ItemBridgeError::TransportLimit.native_state());
    }
    let projection = env.new_string(projection)?;
    construct(env, capture, projection.as_ref())
}

pub(super) fn construct<'local>(
    env: &mut JNIEnv<'local>,
    capture: Capture<'_>,
    projection: &JObject<'_>,
) -> Result<JObject<'local>, ItemBridgeError> {
    let key = capture.key();
    let epoch = env.new_string(key.epoch.to_string())?;
    let id = env.new_string(key.id.to_string())?;
    let result = env.new_object(
        "foton/item/ItemTransfer",
        "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)V",
        &[
            JValue::Object(&epoch),
            JValue::Object(&id),
            JValue::Object(projection),
        ],
    )?;
    capture.publish();
    Ok(result)
}

pub(crate) fn lease_key(
    env: &mut JNIEnv<'_>,
    lease: &JObject<'_>,
) -> Result<LeaseKey, ItemBridgeError> {
    let epoch = JString::from(env.get_field(lease, "epoch", "Ljava/lang/String;")?.l()?);
    let id = JString::from(env.get_field(lease, "id", "Ljava/lang/String;")?.l()?);
    parse_key(env, &epoch, &id)
}

fn parse_key(
    env: &mut JNIEnv<'_>,
    epoch: &JString<'_>,
    id: &JString<'_>,
) -> Result<LeaseKey, ItemBridgeError> {
    let epoch = env.get_string(epoch)?;
    let id = env.get_string(id)?;
    Ok(LeaseKey {
        epoch: Uuid::parse_str(epoch.to_str().map_err(|_| ItemBridgeError::StaleLease)?)
            .map_err(|_| ItemBridgeError::StaleLease)?,
        id: Uuid::parse_str(id.to_str().map_err(|_| ItemBridgeError::StaleLease)?)
            .map_err(|_| ItemBridgeError::StaleLease)?,
    })
}

pub(crate) extern "system" fn release(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    epoch: JString<'_>,
    id: JString<'_>,
) {
    if let Ok(store) = store()
        && let Ok(key) = parse_key(&mut env, &epoch, &id)
    {
        store.release(key);
    }
}

pub(crate) extern "system" fn close(_env: JNIEnv<'_>, _class: JClass<'_>) {
    if let Ok(store) = store() {
        store.close();
    }
}
