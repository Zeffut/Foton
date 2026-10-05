//! Player and ender inventory item boundaries. Core locks never cross JNI construction.

use crate::natives as bridge_native_bridge;
use foton_core::inventory::container::Container;
use jni::{
    JNIEnv,
    objects::{JClass, JIntArray, JObject, JObjectArray, JString},
    sys::{jboolean, jint, jobject},
};
use rustc_hash::FxHashSet;
use std::ptr as bridge_ptr;

use super::{
    ItemBridgeError, mutation,
    snapshot::{Candidate, Capture},
    store, transfer,
};

pub(super) fn validate_slots(
    size: usize,
    values: &[(usize, Candidate)],
) -> Result<(), ItemBridgeError> {
    let mut seen = FxHashSet::default();
    for (slot, _) in values {
        if *slot >= size || !seen.insert(*slot) {
            return Err(ItemBridgeError::InvalidEdit(
                "invalid or duplicate batch slot",
            ));
        }
    }
    Ok(())
}

pub(super) fn commit_batch(
    container: &mut (impl Container + ?Sized),
    values: Vec<(usize, Candidate)>,
) -> Result<(), ItemBridgeError> {
    validate_slots(container.get_container_size(), &values)?;
    for (slot, candidate) in values {
        candidate.commit(|stack| container.set_item(slot, stack));
    }
    Ok(())
}

fn write_batch(
    env: &mut JNIEnv<'_>,
    uuid: &JString<'_>,
    ender: bool,
    slots: &JIntArray<'_>,
    items: &JObjectArray<'_>,
) -> Result<(), ItemBridgeError> {
    let length = env.get_array_length(slots)?;
    if length > 4096 || length != env.get_array_length(items)? {
        return Err(ItemBridgeError::InvalidEdit(
            "invalid inventory batch length",
        ));
    }
    let count = usize::try_from(length)
        .map_err(|_| ItemBridgeError::InvalidEdit("invalid batch length"))?;
    let mut indices = vec![0; count];
    env.get_int_array_region(slots, 0, &mut indices)?;
    let mut staged = Vec::with_capacity(count);
    for (index, slot) in indices.into_iter().enumerate() {
        let slot = usize::try_from(slot)
            .map_err(|_| ItemBridgeError::InvalidEdit("negative batch slot"))?;
        let index = i32::try_from(index)
            .map_err(|_| ItemBridgeError::InvalidEdit("batch index overflow"))?;
        let item = env.get_object_array_element(items, index)?;
        staged.push((slot, mutation::materialize(env, &item)?));
    }
    let Some(player) = bridge_native_bridge::player(env, uuid) else {
        return Ok(());
    };
    if ender {
        commit_batch(&mut *player.ender_chest.lock(), staged)
    } else {
        commit_batch(&mut *player.inventory.lock(), staged)
    }
}

pub(crate) extern "system" fn set_slots(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    ender: jboolean,
    slots: JIntArray<'_>,
    items: JObjectArray<'_>,
) {
    if let Err(error) = write_batch(&mut env, &uuid, ender != 0, &slots, &items) {
        error.throw_java(&mut env);
    }
}

fn capture_slot(
    container: &impl Container,
    slot: usize,
) -> Result<Option<Capture<'static>>, ItemBridgeError> {
    if slot >= container.get_container_size() {
        return Ok(None);
    }
    let stack = container.get_item(slot);
    if stack.is_empty() {
        return Ok(None);
    }
    store()?
        .capture(stack)
        .map(Some)
        .map_err(ItemBridgeError::native_state)
}

fn read(
    env: &mut JNIEnv<'_>,
    uuid: &JString<'_>,
    slot: jint,
    ender: bool,
) -> Result<jobject, ItemBridgeError> {
    let Some(player) = bridge_native_bridge::player(env, uuid) else {
        return Ok(bridge_ptr::null_mut());
    };
    let slot = usize::try_from(slot)
        .map_err(|_| ItemBridgeError::InvalidEdit("negative inventory slot"))?;
    let capture = if ender {
        capture_slot(&*player.ender_chest.lock(), slot)?
    } else {
        capture_slot(&*player.inventory.lock(), slot)?
    };
    match capture {
        Some(capture) => transfer::from_capture(env, capture).map(JObject::into_raw),
        None => Ok(bridge_ptr::null_mut()),
    }
}

fn write(
    env: &mut JNIEnv<'_>,
    uuid: &JString<'_>,
    slot: jint,
    item: &JObject<'_>,
    ender: bool,
) -> Result<(), ItemBridgeError> {
    let candidate = mutation::materialize(env, item)?;
    let slot = usize::try_from(slot)
        .map_err(|_| ItemBridgeError::InvalidEdit("negative inventory slot"))?;
    let Some(player) = bridge_native_bridge::player(env, uuid) else {
        return Ok(());
    };
    if ender {
        let mut container = player.ender_chest.lock();
        if slot >= container.get_container_size() {
            return Err(ItemBridgeError::InvalidEdit("inventory slot out of range"));
        }
        candidate.commit(|stack| container.set_item(slot, stack));
    } else {
        let mut container = player.inventory.lock();
        if slot >= container.get_container_size() {
            return Err(ItemBridgeError::InvalidEdit("inventory slot out of range"));
        }
        candidate.commit(|stack| container.set_item(slot, stack));
    }
    Ok(())
}

pub(crate) extern "system" fn inventory_slot(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    slot: jint,
) -> jobject {
    match read(&mut env, &uuid, slot, false) {
        Ok(value) => value,
        Err(error) => {
            error.throw_java(&mut env);
            bridge_ptr::null_mut()
        }
    }
}

pub(crate) extern "system" fn ender_chest_slot(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    slot: jint,
) -> jobject {
    match read(&mut env, &uuid, slot, true) {
        Ok(value) => value,
        Err(error) => {
            error.throw_java(&mut env);
            bridge_ptr::null_mut()
        }
    }
}

pub(crate) extern "system" fn set_inventory_slot(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    slot: jint,
    item: JObject<'_>,
) {
    if let Err(error) = write(&mut env, &uuid, slot, &item, false) {
        error.throw_java(&mut env);
    }
}

pub(crate) extern "system" fn set_ender_chest_slot(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    slot: jint,
    item: JObject<'_>,
) {
    if let Err(error) = write(&mut env, &uuid, slot, &item, true) {
        error.throw_java(&mut env);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use foton_core::inventory::container::SimpleContainer;
    use foton_registry::{init_vanilla_registry, item_stack::ItemStack, vanilla_items};
    use std::num::NonZeroUsize;

    #[test]
    fn invalid_middle_batch_slot_does_not_apply_the_first_candidate() {
        init_vanilla_registry();
        let store = super::super::SnapshotStore::new(NonZeroUsize::new(3).expect("limit"));
        let mut container =
            SimpleContainer::from_items(vec![ItemStack::new(&vanilla_items::STONE); 2]);
        let replacement = || {
            store
                .candidate(ItemStack::new(&vanilla_items::DIAMOND))
                .expect("candidate")
        };
        assert!(
            commit_batch(
                &mut container,
                vec![(0, replacement()), (2, replacement()), (1, replacement())]
            )
            .is_err()
        );
        assert!(
            container
                .items()
                .iter()
                .all(|item| item.item() == &*vanilla_items::STONE)
        );
        commit_batch(&mut container, vec![(0, replacement()), (1, replacement())])
            .expect("valid sibling batch");
        assert!(
            container
                .items()
                .iter()
                .all(|item| item.item() == &*vanilla_items::DIAMOND)
        );
    }
}
