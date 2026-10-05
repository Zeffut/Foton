//! Queries inspect the same detached canonical candidates as native writes.

use foton_registry::{data_components::vanilla_components::DAMAGE, item_stack::ItemStack};
use jni::{
    JNIEnv,
    objects::{JClass, JObject, JValue},
    sys::{jboolean, jint, jobject},
};
use std::ptr as bridge_ptr;

use super::{ItemBridgeError, mutation};

pub(crate) extern "system" fn has_meta(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    value: JObject<'_>,
) -> jboolean {
    match mutation::materialize(&mut env, &value) {
        Ok(value) => {
            u8::from(!value.stack.is_empty() && !value.stack.components_patch().is_empty())
        }
        Err(error) => {
            error.throw_java(&mut env);
            0
        }
    }
}

pub(crate) extern "system" fn similar(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    left: JObject<'_>,
    right: JObject<'_>,
) -> jboolean {
    let result = (|| -> Result<bool, ItemBridgeError> {
        let left = mutation::materialize(&mut env, &left)?;
        let right = mutation::materialize(&mut env, &right)?;
        Ok(ItemStack::is_same_item_same_components(
            &left.stack,
            &right.stack,
        ))
    })();
    match result {
        Ok(value) => u8::from(value),
        Err(error) => {
            error.throw_java(&mut env);
            0
        }
    }
}

pub(crate) extern "system" fn damage(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    value: JObject<'_>,
) -> jobject {
    let result = (|| -> Result<jobject, ItemBridgeError> {
        let value = mutation::materialize(&mut env, &value)?;
        let Some(damage) = value.stack.get(DAMAGE) else {
            return Ok(bridge_ptr::null_mut());
        };
        Ok(env
            .new_object("java/lang/Integer", "(I)V", &[JValue::Int(*damage)])?
            .into_raw())
    })();
    match result {
        Ok(value) => value,
        Err(error) => {
            error.throw_java(&mut env);
            bridge_ptr::null_mut()
        }
    }
}

pub(crate) extern "system" fn durability(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    value: JObject<'_>,
) -> jint {
    let result = mutation::materialize(&mut env, &value).and_then(|value| {
        if value.stack.get_max_damage() < 0 {
            return Err(ItemBridgeError::NativeState(
                "negative native maximum damage".to_owned(),
            ));
        }
        Ok(value.stack.get_damage_value())
    });
    match result {
        Ok(value) => value,
        Err(error) => {
            error.throw_java(&mut env);
            0
        }
    }
}
