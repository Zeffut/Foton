//! `AttributeInstance`: an entity's attribute, keyed the way vanilla keys it.
//!
//! Since 1.21 a modifier is identified by a namespaced key, which is what
//! Paper's `AttributeModifier` carries and what Foton's `AttributeMap` stores,
//! so the key crosses unchanged -- a modifier a plugin adds under
//! `zeldaciv:goron_speed` is the one `/attribute` lists under that name.

use std::ffi::c_void;
use std::ptr::null_mut;

use foton_core::entity::attribute::{AttributeModifier, AttributeModifierOperation};
use foton_registry::attribute::AttributeRef;
use foton_registry::{REGISTRY, RegistryExt as _};
use jni::JNIEnv;
use jni::objects::{JClass, JString};
use jni::sys::{jboolean, jdouble, jdoubleArray, jobjectArray};

use super::string_array;
use super::support::{doubles, entity, key, method, text};

fn attribute(env: &mut JNIEnv<'_>, name: &JString<'_>) -> Option<AttributeRef> {
    REGISTRY.attributes.by_key(&key(env, name)?)
}

const fn operation_name(operation: AttributeModifierOperation) -> &'static str {
    match operation {
        AttributeModifierOperation::AddValue => "ADD_NUMBER",
        AttributeModifierOperation::AddMultipliedBase => "ADD_SCALAR",
        AttributeModifierOperation::AddMultipliedTotal => "MULTIPLY_SCALAR_1",
    }
}

fn operation(name: &str) -> Option<AttributeModifierOperation> {
    match name {
        "ADD_NUMBER" => Some(AttributeModifierOperation::AddValue),
        "ADD_SCALAR" => Some(AttributeModifierOperation::AddMultipliedBase),
        "MULTIPLY_SCALAR_1" => Some(AttributeModifierOperation::AddMultipliedTotal),
        _ => None,
    }
}

/// `{base, value, default}`, or null when the entity lacks the attribute --
/// Paper's `getAttribute` answers null in exactly that case.
extern "system" fn attribute_values(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    name: JString<'_>,
) -> jdoubleArray {
    let Some(attribute) = attribute(&mut env, &name) else {
        return null_mut();
    };
    let Some((_, entity)) = entity(&mut env, &uuid) else {
        return null_mut();
    };
    let Some(living) = entity.as_living_entity() else {
        return null_mut();
    };
    let values = {
        let attributes = living.attributes().lock();
        attributes.get_instance(attribute).map(|instance| {
            [
                instance.base_value(),
                instance.value(),
                attribute.default_value,
            ]
        })
    };
    doubles(&mut env, values.as_ref().map(<[f64; 3]>::as_slice))
}

/// Every modifier as `key|amount|operation`.
extern "system" fn attribute_modifier_list(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    name: JString<'_>,
) -> jobjectArray {
    let Some(attribute) = attribute(&mut env, &name) else {
        return null_mut();
    };
    let Some((_, entity)) = entity(&mut env, &uuid) else {
        return null_mut();
    };
    let Some(living) = entity.as_living_entity() else {
        return null_mut();
    };
    let values: Option<Vec<String>> = {
        let attributes = living.attributes().lock();
        attributes.get_instance(attribute).map(|instance| {
            instance
                .modifiers()
                .iter()
                .map(|modifier| {
                    format!(
                        "{}|{}|{}",
                        modifier.id,
                        modifier.amount,
                        operation_name(modifier.operation)
                    )
                })
                .collect()
        })
    };
    let Some(values) = values else {
        return null_mut();
    };
    string_array(&mut env, &values)
}

/// Adds a modifier; `false` when one with the same key is already there, which
/// is vanilla's refusal and the plugin's exception.
extern "system" fn add_attribute_modifier_keyed(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    name: JString<'_>,
    modifier: JString<'_>,
    amount: jdouble,
    operation_text: JString<'_>,
    persistent: jboolean,
) -> jboolean {
    let Some(attribute) = attribute(&mut env, &name) else {
        return 0;
    };
    let Some(id) = key(&mut env, &modifier) else {
        return 0;
    };
    let Some(operation) = text(&mut env, &operation_text).and_then(|value| operation(&value))
    else {
        return 0;
    };
    let Some((_, entity)) = entity(&mut env, &uuid) else {
        return 0;
    };
    let Some(living) = entity.as_living_entity() else {
        return 0;
    };
    jboolean::from(living.attributes().lock().add_modifier(
        attribute,
        AttributeModifier {
            id,
            amount,
            operation,
        },
        persistent != 0,
    ))
}

extern "system" fn remove_attribute_modifier_keyed(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    name: JString<'_>,
    modifier: JString<'_>,
) -> jboolean {
    let Some(attribute) = attribute(&mut env, &name) else {
        return 0;
    };
    let Some(id) = key(&mut env, &modifier) else {
        return 0;
    };
    let Some((_, entity)) = entity(&mut env, &uuid) else {
        return 0;
    };
    let Some(living) = entity.as_living_entity() else {
        return 0;
    };
    jboolean::from(living.attributes().lock().remove_modifier(attribute, &id))
}

pub(super) fn bindings() -> Vec<jni::NativeMethod> {
    vec![
        method(
            "attributeValues",
            "(Ljava/lang/String;Ljava/lang/String;)[D",
            attribute_values as *mut c_void,
        ),
        method(
            "attributeModifierList",
            "(Ljava/lang/String;Ljava/lang/String;)[Ljava/lang/String;",
            attribute_modifier_list as *mut c_void,
        ),
        method(
            "addAttributeModifierKeyed",
            "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;DLjava/lang/String;Z)Z",
            add_attribute_modifier_keyed as *mut c_void,
        ),
        method(
            "removeAttributeModifierKeyed",
            "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)Z",
            remove_attribute_modifier_keyed as *mut c_void,
        ),
    ]
}
