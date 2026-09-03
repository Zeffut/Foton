//! Foton's events, carried to whatever plugins are listening.
//!
//! The direction that matters. Everything else in this crate answers a
//! question a plugin asked; this delivers something that happened whether a
//! plugin asked or not, and reads back what the plugins decided about it.
//!
//! Each crossing is one call deep: Foton hands over the facts, Java builds its
//! own event object around them, runs the handlers, and returns the outcome.
//! Nothing on either side holds a reference to the other's objects past the
//! call, which is the only reason the lifetimes work at all.

use std::sync::Arc;

use crate::natives;
use crate::natives::describe_slot;
use crate::natives::parse_slot;
use foton_core::entity::conversion::ConversionReason;
use foton_core::event::EntityTargetEvent;
use foton_core::event::{
    AsyncPlayerPreLoginEvent, AsyncPlayerPreLoginResult, BlockBreakEvent, BlockBurnEvent,
    BlockDamageEvent, BlockDispenseEvent, BlockExpEvent, BlockExplodeEvent, BlockFadeEvent,
    BlockFertilizeEvent, BlockFromToEvent, BlockIgniteEvent, BlockPlaceEvent,
    BlockPreDispenseEvent, ChunkLoadEvent, CommandEvent, CrafterCraftEvent, CreatureSpawnEvent,
    EntityChangeBlockEvent, EntityDamageByEntityEvent, EntityDeathEvent, EntityExplodeEvent,
    EntityMountEvent, EntityPickupItemEvent, EntityPortalEvent, EntityPushedByEntityAttackEvent,
    EntityRegainHealthEvent, EntityRemoveFromWorldEvent, EntityResurrectEvent,
    EntityTransformEvent, ExpBottleEvent, FoodLevelChangeEvent, HangingBreakEvent,
    HangingPlaceEvent, InventoryClickEvent, InventoryCloseEvent, InventoryDragEvent,
    InventoryOpenEvent, ItemSpawnEvent, LeavesDecayEvent, LightningStrikeEvent, PistonEvent,
    PlayerAdvancementCriterionGrantEvent, PlayerAdvancementDoneEvent, PlayerBucketEmptyEvent,
    PlayerBucketFillEvent, PlayerChatEvent, PlayerCommandPreprocessEvent, PlayerCustomPayloadEvent,
    PlayerDeathEvent, PlayerDropItemEvent, PlayerFishEvent, PlayerInteractEntityEvent,
    PlayerInteractEvent, PlayerItemBreakEvent, PlayerJoinEvent, PlayerLocaleChangeEvent,
    PlayerLoginEvent, PlayerMoveEvent, PlayerOpenSignCause, PlayerOpenSignEvent, PlayerPortalEvent,
    PlayerQuitEvent, PlayerRespawnEvent, PlayerSpawnLocationEvent, PlayerTakeLecternBookEvent,
    PortalCreateEvent, PreCreatureSpawnEvent, PrepareItemCraftEvent, ProjectileLaunchEvent,
    ServerTickEvent, SignChangeEvent, ThunderChangeEvent, WeatherChangeEvent,
};
use foton_core::player::Player;
use foton_core::server::Server;
use foton_registry::item_stack::ItemStack;
use foton_utils::text::DisplayResolutor;
use foton_utils::{BlockPos, Identifier};
use jni::JavaVM;
use jni::objects::{JObject, JString, JValue, JValueGen};
use std::net::SocketAddr;
use text_components::TextComponent;
use uuid::Uuid;

/// The Java class that owns the handler lists.
const BRIDGE: &str = "foton/EventBridge";

/// Where a plugin's scheduled tasks wait until a tick can run them.
const SCHEDULER: &str = "foton/FotonScheduler";

/// The Bukkit channel registry and its listeners.
const MESSENGER: &str = "foton/FotonMessenger";

/// Who these subscriptions belong to, so unloading takes them with it.
fn owner() -> Identifier {
    Identifier::from_foton("plugins")
}

/// Subscribes the plugin host to the events plugins can see.
///
/// Every subscription is a copy of the same shape: gather the facts, cross
/// once, apply what came back. The repetition is on purpose -- a generic
/// version would need each event to describe itself to the bridge, which is
/// more machinery than five events justify.
#[expect(
    clippy::too_many_lines,
    reason = "one subscription per event, each the same six lines; a reader looking \
              for an event finds it here, and splitting the list by category would \
              only move the question of which file to open"
)]
pub(crate) fn subscribe(server: &Arc<Server>, vm: Arc<JavaVM>) {
    let events = server.events();
    for snapshot in server.worlds.snapshots() {
        let world = snapshot.world();
        world_call(&vm, "fireWorldLoad", &world.key.to_string());
    }

    let jvm = Arc::clone(&vm);
    events.on::<PlayerInteractEvent, _>(owner(), move |event| {
        if !interact_call(&jvm, &event.player_id().to_string()) {
            event.set_cancelled(true);
        }
    });

    let jvm = Arc::clone(&vm);
    events.on::<PlayerInteractEntityEvent, _>(owner(), move |event| {
        if !interact_entity_call(
            &jvm,
            &event.player_id().to_string(),
            &event.entity_id().to_string(),
        ) {
            event.set_cancelled(true);
        }
    });

    let jvm = Arc::clone(&vm);
    events.on::<InventoryClickEvent, _>(owner(), move |event| {
        let item = event.current_item().map_or(String::new(), |stack| {
            format!("{} {}", stack.item().key, stack.count())
        });
        let cursor = event.cursor_item().map_or(String::new(), |stack| {
            format!("{} {}", stack.item().key, stack.count())
        });
        if !inventory_click_call(
            &jvm,
            &event.player_id().to_string(),
            &item,
            &cursor,
            event.click(),
            event.slot().map_or(-1, |slot| slot as i32),
        ) {
            event.set_cancelled(true);
        }
    });

    let jvm = Arc::clone(&vm);
    events.on::<InventoryDragEvent, _>(owner(), move |event| {
        let slots = event
            .slots()
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(",");
        let old_cursor = format!(
            "{} {}",
            event.old_cursor().item().key,
            event.old_cursor().count()
        );
        if !inventory_drag_call(
            &jvm,
            &event.player_id().to_string(),
            &slots,
            &old_cursor,
            event.drag_type(),
        ) {
            event.set_cancelled(true);
        }
    });

    let jvm = Arc::clone(&vm);
    events.on::<InventoryCloseEvent, _>(owner(), move |event| {
        inventory_close_call(&jvm, &event.player_id().to_string());
    });

    let jvm = Arc::clone(&vm);
    events.on::<PrepareItemCraftEvent, _>(owner(), move |event| {
        let matrix = event
            .matrix()
            .iter()
            .map(natives::describe_slot)
            .collect::<Vec<_>>()
            .join("\u{001e}");
        let result = natives::describe_slot(event.result());
        let Some(answer) = prepare_craft_call(
            &jvm,
            &event.player_id().to_string(),
            &matrix,
            &result,
            event.is_repair(),
        ) else {
            return;
        };
        let Some((matrix, result)) = answer.split_once('\u{001f}') else {
            return;
        };
        let Some(result) = natives::parse_slot(result) else {
            return;
        };
        let matrix = matrix
            .split('\u{001e}')
            .map(natives::parse_slot)
            .collect::<Option<Vec<_>>>();
        let Some(matrix) = matrix else {
            return;
        };
        event.set_matrix(matrix);
        event.set_result(result);
    });

    let jvm = Arc::clone(&vm);
    events.on::<CrafterCraftEvent, _>(owner(), move |event| {
        let result = natives::describe_slot(event.result());
        let remaining = event
            .remaining_items()
            .iter()
            .map(natives::describe_slot)
            .collect::<Vec<_>>()
            .join("\u{001e}");
        let Some(answer) = crafter_craft_call(
            &jvm,
            event.world(),
            event.position(),
            event.recipe(),
            &result,
            &remaining,
        ) else {
            return;
        };
        let mut fields = answer.split('\u{001f}');
        if fields.next() == Some("1") {
            event.set_cancelled(true);
            return;
        }
        let Some(result) = fields.next().and_then(natives::parse_slot) else {
            return;
        };
        event.set_result(result);
        let remaining = fields
            .next()
            .unwrap_or_default()
            .split('\u{001e}')
            .map(natives::parse_slot)
            .collect::<Option<Vec<_>>>();
        if let Some(remaining) = remaining {
            event.set_remaining_items(remaining);
        }
    });

    let jvm = Arc::clone(&vm);
    events.on::<InventoryOpenEvent, _>(owner(), move |event| {
        if !inventory_open_call(&jvm, &event.player_id().to_string()) {
            event.set_cancelled(true);
        }
    });

    let jvm = Arc::clone(&vm);
    events.on::<EntityTargetEvent, _>(owner(), move |event| {
        let target = event
            .target_id()
            .map_or_else(String::new, |id| id.to_string());
        if !entity_target_call(&jvm, &event.entity_id().to_string(), &target) {
            event.set_cancelled(true);
        }
    });

    let jvm = Arc::clone(&vm);
    events.on::<EntityDeathEvent, _>(owner(), move |event| {
        entity_death_call(&jvm, event.entity_id());
    });
    let jvm = Arc::clone(&vm);
    events.on::<EntityResurrectEvent, _>(owner(), move |event| {
        if !entity_resurrect_call(&jvm, event.entity_id()) {
            event.set_cancelled(true);
        }
    });

    let jvm = Arc::clone(&vm);
    events.on::<PlayerTakeLecternBookEvent, _>(owner(), move |event| {
        if !player_take_lectern_book_call(&jvm, event.player_id(), event.world(), event.position())
        {
            event.set_cancelled(true);
        }
    });

    let jvm = Arc::clone(&vm);
    events.on::<WeatherChangeEvent, _>(owner(), move |event| {
        if !weather_change_call(&jvm, event.world(), event.raining()) {
            event.set_cancelled(true);
        }
    });
    let jvm = Arc::clone(&vm);
    events.on::<ThunderChangeEvent, _>(owner(), move |event| {
        if !thunder_change_call(&jvm, event.world(), event.thundering()) {
            event.set_cancelled(true);
        }
    });

    let jvm = Arc::clone(&vm);
    events.on::<LightningStrikeEvent, _>(owner(), move |event| {
        if !lightning_strike_call(&jvm, event.entity(), event.world(), event.cause()) {
            event.set_cancelled(true);
        }
    });
    let jvm = Arc::clone(&vm);
    events.on::<EntityExplodeEvent, _>(owner(), move |event| {
        let Some(entity) = event.entity() else {
            return;
        };
        let Some((cancelled, blocks)) = explode_call(
            &jvm,
            &entity.to_string(),
            event.world(),
            event.blocks(),
            event.explosion_result(),
        ) else {
            return;
        };
        event.set_cancelled(cancelled);
        if !cancelled {
            *event.blocks_mut() = blocks;
        }
    });
    let jvm = Arc::clone(&vm);
    events.on::<BlockExplodeEvent, _>(owner(), move |event| {
        let Some((cancelled, blocks)) =
            block_explode_call(&jvm, event.world(), event.source(), event.blocks())
        else {
            return;
        };
        event.set_cancelled(cancelled);
        *event.blocks_mut() = blocks;
    });
    let jvm = Arc::clone(&vm);
    events.on::<BlockPreDispenseEvent, _>(owner(), move |event| {
        let Some((cancelled, item)) = block_pre_dispense_call(
            &jvm,
            event.world(),
            event.position(),
            event.slot(),
            event.item(),
        ) else {
            return;
        };
        event.set_cancelled(cancelled);
        event.set_item(item);
    });
    let jvm = Arc::clone(&vm);
    events.on::<BlockDispenseEvent, _>(owner(), move |event| {
        let Some((cancelled, item)) =
            block_dispense_call(&jvm, event.world(), event.position(), event.item())
        else {
            return;
        };
        event.set_cancelled(cancelled);
        event.set_item(item);
    });
    let jvm = Arc::clone(&vm);
    events.on::<BlockBurnEvent, _>(owner(), move |event| {
        if !block_burn_call(&jvm, event.world(), event.position()) {
            event.set_cancelled(true);
        }
    });
    let jvm = Arc::clone(&vm);
    events.on::<BlockFadeEvent, _>(owner(), move |event| {
        if !block_fade_call(&jvm, event.world(), event.position()) {
            event.set_cancelled(true);
        }
    });
    let jvm = Arc::clone(&vm);
    events.on::<LeavesDecayEvent, _>(owner(), move |event| {
        if !leaves_decay_call(&jvm, event.world(), event.position()) {
            event.set_cancelled(true);
        }
    });
    let jvm = Arc::clone(&vm);
    events.on::<BlockIgniteEvent, _>(owner(), move |event| {
        if !block_ignite_call(
            &jvm,
            event.world(),
            event.position(),
            event.cause(),
            event.player_id(),
        ) {
            event.set_cancelled(true);
        }
    });
    let jvm = Arc::clone(&vm);
    events.on::<BlockFertilizeEvent, _>(owner(), move |event| {
        if !block_fertilize_call(&jvm, event.world(), event.position(), event.player_id()) {
            event.set_cancelled(true);
        }
    });
    let jvm = Arc::clone(&vm);
    events.on::<EntityTransformEvent, _>(owner(), move |event| {
        if !transform_call(&jvm, event.entity(), event.transformed(), event.reason()) {
            event.set_cancelled(true);
        }
    });
    let jvm = Arc::clone(&vm);
    events.on::<EntityChangeBlockEvent, _>(owner(), move |event| {
        if !entity_change_block_call(
            &jvm,
            event.entity(),
            event.world(),
            event.block(),
            event.to(),
        ) {
            event.set_cancelled(true);
        }
    });
    let jvm = Arc::clone(&vm);
    events.on::<BlockDamageEvent, _>(owner(), move |event| {
        if !block_damage_call(&jvm, event.player_id(), event.world(), event.position()) {
            event.set_cancelled(true);
        }
    });
    let jvm = Arc::clone(&vm);
    events.on::<PlayerAdvancementDoneEvent, _>(owner(), move |event| {
        player_advancement_done_call(&jvm, event.player_id(), event.advancement());
    });

    let jvm = Arc::clone(&vm);
    events.on::<PlayerAdvancementCriterionGrantEvent, _>(owner(), move |event| {
        if !player_advancement_criterion_grant_call(
            &jvm,
            event.player_id(),
            event.advancement(),
            event.criterion(),
        ) {
            event.set_cancelled(true);
        }
    });

    let jvm = Arc::clone(&vm);
    events.on::<PlayerOpenSignEvent, _>(owner(), move |event| {
        if !player_open_sign_call(
            &jvm,
            event.player_id(),
            event.world(),
            event.position(),
            event.front_side(),
            event.cause(),
        ) {
            event.set_cancelled(true);
        }
    });

    let jvm = Arc::clone(&vm);
    events.on::<SignChangeEvent, _>(owner(), move |event| {
        if let Some((cancelled, lines)) = sign_change_call(
            &jvm,
            &event.player_id().to_string(),
            event.world(),
            event.position(),
            event.lines(),
        ) {
            event.set_cancelled(cancelled);
            if !cancelled {
                *event.lines_mut() = lines;
            }
        }
    });

    let jvm = Arc::clone(&vm);
    events.on::<PistonEvent, _>(owner(), move |event| {
        if let Some((cancelled, blocks)) = piston_call(
            &jvm,
            event.world(),
            event.piston(),
            event.direction(),
            event.extending(),
            event.blocks(),
        ) {
            event.set_cancelled(cancelled);
            if !cancelled {
                *event.blocks_mut() = blocks;
            }
        }
    });

    let jvm = Arc::clone(&vm);
    events.on::<EntityRemoveFromWorldEvent, _>(owner(), move |event| {
        remove_call(&jvm, &event.entity().to_string());
    });

    let jvm = Arc::clone(&vm);
    events.on::<ItemSpawnEvent, _>(owner(), move |event| {
        let (x, y, z) = event.position();
        let item = format!("{} {}", event.item().item.key, event.item().count());
        if !item_spawn_call(
            &jvm,
            &event.entity().to_string(),
            event.world(),
            x,
            y,
            z,
            &item,
        ) {
            event.set_cancelled(true);
        }
    });

    let jvm = Arc::clone(&vm);
    events.on::<EntityPickupItemEvent, _>(owner(), move |event| {
        if !pickup_call(&jvm, &event.entity().to_string(), &event.item().to_string()) {
            event.set_cancelled(true);
        }
    });

    let jvm = Arc::clone(&vm);
    let pre_spawn_jvm = Arc::clone(&vm);
    let portal_jvm = Arc::clone(&vm);
    events.on::<EntityPortalEvent, _>(owner(), move |event| {
        let result = entity_portal_call(&portal_jvm, event);
        match result {
            None => event.set_cancelled(true),
            Some((world, position)) => event.set_destination(world, position),
        }
    });
    events.on::<PreCreatureSpawnEvent, _>(owner(), move |event| {
        let (x, y, z) = event.position();
        if !pre_creature_spawn_call(
            &pre_spawn_jvm,
            event.world(),
            x,
            y,
            z,
            event.entity_type(),
            event.reason(),
        ) {
            event.set_cancelled(true);
        }
    });
    events.on::<CreatureSpawnEvent, _>(owner(), move |event| {
        let (x, y, z) = event.position();
        if !creature_spawn_call(
            &jvm,
            &event.entity().to_string(),
            event.world(),
            x,
            y,
            z,
            event.reason(),
        ) {
            event.set_cancelled(true);
        }
    });

    let jvm = Arc::clone(&vm);
    events.on::<EntityMountEvent, _>(owner(), move |event| {
        let Ok(mut env) = jvm.attach_current_thread() else {
            return;
        };
        let (Ok(entity), Ok(vehicle)) = (
            env.new_string(event.entity().to_string()),
            env.new_string(event.vehicle().to_string()),
        ) else {
            return;
        };
        let Ok(ok) = env
            .call_static_method(
                BRIDGE,
                "fireEntityMount",
                "(Ljava/lang/String;Ljava/lang/String;)Z",
                &[JValue::Object(&entity), JValue::Object(&vehicle)],
            )
            .and_then(JValueGen::z)
        else {
            return;
        };
        if !ok {
            event.set_cancelled(true);
        }
    });

    let jvm = Arc::clone(&vm);
    events.on::<ExpBottleEvent, _>(owner(), move |event| {
        let Ok(mut env) = jvm.attach_current_thread() else {
            return;
        };
        let Ok(entity) = env.new_string(event.entity().to_string()) else {
            return;
        };
        let Ok(value) = env
            .call_static_method(
                BRIDGE,
                "fireExpBottle",
                "(Ljava/lang/String;I)Ljava/lang/String;",
                &[JValue::Object(&entity), JValue::Int(event.experience())],
            )
            .and_then(JValueGen::l)
        else {
            return;
        };
        let value_string = JString::from(value);
        let Ok(text) = env.get_string(&value_string) else {
            return;
        };
        let owned = text.to_string_lossy().into_owned();
        let mut parts = owned.split('|');
        if parts.next() == Some("0") {
            event.set_cancelled(true);
        }
        if let Some(xp) = parts.next().and_then(|v| v.parse().ok()) {
            event.set_experience(xp);
        }
    });

    let jvm = Arc::clone(&vm);
    events.on::<EntityRegainHealthEvent, _>(owner(), move |event| {
        if !regain_health_call(&jvm, &event.entity().to_string(), event.amount()) {
            event.set_cancelled(true);
        }
    });

    let jvm = Arc::clone(&vm);
    events.on::<EntityDamageByEntityEvent, _>(owner(), move |event| {
        if !damage_call(
            &jvm,
            &event.damager().to_string(),
            &event.entity().to_string(),
            event.cause(),
        ) {
            event.set_cancelled(true);
        }
    });

    let jvm = Arc::clone(&vm);
    events.on::<EntityPushedByEntityAttackEvent, _>(owner(), move |event| {
        if !pushed_by_entity_attack_call(
            &jvm,
            &event.entity_id().to_string(),
            &event.pushed_by().to_string(),
        ) {
            event.set_cancelled(true);
        }
    });

    let jvm = Arc::clone(&vm);
    events.on::<HangingBreakEvent, _>(owner(), move |event| {
        let remover = event
            .remover()
            .map_or_else(String::new, |id| id.to_string());
        if !hanging_break_call(&jvm, &event.entity().to_string(), event.cause(), &remover) {
            event.set_cancelled(true);
        }
    });

    let jvm = Arc::clone(&vm);
    events.on::<HangingPlaceEvent, _>(owner(), move |event| {
        let block = event.block();
        if !hanging_place_call(
            &jvm,
            &event.entity().to_string(),
            &event.player().to_string(),
            event.world(),
            block.x(),
            block.y(),
            block.z(),
            event.face(),
        ) {
            event.set_cancelled(true);
        }
    });

    let jvm = Arc::clone(&vm);
    events.on::<PlayerCommandPreprocessEvent, _>(owner(), move |event| {
        let message = format!("/{}", event.message());
        match string_call(
            &jvm,
            "fireCommandPreprocess",
            &event.player_id().to_string(),
            Some(&message),
        ) {
            Answer::Nothing => event.set_cancelled(true),
            Answer::Message(rewritten) => {
                event.set_message(rewritten.strip_prefix('/').unwrap_or(&rewritten).to_owned());
            }
            Answer::Unreachable => {}
        }
    });

    let jvm = Arc::clone(&vm);
    events.on::<PlayerLoginEvent, _>(owner(), move |event| {
        let uuid = event.player().gameprofile.id.to_string();
        if let Some(message) = login_call(&jvm, &uuid) {
            event.deny(message);
        }
    });

    let jvm = Arc::clone(&vm);
    events.on::<AsyncPlayerPreLoginEvent, _>(owner(), move |event| {
        if let Some((result, message)) =
            pre_login_call(&jvm, event.name(), event.uuid(), event.address())
        {
            event.disallow(result, message);
        }
    });

    let jvm = Arc::clone(&vm);
    events.on::<PlayerMoveEvent, _>(owner(), move |event| {
        let player = event.player();
        let from = event.from();
        let to = event.to();
        match move_call(
            &jvm,
            &player.gameprofile.id.to_string(),
            &player.get_world().key.to_string(),
            from,
            to,
        ) {
            MoveAnswer::Cancelled => event.set_cancelled(true),
            MoveAnswer::Redirect(destination) => event.set_to(destination),
            MoveAnswer::Accepted | MoveAnswer::Unreachable => {}
        }
    });

    let jvm = Arc::clone(&vm);
    events.on::<FoodLevelChangeEvent, _>(owner(), move |event| {
        match food_level_call(&jvm, &event.player_id().to_string(), event.food_level()) {
            Some(level) => event.set_food_level(level),
            None => event.set_cancelled(true),
        }
    });

    let jvm = Arc::clone(&vm);
    events.on::<PlayerDropItemEvent, _>(owner(), move |event| {
        if !player_drop_call(
            &jvm,
            &event.player_id().to_string(),
            &event.item_id().to_string(),
        ) {
            event.set_cancelled(true);
        }
    });

    let jvm = Arc::clone(&vm);
    events.on::<PlayerBucketEmptyEvent, _>(owner(), move |event| {
        if !player_bucket_empty_call(&jvm, &event.player_id().to_string(), event.bucket()) {
            event.set_cancelled(true);
        }
    });

    let jvm = Arc::clone(&vm);
    events.on::<PlayerBucketFillEvent, _>(owner(), move |event| {
        if !player_bucket_fill_call(
            &jvm,
            &event.player_id().to_string(),
            event.world(),
            event.position(),
            event.bucket(),
        ) {
            event.set_cancelled(true);
        }
    });

    let jvm = Arc::clone(&vm);
    events.on::<PlayerItemBreakEvent, _>(owner(), move |event| {
        player_item_break_call(
            &jvm,
            &event.player_id().to_string(),
            &natives::describe_slot(event.item()),
        );
    });

    let jvm = Arc::clone(&vm);
    events.on::<PlayerFishEvent, _>(owner(), move |event| {
        if !player_fish_call(&jvm, event.player_id(), event.hook_id(), event.state()) {
            event.set_cancelled(true);
        }
    });

    let jvm = Arc::clone(&vm);
    events.on::<ProjectileLaunchEvent, _>(owner(), move |event| {
        if !projectile_launch_call(&jvm, event.shooter(), event.projectile()) {
            event.set_cancelled(true);
        }
    });

    let jvm = Arc::clone(&vm);
    events.on::<PlayerRespawnEvent, _>(owner(), move |event| {
        if let Some((world, position, rotation)) = respawn_call(
            &jvm,
            &event.player_id().to_string(),
            event.world(),
            event.position(),
            event.rotation(),
            event.is_anchor_spawn(),
        ) {
            event.set_spawn(world, position, rotation);
        }
    });

    let jvm = Arc::clone(&vm);
    events.on::<PlayerSpawnLocationEvent, _>(owner(), move |event| {
        if let Some((world, position, rotation)) = spawn_location_call(
            &jvm,
            &event.player_id().to_string(),
            event.world(),
            event.position(),
            event.rotation(),
        ) {
            event.set_spawn(world, position, rotation);
        }
    });

    let jvm = Arc::clone(&vm);
    events.on::<ChunkLoadEvent, _>(owner(), move |event| {
        chunk_load_call(
            &jvm,
            event.world(),
            event.position().0.x,
            event.position().0.y,
            event.new_chunk(),
        );
    });

    let jvm = Arc::clone(&vm);
    events.on::<PortalCreateEvent, _>(owner(), move |event| {
        let encoded = event
            .blocks()
            .iter()
            .map(|pos| format!("{},{},{}", pos.x(), pos.y(), pos.z()))
            .collect::<Vec<_>>()
            .join(";");
        match portal_create_call(&jvm, event.world(), &encoded) {
            Some(blocks) => *event.blocks_mut() = blocks,
            None => event.set_cancelled(true),
        }
    });

    let jvm = Arc::clone(&vm);
    let server_ref = Arc::clone(server);
    events.on::<PlayerPortalEvent, _>(owner(), move |event| {
        let cause = match event.cause() {
            foton_core::TeleportTransitionCause::NetherPortal => "NETHER_PORTAL",
            foton_core::TeleportTransitionCause::EndPortal => "END_PORTAL",
            foton_core::TeleportTransitionCause::EndGateway => "END_GATEWAY",
            _ => "UNKNOWN",
        };
        if let Some(result) = portal_call(
            &jvm,
            event.player_id().to_string().as_str(),
            event.from_world(),
            event.from_position(),
            event.from_rotation(),
            event.to_world(),
            event.to_position(),
            event.to_rotation(),
            cause,
        ) {
            if result.0 {
                event.set_cancelled(true);
                return;
            }
            let Some(key) = result.1.parse().ok() else {
                return;
            };
            if server_ref.worlds.get(&key).is_some() {
                event.set_destination(result.1, result.2, result.3);
            }
        }
    });

    let jvm = Arc::clone(&vm);
    events.on::<PlayerDeathEvent, _>(owner(), move |event| {
        let message = event.death_message().map(str::to_owned);
        let drops = event
            .drops()
            .iter()
            .map(natives::describe_slot)
            .collect::<Vec<_>>()
            .join("\u{001e}");
        if let Some((answer, drops)) = death_call(
            &jvm,
            &event.player_id().to_string(),
            message.as_deref(),
            &drops,
            event.keep_inventory(),
        ) {
            event.set_death_message(answer);
            let parsed = drops
                .split('\u{001e}')
                .filter(|encoded| !encoded.is_empty())
                .filter_map(natives::parse_slot)
                .collect();
            *event.drops_mut() = parsed;
        }
    });

    let jvm = Arc::clone(&vm);
    events.on::<PlayerJoinEvent, _>(owner(), move |event| {
        let uuid = event.player().gameprofile.id.to_string();
        let message = event.message().map(plain);
        match string_call(&jvm, "fireJoin", &uuid, message.as_deref()) {
            Answer::Unreachable => {}
            Answer::Nothing => event.set_message(None),
            Answer::Message(text) => event.set_message(Some(TextComponent::from(text))),
        }
    });

    let jvm = Arc::clone(&vm);
    events.on::<PlayerQuitEvent, _>(owner(), move |event| {
        let uuid = event.player().gameprofile.id.to_string();
        let message = event.message().map(plain);
        match string_call(&jvm, "fireQuit", &uuid, message.as_deref()) {
            Answer::Unreachable => {}
            Answer::Nothing => event.set_message(None),
            Answer::Message(text) => event.set_message(Some(TextComponent::from(text))),
        }
    });

    let jvm = Arc::clone(&vm);
    events.on::<PlayerLocaleChangeEvent, _>(owner(), move |event| {
        locale_change_call(
            &jvm,
            event.player().gameprofile.id,
            event.old_locale(),
            event.new_locale(),
        );
    });

    let jvm = Arc::clone(&vm);
    events.on::<PlayerChatEvent, _>(owner(), move |event| {
        let uuid = event.player().gameprofile.id.to_string();
        let said = event.message().to_owned();
        match chat_call(&jvm, &uuid, &said) {
            None => event.set_cancelled(true),
            Some((rewritten, recipients)) => {
                if rewritten != said {
                    event.set_message(rewritten);
                }
                *event.recipients_mut() = recipients;
            }
        }
    });

    let jvm = Arc::clone(&vm);
    events.on::<BlockExpEvent, _>(owner(), move |event| {
        let Ok(mut env) = jvm.attach_current_thread() else {
            return;
        };
        let Ok(world) = env.new_string(event.world()) else {
            return;
        };
        let result = env.call_static_method(
            BRIDGE,
            "fireBlockExp",
            "(Ljava/lang/String;IIILjava/lang/String;)Ljava/lang/String;",
            &[
                JValue::Object(&world),
                JValue::Int(event.position().x()),
                JValue::Int(event.position().y()),
                JValue::Int(event.position().z()),
                JValue::Int(event.exp_to_drop()),
            ],
        );
        let Ok(value) = result.and_then(JValueGen::l) else {
            return;
        };
        let value = JString::from(value);
        let Ok(text) = env.get_string(&value) else {
            return;
        };
        let text = text.to_string_lossy();
        let mut parts = text.split('|');
        if parts.next() == Some("0") {
            event.set_cancelled(true);
        }
        if let Some(exp) = parts.next().and_then(|v| v.parse().ok()) {
            event.set_exp_to_drop(exp);
        }
    });

    let jvm = Arc::clone(&vm);
    events.on::<BlockBreakEvent, _>(owner(), move |event| {
        if !block_call(&jvm, "fireBlockBreak", event.player(), event.position()) {
            event.set_cancelled(true);
        }
    });

    let jvm = Arc::clone(&vm);
    events.on::<BlockPlaceEvent, _>(owner(), move |event| {
        if !block_place_call(&jvm, event.player(), event.position(), event.item()) {
            event.set_cancelled(true);
        }
    });

    let jvm = Arc::clone(&vm);
    events.on::<BlockFromToEvent, _>(owner(), move |event| {
        if !from_to_call(&jvm, event.world(), event.block(), event.to_block()) {
            event.set_cancelled(true);
        }
    });

    let jvm = Arc::clone(&vm);
    events.on::<CommandEvent, _>(owner(), move |event| {
        let uuid = event
            .player()
            .map(|player| player.gameprofile.id.to_string())
            .unwrap_or_default();
        if command_call(&jvm, &uuid, event.command()) {
            event.set_handled(true);
        }
    });

    let jvm = Arc::clone(&vm);
    events.on::<PlayerCustomPayloadEvent, _>(owner(), move |event| {
        plugin_message_call(
            &jvm,
            &event.player().gameprofile.id.to_string(),
            &event.channel().to_string(),
            event.payload(),
        );
    });

    // The tick. Not a gameplay event: it is what makes `runTask` mean what
    // Bukkit says it means. A plugin hands over a Runnable from whatever
    // thread it likes, and the body runs here -- inside the tick, on the tick
    // thread, where touching the world is safe.
    let jvm = vm;
    let ticking = Arc::clone(server);
    events.on::<ServerTickEvent, _>(owner(), move |_| {
        // Before the plugins run: this is what tells the natives which thread
        // may write to the world, and it runs the writes that could not.
        natives::begin_tick(&ticking);
        drain_scheduler(&jvm);
    });
}

fn block_pre_dispense_call(
    vm: &JavaVM,
    world: &str,
    pos: BlockPos,
    slot: usize,
    item: &ItemStack,
) -> Option<(bool, ItemStack)> {
    let mut env = vm.attach_current_thread().ok()?;
    let world = env.new_string(world).ok()?;
    let encoded = env.new_string(describe_slot(item)).ok()?;
    let answer = env
        .call_static_method(
            BRIDGE,
            "fireBlockPreDispense",
            "(Ljava/lang/String;IIIILjava/lang/String;)Ljava/lang/String;",
            &[
                JValue::Object(&world),
                JValue::Int(pos.x()),
                JValue::Int(pos.y()),
                JValue::Int(pos.z()),
                JValue::Int(i32::try_from(slot).ok()?),
                JValue::Object(&encoded),
            ],
        )
        .ok()?
        .l()
        .ok()?;
    if answer.is_null() {
        return None;
    }
    let value: String = env.get_string(&JString::from(answer)).ok()?.into();
    let mut parts = value.splitn(2, '\u{1f}');
    let cancelled = parts.next()?.parse::<u8>().ok()? != 0;
    let item = parse_slot(parts.next()?)?;
    Some((cancelled, item))
}

/// Drops every subscription the plugin host made.
pub(crate) fn unsubscribe(server: &Arc<Server>) {
    // Plugins receive unload notifications while the JVM is still alive.
    // World instances are not removed from Steel here, so this is the lifecycle boundary.
    // The host calls this before tearing down the JVM.
    server.events().forget(&owner());
}

/// A component as the plain text a Bukkit plugin expects a message to be.
fn world_call(vm: &JavaVM, method: &str, world: &str) {
    let Ok(mut env) = vm.attach_current_thread() else {
        return;
    };
    let Ok(world) = env.new_string(world) else {
        return;
    };
    let _ = env.call_static_method(
        BRIDGE,
        method,
        "(Ljava/lang/String;)V",
        &[JValue::Object(&world)],
    );
}

fn plain(message: &TextComponent) -> String {
    message.to_plain(&DisplayResolutor)
}

/// What the plugins decided about a message.
///
/// Three outcomes that must not be confused. Reaching nobody is not the same
/// as everybody agreeing there should be no message, and a server that
/// silenced its own chat because a JVM thread failed to attach would be a
/// miserable thing to debug.
enum Answer {
    /// The crossing failed. Change nothing.
    Unreachable,
    /// The plugins settled on no message at all.
    Nothing,
    /// The message the plugins settled on.
    Message(String),
}

/// Calls a bridge method that takes a player and a message and returns one.
fn string_call(vm: &JavaVM, method: &str, uuid: &str, message: Option<&str>) -> Answer {
    let reach = || -> Option<Option<String>> {
        let mut env = vm.attach_current_thread().ok()?;
        let uuid = env.new_string(uuid).ok()?;
        let message: JString<'_> = match message {
            Some(text) => env.new_string(text).ok()?,
            None => JString::default(),
        };
        let answer = env
            .call_static_method(
                BRIDGE,
                method,
                "(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
                &[JValue::Object(&uuid), JValue::Object(&message)],
            )
            .ok()?
            .l()
            .ok()?;
        if answer.is_null() {
            return Some(None);
        }
        let answer: JString<'_> = answer.into();
        Some(Some(env.get_string(&answer).ok()?.into()))
    };

    match reach() {
        None => Answer::Unreachable,
        Some(None) => Answer::Nothing,
        Some(Some(message)) => Answer::Message(message),
    }
}

fn chat_call(vm: &JavaVM, uuid: &str, message: &str) -> Option<(String, Vec<Uuid>)> {
    let mut env = vm.attach_current_thread().ok()?;
    let uuid = env.new_string(uuid).ok()?;
    let message = env.new_string(message).ok()?;
    let answer = env
        .call_static_method(
            BRIDGE,
            "fireChat",
            "(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
            &[JValue::Object(&uuid), JValue::Object(&message)],
        )
        .ok()?
        .l()
        .ok()?;
    if answer.is_null() {
        return None;
    }
    let answer: JString<'_> = answer.into();
    let value: String = env.get_string(&answer).ok()?.into();
    let mut parts = value.splitn(2, '\u{1e}');
    let text = parts.next()?.to_owned();
    let recipients = parts
        .next()
        .unwrap_or_default()
        .split(',')
        .filter_map(|value| value.parse::<Uuid>().ok())
        .collect();
    Some((text, recipients))
}

fn login_call(vm: &JavaVM, uuid: &str) -> Option<String> {
    let mut env = vm.attach_current_thread().ok()?;
    let uuid = env.new_string(uuid).ok()?;
    let answer = env
        .call_static_method(
            BRIDGE,
            "fireLogin",
            "(Ljava/lang/String;)Ljava/lang/String;",
            &[JValue::Object(&uuid)],
        )
        .ok()?
        .l()
        .ok()?;
    if answer.is_null() {
        return None;
    }
    let answer: JString<'_> = answer.into();
    let message: String = env.get_string(&answer).ok()?.into();
    (!message.is_empty()).then_some(message)
}

fn interact_call(vm: &JavaVM, player_uuid: &str) -> bool {
    let Ok(mut env) = vm.attach_current_thread() else {
        return true;
    };
    let Ok(uuid) = env.new_string(player_uuid) else {
        return true;
    };
    env.call_static_method(
        BRIDGE,
        "fireInteract",
        "(Ljava/lang/String;)Z",
        &[JValue::Object(&uuid)],
    )
    .and_then(JValueGen::z)
    .unwrap_or(true)
}

fn interact_entity_call(vm: &JavaVM, player_uuid: &str, entity_uuid: &str) -> bool {
    let Ok(mut env) = vm.attach_current_thread() else {
        return true;
    };
    let Ok(player) = env.new_string(player_uuid) else {
        return true;
    };
    let Ok(entity) = env.new_string(entity_uuid) else {
        return true;
    };
    env.call_static_method(
        BRIDGE,
        "fireInteractEntity",
        "(Ljava/lang/String;Ljava/lang/String;)Z",
        &[JValue::Object(&player), JValue::Object(&entity)],
    )
    .and_then(JValueGen::z)
    .unwrap_or(true)
}

fn prepare_craft_call(
    vm: &JavaVM,
    player_uuid: &str,
    matrix: &str,
    result: &str,
    repair: bool,
) -> Option<String> {
    let Ok(mut env) = vm.attach_current_thread() else {
        return None;
    };
    let uuid = env.new_string(player_uuid).ok()?;
    let matrix = env.new_string(matrix).ok()?;
    let result = env.new_string(result).ok()?;
    env.call_static_method(
        BRIDGE,
        "firePrepareCraft",
        "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;Z)Ljava/lang/String;",
        &[
            JValue::Object(&uuid),
            JValue::Object(&matrix),
            JValue::Object(&result),
            JValue::Bool(u8::from(repair)),
        ],
    )
    .ok()?
    .l()
    .ok()
    .and_then(|value| {
        let binding = JString::from(value);
        let text = env.get_string(&binding).ok()?;
        Some(text.to_string_lossy().into_owned())
    })
}

fn crafter_craft_call(
    vm: &JavaVM,
    world: &str,
    position: BlockPos,
    recipe: &str,
    result: &str,
    remaining: &str,
) -> Option<String> {
    let Ok(mut env) = vm.attach_current_thread() else {
        return None;
    };
    let world = env.new_string(world).ok()?;
    let recipe = env.new_string(recipe).ok()?;
    let result = env.new_string(result).ok()?;
    let remaining = env.new_string(remaining).ok()?;
    env.call_static_method(
        BRIDGE,
        "fireCrafterCraft",
        "(Ljava/lang/String;IIILjava/lang/String;Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
        &[JValue::Object(&world), JValue::from(position.x()), JValue::from(position.y()), JValue::from(position.z()), JValue::Object(&recipe), JValue::Object(&result), JValue::Object(&remaining)],
    ).ok()?.l().ok().and_then(|value| {
        let binding = JString::from(value);
        let text = env.get_string(&binding).ok()?;
        Some(text.to_string_lossy().into_owned())
    })
}

fn inventory_click_call(
    vm: &JavaVM,
    player_uuid: &str,
    item: &str,
    cursor: &str,
    click: &str,
    raw_slot: i32,
) -> bool {
    let Ok(mut env) = vm.attach_current_thread() else {
        return true;
    };
    let Ok(uuid) = env.new_string(player_uuid) else {
        return true;
    };
    let Ok(item) = env.new_string(item) else {
        return true;
    };
    let Ok(cursor) = env.new_string(cursor) else {
        return true;
    };
    let Ok(click) = env.new_string(click) else {
        return true;
    };
    env.call_static_method(
        BRIDGE,
        "fireInventoryClick",
        "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;I)Z",
        &[
            JValue::Object(&uuid),
            JValue::Object(&item),
            JValue::Object(&cursor),
            JValue::Object(&click),
            JValue::Int(raw_slot),
        ],
    )
    .and_then(JValueGen::z)
    .unwrap_or(true)
}

fn inventory_open_call(vm: &JavaVM, player_uuid: &str) -> bool {
    let Ok(mut env) = vm.attach_current_thread() else {
        return true;
    };
    let Ok(uuid) = env.new_string(player_uuid) else {
        return true;
    };
    env.call_static_method(
        BRIDGE,
        "fireInventoryOpen",
        "(Ljava/lang/String;)Z",
        &[JValue::Object(&uuid)],
    )
    .and_then(JValueGen::z)
    .unwrap_or(true)
}

fn entity_target_call(vm: &JavaVM, entity_uuid: &str, target_uuid: &str) -> bool {
    let Ok(mut env) = vm.attach_current_thread() else {
        return true;
    };
    let Ok(entity) = env.new_string(entity_uuid) else {
        return true;
    };
    let Ok(target) = env.new_string(target_uuid) else {
        return true;
    };
    env.call_static_method(
        BRIDGE,
        "fireEntityTarget",
        "(Ljava/lang/String;Ljava/lang/String;)Z",
        &[JValue::Object(&entity), JValue::Object(&target)],
    )
    .and_then(JValueGen::z)
    .unwrap_or(true)
}

fn entity_death_call(vm: &JavaVM, entity: Uuid) {
    let Ok(mut env) = vm.attach_current_thread() else {
        return;
    };
    let Ok(uuid) = env.new_string(entity.to_string()) else {
        return;
    };
    let _ = env.call_static_method(
        BRIDGE,
        "fireEntityDeath",
        "(Ljava/lang/String;)V",
        &[JValue::Object(&uuid)],
    );
}

fn entity_resurrect_call(vm: &JavaVM, entity: Uuid) -> bool {
    let Ok(mut env) = vm.attach_current_thread() else {
        return true;
    };
    let Ok(uuid) = env.new_string(entity.to_string()) else {
        return true;
    };
    env.call_static_method(
        BRIDGE,
        "fireEntityResurrect",
        "(Ljava/lang/String;)Z",
        &[JValue::Object(&uuid)],
    )
    .and_then(JValueGen::z)
    .unwrap_or(true)
}

fn player_take_lectern_book_call(
    vm: &JavaVM,
    player: Uuid,
    world: &str,
    position: BlockPos,
) -> bool {
    let Ok(mut env) = vm.attach_current_thread() else {
        return true;
    };
    let Ok(uuid) = env.new_string(player.to_string()) else {
        return true;
    };
    let Ok(world) = env.new_string(world) else {
        return true;
    };
    env.call_static_method(
        BRIDGE,
        "firePlayerTakeLecternBook",
        "(Ljava/lang/String;Ljava/lang/String;III)Z",
        &[
            JValue::Object(&uuid),
            JValue::Object(&world),
            JValue::Int(position.x()),
            JValue::Int(position.y()),
            JValue::Int(position.z()),
        ],
    )
    .and_then(JValueGen::z)
    .unwrap_or(true)
}

fn inventory_drag_call(
    vm: &JavaVM,
    player_uuid: &str,
    slots: &str,
    old_cursor: &str,
    drag_type: &str,
) -> bool {
    let Ok(mut env) = vm.attach_current_thread() else {
        return true;
    };
    let Ok(uuid) = env.new_string(player_uuid) else {
        return true;
    };
    let Ok(slots) = env.new_string(slots) else {
        return true;
    };
    let Ok(cursor) = env.new_string(old_cursor) else {
        return true;
    };
    let Ok(kind) = env.new_string(drag_type) else {
        return true;
    };
    env.call_static_method(
        BRIDGE,
        "fireInventoryDrag",
        "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)Z",
        &[
            JValue::Object(&uuid),
            JValue::Object(&slots),
            JValue::Object(&cursor),
            JValue::Object(&kind),
        ],
    )
    .and_then(JValueGen::z)
    .unwrap_or(true)
}

fn remove_call(vm: &JavaVM, entity: &str) {
    let Ok(mut env) = vm.attach_current_thread() else {
        return;
    };
    let Ok(entity) = env.new_string(entity) else {
        return;
    };
    let _ = env.call_static_method(
        BRIDGE,
        "fireEntityRemove",
        "(Ljava/lang/String;)V",
        &[JValue::Object(&entity)],
    );
}

fn item_spawn_call(
    vm: &JavaVM,
    entity: &str,
    world: &str,
    x: f64,
    y: f64,
    z: f64,
    item: &str,
) -> bool {
    let Ok(mut env) = vm.attach_current_thread() else {
        return true;
    };
    let Ok(entity) = env.new_string(entity) else {
        return true;
    };
    let Ok(world) = env.new_string(world) else {
        return true;
    };
    let Ok(item) = env.new_string(item) else {
        return true;
    };
    env.call_static_method(
        BRIDGE,
        "fireItemSpawn",
        "(Ljava/lang/String;Ljava/lang/String;DDDLjava/lang/String;)Z",
        &[
            JValue::Object(&entity),
            JValue::Object(&world),
            JValue::Double(x),
            JValue::Double(y),
            JValue::Double(z),
            JValue::Object(&item),
        ],
    )
    .and_then(JValueGen::z)
    .unwrap_or(true)
}

fn pickup_call(vm: &JavaVM, entity: &str, item: &str) -> bool {
    let Ok(mut env) = vm.attach_current_thread() else {
        return true;
    };
    let Ok(entity) = env.new_string(entity) else {
        return true;
    };
    let Ok(item) = env.new_string(item) else {
        return true;
    };
    env.call_static_method(
        BRIDGE,
        "fireEntityPickup",
        "(Ljava/lang/String;Ljava/lang/String;)Z",
        &[JValue::Object(&entity), JValue::Object(&item)],
    )
    .and_then(JValueGen::z)
    .unwrap_or(true)
}

fn entity_portal_call(vm: &JavaVM, event: &EntityPortalEvent) -> Option<(String, glam::DVec3)> {
    let Ok(mut env) = vm.attach_current_thread() else {
        return None;
    };
    let Ok(entity) = env.new_string(event.entity().to_string()) else {
        return None;
    };
    let Ok(from_world) = env.new_string(event.from_world()) else {
        return None;
    };
    let Ok(to_world) = env.new_string(event.to_world()) else {
        return None;
    };
    let Ok(portal_type) = env.new_string(event.portal_type()) else {
        return None;
    };
    let from = event.from_position();
    let to = event.to_position();
    let value=env.call_static_method(BRIDGE, "fireEntityPortal", "(Ljava/lang/String;Ljava/lang/String;DDDDLjava/lang/String;DDDDLjava/lang/String;)Ljava/lang/String;", &[JValue::Object(&entity),JValue::Object(&from_world),JValue::Double(from.x),JValue::Double(from.y),JValue::Double(from.z),JValue::Object(&to_world),JValue::Double(to.x),JValue::Double(to.y),JValue::Double(to.z),JValue::Object(&portal_type)]).ok()?.l().ok()?;
    let text: String = env.get_string((&value).into()).ok()?.into();
    if text == "!" {
        return None;
    }
    let mut parts = text.split('|');
    let world = parts.next()?.to_owned();
    let x = parts.next()?.parse().ok()?;
    let y = parts.next()?.parse().ok()?;
    let z = parts.next()?.parse().ok()?;
    Some((world, glam::DVec3::new(x, y, z)))
}
fn pre_creature_spawn_call(
    vm: &JavaVM,
    world: &str,
    x: f64,
    y: f64,
    z: f64,
    entity_type: &str,
    reason: &str,
) -> bool {
    let Ok(mut env) = vm.attach_current_thread() else {
        return true;
    };
    let Ok(world) = env.new_string(world) else {
        return true;
    };
    let Ok(entity_type) = env.new_string(entity_type) else {
        return true;
    };
    let Ok(reason) = env.new_string(reason) else {
        return true;
    };
    env.call_static_method(
        BRIDGE,
        "firePreCreatureSpawn",
        "(Ljava/lang/String;DDDDLjava/lang/String;Ljava/lang/String;)Z",
        &[
            JValue::Object(&world),
            JValue::Double(x),
            JValue::Double(y),
            JValue::Double(z),
            JValue::Object(&entity_type),
            JValue::Object(&reason),
        ],
    )
    .and_then(JValueGen::z)
    .unwrap_or(true)
}
fn creature_spawn_call(
    vm: &JavaVM,
    entity: &str,
    world: &str,
    x: f64,
    y: f64,
    z: f64,
    reason: &str,
) -> bool {
    let Ok(mut env) = vm.attach_current_thread() else {
        return true;
    };
    let Ok(entity) = env.new_string(entity) else {
        return true;
    };
    let Ok(world) = env.new_string(world) else {
        return true;
    };
    let Ok(reason) = env.new_string(reason) else {
        return true;
    };
    env.call_static_method(
        BRIDGE,
        "fireCreatureSpawn",
        "(Ljava/lang/String;Ljava/lang/String;DDDDLjava/lang/String;)Z",
        &[
            JValue::Object(&entity),
            JValue::Object(&world),
            JValue::Double(x),
            JValue::Double(y),
            JValue::Double(z),
            JValue::Object(&reason),
        ],
    )
    .and_then(JValueGen::z)
    .unwrap_or(true)
}

fn regain_health_call(vm: &JavaVM, entity: &str, amount: f32) -> bool {
    let Ok(mut env) = vm.attach_current_thread() else {
        return true;
    };
    let Ok(entity) = env.new_string(entity) else {
        return true;
    };
    env.call_static_method(
        BRIDGE,
        "fireEntityRegainHealth",
        "(Ljava/lang/String;F)Z",
        &[JValue::Object(&entity), JValue::Float(amount)],
    )
    .and_then(JValueGen::z)
    .unwrap_or(true)
}

fn damage_call(vm: &JavaVM, damager: &str, entity: &str, cause: &str) -> bool {
    let Ok(mut env) = vm.attach_current_thread() else {
        return true;
    };
    let Ok(damager) = env.new_string(damager) else {
        return true;
    };
    let Ok(entity) = env.new_string(entity) else {
        return true;
    };
    let Ok(cause) = env.new_string(cause) else {
        return true;
    };
    env.call_static_method(
        BRIDGE,
        "fireEntityDamage",
        "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)Z",
        &[
            JValue::Object(&damager),
            JValue::Object(&entity),
            JValue::Object(&cause),
        ],
    )
    .and_then(JValueGen::z)
    .unwrap_or(true)
}

fn pushed_by_entity_attack_call(vm: &JavaVM, entity: &str, pushed_by: &str) -> bool {
    let Ok(mut env) = vm.attach_current_thread() else {
        return true;
    };
    let Ok(entity) = env.new_string(entity) else {
        return true;
    };
    let Ok(pushed_by) = env.new_string(pushed_by) else {
        return true;
    };
    env.call_static_method(
        BRIDGE,
        "fireEntityPushedByEntityAttack",
        "(Ljava/lang/String;Ljava/lang/String;)Z",
        &[JValue::Object(&entity), JValue::Object(&pushed_by)],
    )
    .and_then(JValueGen::z)
    .unwrap_or(true)
}

fn hanging_break_call(vm: &JavaVM, entity: &str, cause: &str, remover: &str) -> bool {
    let Ok(mut env) = vm.attach_current_thread() else {
        return true;
    };
    let Ok(entity) = env.new_string(entity) else {
        return true;
    };
    let Ok(cause) = env.new_string(cause) else {
        return true;
    };
    let Ok(remover) = env.new_string(remover) else {
        return true;
    };
    env.call_static_method(
        BRIDGE,
        "fireHangingBreak",
        "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)Z",
        &[
            JValue::Object(&entity),
            JValue::Object(&cause),
            JValue::Object(&remover),
        ],
    )
    .and_then(JValueGen::z)
    .unwrap_or(true)
}

#[expect(
    clippy::too_many_arguments,
    reason = "the JNI call it makes takes exactly these; grouping them into a struct \
              would name the halves and still hand them over one by one"
)]
fn hanging_place_call(
    vm: &JavaVM,
    entity: &str,
    player: &str,
    world: &str,
    x: i32,
    y: i32,
    z: i32,
    face: &str,
) -> bool {
    let Ok(mut env) = vm.attach_current_thread() else {
        return true;
    };
    let Ok(entity) = env.new_string(entity) else {
        return true;
    };
    let Ok(player) = env.new_string(player) else {
        return true;
    };
    let Ok(world) = env.new_string(world) else {
        return true;
    };
    let Ok(face) = env.new_string(face) else {
        return true;
    };
    env.call_static_method(
        BRIDGE,
        "fireHangingPlace",
        "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;IIILjava/lang/String;)Z",
        &[
            JValue::Object(&entity),
            JValue::Object(&player),
            JValue::Object(&world),
            JValue::Int(x),
            JValue::Int(y),
            JValue::Int(z),
            JValue::Object(&face),
        ],
    )
    .and_then(JValueGen::z)
    .unwrap_or(true)
}

/// Calls a bridge method about a block. `true` means nothing objected.
///
/// A failed crossing answers `true`: a plugin host that cannot be reached must
/// not silently start cancelling the world's block changes.
fn block_call(vm: &JavaVM, method: &str, player: &Arc<Player>, position: BlockPos) -> bool {
    let Ok(mut env) = vm.attach_current_thread() else {
        return true;
    };
    let Ok(uuid) = env.new_string(player.gameprofile.id.to_string()) else {
        return true;
    };
    let Ok(world) = env.new_string(player.get_world().key.to_string()) else {
        return true;
    };
    env.call_static_method(
        BRIDGE,
        method,
        "(Ljava/lang/String;IIILjava/lang/String;)Z",
        &[
            JValue::Object(&uuid),
            JValue::Int(position.x()),
            JValue::Int(position.y()),
            JValue::Int(position.z()),
            JValue::Object(&world),
        ],
    )
    .and_then(JValueGen::z)
    .unwrap_or(true)
}

fn block_place_call(
    vm: &JavaVM,
    player: &Arc<Player>,
    position: BlockPos,
    item: &ItemStack,
) -> bool {
    let Ok(mut env) = vm.attach_current_thread() else {
        return true;
    };
    let Ok(uuid) = env.new_string(player.gameprofile.id.to_string()) else {
        return true;
    };
    let Ok(world) = env.new_string(player.get_world().key.to_string()) else {
        return true;
    };
    let item = env
        .new_string(format!("{} {}", item.item().key, item.count()))
        .ok();
    let Some(item) = item else {
        return true;
    };
    env.call_static_method(
        BRIDGE,
        "fireBlockPlace",
        "(Ljava/lang/String;IIILjava/lang/String;Ljava/lang/String;)Z",
        &[
            JValue::Object(&uuid),
            JValue::Int(position.x()),
            JValue::Int(position.y()),
            JValue::Int(position.z()),
            JValue::Object(&world),
            JValue::Object(&item),
        ],
    )
    .and_then(JValueGen::z)
    .unwrap_or(true)
}

fn from_to_call(vm: &JavaVM, world: &str, block: BlockPos, to_block: BlockPos) -> bool {
    let Ok(mut env) = vm.attach_current_thread() else {
        return true;
    };
    let Ok(world) = env.new_string(world) else {
        return true;
    };
    env.call_static_method(
        BRIDGE,
        "fireBlockFromTo",
        "(Ljava/lang/String;IIIIII)Z",
        &[
            JValue::Object(&world),
            JValue::Int(block.x()),
            JValue::Int(block.y()),
            JValue::Int(block.z()),
            JValue::Int(to_block.x()),
            JValue::Int(to_block.y()),
            JValue::Int(to_block.z()),
        ],
    )
    .and_then(JValueGen::z)
    .unwrap_or(true)
}

fn death_call(
    vm: &JavaVM,
    uuid: &str,
    message: Option<&str>,
    drops: &str,
    keep_inventory: bool,
) -> Option<(Option<String>, String)> {
    let mut env = vm.attach_current_thread().ok()?;
    let uuid = env.new_string(uuid).ok()?;
    let message = match message {
        Some(value) => env.new_string(value).ok()?,
        None => JString::default(),
    };
    let drops = env.new_string(drops).ok()?;
    let answer = env
        .call_static_method(
            BRIDGE,
            "firePlayerDeath",
            "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;Z)Ljava/lang/String;",
            &[
                JValue::Object(&uuid),
                JValue::Object(&message),
                JValue::Object(&drops),
                JValue::Bool(u8::from(keep_inventory)),
            ],
        )
        .ok()?
        .l()
        .ok()?;
    if answer.is_null() {
        return None;
    }
    let answer: JString<'_> = answer.into();
    let value: String = env.get_string(&answer).ok()?.into();
    let (message, drops) = value.split_once('\u{001f}')?;
    Some((
        (!message.is_empty()).then_some(message.to_owned()),
        drops.to_owned(),
    ))
}

fn inventory_close_call(vm: &JavaVM, uuid: &str) {
    let Ok(mut env) = vm.attach_current_thread() else {
        return;
    };
    let Ok(uuid) = env.new_string(uuid) else {
        return;
    };
    let _ = env.call_static_method(
        BRIDGE,
        "fireInventoryClose",
        "(Ljava/lang/String;)V",
        &[JValue::Object(&uuid)],
    );
}

fn player_open_sign_call(
    vm: &JavaVM,
    uuid: Uuid,
    world: &str,
    position: BlockPos,
    front_side: bool,
    cause: PlayerOpenSignCause,
) -> bool {
    let Ok(mut env) = vm.attach_current_thread() else {
        return true;
    };
    let Ok(uuid) = env.new_string(uuid.to_string()) else {
        return true;
    };
    let Ok(world) = env.new_string(world) else {
        return true;
    };
    let Ok(cause) = env.new_string(match cause {
        PlayerOpenSignCause::Place => "PLACE",
        PlayerOpenSignCause::Interact => "INTERACT",
    }) else {
        return true;
    };
    env.call_static_method(
        BRIDGE,
        "firePlayerOpenSign",
        "(Ljava/lang/String;Ljava/lang/String;IIIZLjava/lang/String;)Z",
        &[
            JValue::Object(&uuid),
            JValue::Object(&world),
            JValue::Int(position.x()),
            JValue::Int(position.y()),
            JValue::Int(position.z()),
            JValue::Bool(front_side.into()),
            JValue::Object(&cause),
        ],
    )
    .ok()
    .and_then(|v| v.z().ok())
    .unwrap_or(true)
}

fn sign_change_call(
    vm: &JavaVM,
    uuid: &str,
    world: &str,
    position: BlockPos,
    lines: &[String; 4],
) -> Option<(bool, [String; 4])> {
    let mut env = vm.attach_current_thread().ok()?;
    let uuid = env.new_string(uuid).ok()?;
    let world = env.new_string(world).ok()?;
    let encoded = lines.join("\u{1f}");
    let lines = env.new_string(encoded).ok()?;
    let answer = env
        .call_static_method(
            BRIDGE,
            "fireSignChange",
            "(Ljava/lang/String;Ljava/lang/String;IIILjava/lang/String;)Ljava/lang/String;",
            &[
                JValue::Object(&uuid),
                JValue::Object(&world),
                JValue::Int(position.x()),
                JValue::Int(position.y()),
                JValue::Int(position.z()),
                JValue::Object(&lines),
            ],
        )
        .ok()?
        .l()
        .ok()?;
    if answer.is_null() {
        return None;
    }
    let answer: JString<'_> = answer.into();
    let value: String = env.get_string(&answer).ok()?.into();
    let mut parts = value.splitn(5, '\u{1f}');
    let cancelled = parts.next()?.parse::<u8>().ok()? != 0;
    let mut out = [String::new(), String::new(), String::new(), String::new()];
    for slot in &mut out {
        parts.next().unwrap_or_default().clone_into(slot);
    }
    Some((cancelled, out))
}

fn weather_change_call(vm: &JavaVM, world: &str, raining: bool) -> bool {
    let Ok(mut env) = vm.attach_current_thread() else {
        return true;
    };
    let Ok(world) = env.new_string(world) else {
        return true;
    };
    env.call_static_method(
        BRIDGE,
        "fireWeatherChange",
        "(Ljava/lang/String;Z)Z",
        &[
            JValue::Object(&JObject::from(world)),
            JValue::Bool(u8::from(raining)),
        ],
    )
    .ok()
    .and_then(|v| v.z().ok())
    .unwrap_or(true)
}

fn thunder_change_call(vm: &JavaVM, world: &str, thundering: bool) -> bool {
    let Ok(mut env) = vm.attach_current_thread() else {
        return true;
    };
    let Ok(world) = env.new_string(world) else {
        return true;
    };
    env.call_static_method(
        BRIDGE,
        "fireThunderChange",
        "(Ljava/lang/String;Z)Z",
        &[
            JValue::Object(&JObject::from(world)),
            JValue::Bool(u8::from(thundering)),
        ],
    )
    .ok()
    .and_then(|v| v.z().ok())
    .unwrap_or(true)
}

fn lightning_strike_call(vm: &JavaVM, entity: &str, world: &str, cause: &str) -> bool {
    // A failed crossing answers "allowed", like every other call here: a
    // plugin host that cannot be reached must not start cancelling the world.
    let Ok(mut env) = vm.attach_current_thread() else {
        return true;
    };
    let Ok(entity) = env.new_string(entity) else {
        return true;
    };
    let Ok(world) = env.new_string(world) else {
        return true;
    };
    let Ok(cause) = env.new_string(cause) else {
        return true;
    };
    env.call_static_method(
        BRIDGE,
        "fireLightningStrike",
        "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)Z",
        &[
            JValue::Object(&entity),
            JValue::Object(&world),
            JValue::Object(&cause),
        ],
    )
    .ok()
    .and_then(|v| v.z().ok())
    .unwrap_or(true)
}

fn explode_call(
    vm: &JavaVM,
    entity: &str,
    world: &str,
    blocks: &[BlockPos],
    explosion_result: &str,
) -> Option<(bool, Vec<BlockPos>)> {
    let mut env = vm.attach_current_thread().ok()?;
    let entity = env.new_string(entity).ok()?;
    let world = env.new_string(world).ok()?;
    let encoded = blocks
        .iter()
        .map(|pos| format!("{}, {}, {}", pos.x(), pos.y(), pos.z()))
        .collect::<Vec<_>>()
        .join(";");
    let blocks = env.new_string(encoded).ok()?;
    let explosion_result = env.new_string(explosion_result).ok()?;
    let answer = env
        .call_static_method(
            BRIDGE,
            "fireEntityExplode",
            "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
            &[
                JValue::Object(&entity),
                JValue::Object(&world),
                JValue::Object(&blocks),
                JValue::Object(&explosion_result),
            ],
        )
        .ok()?
        .l()
        .ok()?;
    if answer.is_null() {
        return None;
    }
    let answer: JString<'_> = answer.into();
    let value: String = env.get_string(&answer).ok()?.into();
    let mut parts = value.splitn(2, '\u{1f}');
    let cancelled = parts.next()?.parse::<u8>().ok()? != 0;
    let mut out = Vec::new();
    for block in parts.next().unwrap_or_default().split(';') {
        let mut coords = block.split(',').map(str::trim);
        let (Some(x), Some(y), Some(z)) = (coords.next(), coords.next(), coords.next()) else {
            continue;
        };
        let (Ok(x), Ok(y), Ok(z)) = (x.parse(), y.parse(), z.parse()) else {
            continue;
        };
        out.push(BlockPos::new(x, y, z));
    }
    Some((cancelled, out))
}

fn block_explode_call(
    vm: &JavaVM,
    world: &str,
    source: BlockPos,
    blocks: &[BlockPos],
) -> Option<(bool, Vec<BlockPos>)> {
    let mut env = vm.attach_current_thread().ok()?;
    let world = env.new_string(world).ok()?;
    let encoded = blocks
        .iter()
        .map(|pos| format!("{}, {}, {}", pos.x(), pos.y(), pos.z()))
        .collect::<Vec<_>>()
        .join(";");
    let encoded = env.new_string(encoded).ok()?;
    let answer = env
        .call_static_method(
            BRIDGE,
            "fireBlockExplode",
            "(Ljava/lang/String;IIILjava/lang/String;)Ljava/lang/String;",
            &[
                JValue::Object(&world),
                JValue::Int(source.x()),
                JValue::Int(source.y()),
                JValue::Int(source.z()),
                JValue::Object(&encoded),
            ],
        )
        .ok()?
        .l()
        .ok()?;
    if answer.is_null() {
        return None;
    }
    let value: String = env.get_string(&JString::from(answer)).ok()?.into();
    let mut parts = value.splitn(2, '\u{1f}');
    let cancelled = parts.next()?.parse::<u8>().ok()? != 0;
    let mut out = Vec::new();
    for block in parts.next().unwrap_or_default().split(';') {
        let mut xyz = block.split(',').map(str::trim);
        let (Some(x), Some(y), Some(z)) = (xyz.next(), xyz.next(), xyz.next()) else {
            continue;
        };
        let (Ok(x), Ok(y), Ok(z)) = (x.parse(), y.parse(), z.parse()) else {
            continue;
        };
        out.push(BlockPos::new(x, y, z));
    }
    Some((cancelled, out))
}

fn block_dispense_call(
    vm: &JavaVM,
    world: &str,
    pos: BlockPos,
    item: &ItemStack,
) -> Option<(bool, ItemStack)> {
    let mut env = vm.attach_current_thread().ok()?;
    let world = env.new_string(world).ok()?;
    let encoded = env.new_string(describe_slot(item)).ok()?;
    let answer = env
        .call_static_method(
            BRIDGE,
            "fireBlockDispense",
            "(Ljava/lang/String;IIILjava/lang/String;)Ljava/lang/String;",
            &[
                JValue::Object(&world),
                JValue::Int(pos.x()),
                JValue::Int(pos.y()),
                JValue::Int(pos.z()),
                JValue::Object(&encoded),
            ],
        )
        .ok()?
        .l()
        .ok()?;
    if answer.is_null() {
        return None;
    }
    let value: String = env.get_string(&JString::from(answer)).ok()?.into();
    let mut parts = value.splitn(2, '\u{1f}');
    let cancelled = parts.next()?.parse::<u8>().ok()? != 0;
    let item = parse_slot(parts.next().unwrap_or_default())?;
    Some((cancelled, item))
}

fn block_burn_call(vm: &JavaVM, world: &str, pos: BlockPos) -> bool {
    let Ok(mut env) = vm.attach_current_thread() else {
        return true;
    };
    let Ok(world) = env.new_string(world) else {
        return true;
    };
    env.call_static_method(
        BRIDGE,
        "fireBlockBurn",
        "(Ljava/lang/String;III)Z",
        &[
            JValue::Object(&world),
            JValue::Int(pos.x()),
            JValue::Int(pos.y()),
            JValue::Int(pos.z()),
        ],
    )
    .and_then(JValueGen::z)
    .unwrap_or(true)
}

fn block_fade_call(vm: &JavaVM, world: &str, pos: BlockPos) -> bool {
    let Ok(mut env) = vm.attach_current_thread() else {
        return true;
    };
    let Ok(world) = env.new_string(world) else {
        return true;
    };
    env.call_static_method(
        BRIDGE,
        "fireBlockFade",
        "(Ljava/lang/String;III)Z",
        &[
            JValue::Object(&world),
            JValue::Int(pos.x()),
            JValue::Int(pos.y()),
            JValue::Int(pos.z()),
        ],
    )
    .and_then(JValueGen::z)
    .unwrap_or(true)
}

fn leaves_decay_call(vm: &JavaVM, world: &str, pos: BlockPos) -> bool {
    let Ok(mut env) = vm.attach_current_thread() else {
        return true;
    };
    let Ok(world) = env.new_string(world) else {
        return true;
    };
    env.call_static_method(
        BRIDGE,
        "fireLeavesDecay",
        "(Ljava/lang/String;III)Z",
        &[
            JValue::Object(&world),
            JValue::Int(pos.x()),
            JValue::Int(pos.y()),
            JValue::Int(pos.z()),
        ],
    )
    .and_then(JValueGen::z)
    .unwrap_or(true)
}

fn block_ignite_call(
    vm: &JavaVM,
    world: &str,
    pos: BlockPos,
    cause: &str,
    player: Option<Uuid>,
) -> bool {
    let Ok(mut env) = vm.attach_current_thread() else {
        return true;
    };
    let Ok(world) = env.new_string(world) else {
        return true;
    };
    let Ok(cause) = env.new_string(cause) else {
        return true;
    };
    let call = |env: &mut jni::JNIEnv<'_>, player: &JObject<'_>| {
        env.call_static_method(
            BRIDGE,
            "fireBlockIgnite",
            "(Ljava/lang/String;IIILjava/lang/String;Ljava/lang/String;)Z",
            &[
                JValue::Object(&world),
                JValue::Int(pos.x()),
                JValue::Int(pos.y()),
                JValue::Int(pos.z()),
                JValue::Object(&cause),
                JValue::Object(player),
            ],
        )
        .and_then(JValueGen::z)
        .unwrap_or(true)
    };

    if let Some(id) = player {
        let Ok(player) = env.new_string(id.to_string()) else {
            return true;
        };
        let player: JObject<'_> = player.into();
        call(&mut env, &player)
    } else {
        call(&mut env, &JObject::null())
    }
}

fn block_fertilize_call(vm: &JavaVM, world: &str, pos: BlockPos, player: Option<Uuid>) -> bool {
    let Ok(mut env) = vm.attach_current_thread() else {
        return true;
    };
    let Ok(world) = env.new_string(world) else {
        return true;
    };
    let player = player.and_then(|id| env.new_string(id.to_string()).ok());
    let mut call = |player: &JObject<'_>| {
        env.call_static_method(
            BRIDGE,
            "fireBlockFertilize",
            "(Ljava/lang/String;IIILjava/lang/String;)Z",
            &[
                JValue::Object(&world),
                JValue::Int(pos.x()),
                JValue::Int(pos.y()),
                JValue::Int(pos.z()),
                JValue::Object(player),
            ],
        )
        .and_then(JValueGen::z)
        .unwrap_or(true)
    };
    if let Some(player) = player {
        let player: JObject<'_> = player.into();
        call(&player)
    } else {
        call(&JObject::null())
    }
}

fn entity_change_block_call(
    vm: &JavaVM,
    entity: uuid::Uuid,
    world: &str,
    pos: BlockPos,
    to: &str,
) -> bool {
    let Ok(mut env) = vm.attach_current_thread() else {
        return true;
    };
    let Ok(entity) = env.new_string(entity.to_string()) else {
        return true;
    };
    let Ok(world) = env.new_string(world) else {
        return true;
    };
    let Ok(to) = env.new_string(to) else {
        return true;
    };
    env.call_static_method(
        BRIDGE,
        "fireEntityChangeBlock",
        "(Ljava/lang/String;Ljava/lang/String;IIILjava/lang/String;)Z",
        &[
            JValue::Object(&entity),
            JValue::Object(&world),
            JValue::Int(pos.x()),
            JValue::Int(pos.y()),
            JValue::Int(pos.z()),
            JValue::Object(&to),
        ],
    )
    .ok()
    .and_then(|value| value.z().ok())
    .unwrap_or(true)
}

fn block_damage_call(vm: &JavaVM, player: uuid::Uuid, world: &str, pos: BlockPos) -> bool {
    let Ok(mut env) = vm.attach_current_thread() else {
        return true;
    };
    let Ok(player) = env.new_string(player.to_string()) else {
        return true;
    };
    let Ok(world) = env.new_string(world) else {
        return true;
    };
    env.call_static_method(
        BRIDGE,
        "fireBlockDamage",
        "(Ljava/lang/String;Ljava/lang/String;III)Z",
        &[
            JValue::Object(&player),
            JValue::Object(&world),
            JValue::Int(pos.x()),
            JValue::Int(pos.y()),
            JValue::Int(pos.z()),
        ],
    )
    .ok()
    .and_then(|value| value.z().ok())
    .unwrap_or(true)
}

fn locale_change_call(vm: &JavaVM, player: uuid::Uuid, old_locale: &str, new_locale: &str) {
    let Ok(mut env) = vm.attach_current_thread() else {
        return;
    };
    let Ok(player) = env.new_string(player.to_string()) else {
        return;
    };
    let Ok(old_locale) = env.new_string(old_locale) else {
        return;
    };
    let Ok(new_locale) = env.new_string(new_locale) else {
        return;
    };
    let _ = env.call_static_method(
        BRIDGE,
        "fireLocaleChange",
        "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)V",
        &[
            JValue::Object(&player),
            JValue::Object(&old_locale),
            JValue::Object(&new_locale),
        ],
    );
}

fn player_advancement_criterion_grant_call(
    vm: &JavaVM,
    player: uuid::Uuid,
    key: &str,
    criterion: &str,
) -> bool {
    let Ok(mut env) = vm.attach_current_thread() else {
        return true;
    };
    let Ok(player) = env.new_string(player.to_string()) else {
        return true;
    };
    let Ok(key) = env.new_string(key) else {
        return true;
    };
    let Ok(criterion) = env.new_string(criterion) else {
        return true;
    };
    let Ok(value) = env.call_static_method(
        BRIDGE,
        "firePlayerAdvancementCriterionGrant",
        "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)Z",
        &[
            JValue::Object(&player),
            JValue::Object(&key),
            JValue::Object(&criterion),
        ],
    ) else {
        return true;
    };
    value.z().unwrap_or(true)
}

fn player_advancement_done_call(vm: &JavaVM, player: uuid::Uuid, key: &str) {
    let Ok(mut env) = vm.attach_current_thread() else {
        return;
    };
    let Ok(player) = env.new_string(player.to_string()) else {
        return;
    };
    let Ok(key) = env.new_string(key) else {
        return;
    };
    let _ = env.call_static_method(
        BRIDGE,
        "firePlayerAdvancementDone",
        "(Ljava/lang/String;Ljava/lang/String;)V",
        &[JValue::Object(&player), JValue::Object(&key)],
    );
}

fn player_fish_call(vm: &JavaVM, player: uuid::Uuid, hook: uuid::Uuid, state: &str) -> bool {
    let Ok(mut env) = vm.attach_current_thread() else {
        return true;
    };
    let Ok(player) = env.new_string(player.to_string()) else {
        return true;
    };
    let Ok(hook) = env.new_string(hook.to_string()) else {
        return true;
    };
    let Ok(state) = env.new_string(state) else {
        return true;
    };
    env.call_static_method(
        BRIDGE,
        "firePlayerFish",
        "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)Z",
        &[
            JValue::Object(&player),
            JValue::Object(&hook),
            JValue::Object(&state),
        ],
    )
    .ok()
    .and_then(|value| value.z().ok())
    .unwrap_or(true)
}

fn projectile_launch_call(vm: &JavaVM, shooter: uuid::Uuid, projectile: uuid::Uuid) -> bool {
    let Ok(mut env) = vm.attach_current_thread() else {
        return true;
    };
    let Ok(shooter) = env.new_string(shooter.to_string()) else {
        return true;
    };
    let Ok(projectile) = env.new_string(projectile.to_string()) else {
        return true;
    };
    env.call_static_method(
        BRIDGE,
        "fireProjectileLaunch",
        "(Ljava/lang/String;Ljava/lang/String;)Z",
        &[JValue::Object(&shooter), JValue::Object(&projectile)],
    )
    .ok()
    .and_then(|value| value.z().ok())
    .unwrap_or(true)
}

fn transform_call(
    vm: &JavaVM,
    entity: uuid::Uuid,
    transformed: uuid::Uuid,
    reason: ConversionReason,
) -> bool {
    let Ok(mut env) = vm.attach_current_thread() else {
        return true;
    };
    let Ok(entity) = env.new_string(entity.to_string()) else {
        return true;
    };
    let Ok(transformed) = env.new_string(transformed.to_string()) else {
        return true;
    };
    let reason = match reason {
        ConversionReason::Cured => "CURED",
        ConversionReason::Drowned => "DROWNED",
        ConversionReason::Frozen => "FROZEN",
        ConversionReason::Infection => "INFECTION",
        ConversionReason::Lightning => "LIGHTNING",
        ConversionReason::PiglinZombification => "PIGLIN_ZOMBIFICATION",
        ConversionReason::Poison => "POISON",
        ConversionReason::Split => "SPLIT",
        ConversionReason::Unknown => "UNKNOWN",
    };
    let Ok(reason) = env.new_string(reason) else {
        return true;
    };
    env.call_static_method(
        BRIDGE,
        "fireEntityTransform",
        "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)Z",
        &[
            JValue::Object(&entity),
            JValue::Object(&transformed),
            JValue::Object(&reason),
        ],
    )
    .ok()
    .and_then(|value| value.z().ok())
    .unwrap_or(true)
}

fn pre_login_call(
    vm: &JavaVM,
    name: &str,
    uuid: uuid::Uuid,
    address: SocketAddr,
) -> Option<(AsyncPlayerPreLoginResult, String)> {
    let mut env = vm.attach_current_thread().ok()?;
    let name = env.new_string(name).ok()?;
    let uuid = env.new_string(uuid.to_string()).ok()?;
    let address = env.new_string(address.ip().to_string()).ok()?;
    let answer = env
        .call_static_method(
            BRIDGE,
            "fireAsyncPreLogin",
            "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
            &[
                JValue::Object(&name),
                JValue::Object(&uuid),
                JValue::Object(&address),
            ],
        )
        .ok()?
        .l()
        .ok()?;
    if answer.is_null() {
        return None;
    }
    let value: String = env.get_string(&JString::from(answer)).ok()?.into();
    let (result, message) = value.split_once('\u{001f}')?;
    let result = match result {
        "KICK_FULL" => AsyncPlayerPreLoginResult::KickFull,
        "KICK_BANNED" => AsyncPlayerPreLoginResult::KickBanned,
        "KICK_WHITELIST" => AsyncPlayerPreLoginResult::KickWhitelist,
        "KICK_OTHER" => AsyncPlayerPreLoginResult::KickOther,
        _ => return None,
    };
    (!message.is_empty()).then_some((result, message.to_owned()))
}

fn piston_call(
    vm: &JavaVM,
    world: &str,
    piston: BlockPos,
    direction: &str,
    extending: bool,
    blocks: &[BlockPos],
) -> Option<(bool, Vec<BlockPos>)> {
    let mut env = vm.attach_current_thread().ok()?;
    let world = env.new_string(world).ok()?;
    let direction = env.new_string(direction).ok()?;
    let encoded = blocks
        .iter()
        .map(|pos| format!("{}, {}, {}", pos.x(), pos.y(), pos.z()))
        .collect::<Vec<_>>()
        .join(";");
    let blocks = env.new_string(encoded).ok()?;
    let answer = env
        .call_static_method(
            BRIDGE,
            "firePiston",
            "(Ljava/lang/String;IIILjava/lang/String;ZLjava/lang/String;)Ljava/lang/String;",
            &[
                JValue::Object(&world),
                JValue::Int(piston.x()),
                JValue::Int(piston.y()),
                JValue::Int(piston.z()),
                JValue::Object(&direction),
                JValue::Bool(u8::from(extending)),
                JValue::Object(&blocks),
            ],
        )
        .ok()?
        .l()
        .ok()?;
    if answer.is_null() {
        return None;
    }
    let answer: JString<'_> = answer.into();
    let value: String = env.get_string(&answer).ok()?.into();
    let mut parts = value.splitn(2, '\u{1f}');
    let cancelled = parts.next()?.parse::<u8>().ok()? != 0;
    let mut out = Vec::new();
    for block in parts.next().unwrap_or_default().split(';') {
        let mut coords = block.split(',').map(str::trim);
        let (Some(x), Some(y), Some(z)) = (coords.next(), coords.next(), coords.next()) else {
            continue;
        };
        let (Ok(x), Ok(y), Ok(z)) = (x.parse(), y.parse(), z.parse()) else {
            continue;
        };
        out.push(BlockPos::new(x, y, z));
    }
    Some((cancelled, out))
}

fn food_level_call(vm: &JavaVM, player: &str, level: i32) -> Option<i32> {
    let Ok(mut env) = vm.attach_current_thread() else {
        return Some(level);
    };
    let Ok(player) = env.new_string(player) else {
        return Some(level);
    };
    env.call_static_method(
        BRIDGE,
        "fireFoodLevelChange",
        "(Ljava/lang/String;I)I",
        &[JValue::Object(&player), JValue::Int(level)],
    )
    .ok()
    .and_then(|value| value.i().ok())
    .and_then(|value| (value >= 0).then_some(value))
}

fn player_drop_call(vm: &JavaVM, player: &str, item: &str) -> bool {
    let Ok(mut env) = vm.attach_current_thread() else {
        return true;
    };
    let Ok(player) = env.new_string(player) else {
        return true;
    };
    let Ok(item) = env.new_string(item) else {
        return true;
    };
    env.call_static_method(
        BRIDGE,
        "firePlayerDropItem",
        "(Ljava/lang/String;Ljava/lang/String;)Z",
        &[JValue::Object(&player), JValue::Object(&item)],
    )
    .and_then(JValueGen::z)
    .unwrap_or(true)
}

fn player_bucket_empty_call(vm: &JavaVM, player: &str, bucket: &str) -> bool {
    let Ok(mut env) = vm.attach_current_thread() else {
        return true;
    };
    let Ok(player) = env.new_string(player) else {
        return true;
    };
    let Ok(bucket) = env.new_string(bucket) else {
        return true;
    };
    env.call_static_method(
        BRIDGE,
        "firePlayerBucketEmpty",
        "(Ljava/lang/String;Ljava/lang/String;)Z",
        &[JValue::Object(&player), JValue::Object(&bucket)],
    )
    .and_then(JValueGen::z)
    .unwrap_or(true)
}

fn player_bucket_fill_call(
    vm: &JavaVM,
    player: &str,
    world: &str,
    pos: BlockPos,
    bucket: &str,
) -> bool {
    let Ok(mut env) = vm.attach_current_thread() else {
        return true;
    };
    let Ok(player) = env.new_string(player) else {
        return true;
    };
    let Ok(world) = env.new_string(world) else {
        return true;
    };
    let Ok(bucket) = env.new_string(bucket) else {
        return true;
    };
    env.call_static_method(
        BRIDGE,
        "firePlayerBucketFill",
        "(Ljava/lang/String;Ljava/lang/String;IIILjava/lang/String;)Z",
        &[
            JValue::Object(&player),
            JValue::Object(&world),
            JValue::Int(pos.x()),
            JValue::Int(pos.y()),
            JValue::Int(pos.z()),
            JValue::Object(&bucket),
        ],
    )
    .and_then(JValueGen::z)
    .unwrap_or(true)
}

fn player_item_break_call(vm: &JavaVM, player: &str, item: &str) {
    let Ok(mut env) = vm.attach_current_thread() else {
        return;
    };
    let (Ok(player), Ok(item)) = (env.new_string(player), env.new_string(item)) else {
        return;
    };
    let _ = env.call_static_method(
        BRIDGE,
        "firePlayerItemBreak",
        "(Ljava/lang/String;Ljava/lang/String;)V",
        &[JValue::Object(&player), JValue::Object(&item)],
    );
}

fn respawn_call(
    vm: &JavaVM,
    uuid: &str,
    world: &str,
    position: [f64; 3],
    rotation: (f32, f32),
    anchor_spawn: bool,
) -> Option<(String, [f64; 3], (f32, f32))> {
    spawn_location_call_named(
        vm,
        "firePlayerRespawn",
        uuid,
        world,
        position,
        rotation,
        anchor_spawn,
    )
}

fn spawn_location_call(
    vm: &JavaVM,
    uuid: &str,
    world: &str,
    position: [f64; 3],
    rotation: (f32, f32),
) -> Option<(String, [f64; 3], (f32, f32))> {
    spawn_location_call_named(
        vm,
        "firePlayerSpawnLocation",
        uuid,
        world,
        position,
        rotation,
        false,
    )
}

fn spawn_location_call_named(
    vm: &JavaVM,
    method: &str,
    uuid: &str,
    world: &str,
    position: [f64; 3],
    rotation: (f32, f32),
    anchor_spawn: bool,
) -> Option<(String, [f64; 3], (f32, f32))> {
    let Ok(mut env) = vm.attach_current_thread() else {
        return None;
    };
    let uuid = env.new_string(uuid).ok()?;
    let encoded = format!(
        "{}|{}|{}|{}|{}|{}",
        world, position[0], position[1], position[2], rotation.0, rotation.1
    );
    let encoded = env.new_string(encoded).ok()?;
    let signature = if method == "firePlayerRespawn" {
        "(Ljava/lang/String;Ljava/lang/String;Z)Ljava/lang/String;"
    } else {
        "(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;"
    };
    let args = if method == "firePlayerRespawn" {
        vec![
            JValue::Object(&uuid),
            JValue::Object(&encoded),
            JValue::Bool(anchor_spawn.into()),
        ]
    } else {
        vec![JValue::Object(&uuid), JValue::Object(&encoded)]
    };
    let value = env
        .call_static_method(BRIDGE, method, signature, &args)
        .ok()?
        .l()
        .ok()?;
    if value.is_null() {
        return None;
    }
    let result: String = env
        .get_string(&jni::objects::JString::from(value))
        .ok()?
        .into();
    let mut fields = result.split('|');
    let world = fields.next()?.to_owned();
    let x = fields.next()?.parse().ok()?;
    let y = fields.next()?.parse().ok()?;
    let z = fields.next()?.parse().ok()?;
    let yaw = fields.next()?.parse().ok()?;
    let pitch = fields.next()?.parse().ok()?;
    Some((world, [x, y, z], (yaw, pitch)))
}

#[expect(
    clippy::too_many_arguments,
    reason = "a portal crossing is a where-from and a where-to, each a world, a \
              position and a rotation, plus who and why"
)]
fn portal_call(
    vm: &JavaVM,
    uuid: &str,
    from_world: &str,
    from_position: glam::DVec3,
    from_rotation: (f32, f32),
    to_world: &str,
    to_position: glam::DVec3,
    to_rotation: (f32, f32),
    cause: &str,
) -> Option<(bool, String, glam::DVec3, (f32, f32))> {
    let mut env = vm.attach_current_thread().ok()?;
    let uuid = env.new_string(uuid).ok()?;
    let encoded = format!(
        "{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
        from_world,
        from_position.x,
        from_position.y,
        from_position.z,
        from_rotation.0,
        from_rotation.1,
        to_world,
        to_position.x,
        to_position.y,
        to_position.z,
        to_rotation.0,
        to_rotation.1,
        cause
    );
    let encoded = env.new_string(encoded).ok()?;
    let value = env
        .call_static_method(
            BRIDGE,
            "firePlayerPortal",
            "(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
            &[JValue::Object(&uuid), JValue::Object(&encoded)],
        )
        .ok()?
        .l()
        .ok()?;
    let result: String = env.get_string(&JString::from(value)).ok()?.into();
    if result.starts_with("0|") {
        return Some((true, String::new(), to_position, to_rotation));
    }
    let mut fields = result.split('|');
    fields.next()?;
    let world = fields.next()?.to_owned();
    let x = fields.next()?.parse().ok()?;
    let y = fields.next()?.parse().ok()?;
    let z = fields.next()?.parse().ok()?;
    let yaw = fields.next()?.parse().ok()?;
    let pitch = fields.next()?.parse().ok()?;
    Some((false, world, glam::DVec3::new(x, y, z), (yaw, pitch)))
}
fn chunk_load_call(vm: &JavaVM, world: &str, x: i32, z: i32, new_chunk: bool) {
    let Ok(mut env) = vm.attach_current_thread() else {
        return;
    };
    let Ok(world) = env.new_string(world) else {
        return;
    };
    let _ = env.call_static_method(
        BRIDGE,
        "fireChunkLoad",
        "(Ljava/lang/String;IIZ)V",
        &[
            JValue::Object(&world),
            JValue::Int(x),
            JValue::Int(z),
            JValue::Bool(u8::from(new_chunk)),
        ],
    );
}

fn portal_create_call(vm: &JavaVM, world: &str, blocks: &str) -> Option<Vec<BlockPos>> {
    let fallback = || parse_portal_positions(blocks);
    let Ok(mut env) = vm.attach_current_thread() else {
        return fallback();
    };
    let Ok(world) = env.new_string(world) else {
        return fallback();
    };
    let Ok(blocks) = env.new_string(blocks) else {
        return fallback();
    };
    let value = env
        .call_static_method(
            BRIDGE,
            "firePortalCreate",
            "(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
            &[JValue::Object(&world), JValue::Object(&blocks)],
        )
        .ok()?
        .l()
        .ok()?;
    let value = env
        .get_string(&JString::from(value))
        .ok()?
        .to_string_lossy()
        .into_owned();
    let (allowed, encoded) = value.split_once('|')?;
    if allowed != "1" {
        return None;
    }
    let parsed = encoded
        .split(';')
        .filter(|entry| !entry.is_empty())
        .map(|entry| {
            let mut coordinates = entry.split(',');
            let x = coordinates.next()?.parse().ok()?;
            let y = coordinates.next()?.parse().ok()?;
            let z = coordinates.next()?.parse().ok()?;
            Some(BlockPos::new(x, y, z))
        })
        .collect::<Option<Vec<_>>>()?;
    Some(parsed)
}

fn parse_portal_positions(encoded: &str) -> Option<Vec<BlockPos>> {
    encoded
        .split(';')
        .filter(|entry| !entry.is_empty())
        .map(|entry| {
            let mut coordinates = entry.split(',');
            Some(BlockPos::new(
                coordinates.next()?.parse().ok()?,
                coordinates.next()?.parse().ok()?,
                coordinates.next()?.parse().ok()?,
            ))
        })
        .collect()
}

enum MoveAnswer {
    Cancelled,
    Accepted,
    Redirect(glam::DVec3),
    Unreachable,
}

fn move_call(
    vm: &JavaVM,
    uuid: &str,
    world: &str,
    from: glam::DVec3,
    to: glam::DVec3,
) -> MoveAnswer {
    let Ok(mut env) = vm.attach_current_thread() else {
        return MoveAnswer::Unreachable;
    };
    let Ok(uuid) = env.new_string(uuid) else {
        return MoveAnswer::Unreachable;
    };
    let Ok(world) = env.new_string(world) else {
        return MoveAnswer::Unreachable;
    };
    let answer = env
        .call_static_method(
            BRIDGE,
            "fireMove",
            "(Ljava/lang/String;Ljava/lang/String;DDDDDD)Ljava/lang/String;",
            &[
                JValue::Object(&uuid),
                JValue::Object(&world),
                JValue::Double(from.x),
                JValue::Double(from.y),
                JValue::Double(from.z),
                JValue::Double(to.x),
                JValue::Double(to.y),
                JValue::Double(to.z),
            ],
        )
        .ok()
        .and_then(|value| value.l().ok());
    let Some(answer) = answer else {
        return MoveAnswer::Unreachable;
    };
    if answer.is_null() {
        return MoveAnswer::Cancelled;
    }
    let answer: JString<'_> = answer.into();
    let Ok(value) = env.get_string(&answer) else {
        return MoveAnswer::Unreachable;
    };
    let value: String = value.into();
    if value.is_empty() {
        return MoveAnswer::Accepted;
    }
    let mut parts = value.split(',');
    let (Some(x), Some(y), Some(z)) = (parts.next(), parts.next(), parts.next()) else {
        return MoveAnswer::Unreachable;
    };
    let (Ok(x), Ok(y), Ok(z)) = (x.parse(), y.parse(), z.parse()) else {
        return MoveAnswer::Unreachable;
    };
    MoveAnswer::Redirect(glam::DVec3::new(x, y, z))
}

/// Runs whatever the plugins queued for this tick.
///
/// A failed crossing is not worth a log line here -- the same message twenty
/// times a second helps nobody -- so it runs nothing this tick and tries again
/// on the next.
fn drain_scheduler(vm: &JavaVM) {
    let Ok(mut env) = vm.attach_current_thread() else {
        return;
    };
    // The answer is how many task bodies ran. Nothing here needs it; the
    // fixture asserts on it through the same call.
    let _ = env.call_static_method(SCHEDULER, "tick", "()I", &[]);
}

/// Offers one typed command to the plugins, and reads back who owned it.
///
/// A failed crossing answers "nobody owned it", so the server carries on to
/// its own dispatcher. Answering the other way would swallow every command
/// typed on a server whose plugin host had stopped responding.
fn command_call(vm: &JavaVM, uuid: &str, line: &str) -> bool {
    let reach = || -> Option<bool> {
        let mut env = vm.attach_current_thread().ok()?;
        let uuid = env.new_string(uuid).ok()?;
        let line = env.new_string(line).ok()?;
        env.call_static_method(
            BRIDGE,
            "fireCommand",
            "(Ljava/lang/String;Ljava/lang/String;)Z",
            &[JValue::Object(&uuid), JValue::Object(&line)],
        )
        .ok()?
        .z()
        .ok()
    };
    reach().unwrap_or(false)
}

/// Delivers one opaque client payload to the Bukkit channel registry.
fn plugin_message_call(vm: &JavaVM, uuid: &str, channel: &str, payload: &[u8]) {
    let Ok(mut env) = vm.attach_current_thread() else {
        return;
    };
    let Ok(uuid) = env.new_string(uuid) else {
        return;
    };
    let Ok(channel) = env.new_string(channel) else {
        return;
    };
    let Ok(payload) = env.byte_array_from_slice(payload) else {
        return;
    };
    let _ = env.call_static_method(
        MESSENGER,
        "dispatchFromNetwork",
        "(Ljava/lang/String;Ljava/lang/String;[B)V",
        &[
            JValue::Object(&uuid),
            JValue::Object(&channel),
            JValue::Object(&payload),
        ],
    );
}
