//! Dropped-item and item-frame owning boundaries.

use crate::natives as bridge_native_bridge;
use foton_core::entity::{
    Entity as _,
    entities::objects::{
        display_ui::{GlowItemFrameEntity, ItemFrameEntity},
        items::ItemEntity,
        projectiles::{LargeFireballEntity, SmallFireballEntity},
    },
};
use foton_registry::item_stack::ItemStack;
use foton_utils::Downcast as _;
use jni::sys as bridge_sys;
use jni::{
    JNIEnv,
    objects::{JClass, JObject, JString},
    sys::jobject,
};
#[cfg(test)]
use std::num as bridge_num;
use std::ptr as bridge_ptr;
#[cfg(test)]
use std::sync as bridge_sync;
use uuid::Uuid;

use super::{ItemBridgeError, SnapshotStore, mutation, snapshot::Capture, store, transfer};

pub(super) fn identity(env: &mut JNIEnv<'_>, uuid: &JString<'_>) -> Result<Uuid, ItemBridgeError> {
    let text = env.get_string(uuid)?;
    Uuid::parse_str(
        text.to_str()
            .map_err(|_| ItemBridgeError::InvalidEdit("invalid entity UUID"))?,
    )
    .map_err(|_| ItemBridgeError::InvalidEdit("invalid entity UUID"))
}

fn capture_item<'a>(
    store: &'a SnapshotStore,
    stack: &ItemStack,
) -> Result<Option<Capture<'a>>, ItemBridgeError> {
    if stack.is_empty() {
        return Ok(None);
    }
    store
        .capture(stack)
        .map(Some)
        .map_err(ItemBridgeError::native_state)
}

pub(crate) extern "system" fn get(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jobject {
    let result = (|| -> Result<jobject, ItemBridgeError> {
        let id = identity(&mut env, &uuid)?;
        let Some((_, entity)) = bridge_native_bridge::entity_by_uuid(&id) else {
            return Ok(bridge_ptr::null_mut());
        };
        let store = store()?;
        let capture = if let Some(item) = entity.as_ref().downcast_ref::<ItemEntity>() {
            item.read_item(|stack| capture_item(store, stack))?
        } else if let Some(frame) = entity.as_ref().downcast_ref::<ItemFrameEntity>() {
            frame.with_framed_item(|stack| capture_item(store, stack))?
        } else if let Some(frame) = entity.as_ref().downcast_ref::<GlowItemFrameEntity>() {
            frame.with_framed_item(|stack| capture_item(store, stack))?
        } else if let Some(fireball) = entity.as_ref().downcast_ref::<LargeFireballEntity>() {
            fireball.with_item(|stack| capture_item(store, stack))?
        } else if let Some(fireball) = entity.as_ref().downcast_ref::<SmallFireballEntity>() {
            fireball.with_item(|stack| capture_item(store, stack))?
        } else {
            None
        };
        match capture {
            Some(capture) => transfer::from_capture(&mut env, capture).map(JObject::into_raw),
            None => Ok(bridge_ptr::null_mut()),
        }
    })();
    match result {
        Ok(value) => value,
        Err(error) => {
            error.throw_java(&mut env);
            bridge_ptr::null_mut()
        }
    }
}

pub(crate) extern "system" fn set(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    mutation: JObject<'_>,
) {
    let result = (|| -> Result<(), ItemBridgeError> {
        let candidate = mutation::materialize(&mut env, &mutation)?;
        let id = identity(&mut env, &uuid)?;
        let Some((_, entity)) = bridge_native_bridge::entity_by_uuid(&id) else {
            return Ok(());
        };
        if let Some(item) = entity.as_ref().downcast_ref::<ItemEntity>() {
            candidate.commit(|stack| item.set_item(stack));
        } else if let Some(frame) = entity.as_ref().downcast_ref::<ItemFrameEntity>() {
            candidate.commit(|stack| frame.set_item(stack));
        } else if let Some(frame) = entity.as_ref().downcast_ref::<GlowItemFrameEntity>() {
            candidate.commit(|stack| frame.set_item(stack));
        } else if let Some(fireball) = entity.as_ref().downcast_ref::<LargeFireballEntity>() {
            candidate.commit(|stack| fireball.set_item(stack));
        } else if let Some(fireball) = entity.as_ref().downcast_ref::<SmallFireballEntity>() {
            candidate.commit(|stack| fireball.set_item(stack));
        }
        Ok(())
    })();
    if let Err(error) = result {
        error.throw_java(&mut env);
    }
}

pub(crate) extern "system" fn drop_item(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    x: f64,
    y: f64,
    z: f64,
    value: JObject<'_>,
) -> bridge_sys::jstring {
    let result = (|| -> Result<bridge_sys::jstring, ItemBridgeError> {
        let candidate = mutation::materialize(&mut env, &value)?;
        if !x.is_finite() || !y.is_finite() || !z.is_finite() {
            return Err(ItemBridgeError::InvalidEdit("nonfinite item position"));
        }
        let Some(world) = bridge_native_bridge::world(&mut env, &name) else {
            return Ok(bridge_ptr::null_mut());
        };
        let entity = candidate.commit(|stack| world.spawn_item(glam::DVec3::new(x, y, z), stack));
        match entity {
            Some(entity) => Ok(env.new_string(entity.uuid().to_string())?.into_raw()),
            None => Ok(bridge_ptr::null_mut()),
        }
    })();
    match result {
        Ok(value) => value,
        Err(error) => {
            error.throw_java(&mut env);
            bridge_ptr::null_mut()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use foton_registry::{
        data_components::vanilla_components::CUSTOM_NAME, init_vanilla_registry, vanilla_entities,
        vanilla_items,
    };
    use text_components::TextComponent;

    #[test]
    fn actual_entity_borrow_rejects_depth_without_touching_source_then_recovers() {
        init_vanilla_registry();
        let store = SnapshotStore::new(bridge_num::NonZeroUsize::MIN);
        let mut text = TextComponent::plain("leaf");
        for _ in 0..super::super::preflight::MAX_DEPTH {
            let mut parent = TextComponent::plain("parent");
            parent.children.push(text);
            text = parent;
        }
        let mut stack = ItemStack::new(&vanilla_items::STONE);
        stack.set(CUSTOM_NAME, text);
        let entity = ItemEntity::with_item_and_velocity(
            &vanilla_entities::ITEM,
            1,
            glam::DVec3::ZERO,
            stack,
            glam::DVec3::ZERO,
            bridge_sync::Weak::new(),
        );
        assert!(
            entity
                .read_item(|stack| capture_item(&store, stack))
                .is_err()
        );
        entity
            .read_item(|stack| assert!(stack.get(CUSTOM_NAME).is_some(), "source was not cleared"));
        entity.set_item(ItemStack::new(&vanilla_items::DIAMOND));
        let capture = entity
            .read_item(|stack| capture_item(&store, stack))
            .expect("sibling capture")
            .expect("nonempty");
        assert_eq!(capture.stack().item(), &*vanilla_items::DIAMOND);
        drop(capture);

        let frame = ItemFrameEntity::new_attached(
            &vanilla_entities::ITEM_FRAME,
            2,
            foton_utils::BlockPos::new(0, 0, 0),
            foton_utils::Direction::North,
            bridge_sync::Weak::new(),
        );
        frame.set_item(ItemStack::new(&vanilla_items::DIAMOND));
        let capture = frame
            .with_framed_item(|stack| capture_item(&store, stack))
            .expect("frame capture")
            .expect("nonempty");
        assert_eq!(capture.stack().item(), &*vanilla_items::DIAMOND);
    }
}
