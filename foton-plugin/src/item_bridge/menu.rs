//! Open-menu reads borrow the actual slot before recursive clone admission.

use jni::{
    JNIEnv,
    objects::{JClass, JIntArray, JObject, JObjectArray, JString},
    sys::{jboolean, jint, jobject},
};

use super::{ItemBridgeError, store, transfer};

pub(crate) extern "system" fn set(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    indices: JIntArray<'_>,
    values: JObjectArray<'_>,
    full: jboolean,
) {
    let result = (|| -> Result<(), ItemBridgeError> {
        let length = env.get_array_length(&indices)?;
        if length > 4096 || length != env.get_array_length(&values)? {
            return Err(ItemBridgeError::InvalidEdit("invalid menu batch length"));
        }
        let mut indices_buffer = vec![
            0;
            usize::try_from(length).map_err(|_| {
                ItemBridgeError::InvalidEdit("invalid menu length")
            })?
        ];
        env.get_int_array_region(&indices, 0, &mut indices_buffer)?;
        let mut seen = std::collections::BTreeSet::new();
        let mut staged = Vec::with_capacity(indices_buffer.len());
        for (position, slot) in indices_buffer.into_iter().enumerate() {
            let slot = usize::try_from(slot)
                .map_err(|_| ItemBridgeError::InvalidEdit("negative menu slot"))?;
            if !seen.insert(slot) {
                return Err(ItemBridgeError::InvalidEdit("duplicate menu slot"));
            }
            let value = env.get_object_array_element(
                &values,
                i32::try_from(position).map_err(|_| ItemBridgeError::TransportLimit)?,
            )?;
            staged.push((slot, super::mutation::materialize(&mut env, &value)?));
        }
        let size = staged.len();
        if full != 0 && seen.iter().copied().ne(0..size) {
            return Err(ItemBridgeError::InvalidEdit("incomplete menu replacement"));
        }
        let batch = super::snapshot::menu_batch(staged, (full != 0).then_some(size))?;
        let Some(player) = crate::natives::player(&mut env, &uuid) else {
            return Err(ItemBridgeError::NativeState(
                "menu player unavailable".into(),
            ));
        };
        player
            .set_open_container_items(batch)
            .map_err(|error| match error {
                foton_core::player::player_inventory::MenuItemBatchError::UnsupportedLiveRead => {
                    ItemBridgeError::Unsupported("menu slot live reads")
                }
                foton_core::player::player_inventory::MenuItemBatchError::Closed => {
                    ItemBridgeError::NativeState("menu is closed".into())
                }
                _ => ItemBridgeError::InvalidEdit("invalid menu batch target"),
            })?;
        Ok(())
    })();
    if let Err(error) = result {
        error.throw_java(&mut env);
    }
}

pub(crate) extern "system" fn get(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    slot: jint,
) -> jobject {
    let result = (|| -> Result<jobject, ItemBridgeError> {
        let slot = usize::try_from(slot)
            .map_err(|_| ItemBridgeError::InvalidEdit("negative menu slot"))?;
        let Some(player) = crate::natives::player(&mut env, &uuid) else {
            return Ok(std::ptr::null_mut());
        };
        let store = store()?;
        let capture = player
            .with_open_container_item(slot, |stack| {
                if stack.is_empty() {
                    return Ok(None);
                }
                store
                    .capture(stack)
                    .map(Some)
                    .map_err(ItemBridgeError::native_state)
            })
            .transpose()?
            .flatten();
        match capture {
            Some(capture) => transfer::from_capture(&mut env, capture).map(JObject::into_raw),
            None => Ok(std::ptr::null_mut()),
        }
    })();
    match result {
        Ok(value) => value,
        Err(error) => {
            error.throw_java(&mut env);
            std::ptr::null_mut()
        }
    }
}
