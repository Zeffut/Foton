//! The two currently exposed Paper component keys share the canonical native state.

use foton_registry::data_components::{ComponentPatchEntry, vanilla_components::CUSTOM_MODEL_DATA};
use foton_utils::Identifier;
use jni::sys as bridge_sys;
use jni::{
    JNIEnv,
    objects::{JClass, JObject, JString, JValue},
    sys::{jint, jobject},
};
use std::ptr as bridge_ptr;

use super::{ItemBridgeError, mutation, store, transfer};

fn key(env: &mut JNIEnv<'_>, value: &JString<'_>) -> Result<Identifier, ItemBridgeError> {
    let name = env.get_string(value)?;
    match name
        .to_str()
        .map_err(|_| ItemBridgeError::InvalidEdit("invalid component name"))?
    {
        "damage" => Ok(Identifier::vanilla_static("damage")),
        "custom_model_data" => Ok(Identifier::vanilla_static("custom_model_data")),
        "banner_patterns" => Ok(Identifier::vanilla_static("banner_patterns")),
        "base_color" => Ok(Identifier::vanilla_static("base_color")),
        "trim" => Ok(Identifier::vanilla_static("trim")),
        _ => Err(ItemBridgeError::Unsupported("public component key")),
    }
}

pub(crate) extern "system" fn state(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    value: JObject<'_>,
    name: JString<'_>,
) -> jint {
    let result = (|| -> Result<jint, ItemBridgeError> {
        let key = key(&mut env, &name)?;
        let value = mutation::materialize(&mut env, &value)?;
        if value.stack.is_empty() {
            return Ok(0);
        }
        Ok(match value.stack.patch().get_entry(&key) {
            Some(ComponentPatchEntry::Set(_)) => 2,
            Some(ComponentPatchEntry::Removed) => 3,
            None if value.stack.get_effective_value_raw(&key).is_some() => 1,
            None => 0,
        })
    })();
    match result {
        Ok(value) => value,
        Err(error) => {
            error.throw_java(&mut env);
            0
        }
    }
}

pub(crate) extern "system" fn edit(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    value: JObject<'_>,
    name: JString<'_>,
    reset: bridge_sys::jboolean,
) -> jobject {
    let result = (|| -> Result<jobject, ItemBridgeError> {
        let key = key(&mut env, &name)?;
        let mut value = mutation::materialize(&mut env, &value)?;
        let edit = if reset != 0 {
            value.stack.reset_raw(&key)
        } else {
            value.stack.remove_raw(key)
        };
        edit.map_err(|_| ItemBridgeError::InvalidEdit("invalid public component edit"))?;
        let capture = store()?.capture_candidate(value)?;
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

fn length(value: usize) -> Result<jint, ItemBridgeError> {
    jint::try_from(value).map_err(|_| ItemBridgeError::TransportLimit)
}

pub(crate) extern "system" fn model(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    value: JObject<'_>,
) -> jobject {
    let result = (|| -> Result<jobject, ItemBridgeError> {
        let value = mutation::materialize(&mut env, &value)?;
        let Some(model) = value.stack.get(CUSTOM_MODEL_DATA) else {
            return Ok(bridge_ptr::null_mut());
        };
        let floats = env.new_float_array(length(model.floats().len())?)?;
        env.set_float_array_region(&floats, 0, model.floats())?;
        let flags = env.new_boolean_array(length(model.flags().len())?)?;
        let flag_values: Vec<_> = model.flags().iter().map(|flag| u8::from(*flag)).collect();
        env.set_boolean_array_region(&flags, 0, &flag_values)?;
        let colors = env.new_int_array(length(model.colors().len())?)?;
        env.set_int_array_region(&colors, 0, model.colors())?;
        let strings = env.new_object_array(
            length(model.strings().len())?,
            "java/lang/String",
            JObject::null(),
        )?;
        for (index, string) in model.strings().iter().enumerate() {
            let string = env.new_string(string)?;
            env.set_object_array_element(&strings, length(index)?, &string)?;
            env.delete_local_ref(string)?;
        }
        Ok(env
            .call_static_method(
                "foton/item/NativeComponents",
                "model",
                "([F[Z[Ljava/lang/String;[I)Lio/papermc/paper/datacomponent/item/CustomModelData;",
                &[
                    JValue::Object(&floats),
                    JValue::Object(&flags),
                    JValue::Object(&strings),
                    JValue::Object(&colors),
                ],
            )?
            .l()?
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
