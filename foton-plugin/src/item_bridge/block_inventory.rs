//! Owning block-container slots. Admission precedes even lazy loot unpacking.

use crate::natives as bridge_native_bridge;
use foton_core::inventory::lock::{ContainerLockGuard, ContainerRef};
use foton_utils::BlockPos;
use jni::{
    JNIEnv,
    objects::{JClass, JObject, JObjectArray, JString},
    sys::{jint, jobject},
};
#[cfg(test)]
use std::num as bridge_num;
use std::ptr as bridge_ptr;

use super::{ItemBridgeError, inventory, mutation, snapshot::Candidate, store, transfer};

fn commit(
    container: &ContainerRef,
    candidates: Vec<(usize, Candidate)>,
    full: bool,
) -> Result<(), ItemBridgeError> {
    let size = container.container_size();
    if full && candidates.len() != size {
        return Err(ItemBridgeError::InvalidEdit("block inventory size changed"));
    }
    inventory::validate_slots(size, &candidates)?;
    // lock_all can unpack loot. Invalid input must be rejected before that side effect.
    let mut guard = ContainerLockGuard::lock_all(&[container]);
    let container = guard
        .get_mut(container.container_id())
        .ok_or(ItemBridgeError::NativeState(
            "block container unavailable".to_owned(),
        ))?;
    if full && candidates.len() != container.get_container_size() {
        return Err(ItemBridgeError::InvalidEdit("block inventory size changed"));
    }
    // Preserve this bridge's existing storage-local setter policy; owner notifications
    // are not newly introduced or coalesced by the representation migration.
    inventory::commit_batch(container, candidates)
}

pub(crate) extern "system" fn get(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    x: jint,
    y: jint,
    z: jint,
    slot: jint,
) -> jobject {
    let result = (|| -> Result<jobject, ItemBridgeError> {
        let slot = usize::try_from(slot)
            .map_err(|_| ItemBridgeError::InvalidEdit("negative block slot"))?;
        let Some(world) = bridge_native_bridge::world(&mut env, &name) else {
            return Ok(bridge_ptr::null_mut());
        };
        let Some(entity) = world.get_block_entity(BlockPos::new(x, y, z)) else {
            return Ok(bridge_ptr::null_mut());
        };
        let Some(container) = ContainerRef::from_block_entity(entity) else {
            return Ok(bridge_ptr::null_mut());
        };
        if slot >= container.container_size() {
            return Err(ItemBridgeError::InvalidEdit("block slot out of range"));
        }
        let capture = {
            let guard = ContainerLockGuard::lock_all(&[&container]);
            let container =
                guard
                    .get(container.container_id())
                    .ok_or(ItemBridgeError::NativeState(
                        "block container unavailable".to_owned(),
                    ))?;
            if slot >= container.get_container_size() {
                return Err(ItemBridgeError::InvalidEdit("block slot out of range"));
            }
            let stack = container.get_item(slot);
            if stack.is_empty() {
                None
            } else {
                Some(
                    store()?
                        .capture(stack)
                        .map_err(ItemBridgeError::native_state)?,
                )
            }
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

fn write(
    env: &mut JNIEnv<'_>,
    name: &JString<'_>,
    pos: BlockPos,
    candidates: Vec<(usize, Candidate)>,
    full: bool,
) -> Result<(), ItemBridgeError> {
    let Some(world) = bridge_native_bridge::world(env, name) else {
        return Ok(());
    };
    let Some(entity) = world.get_block_entity(pos) else {
        return Ok(());
    };
    let Some(container) = ContainerRef::from_block_entity(entity) else {
        return Ok(());
    };
    commit(&container, candidates, full)
}

pub(crate) extern "system" fn set(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    x: jint,
    y: jint,
    z: jint,
    slot: jint,
    value: JObject<'_>,
) {
    let result = (|| -> Result<(), ItemBridgeError> {
        let slot = usize::try_from(slot)
            .map_err(|_| ItemBridgeError::InvalidEdit("negative block slot"))?;
        let candidate = mutation::materialize(&mut env, &value)?;
        write(
            &mut env,
            &name,
            BlockPos::new(x, y, z),
            vec![(slot, candidate)],
            false,
        )
    })();
    if let Err(error) = result {
        error.throw_java(&mut env);
    }
}

pub(crate) extern "system" fn set_contents(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    x: jint,
    y: jint,
    z: jint,
    values: JObjectArray<'_>,
) {
    let result = (|| -> Result<(), ItemBridgeError> {
        let length = env.get_array_length(&values)?;
        if length > 4096 {
            return Err(ItemBridgeError::InvalidEdit(
                "block inventory batch too large",
            ));
        }
        let mut candidates = Vec::new();
        for index in 0..length {
            let value = env.get_object_array_element(&values, index)?;
            let slot = usize::try_from(index)
                .map_err(|_| ItemBridgeError::InvalidEdit("negative batch slot"))?;
            candidates.push((slot, mutation::materialize(&mut env, &value)?));
            env.delete_local_ref(value)?;
        }
        write(&mut env, &name, BlockPos::new(x, y, z), candidates, true)
    })();
    if let Err(error) = result {
        error.throw_java(&mut env);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use foton_core::{
        block_entity::{BlockEntityBase, ContainerLoot},
        inventory::container::{Container as _, SimpleContainer},
    };
    use foton_registry::{
        RegistryExt as _, data_components::vanilla_components::GLIDER, item_stack::ItemStack,
        vanilla_block_entity_types, vanilla_blocks, vanilla_items,
    };
    use foton_utils::{Identifier, locks::IntoShared as _};
    use std::sync::Arc;

    #[test]
    fn malformed_batch_precedes_loot_unpack_and_all_storage_changes() {
        let world = super::super::test_world::world();
        let mut original = ItemStack::new(&vanilla_items::STONE);
        original.set(GLIDER, ());
        let storage = SimpleContainer::from_items(vec![original.clone(); 27]).into_shared();
        let loot = Arc::new(ContainerLoot::new());
        let key = Identifier::vanilla_static("chests/simple_dungeon");
        assert!(
            foton_registry::REGISTRY.loot_tables.by_key(&key).is_some(),
            "real extracted loot table"
        );
        loot.set_loot_table(key.clone(), 42);
        let base = Arc::new(BlockEntityBase::new(
            &vanilla_block_entity_types::BARREL,
            Arc::downgrade(world),
            BlockPos::new(0, 64, 0),
            vanilla_blocks::BARREL.default_state(),
        ));
        let container = ContainerRef::owned_by_randomizable_block_entity(
            storage.clone(),
            base,
            Arc::clone(&loot),
        );
        let store =
            super::super::SnapshotStore::new(bridge_num::NonZeroUsize::new(30).expect("capacity"));
        let candidate = || {
            store
                .candidate(ItemStack::new(&vanilla_items::DIAMOND))
                .expect("candidate")
        };
        for slots in [vec![0, 27, 1], vec![0, 0]] {
            assert!(
                commit(
                    &container,
                    slots.into_iter().map(|slot| (slot, candidate())).collect(),
                    false
                )
                .is_err()
            );
            assert_eq!(
                loot.loot_table(),
                Some(key.clone()),
                "invalid input must not unpack loot"
            );
            assert!(storage.lock().items().iter().all(|item| item == &original));
        }
        assert!(
            commit(&container, vec![(0, candidate())], true).is_err(),
            "short whole contents rejected"
        );
        assert!(loot.is_packed());
        commit(&container, vec![(0, candidate())], false).expect("valid sibling");
        assert!(
            !loot.is_packed(),
            "normal accepted operation still unpacks loot"
        );
        assert_eq!(storage.lock().get_item(0).item(), &*vanilla_items::DIAMOND);
    }
}
