use foton_core::behavior as bridge_behavior;
use foton_registry::data_components::components::ItemAttributeModifiers;
use foton_registry::data_components::vanilla_components::{
    ATTRIBUTE_MODIFIERS, CUSTOM_NAME, DAMAGE, GLIDER,
};
use foton_registry::vanilla_mob_effects as bridge_vanilla_mob_effects;
use foton_registry::{init_vanilla_registry, item_stack::ItemStack, vanilla_items};
use jni::JavaVM;
use jni::objects::{JObject, JValue};
use std::num as bridge_num;
use std::thread as bridge_thread;
use std::time as bridge_time;
use text_components::TextComponent;

mod attributes_profile;
mod components;
mod java_thread;
mod migrated;
mod unsupported;

/// Runs in the host's one real JVM rather than treating lease IDs as portable strings.
pub(crate) fn check(vm: &JavaVM) {
    let mut env = vm
        .attach_current_thread()
        .expect("attach item carrier test");
    // The targeted JVM test starts before registry publication; a full parallel
    // native suite may have already initialized it in another registry test.
    if foton_registry::REGISTRY.get().is_none() {
        let mutation = env
            .call_static_method(
                "foton/item/ItemMutation",
                "empty",
                "()Lfoton/item/ItemMutation;",
                &[],
            )
            .expect("owning empty mutation before registry publication")
            .l()
            .expect("mutation");
        let Err(error) = super::mutation::materialize(&mut env, &mutation) else {
            panic!("pre-registry canonical mutation unexpectedly accepted");
        };
        assert!(matches!(error, super::ItemBridgeError::RegistryNotReady));
        error.throw_java(&mut env);
        let exception = env.exception_occurred().expect("readiness exception");
        env.exception_clear()
            .expect("clear expected readiness refusal");
        assert!(
            env.is_instance_of(exception, "java/lang/IllegalStateException")
                .expect("exception category")
        );
        eprintln!("verified actual pre-registry canonical refusal: IllegalStateException");
    }
    init_vanilla_registry();
    bridge_behavior::init_behaviors();
    components::check(&mut env);
    migrated::check(&mut env);
    unsupported::check(&mut env);
    attributes_profile::check(&mut env);
    super::merchant::check(&mut env);
    java_thread::check(&mut env);
    check_gc_shared_referent(&mut env);
    check_block_state_conversion(&mut env);
    check_unprojected_potion_refusal(&mut env);
    check_deep_snapshot(&mut env);
}

fn check_deep_snapshot(env: &mut jni::JNIEnv<'_>) {
    let mut stack = ItemStack::new(&vanilla_items::IRON_CHESTPLATE);
    stack.set(GLIDER, ());
    stack.remove(DAMAGE);
    stack.set(ATTRIBUTE_MODIFIERS, ItemAttributeModifiers::empty());
    let mut text = TextComponent::plain("leaf");
    for _ in 0..126 {
        let mut parent = TextComponent::plain("parent");
        parent.children.push(text);
        text = parent;
    }
    stack.set(CUSTOM_NAME, text);
    let transfer = super::transfer::capture(env, &stack).expect("boundary JNI capture");
    check_block_state_clone(env, &transfer);
    let decoded = env.call_static_method(
        "foton/FotonInventory",
        "decodeTransfer",
        "(Lfoton/item/ItemTransfer;)Lorg/bukkit/inventory/ItemStack;",
        &[JValue::Object(&transfer)],
    );
    if decoded.is_err() {
        env.exception_describe()
            .expect("describe hydration failure");
    }
    let item = decoded
        .expect("hydrate owning transfer")
        .l()
        .expect("stack");
    assert!(
        env.call_method(&item, "hasItemMeta", "()Z", &[])
            .expect("canonical patch presence")
            .z()
            .expect("presence")
    );
    check_snapshot_metadata(env, &item);
    let lease = env
        .call_method(&transfer, "lease", "()Lfoton/item/NativeItemLease;", &[])
        .expect("owning transfer")
        .l()
        .expect("lease");
    let key = super::transfer::lease_key(env, &lease).expect("key");
    let held = super::store()
        .expect("store")
        .lookup(key)
        .expect("snapshot guard");
    assert!(held.stack().has(GLIDER));
    assert!(!held.stack().has(DAMAGE));
    let second_owner = env.new_global_ref(&lease).expect("independent Java owner");
    env.delete_local_ref(transfer)
        .expect("discard transfer local");
    env.delete_local_ref(lease)
        .expect("discard first lease local");
    let cleanup = env
        .get_field(
            second_owner.as_obj(),
            "cleanup",
            "Ljava/lang/ref/Cleaner$Cleanable;",
        )
        .expect("deterministic cleanup handle")
        .l()
        .expect("cleanable");
    env.call_method(&cleanup, "clean", "()V", &[])
        .expect("Cleaner native release");
    env.call_method(&cleanup, "clean", "()V", &[])
        .expect("idempotent cleanup");
    assert!(matches!(
        super::store().expect("store").lookup(key),
        Err(super::ItemBridgeError::StaleLease)
    ));
    assert!(
        held.stack().has(GLIDER),
        "native lookup guard outlives table removal"
    );
    assert!(!env.exception_check().expect("exception state"));
    let local = super::SnapshotStore::new(bridge_num::NonZeroUsize::MIN);
    let unpublished = local.capture(&stack).expect("one retained permit");
    assert!(super::transfer::construct(env, unpublished, &JObject::null()).is_err());
    assert!(env.exception_check().expect("constructor failure"));
    env.exception_clear()
        .expect("clear expected null projection failure");
    assert!(
        local.capture(&stack).is_ok(),
        "failed JNI constructor must roll back admission"
    );
}

fn check_gc_shared_referent(env: &mut jni::JNIEnv<'_>) {
    let source = ItemStack::new(&vanilla_items::STONE);
    let transfer = super::transfer::capture(env, &source).expect("GC fixture transfer");
    let lease = env
        .call_method(&transfer, "lease", "()Lfoton/item/NativeItemLease;", &[])
        .expect("GC fixture lease")
        .l()
        .expect("lease");
    let key = super::transfer::lease_key(env, &lease).expect("lease key");
    let item = env
        .call_static_method(
            "foton/FotonInventory",
            "decodeTransfer",
            "(Lfoton/item/ItemTransfer;)Lorg/bukkit/inventory/ItemStack;",
            &[JValue::Object(&transfer)],
        )
        .expect("GC fixture item")
        .l()
        .expect("item");
    let clone = env
        .call_method(&item, "clone", "()Lorg/bukkit/inventory/ItemStack;", &[])
        .expect("GC fixture clone")
        .l()
        .expect("clone");
    let meta = env
        .call_method(
            &clone,
            "getItemMeta",
            "()Lorg/bukkit/inventory/meta/ItemMeta;",
            &[],
        )
        .expect("GC fixture detached meta")
        .l()
        .expect("meta");
    for object in [item, clone, transfer, lease] {
        env.delete_local_ref(object).expect("drop Java owner");
    }
    env.call_static_method("java/lang/System", "gc", "()V", &[])
        .expect("fixture GC");
    assert!(
        super::store().expect("store").lookup(key).is_ok(),
        "detached metadata retains the shared referent"
    );
    env.delete_local_ref(meta)
        .expect("release final Java metadata owner");
    let deadline = bridge_time::Instant::now() + bridge_time::Duration::from_secs(5);
    loop {
        if matches!(
            super::store().expect("store").lookup(key),
            Err(super::ItemBridgeError::StaleLease)
        ) {
            break;
        }
        assert!(
            bridge_time::Instant::now() < deadline,
            "Cleaner did not release unreachable shared lease within fixture deadline"
        );
        env.call_static_method("java/lang/System", "gc", "()V", &[])
            .expect("fixture GC");
        // Test-only bounded observation, never a game-tick or production cleanup policy.
        bridge_thread::sleep(bridge_time::Duration::from_millis(25));
    }
}

fn check_block_state_clone(env: &mut jni::JNIEnv<'_>, transfer: &JObject<'_>) {
    let meta = env
        .new_object("org/bukkit/inventory/meta/SimpleBlockStateMeta", "()V", &[])
        .expect("block-state meta");
    env.call_method(
        &meta,
        "attachNativeState",
        "(Lfoton/item/ItemTransfer;)V",
        &[JValue::Object(transfer)],
    )
    .expect("attach block-state carrier");
    let name = env.new_string("retained common name").expect("common name");
    env.call_method(
        &meta,
        "setDisplayName",
        "(Ljava/lang/String;)V",
        &[JValue::Object(&name)],
    )
    .expect("edit common field");
    let copy = env
        .call_method(
            &meta,
            "clone",
            "()Lorg/bukkit/inventory/meta/SimpleBlockStateMeta;",
            &[],
        )
        .expect("clone block-state metadata")
        .l()
        .expect("clone");
    let copied_name = env
        .call_method(&copy, "getDisplayName", "()Ljava/lang/String;", &[])
        .expect("copied name")
        .l()
        .expect("string");
    assert_eq!(
        env.get_string(&copied_name.into())
            .expect("name value")
            .to_str()
            .expect("UTF8"),
        "retained common name"
    );
    let state = env
        .call_method(&copy, "nativeState", "()Lfoton/item/LiveItemState;", &[])
        .expect("cloned carrier")
        .l()
        .expect("carrier");
    assert!(
        env.call_method(&state, "hasNativeBase", "()Z", &[])
            .expect("carrier retained by clone")
            .z()
            .expect("carrier presence")
    );
    assert!(
        env.call_method(
            &copy,
            "getBlockState",
            "()Lorg/bukkit/block/BlockState;",
            &[]
        )
        .is_err()
    );
    let error = env.exception_occurred().expect("materialization exception");
    env.exception_clear()
        .expect("clear explicit unsupported capability");
    assert!(
        env.is_instance_of(error, "java/lang/UnsupportedOperationException")
            .expect("capability category")
    );
    assert!(
        env.call_method(
            &copy,
            "setBlockState",
            "(Lorg/bukkit/block/BlockState;)V",
            &[JValue::Object(&JObject::null())]
        )
        .is_err()
    );
    let error = env.exception_occurred().expect("mutation exception");
    env.exception_clear().expect("clear unsupported mutation");
    assert!(
        env.is_instance_of(error, "java/lang/UnsupportedOperationException")
            .expect("mutation category")
    );
}

fn check_donor_targets(env: &mut jni::JNIEnv<'_>, meta: &JObject<'_>) {
    for (material, accepted) in [
        ("DIAMOND_CHESTPLATE", true),
        ("STONE", false),
        ("LEATHER_CHESTPLATE", false),
    ] {
        let kind = env
            .get_static_field("org/bukkit/Material", material, "Lorg/bukkit/Material;")
            .expect("material")
            .l()
            .expect("material object");
        let target = env
            .new_object(
                "org/bukkit/inventory/ItemStack",
                "(Lorg/bukkit/Material;)V",
                &[JValue::Object(&kind)],
            )
            .expect("target stack");
        let applied = env.call_method(
            &target,
            "setItemMeta",
            "(Lorg/bukkit/inventory/meta/ItemMeta;)Z",
            &[JValue::Object(meta)],
        );
        if applied.is_err() {
            env.exception_describe().expect("donor failure");
        }
        assert_eq!(
            applied
                .expect("donor application")
                .z()
                .expect("applicability"),
            accepted,
            "{material}"
        );
        let mutation = env
            .call_method(
                &target,
                "nativeMutation",
                "()Lfoton/item/ItemMutation;",
                &[],
            )
            .expect("target mutation")
            .l()
            .expect("mutation");
        let state = super::mutation::materialize(env, &mutation).expect("target state");
        assert_eq!(
            state.stack.has(GLIDER),
            accepted,
            "{material}: donor hidden state"
        );
        if accepted {
            assert!(!state.stack.has(DAMAGE));
            assert_eq!(
                state.stack.get(ATTRIBUTE_MODIFIERS),
                Some(&ItemAttributeModifiers::empty())
            );
        } else {
            assert!(
                state.stack.patch().is_empty(),
                "rejected donor modified target"
            );
        }
    }
}

fn check_block_state_conversion(env: &mut jni::JNIEnv<'_>) {
    let mut source = ItemStack::new(&vanilla_items::SHULKER_BOX);
    source.set(GLIDER, ());
    let transfer = super::transfer::capture(env, &source).expect("block-state carrier");
    let item = env
        .call_static_method(
            "foton/FotonInventory",
            "decodeTransfer",
            "(Lfoton/item/ItemTransfer;)Lorg/bukkit/inventory/ItemStack;",
            &[JValue::Object(&transfer)],
        )
        .expect("block-state hydration")
        .l()
        .expect("item");
    let donor = env
        .call_method(
            &item,
            "getItemMeta",
            "()Lorg/bukkit/inventory/meta/ItemMeta;",
            &[],
        )
        .expect("block-state donor")
        .l()
        .expect("meta");
    assert!(
        env.call_method(
            &item,
            "setItemMeta",
            "(Lorg/bukkit/inventory/meta/ItemMeta;)Z",
            &[JValue::Object(&donor)]
        )
        .expect("same-material transfer")
        .z()
        .expect("accepted")
    );
    let changed = env
        .call_method(&item, "nativeMutation", "()Lfoton/item/ItemMutation;", &[])
        .expect("same-material mutation")
        .l()
        .expect("mutation");
    assert_eq!(
        super::mutation::materialize(env, &changed)
            .expect("same-material native state")
            .stack,
        source
    );
    check_cross_block_partition(env, &donor, &changed, &source);
    let empty = env
        .new_object("org/bukkit/inventory/meta/SimplePotionMeta", "()V", &[])
        .expect("empty incompatible meta");
    assert!(
        env.call_method(
            &item,
            "setItemMeta",
            "(Lorg/bukkit/inventory/meta/ItemMeta;)Z",
            &[JValue::Object(&empty)]
        )
        .expect("empty incompatible clears before applicability")
        .z()
        .expect("clear accepted")
    );
    let cleared = env
        .call_method(&item, "nativeMutation", "()Lfoton/item/ItemMutation;", &[])
        .expect("cleared mutation")
        .l()
        .expect("mutation");
    assert!(
        super::mutation::materialize(env, &cleared)
            .expect("cleared patch")
            .stack
            .patch()
            .is_empty()
    );
}

fn check_unprojected_potion_refusal(env: &mut jni::JNIEnv<'_>) {
    use foton_registry::data_components::{
        components::PotionContents, vanilla_components::POTION_CONTENTS,
    };
    use foton_registry::mob_effect_instance::{MobEffectInstance, MobEffectInstanceDetails};
    let hidden = MobEffectInstanceDetails::new(0, 40, false, true, true, None);
    let effect = MobEffectInstance::new(
        bridge_vanilla_mob_effects::SPEED,
        80,
        1,
        true,
        false,
        false,
        Some(hidden),
    );
    let mut source = ItemStack::new(&vanilla_items::POTION);
    source.set(
        POTION_CONTENTS,
        PotionContents::new(
            None,
            Some(123),
            vec![effect],
            Some("hidden name".to_owned()),
        ),
    );
    source.set(GLIDER, ());
    let transfer = super::transfer::capture(env, &source).expect("potion capture");
    let item = env
        .call_static_method(
            "foton/FotonInventory",
            "decodeTransfer",
            "(Lfoton/item/ItemTransfer;)Lorg/bukkit/inventory/ItemStack;",
            &[JValue::Object(&transfer)],
        )
        .expect("potion transfer")
        .l()
        .expect("item");
    let donor = env
        .call_method(
            &item,
            "getItemMeta",
            "()Lorg/bukkit/inventory/meta/ItemMeta;",
            &[],
        )
        .expect("potion donor")
        .l()
        .expect("meta");
    env.call_method(&donor, "clearCustomEffects", "()Z", &[])
        .expect("record explicit effect clear");
    assert!(
        env.call_method(
            &item,
            "setItemMeta",
            "(Lorg/bukkit/inventory/meta/ItemMeta;)Z",
            &[JValue::Object(&donor)]
        )
        .is_err()
    );
    let error = env
        .exception_occurred()
        .expect("unprojected effect refusal");
    env.exception_clear()
        .expect("clear expected effect refusal");
    assert!(
        env.is_instance_of(error, "java/lang/UnsupportedOperationException")
            .expect("effect capability category")
    );
    let mutation = env
        .call_method(&item, "nativeMutation", "()Lfoton/item/ItemMutation;", &[])
        .expect("unchanged potion")
        .l()
        .expect("mutation");
    assert_eq!(
        super::mutation::materialize(env, &mutation)
            .expect("unchanged potion state")
            .stack,
        source
    );
}

pub(crate) fn check_terminal(host: &crate::PluginHost) {
    use std::{
        future::Future,
        task::{Context, Poll, Waker},
    };
    let candidate = super::store()
        .expect("store")
        .candidate(ItemStack::new(&vanilla_items::STONE))
        .expect("unmanaged operation");
    host.mark_stopping()
        .expect("zero-plugin Java terminal handshake");
    host.disable_all().expect("Java teardown");
    assert!(matches!(
        super::store()
            .expect("store")
            .candidate(ItemStack::new(&vanilla_items::STONE)),
        Err(super::ItemBridgeError::Closed)
    ));
    let mut barrier = std::pin::pin!(crate::PluginHost::drain_item_operations());
    let mut context = Context::from_waker(Waker::noop());
    assert!(
        matches!(barrier.as_mut().poll(&mut context), Poll::Pending),
        "Java teardown must not imply native-operation completion"
    );
    drop(candidate);
    assert!(matches!(
        barrier.as_mut().poll(&mut context),
        Poll::Ready(())
    ));
}

fn check_removed_damage(env: &mut jni::JNIEnv<'_>, item: &JObject<'_>) {
    let damage_type = env
        .get_static_field(
            "io/papermc/paper/datacomponent/DataComponentTypes",
            "DAMAGE",
            "Lio/papermc/paper/datacomponent/DataComponentType$Valued;",
        )
        .expect("damage type")
        .l()
        .expect("type");
    let damage = env
        .call_method(
            item,
            "getData",
            "(Lio/papermc/paper/datacomponent/DataComponentType$Valued;)Ljava/lang/Object;",
            &[JValue::Object(&damage_type)],
        )
        .expect("canonical component query")
        .l()
        .expect("nullable damage");
    assert!(
        damage.is_null(),
        "explicit removal must not become a Java zero default"
    );
}

fn check_snapshot_metadata(env: &mut jni::JNIEnv<'_>, item: &JObject<'_>) {
    check_removed_damage(env, item);
    let meta = env
        .call_method(
            item,
            "getItemMeta",
            "()Lorg/bukkit/inventory/meta/ItemMeta;",
            &[],
        )
        .expect("detached metadata")
        .l()
        .expect("metadata");
    for (object, method, signature) in [
        (item, "serialize", "()Ljava/util/Map;"),
        (item, "serializeAsBytes", "()[B"),
        (&meta, "serialize", "()Ljava/util/Map;"),
    ] {
        assert!(
            env.call_method(object, method, signature, &[]).is_err(),
            "live persistence must fail explicitly"
        );
        let error = env.exception_occurred().expect("persistence refusal");
        env.exception_clear()
            .expect("clear expected persistence refusal");
        assert!(
            env.is_instance_of(error, "java/lang/UnsupportedOperationException")
                .expect("persistence category")
        );
    }
    check_donor_targets(env, &meta);
    let name = env.new_string("Unrelated edit").expect("name");
    env.call_method(
        &meta,
        "setDisplayName",
        "(Ljava/lang/String;)V",
        &[JValue::Object(&name)],
    )
    .expect("unrelated name edit");
    assert!(
        env.call_method(
            item,
            "setItemMeta",
            "(Lorg/bukkit/inventory/meta/ItemMeta;)Z",
            &[JValue::Object(&meta)]
        )
        .expect("apply metadata")
        .z()
        .expect("meta applicability")
    );
    let mutation = env
        .call_method(item, "nativeMutation", "()Lfoton/item/ItemMutation;", &[])
        .expect("owning mutation")
        .l()
        .expect("mutation");
    let candidate = super::mutation::materialize(env, &mutation).expect("native candidate");
    assert!(
        candidate.stack.has(GLIDER),
        "unrelated Java edit lost GLIDER"
    );
    assert!(
        !candidate.stack.has(DAMAGE),
        "unrelated Java edit lost explicit removal"
    );
    assert_eq!(
        candidate.stack.get(ATTRIBUTE_MODIFIERS),
        Some(&ItemAttributeModifiers::empty()),
        "null projected attribute getters must not restore prototype modifiers"
    );
}

fn check_cross_block_partition(
    env: &mut jni::JNIEnv<'_>,
    donor: &JObject<'_>,
    changed: &JObject<'_>,
    source: &ItemStack,
) {
    let color = env
        .get_static_field(
            "org/bukkit/Material",
            "RED_SHULKER_BOX",
            "Lorg/bukkit/Material;",
        )
        .expect("colored material")
        .l()
        .expect("material");
    let target = env
        .new_object(
            "org/bukkit/inventory/ItemStack",
            "(Lorg/bukkit/Material;)V",
            &[JValue::Object(&color)],
        )
        .expect("colored target");
    assert!(
        env.call_method(
            &target,
            "setItemMeta",
            "(Lorg/bukkit/inventory/meta/ItemMeta;)Z",
            &[JValue::Object(donor)]
        )
        .is_err()
    );
    let error = env.exception_occurred().expect("partition refusal");
    env.exception_clear()
        .expect("clear expected partition refusal");
    assert!(
        env.is_instance_of(error, "java/lang/UnsupportedOperationException")
            .expect("partition category")
    );
    let target_mutation = env
        .call_method(
            &target,
            "nativeMutation",
            "()Lfoton/item/ItemMutation;",
            &[],
        )
        .expect("refused target")
        .l()
        .expect("mutation");
    assert!(
        super::mutation::materialize(env, &target_mutation)
            .expect("unchanged target")
            .stack
            .patch()
            .is_empty()
    );
    assert_eq!(
        super::mutation::materialize(env, changed)
            .expect("unchanged source")
            .stack,
        *source
    );
}
