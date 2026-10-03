//! Bukkit enchantment-view transport, keeping registry resolution on the Rust side.

use foton_core::event::{EnchantmentOffer, EnchantmentViewState};
use foton_registry::{REGISTRY, RegistryExt as _};
use jni::{
    JNIEnv,
    objects::{JClass, JString},
    sys::{jboolean, jint, jlong, jstring},
};

use crate::natives;

pub(crate) fn encode(state: &EnchantmentViewState) -> String {
    let mut fields = vec![state.seed.to_string()];
    for (index, offer) in state.offers.iter().enumerate() {
        fields.push(offer.map_or_else(
            || format!("{},,-1", state.costs[index]),
            |offer| {
                format!(
                    "{},{},{}",
                    state.costs[index], offer.enchantment.key, offer.level
                )
            },
        ));
    }
    fields.join(";")
}

pub(crate) fn decode(value: &str) -> Option<EnchantmentViewState> {
    let mut fields = value.split(';');
    let mut state = EnchantmentViewState {
        seed: fields.next()?.parse().ok()?,
        costs: [0; 3],
        offers: [None; 3],
    };
    for index in 0..3 {
        let mut offer = fields.next()?.split(',');
        let cost = offer.next()?.parse().ok()?;
        let key = offer.next()?;
        let level = offer.next()?.parse().ok()?;
        if offer.next().is_some() {
            return None;
        }
        state.costs[index] = cost;
        if !key.is_empty() {
            let enchantment = REGISTRY.enchantments.by_key(&key.parse().ok()?)?;
            state.offers[index] = Some(EnchantmentOffer {
                enchantment,
                level,
                cost,
            });
        }
    }
    fields.next().is_none().then_some(state)
}

pub(crate) extern "system" fn view(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jstring {
    let result = natives::player(&mut env, &uuid)
        .and_then(|player| player.enchantment_view())
        .map(|(instance, pos, world, state)| {
            format!(
                "{instance} {world} {} {} {}\u{001f}{}",
                pos.x(),
                pos.y(),
                pos.z(),
                encode(&state)
            )
        });
    natives::to_java(&mut env, result)
}

pub(crate) extern "system" fn set_view(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    instance: jlong,
    state: JString<'_>,
) -> jboolean {
    let Ok(encoded) = env.get_string(&state) else {
        return 0;
    };
    let Some(state) = decode(&encoded.to_string_lossy()) else {
        return 0;
    };
    jboolean::from(
        natives::player(&mut env, &uuid)
            .is_some_and(|player| player.set_enchantment_view(instance as u64, state)),
    )
}

pub(crate) extern "system" fn title(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    instance: jlong,
) -> jstring {
    let result = natives::player(&mut env, &uuid)
        .and_then(|player| player.enchantment_title(instance as u64));
    natives::to_java(&mut env, result)
}

pub(crate) extern "system" fn item(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    instance: jlong,
    slot: jint,
) -> jstring {
    let result = usize::try_from(slot)
        .ok()
        .and_then(|index| {
            natives::player(&mut env, &uuid)?.enchantment_item(instance as u64, index)
        })
        .map(|stack| natives::describe_slot(&stack));
    natives::to_java(&mut env, result)
}

pub(crate) extern "system" fn set_item(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    instance: jlong,
    slot: jint,
    item: JString<'_>,
) -> jboolean {
    let Ok(index) = usize::try_from(slot) else {
        return 0;
    };
    let Ok(encoded) = env.get_string(&item) else {
        return 0;
    };
    let Some(stack) = natives::parse_slot(&encoded.to_string_lossy()) else {
        return 0;
    };
    jboolean::from(
        natives::player(&mut env, &uuid)
            .is_some_and(|player| player.set_enchantment_item(instance as u64, index, stack)),
    )
}

pub(crate) extern "system" fn close(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    instance: jlong,
) -> jboolean {
    jboolean::from(
        natives::player(&mut env, &uuid)
            .is_some_and(|player| player.close_enchantment_view(instance as u64)),
    )
}
