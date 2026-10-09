//! `ItemStack#serializeAsBytes` for a stack the legacy byte layout cannot hold:
//! the server's own item NBT, so nothing a native stack carries is dropped.

use std::io::Cursor;
use std::ptr as bridge_ptr;

use foton_registry::item_stack::ItemStack;
use jni::{
    JNIEnv,
    objects::{JByteArray, JClass, JObject},
    sys::{jbyteArray, jobject},
};
use simdnbt::FromNbtTag as _;
use simdnbt::borrow::read_tag;

use super::{ItemBridgeError, mutation, store, transfer};

/// Largest encoded stack read back, matching the bridge's own text limit.
const MAX_SERIALIZED_BYTES: usize = 8_388_608;

pub(crate) extern "system" fn serialize(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    value: JObject<'_>,
) -> jbyteArray {
    let result = (|| -> Result<jbyteArray, ItemBridgeError> {
        let candidate = mutation::materialize(&mut env, &value)?;
        if candidate.stack.is_empty() {
            return Err(ItemBridgeError::InvalidEdit(
                "cannot serialize an empty item",
            ));
        }
        candidate
            .stack
            .validate_persistent_encoding()
            .map_err(|_| ItemBridgeError::InvalidEdit("item is outside the persistent range"))?;
        let mut bytes = Vec::new();
        candidate.stack.to_nbt_tag_ref().write(&mut bytes);
        Ok(env.byte_array_from_slice(&bytes)?.into_raw())
    })();
    match result {
        Ok(bytes) => bytes,
        Err(error) => {
            error.throw_java(&mut env);
            bridge_ptr::null_mut()
        }
    }
}

pub(crate) extern "system" fn deserialize(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    bytes: JByteArray<'_>,
) -> jobject {
    let result = (|| -> Result<jobject, ItemBridgeError> {
        super::require_registry()?;
        let bytes = env.convert_byte_array(&bytes)?;
        if bytes.len() > MAX_SERIALIZED_BYTES {
            return Err(ItemBridgeError::TransportLimit);
        }
        let candidate = store()?.stage(|| {
            let mut cursor = Cursor::new(bytes.as_slice());
            let tag = read_tag(&mut cursor)
                .map_err(|_| ItemBridgeError::InvalidEdit("malformed ItemStack NBT"))?;
            if usize::try_from(cursor.position()).ok() != Some(bytes.len()) {
                return Err(ItemBridgeError::InvalidEdit("trailing ItemStack NBT"));
            }
            ItemStack::from_nbt_tag(tag.as_tag())
                .filter(|stack| !stack.is_empty())
                .ok_or(ItemBridgeError::InvalidEdit("invalid ItemStack NBT"))
        })?;
        let capture = store()?.capture_candidate(candidate)?;
        Ok(transfer::from_capture(&mut env, capture)?.into_raw())
    })();
    match result {
        Ok(value) => value,
        Err(error) => {
            error.throw_java(&mut env);
            bridge_ptr::null_mut()
        }
    }
}
