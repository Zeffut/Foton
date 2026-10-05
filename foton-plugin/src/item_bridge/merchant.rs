//! Complete offer batches own every item permit until destination replacement.

use crate::natives as bridge_native_bridge;
use foton_core::entity::entities::mobs::npc::VillagerEntity;
use foton_registry::{
    item_stack::ItemStack,
    trading::{ItemCost, MerchantOffer, MerchantOffers},
};
use foton_utils::Downcast as _;
use jni::{
    JNIEnv,
    objects::{JClass, JObject, JObjectArray, JString, JValue},
    sys::jobjectArray,
};
use std::mem as bridge_mem;
use std::ptr as bridge_ptr;

use super::{
    ItemBridgeError,
    entity::identity,
    mutation,
    snapshot::{Candidate, Capture},
    store, transfer,
};

pub(crate) struct OfferBatch {
    pub(crate) offers: MerchantOffers,
    // Stacks move into offers, but operation admission survives the final replacement.
    _candidates: Vec<Candidate>,
}

fn integer(env: &mut JNIEnv<'_>, value: &JObject<'_>, name: &str) -> Result<i32, ItemBridgeError> {
    Ok(env.get_field(value, name, "I")?.i()?)
}

fn take(candidate: &mut Candidate) -> ItemStack {
    bridge_mem::replace(&mut candidate.stack, ItemStack::empty())
}

pub(crate) fn prepare(
    env: &mut JNIEnv<'_>,
    values: &JObjectArray<'_>,
) -> Result<OfferBatch, ItemBridgeError> {
    let size = env.get_array_length(values)?;
    if size > 4096 {
        return Err(ItemBridgeError::TransportLimit);
    }
    let mut offers = MerchantOffers::new();
    let mut candidates = vec![store()?.stage(|| Ok(ItemStack::empty()))?];
    for index in 0..size {
        let value = env.get_object_array_element(values, index)?;
        if value.is_null() {
            return Err(ItemBridgeError::InvalidEdit("missing merchant offer"));
        }
        let inputs = JObjectArray::from(
            env.get_field(&value, "ingredients", "[Lfoton/item/ItemMutation;")?
                .l()?,
        );
        let count = env.get_array_length(&inputs)?;
        if !(1..=2).contains(&count) {
            return Err(ItemBridgeError::NativeState(
                "merchant recipe requires one or two ingredients".into(),
            ));
        }
        let mut costs = Vec::new();
        for input in 0..count {
            let input = env.get_object_array_element(&inputs, input)?;
            let mut candidate = mutation::materialize(env, &input)?;
            let cost = ItemCost::try_from_ingredient(take(&mut candidate)).ok_or(
                ItemBridgeError::InvalidEdit("empty or invalid merchant ingredient predicate"),
            )?;
            costs.push(cost);
            candidates.push(candidate);
        }
        let result = env
            .get_field(&value, "result", "Lfoton/item/ItemMutation;")?
            .l()?;
        let mut result = mutation::materialize(env, &result)?;
        if result.stack.is_empty() {
            return Err(ItemBridgeError::InvalidEdit("empty merchant result"));
        }
        let mut costs = costs.into_iter();
        let first = costs
            .next()
            .ok_or(ItemBridgeError::InvalidEdit("missing merchant ingredient"))?;
        let mut offer = MerchantOffer::with_uses(
            first,
            costs.next(),
            take(&mut result),
            integer(env, &value, "uses")?,
            integer(env, &value, "maxUses")?,
            integer(env, &value, "experience")?,
            env.get_field(&value, "priceMultiplier", "F")?.f()?,
            integer(env, &value, "demand")?,
        );
        offer.set_special_price_diff(integer(env, &value, "specialPrice")?);
        offer.set_reward_exp(env.get_field(&value, "rewardExperience", "Z")?.z()?);
        offers.push(offer);
        candidates.push(result);
    }
    Ok(OfferBatch {
        offers,
        _candidates: candidates,
    })
}

pub(crate) extern "system" fn set(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    values: JObjectArray<'_>,
) {
    let result = (|| -> Result<(), ItemBridgeError> {
        let mut batch = prepare(&mut env, &values)?;
        let id = identity(&mut env, &uuid)?;
        if let Some((_, entity)) = bridge_native_bridge::entity_by_uuid(&id)
            && let Some(villager) = entity.downcast_ref::<VillagerEntity>()
        {
            villager
                .merchant()
                .set_offers(bridge_mem::take(&mut batch.offers));
        }
        Ok(())
    })();
    if let Err(error) = result {
        error.throw_java(&mut env);
    }
}

pub(crate) struct CapturedOffer {
    result: Capture<'static>,
    first: Capture<'static>,
    second: Option<Capture<'static>>,
    uses: i32,
    max_uses: i32,
    demand: i32,
    experience: i32,
    special_price: i32,
    multiplier: f32,
    reward: bool,
}

pub(crate) fn capture(offers: &MerchantOffers) -> Result<Vec<CapturedOffer>, ItemBridgeError> {
    let store = store()?;
    if offers.len() > 4096 {
        return Err(ItemBridgeError::TransportLimit.native_state());
    }
    offers
        .iter()
        .map(|offer| {
            if offer.result().is_empty() {
                return Err(ItemBridgeError::NativeState("empty merchant result".into()));
            }
            Ok(CapturedOffer {
                result: store
                    .capture(offer.result())
                    .map_err(ItemBridgeError::native_state)?,
                first: store
                    .capture(offer.item_cost_a().cost_stack())
                    .map_err(ItemBridgeError::native_state)?,
                second: offer
                    .item_cost_b()
                    .map(|cost| {
                        store
                            .capture(cost.cost_stack())
                            .map_err(ItemBridgeError::native_state)
                    })
                    .transpose()?,
                uses: offer.uses(),
                max_uses: offer.max_uses(),
                demand: offer.demand(),
                experience: offer.xp(),
                special_price: offer.special_price_diff(),
                multiplier: offer.price_multiplier(),
                reward: offer.should_reward_exp(),
            })
        })
        .collect()
}

fn read(env: &mut JNIEnv<'_>, uuid: &JString<'_>) -> Result<jobjectArray, ItemBridgeError> {
    let id = identity(env, uuid)?;
    let Some((_, entity)) = bridge_native_bridge::entity_by_uuid(&id) else {
        return Ok(bridge_ptr::null_mut());
    };
    let Some(villager) = entity.downcast_ref::<VillagerEntity>() else {
        return Ok(bridge_ptr::null_mut());
    };
    let offers = villager.with_offers(capture)?;
    transfers(env, offers).map(JObjectArray::into_raw)
}

pub(crate) fn transfers<'local>(
    env: &mut JNIEnv<'local>,
    offers: Vec<CapturedOffer>,
) -> Result<JObjectArray<'local>, ItemBridgeError> {
    let result = env.new_object_array(
        i32::try_from(offers.len()).map_err(|_| ItemBridgeError::TransportLimit)?,
        "foton/item/MerchantOfferTransfer",
        JObject::null(),
    )?;
    for (index, offer) in offers.into_iter().enumerate() {
        let output = transfer::from_capture(env, offer.result)?;
        let first = transfer::from_capture(env, offer.first)?;
        let second = match offer.second {
            Some(value) => transfer::from_capture(env, value)?,
            None => JObject::null(),
        };
        let value = env.new_object(
            "foton/item/MerchantOfferTransfer",
            "(Lfoton/item/ItemTransfer;Lfoton/item/ItemTransfer;Lfoton/item/ItemTransfer;IIIIIFZ)V",
            &[
                JValue::Object(&output),
                JValue::Object(&first),
                JValue::Object(&second),
                JValue::Int(offer.uses),
                JValue::Int(offer.max_uses),
                JValue::Int(offer.demand),
                JValue::Int(offer.experience),
                JValue::Int(offer.special_price),
                JValue::Float(offer.multiplier),
                JValue::Bool(u8::from(offer.reward)),
            ],
        )?;
        env.set_object_array_element(
            &result,
            i32::try_from(index).map_err(|_| ItemBridgeError::TransportLimit)?,
            value,
        )?;
    }
    Ok(result)
}

#[cfg(test)]
mod tests;
#[cfg(test)]
pub(crate) use tests::check;

pub(crate) extern "system" fn get(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jobjectArray {
    match read(&mut env, &uuid) {
        Ok(value) => value,
        Err(error) => {
            error.throw_java(&mut env);
            bridge_ptr::null_mut()
        }
    }
}
