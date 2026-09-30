//! Paper's events that Foton learned to carry after the first set.
//!
//! Every crossing here has the same shape, so it is written once: the facts go
//! over as strings to a static method of `foton.EventRelay`, Java builds its
//! event, runs the handlers, and answers with one string that the subscription
//! below reads back. Fields in an answer are separated by [`FIELD`]; a boolean
//! is `1` or `0`. A crossing that fails -- no JVM thread, a throwing handler --
//! answers `None`, and every subscription treats that as "nobody objected",
//! because a broken plugin must not be able to freeze the game.

use std::sync::Arc;

use foton_core::event::Event as _;
use foton_core::event::{
    BlockBreakEvent, BlockDropItemEvent, BrewEvent, EnchantOffer, EntitiesLoadEvent,
    EntitiesUnloadEvent, EntityDamageEvent, EntityDismountEvent, EntityPlaceEvent,
    FurnaceBurnEvent, FurnaceSmeltEvent, FurnaceStartSmeltEvent, PlayerArmorChangeEvent,
    PlayerFailMoveEvent, PlayerHarvestBlockEvent, PlayerItemConsumeEvent, PlayerPurchaseEvent,
    PlayerTeleportEvent, PlayerToggleFlightEvent, PlayerVelocityEvent, PrepareItemEnchantEvent,
    PrepareSmithingEvent, ServerListPingEvent, TeleportPoint,
};
use foton_core::server::Server;
use foton_registry::REGISTRY;
use foton_registry::equipment::EquipmentSlot;
use foton_registry::item_stack::ItemStack;
use foton_registry::recipe::{CookingKind, SmeltingRecipe};
use foton_registry::trading::MerchantOffer;
use foton_utils::Identifier;
use foton_utils::text::json;
use foton_utils::types::InteractionHand;
use foton_utils::{BlockPos, ChunkPos};
use glam::DVec3;
use jni::JavaVM;
use jni::errors::Error as JniError;
use jni::objects::{JObject, JString, JValue};
use text_components::TextComponent;
use uuid::Uuid;

use crate::forward::{BridgeEnv, owner};
use crate::natives::{describe_slot, describe_state, parse_slot};

/// The Java class these events are built in.
const RELAY: &str = "foton/EventRelay";

/// Separates the fields of one answer.
const FIELD: char = '\u{1f}';

/// Separates the stacks of an item list within one field.
const ITEM: &str = "\u{1e}";

/// Calls `EventRelay.<method>(String...)` and returns its `String` answer.
///
/// The arguments are created inside a local frame, so a long-lived attached
/// thread -- the tick thread is one -- does not keep a reference per call.
fn text_call(vm: &JavaVM, method: &str, args: &[&str]) -> Option<String> {
    let mut env = BridgeEnv::attach(vm)?;
    let signature = format!(
        "({})Ljava/lang/String;",
        "Ljava/lang/String;".repeat(args.len())
    );
    env.with_local_frame(
        i32::try_from(args.len())
            .unwrap_or(i32::MAX)
            .saturating_add(4),
        |env| {
            let mut strings: Vec<JObject<'_>> = Vec::with_capacity(args.len());
            for arg in args {
                strings.push(env.new_string(arg)?.into());
            }
            let values: Vec<JValue<'_, '_>> = strings.iter().map(JValue::Object).collect();
            let answer = env
                .call_static_method(RELAY, method, &signature, &values)?
                .l()?;
            if answer.is_null() {
                return Ok(None);
            }
            let answer = JString::from(answer);
            let text: String = env.get_string(&answer)?.into();
            Ok::<_, JniError>(Some(text))
        },
    )
    .ok()
    .flatten()
}

/// Splits an answer into its fields.
fn fields(answer: &str) -> Vec<&str> {
    answer.split(FIELD).collect()
}

/// Reads a `1`/`0` field.
fn flag(field: Option<&&str>) -> Option<bool> {
    match field.copied() {
        Some("1") => Some(true),
        Some("0") => Some(false),
        _ => None,
    }
}

/// Writes a boolean the way the Java side reads it.
const fn bit(value: bool) -> &'static str {
    if value { "1" } else { "0" }
}

/// Reads `x y z` back into a vector.
fn vector(field: Option<&&str>) -> Option<DVec3> {
    let mut parts = field?.split(' ').map(str::parse::<f64>);
    let vector = DVec3::new(
        parts.next()?.ok()?,
        parts.next()?.ok()?,
        parts.next()?.ok()?,
    );
    vector.is_finite().then_some(vector)
}

/// Writes a position and rotation as `x y z yaw pitch`.
fn location(position: DVec3, rotation: (f32, f32)) -> String {
    format!(
        "{} {} {} {} {}",
        position.x, position.y, position.z, rotation.0, rotation.1
    )
}

/// Reads what [`location`] writes.
fn placed(field: Option<&&str>) -> Option<(DVec3, (f32, f32))> {
    let parts = field?.split(' ').collect::<Vec<_>>();
    let [x, y, z, yaw, pitch] = parts.as_slice() else {
        return None;
    };
    let position = DVec3::new(x.parse().ok()?, y.parse().ok()?, z.parse().ok()?);
    let rotation: (f32, f32) = (yaw.parse().ok()?, pitch.parse().ok()?);
    (position.is_finite() && rotation.0.is_finite() && rotation.1.is_finite())
        .then_some((position, rotation))
}

/// Reads a component the Java side wrote as JSON text; anything that is not
/// JSON text is taken as plain text, which is what a legacy string is.
pub(crate) fn component(text: &str) -> TextComponent {
    json::from_json(text).unwrap_or_else(|| TextComponent::from(text.to_owned()))
}

/// Subscribes the relay to the events it carries.
pub(crate) fn subscribe(server: &Arc<Server>, vm: &Arc<JavaVM>) {
    subscribe_movement(server, vm);
    subscribe_player_items(server, vm);
    subscribe_entities(server, vm);
    subscribe_blocks(server, vm);
    subscribe_cooking(server, vm);
    subscribe_server(server, vm);
    subscribe_menus(server, vm);
}

/// How players move, and being moved.
fn subscribe_movement(server: &Arc<Server>, vm: &Arc<JavaVM>) {
    let events = server.events();

    let jvm = Arc::clone(vm);
    events.on::<PlayerFailMoveEvent, _>(owner(), move |event| {
        let (from, from_rotation) = event.from();
        let (to, to_rotation) = event.to();
        let Some(answer) = text_call(
            &jvm,
            "fireFailMove",
            &[
                &event.player().to_string(),
                event.world(),
                event.reason().name(),
                &location(from, from_rotation),
                &location(to, to_rotation),
                bit(event.log_warning()),
            ],
        ) else {
            return;
        };
        let answer = fields(&answer);
        if let Some(allowed) = flag(answer.first()) {
            event.set_allowed(allowed);
        }
        if let Some(log_warning) = flag(answer.get(1)) {
            event.set_log_warning(log_warning);
        }
    });

    let jvm = Arc::clone(vm);
    events.on::<PlayerToggleFlightEvent, _>(owner(), move |event| {
        let Some(answer) = text_call(
            &jvm,
            "fireToggleFlight",
            &[&event.player().to_string(), bit(event.flying())],
        ) else {
            return;
        };
        if flag(fields(&answer).first()) == Some(true) {
            event.set_cancelled(true);
        }
    });

    let jvm = Arc::clone(vm);
    events.on::<PlayerVelocityEvent, _>(owner(), move |event| {
        let velocity = event.velocity();
        let Some(answer) = text_call(
            &jvm,
            "fireVelocity",
            &[
                &event.player().to_string(),
                &format!("{} {} {}", velocity.x, velocity.y, velocity.z),
            ],
        ) else {
            return;
        };
        let answer = fields(&answer);
        if flag(answer.first()) == Some(true) {
            event.set_cancelled(true);
            return;
        }
        if let Some(velocity) = vector(answer.get(1)) {
            event.set_velocity(velocity);
        }
    });

    let jvm = Arc::clone(vm);
    events.on::<PlayerTeleportEvent, _>(owner(), move |event| {
        let (from, to) = (event.from(), event.to());
        let Some(answer) = text_call(
            &jvm,
            "fireTeleport",
            &[
                &event.player().to_string(),
                &from.world,
                &location(from.position, from.rotation),
                &to.world,
                &location(to.position, to.rotation),
                event.cause().bukkit_name(),
            ],
        ) else {
            return;
        };
        let answer = fields(&answer);
        if flag(answer.first()) == Some(true) {
            event.set_cancelled(true);
            return;
        }
        if let (Some(world), Some((position, rotation))) = (answer.get(1), placed(answer.get(2))) {
            event.set_to(TeleportPoint {
                world: (*world).to_owned(),
                position,
                rotation,
            });
        }
    });
}

/// What a player wears, finishes using and harvests.
fn subscribe_player_items(server: &Arc<Server>, vm: &Arc<JavaVM>) {
    let events = server.events();

    let jvm = Arc::clone(vm);
    events.on::<PlayerArmorChangeEvent, _>(owner(), move |event| {
        let Some(slot) = armor_slot_name(event.slot()) else {
            return;
        };
        let _ = text_call(
            &jvm,
            "fireArmorChange",
            &[
                &event.player().to_string(),
                slot,
                &describe_slot(event.old_item()),
                &describe_slot(event.new_item()),
            ],
        );
    });

    let jvm = Arc::clone(vm);
    events.on::<PlayerItemConsumeEvent, _>(owner(), move |event| {
        let Some(answer) = text_call(
            &jvm,
            "fireItemConsume",
            &[
                &event.player().to_string(),
                hand_name(event.hand()),
                &describe_slot(event.item()),
            ],
        ) else {
            return;
        };
        let answer = fields(&answer);
        if flag(answer.first()) == Some(true) {
            event.set_cancelled(true);
            return;
        }
        if let Some(item) = answer.get(1).and_then(|text| parse_slot(text)) {
            event.set_item(item);
        }
        if flag(answer.get(2)) == Some(true) {
            event.set_replacement(answer.get(3).and_then(|text| parse_slot(text)));
        }
    });

    let jvm = Arc::clone(vm);
    events.on::<PlayerHarvestBlockEvent, _>(owner(), move |event| {
        let items = event
            .items()
            .iter()
            .map(describe_slot)
            .collect::<Vec<_>>()
            .join(ITEM);
        let Some(answer) = text_call(
            &jvm,
            "fireHarvest",
            &[
                &event.player().to_string(),
                event.world(),
                &block_position(event.position()),
                hand_name(event.hand()),
                &items,
            ],
        ) else {
            return;
        };
        let answer = fields(&answer);
        if flag(answer.first()) == Some(true) {
            event.set_cancelled(true);
            return;
        }
        let harvested = answer
            .get(1)
            .map(|list| {
                list.split(ITEM)
                    .filter(|one| !one.is_empty())
                    .filter_map(parse_slot)
                    .filter(|stack| !stack.is_empty())
                    .collect()
            })
            .unwrap_or_default();
        event.set_items(harvested);
    });
}

/// Entities arriving, leaving their vehicles, and being hurt by the world.
fn subscribe_entities(server: &Arc<Server>, vm: &Arc<JavaVM>) {
    let events = server.events();

    let jvm = Arc::clone(vm);
    events.on::<EntityPlaceEvent, _>(owner(), move |event| {
        let Some(answer) = text_call(
            &jvm,
            "fireEntityPlace",
            &[
                &event.entity().to_string(),
                &event.player().to_string(),
                event.world(),
                &block_position(event.block()),
                event.face(),
                hand_name(event.hand()),
            ],
        ) else {
            return;
        };
        if flag(fields(&answer).first()) == Some(true) {
            event.set_cancelled(true);
        }
    });

    let jvm = Arc::clone(vm);
    events.on::<EntityDismountEvent, _>(owner(), move |event| {
        let Some(answer) = text_call(
            &jvm,
            "fireDismount",
            &[
                &event.entity().to_string(),
                &event.vehicle().to_string(),
                bit(event.cancellable()),
            ],
        ) else {
            return;
        };
        if flag(fields(&answer).first()) == Some(true) {
            event.set_cancelled(true);
        }
    });

    let jvm = Arc::clone(vm);
    events.on::<EntitiesLoadEvent, _>(owner(), move |event| {
        let _ = text_call(
            &jvm,
            "fireEntitiesLoad",
            &[
                event.world(),
                &chunk_position(event.chunk()),
                &uuid_list(event.entities()),
            ],
        );
    });

    let jvm = Arc::clone(vm);
    events.on::<EntitiesUnloadEvent, _>(owner(), move |event| {
        let _ = text_call(
            &jvm,
            "fireEntitiesUnload",
            &[
                event.world(),
                &chunk_position(event.chunk()),
                &uuid_list(event.entities()),
            ],
        );
    });

    let jvm = Arc::clone(vm);
    events.on::<EntityDamageEvent, _>(owner(), move |event| {
        let Some(answer) = text_call(
            &jvm,
            "fireEnvironmentDamage",
            &[
                &event.entity().to_string(),
                event.cause(),
                &event.damage().to_string(),
            ],
        ) else {
            return;
        };
        let answer = fields(&answer);
        if flag(answer.first()) == Some(true) {
            event.set_cancelled(true);
            return;
        }
        if let Some(damage) = answer
            .get(1)
            .and_then(|text| text.parse::<f64>().ok())
            .filter(|damage| damage.is_finite())
        {
            event.set_damage(damage);
        }
    });
}

/// Blocks broken, and what they and brewing stands give.
fn subscribe_blocks(server: &Arc<Server>, vm: &Arc<JavaVM>) {
    let events = server.events();

    let jvm = Arc::clone(vm);
    events.on::<BlockBreakEvent, _>(owner(), move |event| {
        let player = event.player();
        let Some(answer) = text_call(
            &jvm,
            "fireBlockBreak",
            &[
                &player.gameprofile.id.to_string(),
                &player.get_world().key.to_string(),
                &block_position(event.position()),
                &event.exp_to_drop().to_string(),
            ],
        ) else {
            return;
        };
        let answer = fields(&answer);
        if flag(answer.first()) == Some(true) {
            event.set_cancelled(true);
            return;
        }
        if let Some(drop_items) = flag(answer.get(1)) {
            event.set_drop_items(drop_items);
        }
        if let Some(exp) = answer.get(2).and_then(|text| text.parse::<i32>().ok()) {
            event.set_exp_to_drop(exp);
        }
    });

    let jvm = Arc::clone(vm);
    events.on::<BlockDropItemEvent, _>(owner(), move |event| {
        let Some(state) = describe_state(event.broken()) else {
            return;
        };
        let Some(answer) = text_call(
            &jvm,
            "fireBlockDropItem",
            &[
                &event.player().to_string(),
                event.world(),
                &block_position(event.position()),
                &state,
                &uuid_list(event.items()),
            ],
        ) else {
            return;
        };
        let answer = fields(&answer);
        if flag(answer.first()) == Some(true) {
            event.set_cancelled(true);
            return;
        }
        let kept = answer
            .get(1)
            .map(|list| {
                list.split(',')
                    .filter_map(|id| id.parse::<Uuid>().ok())
                    .collect()
            })
            .unwrap_or_default();
        event.set_items(kept);
    });

    let jvm = Arc::clone(vm);
    events.on::<BrewEvent, _>(owner(), move |event| {
        let Some(answer) = text_call(
            &jvm,
            "fireBrew",
            &[
                event.world(),
                &block_position(event.position()),
                &slot_list(event.results()),
                &event.fuel_level().to_string(),
            ],
        ) else {
            return;
        };
        let answer = fields(&answer);
        if flag(answer.first()) == Some(true) {
            event.set_cancelled(true);
            return;
        }
        let Some(list) = answer.get(1) else {
            return;
        };
        let results: Option<Vec<ItemStack>> = list.split(ITEM).map(parse_slot).collect();
        if let Some(results) = results {
            event.set_results(results);
        }
    });
}

/// The furnace events.
fn subscribe_cooking(server: &Arc<Server>, vm: &Arc<JavaVM>) {
    let events = server.events();

    let jvm = Arc::clone(vm);
    events.on::<FurnaceBurnEvent, _>(owner(), move |event| {
        let Some(answer) = text_call(
            &jvm,
            "fireFurnaceBurn",
            &[
                event.world(),
                &block_position(event.position()),
                &describe_slot(event.fuel()),
                &event.burn_time().to_string(),
            ],
        ) else {
            return;
        };
        let answer = fields(&answer);
        if flag(answer.first()) == Some(true) {
            event.set_cancelled(true);
            return;
        }
        if let Some(burn_time) = answer.get(1).and_then(|text| text.parse::<i32>().ok()) {
            event.set_burn_time(burn_time);
        }
        if let Some(burning) = flag(answer.get(2)) {
            event.set_burning(burning);
        }
        if let Some(consume) = flag(answer.get(3)) {
            event.set_consume_fuel(consume);
        }
    });

    let jvm = Arc::clone(vm);
    events.on::<FurnaceStartSmeltEvent, _>(owner(), move |event| {
        let Some(answer) = text_call(
            &jvm,
            "fireFurnaceStartSmelt",
            &[
                event.world(),
                &block_position(event.position()),
                &describe_slot(event.source()),
                &describe_cooking_id(event.recipe()),
                &event.total_cook_time().to_string(),
            ],
        ) else {
            return;
        };
        if let Some(total) = fields(&answer)
            .first()
            .and_then(|text| text.parse::<i32>().ok())
        {
            event.set_total_cook_time(total);
        }
    });

    let jvm = Arc::clone(vm);
    events.on::<FurnaceSmeltEvent, _>(owner(), move |event| {
        let Some(answer) = text_call(
            &jvm,
            "fireFurnaceSmelt",
            &[
                event.world(),
                &block_position(event.position()),
                &describe_slot(event.source()),
                &describe_slot(event.result()),
                &describe_cooking_id(event.recipe()),
            ],
        ) else {
            return;
        };
        let answer = fields(&answer);
        if flag(answer.first()) == Some(true) {
            event.set_cancelled(true);
            return;
        }
        if let Some(result) = answer.get(1).and_then(|text| parse_slot(text)) {
            event.set_result(result);
        }
    });
}

/// The server-list ping.
fn subscribe_server(server: &Arc<Server>, vm: &Arc<JavaVM>) {
    let events = server.events();

    let jvm = Arc::clone(vm);
    events.on::<ServerListPingEvent, _>(owner(), move |event| {
        let Some(answer) = text_call(
            &jvm,
            "fireServerListPing",
            &[
                &event.address().to_string(),
                &json::to_json(event.motd()).to_string(),
                &event.online().to_string(),
                &event.max_players().to_string(),
            ],
        ) else {
            return;
        };
        let answer = fields(&answer);
        if let Some(motd) = answer.first() {
            event.set_motd(component(motd));
        }
        if let Some(max) = answer.get(1).and_then(|text| text.parse::<i32>().ok()) {
            event.set_max_players(max);
        }
    });
}

/// Separates the fields of one merchant offer.
const OFFER_FIELD: &str = "\u{1c}";

/// Describes an offer as `result, costA, costB, uses, maxUses, rewardExp, xp,
/// priceMultiplier, demand`, the costs being the base ones Paper reports.
fn describe_offer(offer: &MerchantOffer) -> String {
    [
        describe_slot(offer.result()),
        describe_slot(offer.base_cost_a()),
        describe_slot(&offer.cost_b()),
        offer.uses().to_string(),
        offer.max_uses().to_string(),
        bit(offer.should_reward_exp()).to_owned(),
        offer.xp().to_string(),
        offer.price_multiplier().to_string(),
        offer.demand().to_string(),
    ]
    .join(OFFER_FIELD)
}

/// Writes the three enchanting offers, `key level cost` each, empty where
/// the row is.
fn enchant_offers(offers: &[Option<EnchantOffer>; 3]) -> String {
    offers
        .iter()
        .map(|offer| {
            offer.as_ref().map_or_else(String::new, |offer| {
                format!("{} {} {}", offer.enchantment, offer.level, offer.cost)
            })
        })
        .collect::<Vec<_>>()
        .join(ITEM)
}

/// Reads [`enchant_offers`] back; `None` when the answer is malformed.
fn parse_enchant_offers(text: &str) -> Option<[Option<EnchantOffer>; 3]> {
    let rows = text.split(ITEM).collect::<Vec<_>>();
    if rows.len() != 3 {
        return None;
    }
    let mut offers: [Option<EnchantOffer>; 3] = [None, None, None];
    for (slot, row) in rows.into_iter().enumerate() {
        if row.is_empty() {
            continue;
        }
        let mut parts = row.split(' ');
        let enchantment = parts.next()?.parse::<Identifier>().ok()?;
        let level = parts.next()?.parse::<i32>().ok()?;
        let cost = parts.next()?.parse::<i32>().ok()?;
        offers[slot] = Some(EnchantOffer {
            enchantment,
            level,
            cost,
        });
    }
    Some(offers)
}

/// The workstation screen events.
fn subscribe_menus(server: &Arc<Server>, vm: &Arc<JavaVM>) {
    let events = server.events();

    let jvm = Arc::clone(vm);
    events.on::<PrepareItemEnchantEvent, _>(owner(), move |event| {
        let Some(answer) = text_call(
            &jvm,
            "firePrepareEnchant",
            &[
                &event.player().to_string(),
                event.world(),
                &block_position(event.table()),
                &describe_slot(event.item()),
                &enchant_offers(event.offers()),
                &event.bonus().to_string(),
                bit(event.is_cancelled()),
            ],
        ) else {
            return;
        };
        let answer = fields(&answer);
        let Some(cancelled) = flag(answer.first()) else {
            return;
        };
        event.set_cancelled(cancelled);
        if let Some(offers) = answer.get(1).and_then(|text| parse_enchant_offers(text)) {
            event.set_offers(offers);
        }
    });

    let jvm = Arc::clone(vm);
    events.on::<PrepareSmithingEvent, _>(owner(), move |event| {
        let Some(answer) = text_call(
            &jvm,
            "firePrepareSmithing",
            &[
                &event.player().to_string(),
                &slot_list(event.inputs()),
                &describe_slot(event.result()),
            ],
        ) else {
            return;
        };
        if let Some(result) = parse_slot(&answer) {
            event.set_result(result);
        }
    });

    let jvm = Arc::clone(vm);
    events.on::<PlayerPurchaseEvent, _>(owner(), move |event| {
        let offers = event
            .offers()
            .iter()
            .map(describe_offer)
            .collect::<Vec<_>>()
            .join(ITEM);
        let Some(answer) = text_call(
            &jvm,
            "firePurchase",
            &[
                &event.player().to_string(),
                &event.trader().map(|id| id.to_string()).unwrap_or_default(),
                &describe_offer(event.offer()),
                &offers,
            ],
        ) else {
            return;
        };
        if flag(fields(&answer).first()) == Some(true) {
            event.set_cancelled(true);
        }
    });
}

/// The cooking family of a furnace-like block, by the block's key.
pub(crate) fn cooking_kind(block: &str) -> Option<CookingKind> {
    match block {
        "minecraft:furnace" => Some(CookingKind::Smelting),
        "minecraft:blast_furnace" => Some(CookingKind::Blasting),
        "minecraft:smoker" => Some(CookingKind::Smoking),
        _ => None,
    }
}

/// Describes a cooking recipe as `key, experience, cooking time, result,
/// inputs`, the inputs being item keys separated by spaces.
pub(crate) fn describe_cooking(recipe: &SmeltingRecipe) -> String {
    let inputs = recipe
        .ingredient
        .get_items()
        .iter()
        .map(|item| item.key.to_string())
        .collect::<Vec<_>>()
        .join(" ");
    [
        recipe.id.to_string(),
        recipe.experience.to_string(),
        recipe.cooking_time.to_string(),
        describe_slot(&recipe.assemble_result(1, false)),
        inputs,
    ]
    .join(&FIELD.to_string())
}

/// Describes the cooking recipe stored under `id`, or nothing.
fn describe_cooking_id(id: &Identifier) -> String {
    REGISTRY
        .recipes
        .find_cooking_recipe_by_id(id)
        .map(describe_cooking)
        .unwrap_or_default()
}

/// Writes stacks slot by slot, an empty slot as an empty entry.
fn slot_list(stacks: &[ItemStack]) -> String {
    stacks
        .iter()
        .map(describe_slot)
        .collect::<Vec<_>>()
        .join(ITEM)
}

/// The Bukkit `SlotType` name of an armor slot.
const fn armor_slot_name(slot: EquipmentSlot) -> Option<&'static str> {
    match slot {
        EquipmentSlot::Head => Some("HEAD"),
        EquipmentSlot::Chest => Some("CHEST"),
        EquipmentSlot::Legs => Some("LEGS"),
        EquipmentSlot::Feet => Some("FEET"),
        _ => None,
    }
}

/// The Bukkit `EquipmentSlot` name of a hand.
const fn hand_name(hand: InteractionHand) -> &'static str {
    match hand {
        InteractionHand::MainHand => "HAND",
        InteractionHand::OffHand => "OFF_HAND",
    }
}

/// Writes a block position as `x y z`.
fn block_position(pos: BlockPos) -> String {
    format!("{} {} {}", pos.x(), pos.y(), pos.z())
}

/// Writes a chunk position as `x z`.
fn chunk_position(pos: ChunkPos) -> String {
    format!("{} {}", pos.0.x, pos.0.y)
}

/// Writes entity ids comma-separated.
fn uuid_list(ids: &[Uuid]) -> String {
    ids.iter()
        .map(Uuid::to_string)
        .collect::<Vec<_>>()
        .join(",")
}
