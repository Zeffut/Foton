//! Single-item block entity boundaries, with borrowed preflight before capture.

use crate::natives as bridge_native_bridge;
use foton_core::block_entity::entities::{JukeboxBlockEntity, LecternBlockEntity};
use foton_registry::{
    data_components::vanilla_components::{WRITABLE_BOOK_CONTENT, WRITTEN_BOOK_CONTENT},
    vanilla_items,
};
use foton_utils::{BlockPos, Downcast as _, text::DisplayResolutor};
use jni::{
    JNIEnv,
    objects::{JClass, JObject, JString},
    sys::{jboolean, jint, jobject, jobjectArray},
};
use std::ptr as bridge_ptr;

use super::{ItemBridgeError, mutation, snapshot::Capture, store, transfer};

#[cfg(test)]
mod tests;

fn capture(
    env: &mut JNIEnv<'_>,
    name: &JString<'_>,
    pos: BlockPos,
    lectern: bool,
) -> Result<Option<Capture<'static>>, ItemBridgeError> {
    let Some(world) = bridge_native_bridge::world(env, name) else {
        return Ok(None);
    };
    let Some(entity) = world.get_block_entity(pos) else {
        return Ok(None);
    };
    let store = store()?;
    let capture = if lectern {
        let Some(lectern) = entity.downcast_ref::<LecternBlockEntity>() else {
            return Ok(None);
        };
        lectern.with_book(|stack| store.capture(stack))
    } else {
        let Some(jukebox) = entity.downcast_ref::<JukeboxBlockEntity>() else {
            return Ok(None);
        };
        jukebox.read_item(|stack| store.capture(stack))
    };
    capture.map(Some).map_err(ItemBridgeError::native_state)
}

fn read(
    env: &mut JNIEnv<'_>,
    name: &JString<'_>,
    pos: BlockPos,
    lectern: bool,
) -> Result<jobject, ItemBridgeError> {
    let Some(capture) = capture(env, name, pos, lectern)? else {
        return Ok(bridge_ptr::null_mut());
    };
    if capture.stack().is_empty() {
        return Ok(bridge_ptr::null_mut());
    }
    transfer::from_capture(env, capture).map(JObject::into_raw)
}

pub(crate) extern "system" fn lectern(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    x: jint,
    y: jint,
    z: jint,
) -> jobject {
    match read(&mut env, &name, BlockPos::new(x, y, z), true) {
        Ok(value) => value,
        Err(error) => {
            error.throw_java(&mut env);
            bridge_ptr::null_mut()
        }
    }
}

pub(crate) extern "system" fn jukebox(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    x: jint,
    y: jint,
    z: jint,
) -> jobject {
    match read(&mut env, &name, BlockPos::new(x, y, z), false) {
        Ok(value) => value,
        Err(error) => {
            error.throw_java(&mut env);
            bridge_ptr::null_mut()
        }
    }
}

pub(crate) extern "system" fn pages(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    x: jint,
    y: jint,
    z: jint,
) -> jobjectArray {
    let result = (|| -> Result<jobjectArray, ItemBridgeError> {
        let Some(capture) = capture(&mut env, &name, BlockPos::new(x, y, z), true)? else {
            return Ok(bridge_ptr::null_mut());
        };
        let stack = capture.stack();
        let pages: Vec<String> = if let Some(book) = stack.get(WRITTEN_BOOK_CONTENT) {
            book.pages()
                .iter()
                .map(|page| page.get(false).to_plain(&DisplayResolutor))
                .collect()
        } else if let Some(book) = stack.get(WRITABLE_BOOK_CONTENT) {
            book.pages()
                .iter()
                .map(|page| page.get(false).clone())
                .collect()
        } else {
            Vec::new()
        };
        let bytes = pages
            .iter()
            .try_fold(0_usize, |sum, page| sum.checked_add(page.len()));
        if bytes.is_none_or(|bytes| bytes > 8_388_608) {
            return Err(ItemBridgeError::TransportLimit.native_state());
        }
        let length = jint::try_from(pages.len())
            .map_err(|_| ItemBridgeError::TransportLimit.native_state())?;
        let values = env.new_object_array(length, "java/lang/String", JObject::null())?;
        for (index, page) in pages.iter().enumerate() {
            let page = env.new_string(page)?;
            env.set_object_array_element(
                &values,
                jint::try_from(index).map_err(|_| ItemBridgeError::TransportLimit)?,
                &page,
            )?;
            env.delete_local_ref(page)?;
        }
        Ok(values.into_raw())
    })();
    match result {
        Ok(value) => value,
        Err(error) => {
            error.throw_java(&mut env);
            bridge_ptr::null_mut()
        }
    }
}

pub(crate) extern "system" fn set_lectern(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    x: jint,
    y: jint,
    z: jint,
    value: JObject<'_>,
) -> jboolean {
    let result = (|| -> Result<jboolean, ItemBridgeError> {
        let candidate = mutation::materialize(&mut env, &value)?;
        if candidate.stack.item() != &*vanilla_items::WRITABLE_BOOK
            && candidate.stack.item() != &*vanilla_items::WRITTEN_BOOK
        {
            return Ok(0);
        }
        let Some(world) = bridge_native_bridge::world(&mut env, &name) else {
            return Ok(0);
        };
        let Some(entity) = world.get_block_entity(BlockPos::new(x, y, z)) else {
            return Ok(0);
        };
        let Some(lectern) = entity.downcast_ref::<LecternBlockEntity>() else {
            return Ok(0);
        };
        candidate.commit(|stack| lectern.set_book(stack));
        Ok(1)
    })();
    match result {
        Ok(value) => value,
        Err(error) => {
            error.throw_java(&mut env);
            0
        }
    }
}

pub(crate) extern "system" fn set_jukebox(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    x: jint,
    y: jint,
    z: jint,
    value: JObject<'_>,
) {
    let result = (|| -> Result<(), ItemBridgeError> {
        let candidate = mutation::materialize(&mut env, &value)?;
        let Some(world) = bridge_native_bridge::world(&mut env, &name) else {
            return Ok(());
        };
        let Some(entity) = world.get_block_entity(BlockPos::new(x, y, z)) else {
            return Ok(());
        };
        let Some(jukebox) = entity.downcast_ref::<JukeboxBlockEntity>() else {
            return Ok(());
        };
        candidate.commit(|stack| jukebox.insert(&world, stack));
        Ok(())
    })();
    if let Err(error) = result {
        error.throw_java(&mut env);
    }
}
