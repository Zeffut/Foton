//! Canonical material rebasing and whole-patch resets for detached Java stacks.

use foton_registry::{REGISTRY, RegistryExt, item_stack::ItemStack};
use foton_utils::Identifier;
use jni::{
    JNIEnv,
    objects::{JClass, JObject, JString},
    sys::{jboolean, jint, jobject},
};

use super::{ItemBridgeError, mutation, store, transfer};

pub(crate) extern "system" fn rebase(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    mutation: JObject<'_>,
    material: JString<'_>,
    reset: jboolean,
) -> jobject {
    transform(&mut env, &mutation, &material, reset != 0, false, None)
}

pub(crate) extern "system" fn convert_meta(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    mutation: JObject<'_>,
    recipient: JObject<'_>,
    material: JString<'_>,
    discard_dye: jboolean,
) -> jobject {
    transform(
        &mut env,
        &mutation,
        &material,
        false,
        discard_dye != 0,
        Some(&recipient),
    )
}

pub(crate) extern "system" fn durability(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    mutation: JObject<'_>,
    damage: jint,
) -> jobject {
    let result = (|| -> Result<jobject, ItemBridgeError> {
        let mut candidate = mutation::materialize(&mut env, &mutation)?;
        if candidate.stack.get_max_damage() < 0 {
            return Err(ItemBridgeError::NativeState(
                "negative native maximum damage".to_owned(),
            ));
        }
        candidate.stack.set_damage_value(damage);
        let capture = store()?.capture_candidate(candidate)?;
        Ok(transfer::from_capture(&mut env, capture)?.into_raw())
    })();
    match result {
        Ok(value) => value,
        Err(error) => {
            error.throw_java(&mut env);
            std::ptr::null_mut()
        }
    }
}

fn transform(
    env: &mut JNIEnv<'_>,
    mutation: &JObject<'_>,
    material: &JString<'_>,
    reset: bool,
    discard_dye: bool,
    recipient: Option<&JObject<'_>>,
) -> jobject {
    let result = (|| -> Result<jobject, ItemBridgeError> {
        super::require_registry()?;
        let material = env.get_string(material)?;
        let key: Identifier = material
            .to_str()
            .map_err(|_| ItemBridgeError::InvalidEdit("invalid material encoding"))?
            .parse()
            .map_err(|_| ItemBridgeError::InvalidEdit("invalid material"))?;
        let item = REGISTRY
            .items
            .by_key(&key)
            .ok_or(ItemBridgeError::InvalidEdit("unknown material"))?;
        let mut candidate = mutation::materialize(env, mutation)?;
        // Foton's opaque extension belongs to the recipient stack, not ItemMeta.
        // Read it from canonical recipient state, never from a projected field overlay.
        if let Some(recipient) = recipient {
            let recipient = mutation::materialize(env, recipient)?;
            candidate
                .stack
                .set_opaque_nbt(recipient.stack.opaque_nbt().map(str::to_owned));
        }
        if discard_dye {
            use foton_registry::data_components::{
                ComponentPatchEntry, vanilla_components::DYED_COLOR,
            };
            // CraftMetaColorableArmor copies no positive horse-armor color;
            // common removedTags still carry all removal tombstones.
            if matches!(
                candidate.stack.patch().get_entry(DYED_COLOR.key()),
                Some(ComponentPatchEntry::Set(_))
            ) {
                candidate.stack.clear(DYED_COLOR);
            }
        }
        if reset {
            candidate.stack = ItemStack::with_count(item, candidate.stack.count());
        } else {
            candidate.stack.set_item(&key);
        }
        let capture = store()?.capture_candidate(candidate)?;
        Ok(transfer::from_capture(env, capture)?.into_raw())
    })();
    match result {
        Ok(value) => value,
        Err(error) => {
            error.throw_java(env);
            std::ptr::null_mut()
        }
    }
}
