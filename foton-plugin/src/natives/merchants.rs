//! Merchants a plugin makes with `Bukkit.createMerchant`: a trading screen
//! with the plugin's offers and no villager behind it.
//!
//! Vanilla parity: `ClientSideMerchant`, the merchant vanilla uses when there
//! is no mob -- no experience, no level badge, the villager's "yes" on a trade,
//! and uses counted on its own offers. The offers live here, so a plugin that
//! reads its merchant after a trade sees the uses the trade added.
//!
//! An offer crosses as its fields separated by `\u{1e}`: result, uses, max
//! uses, experience reward (0/1), villager experience, price multiplier,
//! demand, special price, first cost, second cost (empty for none). Items use
//! the inventory encoding, components included, so a trade can ask for a
//! plugin's custom item and not just its material.

use std::ffi::c_void;
use std::ptr::null_mut;
use std::sync::{Arc, OnceLock};

use foton_core::entity::Entity as _;
use foton_core::entity::entities::{VillagerEntity, WanderingTraderEntity};
use foton_core::player::Player;
use foton_core::trading::{Merchant, open_trading_screen};
use foton_registry::data_component_predicate::DataComponentExactPredicate;
use foton_registry::item_stack::ItemStack;
use foton_registry::sound_event::SoundEventRef;
use foton_registry::sound_events;
use foton_registry::trading::{ItemCost, MerchantOffer, MerchantOffers};
use foton_utils::Downcast as _;
use foton_utils::locks::SyncMutex;
use jni::JNIEnv;
use jni::objects::{JClass, JObjectArray, JString};
use jni::sys::{jboolean, jint, jobjectArray, jstring};
use rustc_hash::FxHashMap;
use text_components::TextComponent;
use uuid::Uuid;

use super::support::{component, entity, method, player, text};
use super::{describe_slot, parse_slot, read_string_array, server, string_array, to_java};

const FIELD: char = '\u{1e}';

/// A merchant with no mob behind it.
struct PluginMerchant {
    title: TextComponent,
    offers: SyncMutex<MerchantOffers>,
    trading_player: SyncMutex<Option<Uuid>>,
}

impl Merchant for PluginMerchant {
    fn offers(&self) -> &SyncMutex<MerchantOffers> {
        &self.offers
    }

    fn trading_player(&self) -> Option<Uuid> {
        *self.trading_player.lock()
    }

    fn set_trading_player(&self, player: Option<Uuid>) {
        *self.trading_player.lock() = player;
    }

    fn notify_trade(&self, offer_index: usize) {
        if let Some(offer) = self.offers.lock().get_mut(offer_index) {
            offer.increase_uses();
        }
    }

    fn notify_trade_updated(&self, _result: &ItemStack) {}

    fn villager_xp(&self) -> i32 {
        0
    }

    fn merchant_level(&self) -> i32 {
        0
    }

    fn show_progress_bar(&self) -> bool {
        false
    }

    fn notify_trade_sound(&self) -> SoundEventRef {
        &sound_events::ENTITY_VILLAGER_YES
    }

    fn still_valid(&self, player: &Player) -> bool {
        self.trading_player() == Some(player.uuid())
    }
}

static MERCHANTS: OnceLock<SyncMutex<FxHashMap<Uuid, Arc<PluginMerchant>>>> = OnceLock::new();

fn merchants() -> &'static SyncMutex<FxHashMap<Uuid, Arc<PluginMerchant>>> {
    MERCHANTS.get_or_init(|| SyncMutex::new(FxHashMap::default()))
}

fn merchant(env: &mut JNIEnv<'_>, handle: &JString<'_>) -> Option<Arc<PluginMerchant>> {
    let id = Uuid::parse_str(&text(env, handle)?).ok()?;
    merchants().lock().get(&id).map(Arc::clone)
}

/// A cost that asks for exactly the given stack's item, count and components,
/// as `CraftMerchantRecipe` builds one from a Bukkit ingredient.
fn cost(stack: &ItemStack) -> Option<ItemCost> {
    let components = DataComponentExactPredicate::all_of(&stack.patch().added())?;
    Some(ItemCost::with_components(
        stack.item(),
        stack.count(),
        components,
    ))
}

fn encode(offer: &MerchantOffer) -> String {
    let cost_b = offer
        .item_cost_b()
        .map(|cost| describe_slot(cost.cost_stack()))
        .unwrap_or_default();
    [
        describe_slot(offer.result()),
        offer.uses().to_string(),
        offer.max_uses().to_string(),
        u8::from(offer.should_reward_exp()).to_string(),
        offer.xp().to_string(),
        offer.price_multiplier().to_string(),
        offer.demand().to_string(),
        offer.special_price_diff().to_string(),
        describe_slot(offer.item_cost_a().cost_stack()),
        cost_b,
    ]
    .join(&FIELD.to_string())
}

fn decode(encoded: &str) -> Option<MerchantOffer> {
    let fields: Vec<&str> = encoded.split(FIELD).collect();
    let [
        result,
        uses,
        max_uses,
        reward,
        xp,
        multiplier,
        demand,
        special,
        cost_a,
        cost_b,
    ] = fields.as_slice()
    else {
        return None;
    };
    let cost_a = cost(&parse_slot(cost_a).filter(|stack| !stack.is_empty())?)?;
    let cost_b = match parse_slot(cost_b)? {
        stack if stack.is_empty() => None,
        stack => Some(cost(&stack)?),
    };
    let mut offer = MerchantOffer::with_uses(
        cost_a,
        cost_b,
        parse_slot(result)?,
        uses.parse().ok()?,
        max_uses.parse().ok()?,
        xp.parse().ok()?,
        multiplier.parse().ok()?,
        demand.parse().ok()?,
    );
    offer.set_reward_exp(*reward == "1");
    offer.set_special_price_diff(special.parse().ok()?);
    Some(offer)
}

/// A new merchant titled with a component's JSON (null for vanilla's default).
extern "system" fn create_merchant(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    title: JString<'_>,
) -> jstring {
    let title = component(&mut env, &title).unwrap_or_else(|| TextComponent::plain(""));
    let id = Uuid::new_v4();
    merchants().lock().insert(
        id,
        Arc::new(PluginMerchant {
            title,
            offers: SyncMutex::new(MerchantOffers::new()),
            trading_player: SyncMutex::new(None),
        }),
    );
    to_java(&mut env, Some(id.to_string()))
}

/// Forgets a merchant the plugin no longer holds; a screen still open on it
/// keeps it alive until it closes.
extern "system" fn release_merchant(mut env: JNIEnv<'_>, _class: JClass<'_>, handle: JString<'_>) {
    if let Some(id) = text(&mut env, &handle).and_then(|value| Uuid::parse_str(&value).ok()) {
        merchants().lock().remove(&id);
    }
}

extern "system" fn merchant_offers(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    handle: JString<'_>,
) -> jobjectArray {
    let Some(merchant) = merchant(&mut env, &handle) else {
        return null_mut();
    };
    let values: Vec<String> = merchant.offers.lock().iter().map(encode).collect();
    string_array(&mut env, &values)
}

/// Replaces every offer; `false`, changing nothing, if one does not decode.
extern "system" fn set_merchant_offers(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    handle: JString<'_>,
    offers: JObjectArray<'_>,
) -> jboolean {
    let Some(merchant) = merchant(&mut env, &handle) else {
        return 0;
    };
    let Some(encoded) = read_string_array(&mut env, &offers) else {
        return 0;
    };
    let Some(decoded) = encoded
        .iter()
        .map(|offer| decode(offer))
        .collect::<Option<Vec<_>>>()
    else {
        return 0;
    };
    let mut offers = MerchantOffers::new();
    offers.extend(decoded);
    merchant.override_offers(offers);
    1
}

/// Any merchant a plugin can name: one of its own, by handle, or a villager
/// or wandering trader, by UUID -- with the title its screen opens under.
fn any_merchant(
    env: &mut JNIEnv<'_>,
    handle: &JString<'_>,
) -> Option<(Arc<dyn Merchant>, TextComponent)> {
    if let Some(merchant) = merchant(env, handle) {
        let title = merchant.title.clone();
        return Some((merchant, title));
    }
    let (_, entity) = entity(env, handle)?;
    let title = entity.display_name();
    let entity = entity.as_ref();
    if let Some(villager) = entity.downcast_ref::<VillagerEntity>() {
        return Some((Arc::clone(villager.merchant()) as Arc<dyn Merchant>, title));
    }
    let trader = entity.downcast_ref::<WanderingTraderEntity>()?;
    Some((Arc::clone(trader.merchant()) as Arc<dyn Merchant>, title))
}

/// The player trading with a merchant, or null.
/// Writes one offer back after a plugin changed it through a recipe it read
/// from this merchant; `false` when the index is gone or it does not decode.
extern "system" fn set_merchant_offer(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    handle: JString<'_>,
    index: jint,
    encoded: JString<'_>,
) -> jboolean {
    let Some(merchant) = merchant(&mut env, &handle) else {
        return 0;
    };
    let Some(offer) = text(&mut env, &encoded).and_then(|value| decode(&value)) else {
        return 0;
    };
    let Ok(index) = usize::try_from(index) else {
        return 0;
    };
    let mut offers = merchant.offers.lock();
    let Some(slot) = offers.get_mut(index) else {
        return 0;
    };
    *slot = offer;
    1
}

extern "system" fn merchant_trader(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    handle: JString<'_>,
) -> jstring {
    let trader =
        any_merchant(&mut env, &handle).and_then(|(merchant, _)| merchant.trading_player());
    to_java(&mut env, trader.map(|id| id.to_string()))
}

/// Opens the merchant's screen for a player, as `CraftHumanEntity`'s
/// `openMerchant`: without `force` a merchant already trading refuses; with
/// it, whoever was trading has their screen closed first.
extern "system" fn open_merchant(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    handle: JString<'_>,
    force: jboolean,
) -> jboolean {
    let Some(player) = player(&mut env, &uuid) else {
        return 0;
    };
    let Some((merchant, title)) = any_merchant(&mut env, &handle) else {
        return 0;
    };
    if let Some(trader) = merchant.trading_player() {
        if force == 0 {
            return 0;
        }
        if let Some(trader) =
            server().and_then(|server| server.online_players().get_by_uuid(&trader))
        {
            trader.close_container();
        }
    }
    merchant.set_trading_player(Some(player.uuid()));
    open_trading_screen(&merchant, &player, title);
    1
}

pub(super) fn bindings() -> Vec<jni::NativeMethod> {
    vec![
        method(
            "createMerchant",
            "(Ljava/lang/String;)Ljava/lang/String;",
            create_merchant as *mut c_void,
        ),
        method(
            "releaseMerchant",
            "(Ljava/lang/String;)V",
            release_merchant as *mut c_void,
        ),
        method(
            "merchantOffers",
            "(Ljava/lang/String;)[Ljava/lang/String;",
            merchant_offers as *mut c_void,
        ),
        method(
            "setMerchantOffers",
            "(Ljava/lang/String;[Ljava/lang/String;)Z",
            set_merchant_offers as *mut c_void,
        ),
        method(
            "setMerchantOffer",
            "(Ljava/lang/String;ILjava/lang/String;)Z",
            set_merchant_offer as *mut c_void,
        ),
        method(
            "merchantTrader",
            "(Ljava/lang/String;)Ljava/lang/String;",
            merchant_trader as *mut c_void,
        ),
        method(
            "openMerchant",
            "(Ljava/lang/String;Ljava/lang/String;Z)Z",
            open_merchant as *mut c_void,
        ),
    ]
}

#[cfg(test)]
mod tests {
    use foton_registry::item_stack::ItemStack;
    use foton_registry::{init_vanilla_registry, vanilla_items};

    use super::{decode, encode};

    #[test]
    fn an_offer_survives_the_trip_to_java_and_back() {
        init_vanilla_registry();
        let encoded = [
            "minecraft:diamond 2",
            "3",
            "12",
            "0",
            "5",
            "0.2",
            "4",
            "-1",
            "minecraft:emerald 7",
            "",
        ]
        .join("\u{1e}");
        let Some(offer) = decode(&encoded) else {
            panic!("a well-formed offer should decode");
        };
        assert_eq!(
            offer.result(),
            &ItemStack::with_count(&vanilla_items::DIAMOND, 2)
        );
        assert_eq!(offer.uses(), 3);
        assert!(!offer.should_reward_exp());
        assert!(offer.item_cost_b().is_none());
        assert_eq!(offer.special_price_diff(), -1);
        assert_eq!(decode(&encode(&offer)), Some(offer));
    }
}
