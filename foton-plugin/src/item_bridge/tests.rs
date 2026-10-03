use std::num::NonZeroUsize;

use foton_registry::data_components::components::BundleContents;
use foton_registry::data_components::vanilla_components::{
    ADDITIONAL_TRADE_COST, BUNDLE_CONTENTS, CUSTOM_NAME, DAMAGE, GLIDER,
};
use foton_registry::{
    ItemStackTemplate, init_vanilla_registry, item_stack::ItemStack, vanilla_items,
};
use text_components::TextComponent;

use super::{ItemBridgeError, SnapshotStore, preflight};

fn store(limit: usize) -> SnapshotStore {
    init_vanilla_registry();
    SnapshotStore::new(NonZeroUsize::new(limit).expect("nonzero test capacity"))
}

#[test]
fn deferred_menu_owner_keeps_admission_after_consuming_its_item_values() {
    let store = store(1);
    let candidate = store
        .candidate(ItemStack::new(&vanilla_items::STONE))
        .expect("candidate");
    let mut batch = super::snapshot::menu_batch(vec![(0, candidate)], None).expect("owned batch");
    store.close();
    let values = batch.take_writes();
    assert_eq!(values.len(), 1);
    assert!(
        !store.is_terminal(),
        "taking writes must not release the queued operation permit"
    );
    drop(values);
    assert!(
        !store.is_terminal(),
        "item ownership and operation admission are distinct"
    );
    drop(batch);
    assert!(
        store.is_terminal(),
        "only final owner disposal completes terminal drain"
    );
}

#[test]
fn raw_nbt_readability_failure_is_not_an_empty_string() {
    use simdnbt::{Mutf8Str, owned::NbtCompound};
    let mut compound = NbtCompound::new();
    compound.insert(
        "bad",
        simdnbt::owned::NbtTag::String(Mutf8Str::from_slice(&[0xff]).to_owned()),
    );
    assert!(preflight::check_java_nbt(&compound).is_err());
    assert_eq!(
        compound
            .string("bad")
            .expect("raw value retained")
            .as_bytes(),
        &[0xff]
    );
}

#[test]
fn snapshot_keeps_native_strings_that_the_java_projection_cannot_represent() {
    use foton_registry::data_components::{
        components::CustomData, vanilla_components::CUSTOM_DATA,
    };
    use simdnbt::owned::NbtCompound;
    let store = store(1);
    let mut compound = NbtCompound::new();
    compound.insert("large", "x".repeat(65_536));
    let mut item = ItemStack::new(&vanilla_items::STONE);
    item.set(
        CUSTOM_DATA,
        CustomData::try_from_compound(compound).expect("valid native string"),
    );
    let key = store
        .capture(&item)
        .expect("lossless native capture")
        .publish();
    let snapshot = store.lookup(key).expect("snapshot");
    let data = snapshot
        .stack()
        .get(CUSTOM_DATA)
        .expect("full custom data")
        .as_compound();
    assert_eq!(data.string("large").expect("large string").len(), 65_536);
    assert!(preflight::check_java_nbt(data).is_err());
}

#[test]
fn reentrant_close_waits_for_commit_without_holding_a_callback_lock() {
    let store = store(1);
    let key = store
        .capture(&ItemStack::new(&vanilla_items::STONE))
        .expect("capture")
        .publish();
    let candidate = store.materialize(key).expect("admitted operation");
    candidate.commit(|stack| {
        store.close();
        assert!(
            !store.is_terminal(),
            "the admitted commit is still in flight"
        );
        assert!(matches!(
            store.materialize(key),
            Err(ItemBridgeError::Closed)
        ));
        assert_eq!(stack.item(), &*vanilla_items::STONE);
    });
    assert!(store.is_terminal());
}

#[tokio::test]
async fn async_terminal_barrier_retains_unmanaged_candidate_until_final_drop() {
    use std::{
        future::Future,
        task::{Context, Poll, Waker},
    };
    let store = store(1);
    let candidate = store
        .candidate(ItemStack::new(&vanilla_items::STONE))
        .expect("candidate");
    let mut drain = std::pin::pin!(store.drained());
    let mut context = Context::from_waker(Waker::noop());
    assert!(matches!(drain.as_mut().poll(&mut context), Poll::Pending));
    store.close();
    assert!(matches!(drain.as_mut().poll(&mut context), Poll::Pending));
    drop(candidate);
    drain.await;
    // Close and last drop both preceded waiter registration on this second wait.
    store.drained().await;
}

#[tokio::test]
async fn async_terminal_barrier_wakes_registered_waiters() {
    let store = std::sync::Arc::new(store(1));
    let candidate = store
        .candidate(ItemStack::new(&vanilla_items::STONE))
        .expect("candidate");
    store.close();
    let waiting_store = std::sync::Arc::clone(&store);
    let waiter = tokio::spawn(async move { waiting_store.drained().await });
    // Either scheduling order is valid: final-drop notification or terminal recheck.
    drop(candidate);
    tokio::time::timeout(std::time::Duration::from_secs(2), waiter)
        .await
        .expect("terminal wake")
        .expect("waiter completed");
    let zero = self::store(1);
    zero.close();
    zero.drained().await;
}

#[test]
fn hidden_effect_chains_are_checked_in_every_component_carrier() {
    use foton_registry::consume_effect::{
        ApplyStatusEffectsConsumeEffect, ConsumeEffectData, vanilla_consume_effect_types,
    };
    use foton_registry::data_components::components::{
        Consumable, DeathProtection, PotionContents,
    };
    use foton_registry::data_components::vanilla_components::{
        CONSUMABLE, DEATH_PROTECTION, POTION_CONTENTS,
    };
    use foton_registry::mob_effect_instance::{MobEffectInstance, MobEffectInstanceDetails};
    use foton_registry::vanilla_mob_effects;

    let store = store(1);
    let mut hidden = None;
    for _ in 0..128 {
        hidden = Some(MobEffectInstanceDetails::new(
            0, 20, false, true, true, hidden,
        ));
    }
    let effect = MobEffectInstance::new(
        &vanilla_mob_effects::SPEED,
        20,
        0,
        false,
        true,
        true,
        hidden,
    );
    let mut item = ItemStack::new(&vanilla_items::STONE);
    item.set(
        POTION_CONTENTS,
        PotionContents::new(None, None, vec![effect.clone()], None),
    );
    assert!(matches!(
        store.capture(&item),
        Err(ItemBridgeError::Depth(_))
    ));
    item.clear(POTION_CONTENTS);
    let consume = || {
        ConsumeEffectData::new(
            &vanilla_consume_effect_types::APPLY_EFFECTS,
            ApplyStatusEffectsConsumeEffect::new(vec![effect.clone()], 1.0).expect("effect"),
        )
    };
    item.set(DEATH_PROTECTION, DeathProtection::new(vec![consume()]));
    assert!(matches!(
        store.capture(&item),
        Err(ItemBridgeError::Depth(_))
    ));
    item.clear(DEATH_PROTECTION);
    let prototype = ItemStack::new(&vanilla_items::GOLDEN_APPLE);
    let prototype = prototype.get(CONSUMABLE).expect("golden apple consumable");
    item.set(
        CONSUMABLE,
        Consumable::new(
            prototype.consume_seconds(),
            prototype.animation(),
            prototype.sound().clone(),
            prototype.has_consume_particles(),
            vec![consume()],
        )
        .expect("consumable"),
    );
    assert!(matches!(
        store.capture(&item),
        Err(ItemBridgeError::Depth(_))
    ));
    item.clear(CONSUMABLE);
    assert!(store.capture(&item).is_ok());
}

#[test]
fn snapshot_preserves_runtime_values_without_a_codec() {
    let store = store(2);
    let mut original = ItemStack::new(&vanilla_items::IRON_CHESTPLATE);
    original.set(GLIDER, ());
    original.remove(DAMAGE);
    original.set(ADDITIONAL_TRADE_COST, 7);
    original.set(
        BUNDLE_CONTENTS,
        BundleContents::with_selected_item(
            vec![
                ItemStackTemplate::new(&vanilla_items::STONE),
                ItemStackTemplate::new(&vanilla_items::DIAMOND),
            ],
            1,
        ),
    );
    let key = store.capture(&original).expect("capture").publish();
    original.remove(GLIDER);
    let mut first = store.materialize(key).expect("materialize");
    assert!(first.stack.has(GLIDER));
    assert!(!first.stack.has(DAMAGE));
    assert_eq!(first.stack.get(ADDITIONAL_TRADE_COST), Some(&7));
    let bundle = first.stack.get(BUNDLE_CONTENTS).expect("bundle retained");
    assert_eq!(bundle.selected_item_index(), 1);
    assert_eq!(
        bundle.selected_item().expect("selected item").item(),
        &*vanilla_items::DIAMOND
    );
    let extracted = foton_core::behavior::items::MutableBundleContents::new(bundle)
        .remove_one()
        .expect("actual next bundle extraction");
    assert_eq!(extracted.item(), &*vanilla_items::DIAMOND);
    first.stack.set(CUSTOM_NAME, TextComponent::plain("edited"));
    let second = store.materialize(key).expect("independent candidate");
    assert!(!second.stack.has(CUSTOM_NAME));
}

#[test]
fn rollback_and_last_lookup_guard_own_capacity() {
    let store = store(1);
    let item = ItemStack::new(&vanilla_items::STONE);
    let aborted = store
        .capture(&item)
        .expect("capture before construction failure");
    assert!(matches!(
        store.capture(&item),
        Err(ItemBridgeError::Capacity)
    ));
    drop(aborted);
    let key = store
        .capture(&item)
        .expect("rollback released permit")
        .publish();
    let guard = store.lookup(key).expect("in-flight lookup");
    store.release(key);
    store.release(key);
    assert!(matches!(
        store.lookup(key),
        Err(ItemBridgeError::StaleLease)
    ));
    assert!(matches!(
        store.capture(&item),
        Err(ItemBridgeError::Capacity)
    ));
    assert_eq!(guard.stack().item(), &*vanilla_items::STONE);
    drop(guard);
    assert!(store.capture(&item).is_ok());
}

#[test]
fn candidates_have_independent_capacity_and_closed_epochs_do_not_alias() {
    let store = store(1);
    let item = ItemStack::new(&vanilla_items::STONE);
    let key = store
        .capture(&item)
        .expect("full retained capacity")
        .publish();
    let candidate = store.materialize(key).expect("separate candidate budget");
    assert!(matches!(
        store.materialize(key),
        Err(ItemBridgeError::Capacity)
    ));
    drop(candidate);
    assert!(store.materialize(key).is_ok());
    let other = SnapshotStore::new(NonZeroUsize::new(1).expect("limit"));
    assert!(matches!(
        other.lookup(key),
        Err(ItemBridgeError::StaleLease)
    ));
    let guard = store.lookup(key).expect("lookup before terminal close");
    store.close();
    assert_eq!(guard.stack().item(), &*vanilla_items::STONE);
    assert!(matches!(store.capture(&item), Err(ItemBridgeError::Closed)));
    assert!(matches!(
        store.materialize(key),
        Err(ItemBridgeError::Closed)
    ));
    store.release(key);
}

fn text_chain(children: usize) -> TextComponent {
    let mut text = TextComponent::plain("leaf");
    for _ in 0..children {
        let mut parent = TextComponent::new();
        parent.children.push(text);
        text = parent;
    }
    text
}

#[test]
fn preflight_rejects_before_clone_and_recovers_for_next_capture() {
    let store = store(1);
    let mut item = ItemStack::new(&vanilla_items::STONE);
    // Root patch is level zero, component one and its text root two.
    item.set(CUSTOM_NAME, text_chain(preflight::MAX_DEPTH - 2));
    let capture = store.capture(&item).expect("accepted boundary");
    assert!(store.lookup(capture.key()).is_ok());
    drop(capture);
    item.set(CUSTOM_NAME, text_chain(preflight::MAX_DEPTH - 1));
    assert!(matches!(
        store.capture(&item),
        Err(ItemBridgeError::Depth(_))
    ));
    item.set(CUSTOM_NAME, TextComponent::plain("valid sibling"));
    assert!(store.capture(&item).is_ok());
}
