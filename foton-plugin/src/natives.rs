//! What the Java side asks Foton, answered.
//!
//! Registered with `RegisterNatives` rather than found by symbol lookup: Foton
//! is one binary and its symbols are its own business, and a JVM searching a
//! statically linked executable for `Java_foton_Native_serverName` is a way to
//! discover at runtime that the linker discarded it.
//!
//! Every function here is called from a JVM thread, not from the game tick.
//! World mutations remain scheduler-owned; narrowly scoped plugin registries
//! (such as runtime recipes) expose their own synchronized write path.

use crate::packet_tap;
use foton_utils::serial::budget;
use std::borrow::Cow;
use std::fmt::Write;
use std::io::{Cursor, Error as IoError, Result as IoResult, Write as IoWrite};
use std::mem;
use std::ptr::null_mut;
use std::str::{self, FromStr as _};
#[cfg(test)]
use std::sync::LazyLock;
use std::sync::{Arc, OnceLock, Weak};
use std::thread::{self, ThreadId};
use std::time::{Duration, Instant};

use crate::relay::{self, cooking_kind, describe_cooking};
use foton_core::advancement::ADVANCEMENT_TREE;
use foton_core::behavior::blocks::vegetation::tree_grower::generate_tree as grow_tree;
use foton_core::block_entity::entities::BannerBlockEntity;
use foton_core::block_entity::entities::FurnaceBlockEntity;
use foton_core::block_entity::entities::HopperBlockEntity;
use foton_core::block_entity::entities::JukeboxBlockEntity;
use foton_core::block_entity::entities::LecternBlockEntity;
use foton_core::block_entity::entities::SpawnerBlockEntity;
use foton_core::block_entity::entities::{
    BREWING_STAND_SLOTS, BrewingStandBlockEntity, BrewingStandStateSnapshot, BrewingStandStateView,
};
use foton_core::block_entity::entities::{SignBlockEntity, SignText};
use foton_core::block_entity::{BLOCK_ENTITIES, SharedBlockEntity};
use foton_core::boss_event::ServerBossEvent;
use foton_core::chunk::chunk_request::ChunkRequestState;
use foton_core::chunk::light::LightLayer;
use foton_core::chunk::{
    chunk_request::{ChunkRequestHandle, ChunkTicketKind},
    status::ChunkStatus,
};
use foton_core::entity::ItemFrame;
use foton_core::entity::LivingEntity;
use foton_core::entity::NeutralMob;
use foton_core::entity::PatrollingMonster;
use foton_core::entity::SharedEntity;
use foton_core::entity::TamableAnimal;
use foton_core::entity::conversion::{
    ConversionParams, ConversionReason, convert_to, replace_entity,
};
use foton_core::entity::entities::TropicalFishPattern;
use foton_core::entity::entities::decoration::ArmorStandEntity;
use foton_core::entity::entities::mobs::hostile::PhantomEntity;
use foton_core::entity::entities::mobs::hostile::ZombieEntity;
use foton_core::entity::entities::mobs::hostile::{
    CreeperEntity, EndermanEntity, EvokerEntity, SlimeEntity,
};
use foton_core::entity::entities::mobs::neutral::{IronGolemEntity, WolfEntity};
use foton_core::entity::entities::mobs::npc::{VillagerEntity, ZombieVillagerEntity};
use foton_core::entity::entities::mobs::passive::AxolotlEntity;
use foton_core::entity::entities::mobs::passive::BeeEntity;
use foton_core::entity::entities::mobs::passive::CatEntity;
use foton_core::entity::entities::mobs::passive::ChickenEntity;
use foton_core::entity::entities::mobs::passive::CowEntity;
use foton_core::entity::entities::mobs::passive::NautilusEntity;
use foton_core::entity::entities::mobs::passive::PigEntity;
use foton_core::entity::entities::mobs::passive::SheepEntity;
use foton_core::entity::entities::mobs::passive::ZombieNautilusEntity;
use foton_core::entity::entities::mobs::passive::{
    FoxEntity, FoxVariant, FrogEntity, GoatEntity, HorseEntity, HorseMarkings, HorseVariant,
    MushroomCowEntity, MushroomCowVariant, ParrotEntity, ParrotVariant,
};
use foton_core::entity::entities::mobs::passive::{PandaEntity, PandaGene};
use foton_core::entity::entities::mobs::water::TropicalFishEntity;
use foton_core::entity::entities::objects::AreaEffectCloudEntity;
use foton_core::entity::entities::objects::display_ui::PaintingEntity;
use foton_core::entity::entities::objects::display_ui::{BlockDisplayEntity, ItemFrameEntity};
use foton_core::entity::entities::objects::explosives::EndCrystalEntity;
use foton_core::entity::entities::objects::explosives::PrimedTntEntity;
use foton_core::entity::entities::objects::items::ExperienceOrbEntity;
use foton_core::entity::entities::objects::items::FallingBlockEntity;
use foton_core::entity::entities::objects::items::ItemEntity;
use foton_core::entity::entities::objects::projectiles::FireworkRocketEntity;
use foton_core::entity::entities::objects::projectiles::{ArrowEntity, ArrowPickup};
use foton_core::entity::entities::objects::projectiles::{
    LargeFireballEntity, SmallFireballEntity,
};
use foton_core::entity::entities::objects::vehicles::{BoatEntity, RaftEntity};
use foton_core::entity::spawn_util::{
    SpawnEntityInitialization, create_entity_at, create_entity_at_initialized, prepare_then_publish,
};
use foton_core::entity::spellcaster_illager::{IllagerSpell, SpellcasterIllager};
use foton_core::entity::{
    Animal, Entity, LlamaVariant, MobEffectInstance, PluginSpawnReason, RemovalReason, is_tamed,
    owner_uuid, set_owner_uuid, set_tamed, start_riding_entities,
};
use foton_core::event::TeleportPoint;
use foton_core::inventory::container::Container;
use foton_core::inventory::equipment::EquipmentSlot;
use foton_core::inventory::lock::{ContainerLockGuard, ContainerRef};
use foton_core::inventory::menu::kinds::anvil;
use foton_core::inventory::menu::kinds::cartography;
use foton_core::inventory::menu::kinds::crafting;
use foton_core::inventory::menu::kinds::grindstone;
use foton_core::inventory::menu::kinds::loom;
use foton_core::inventory::menu::kinds::smithing;
use foton_core::inventory::menu::kinds::stonecutter;
use foton_core::permission::{PermissionExpr, PermissionKey, PermissionState};
use foton_core::player::Player;
use foton_core::player::connection::NetworkConnection;
use foton_core::scoreboard::ScoreHolder;
use foton_core::server::{Server, WorldCreationRequest, WorldCreationState};
use foton_core::trading::Merchant;
use foton_core::world::LevelReader as _;
use foton_core::world::SignalGetter as _;
use foton_core::world::World;
use foton_core::world::base_spawner::Spawner as _;
use foton_core::world::explosion::{ExplosionBlockInteraction, ExplosionSpec};
use foton_core::worldgen::ChunkGenerator;
use foton_protocol::packets::common::CCustomPayload;
use foton_protocol::packets::game::{
    BossBarColor, BossBarOverlay, CBlockEntityData, CBlockUpdate, CClearTitles, COpenBook,
    CSetDefaultSpawnPosition, CSetSubtitleText, CSetTitleText, CSetTitlesAnimation, CStopSound,
    CSystemChat, CTabList, SoundSource,
};
use foton_registry::MobEffectInstance as RegistryMobEffectInstance;
use foton_registry::attribute::AttributeRef;
use foton_registry::blocks::behavior::PushReaction;
use foton_registry::blocks::block_state_ext::BlockStateExt;
use foton_registry::blocks::properties::BlockStateProperties;
use foton_registry::data_components::DataComponentPatch;
use foton_registry::data_components::components::{
    CustomModelData, ItemEnchantments, ItemLore, SuspiciousStewEffect, SuspiciousStewEffects,
    TooltipDisplay,
};
use foton_registry::data_components::vanilla_components::{
    CUSTOM_MODEL_DATA, DAMAGE, ENCHANTMENTS, FIREWORKS, FireworkExplosion, FireworkExplosionShape,
    Fireworks, ITEM_MODEL, ITEM_NAME, LORE, POTION_CONTENTS, STORED_ENCHANTMENTS, TOOLTIP_DISPLAY,
    TOOLTIP_STYLE, UNBREAKABLE, WRITABLE_BOOK_CONTENT, WRITTEN_BOOK_CONTENT,
};
use foton_registry::enchantment::{Enchantment, EnchantmentRef};
use foton_registry::entity_type::EntityTypeRef;
use foton_registry::entity_type::MobCategory;
use foton_registry::entity_variant::AxolotlVariant;
use foton_registry::fuel;
use foton_registry::game_rules::GameRuleType;
use foton_registry::game_rules::GameRuleValue;
use foton_registry::item_predicate::LockCode;
use foton_registry::item_stack::ItemStack;
use foton_registry::mob_effect::MobEffect;
use foton_registry::recipe::{
    CraftingCategory, CraftingInput, Ingredient, RecipeResult, ShapedRecipe, ShapelessRecipe,
};
use foton_registry::stat::{CustomStatRef, Stat};
use foton_registry::trading::ItemCost;
use foton_registry::trading::MerchantOffer;
use foton_registry::vanilla_block_entity_types::BREWING_STAND as BREWING_STAND_BLOCK_ENTITY;
use foton_registry::vanilla_block_entity_types::SIGN;
use foton_registry::vanilla_game_rules::TNT_EXPLOSION_DROP_DECAY;
use foton_registry::{
    REGISTRY, RegistryEntry as _, RegistryExt as _, RegistryReference, TaggedRegistryExt as _,
    vanilla_blocks, vanilla_entities, vanilla_items,
};
use foton_utils::codec::VarInt;
use foton_utils::entity_events::EntityStatus;
use foton_utils::locks::{SyncMutex, SyncRwLock};
use foton_utils::nbt::{merge_nbt_compounds, parse_snbt_compound, to_canonical_snbt};
use foton_utils::serial::{OptionalNbt, ReadFrom as _, WriteTo as _};
use foton_utils::serial::{budget::DecodeBudget, text_stream::write_bounded};
use foton_utils::text::DisplayResolutor;
use foton_utils::translations::CONTAINER_CARTOGRAPHY_TABLE;
use foton_utils::translations::CONTAINER_CRAFTING;
use foton_utils::translations::CONTAINER_GRINDSTONE_TITLE;
use foton_utils::translations::CONTAINER_LOOM;
use foton_utils::translations::CONTAINER_REPAIR;
use foton_utils::translations::CONTAINER_STONECUTTER;
use foton_utils::translations::CONTAINER_UPGRADE;
use foton_utils::types::Difficulty;
use foton_utils::types::UpdateFlags;
use foton_utils::types::{GameType, InteractionHand};
use foton_utils::{BlockPos, BlockStateId, WorldAabb};
use foton_utils::{Downcast as _, Identifier};
use glam::DVec3;
use jni::JNIEnv;
use jni::objects::{
    GlobalRef, JByteArray, JClass, JDoubleArray, JIntArray, JObject, JObjectArray, JString,
};
use jni::sys::{
    jboolean, jbyte, jbyteArray, jdouble, jdoubleArray, jfloat, jint, jintArray, jlong, jobject,
    jobjectArray, jstring,
};
use rustc_hash::{FxHashMap, FxHashSet};
use simdnbt::owned::NbtCompound;
use simdnbt::owned::NbtTag;
use text_components::{TextComponent, content::Content as TextContent};
use uuid::Uuid;

mod attributes;
mod blocks;
mod displays;
mod entities;
mod lifecycle;
mod merchants;
mod particles;
mod players;
mod support;
use crate::item_components;
use crate::scoreboard_natives;

/// The server the natives answer about.
///
/// A `static` because a JNI native is a bare function pointer with nowhere to
/// put context. `Weak` because the plugin host must never be the reason a
/// server cannot shut down.
static SERVER: OnceLock<SyncRwLock<Weak<Server>>> = OnceLock::new();

/// Outstanding asynchronous Bukkit chunk requests, retained until Full status.
///
/// The `Instant` is when the request was made. A handle holds a chunk ticket
/// and only releases it when it is dropped, and the only thing that drops one
/// is a plugin polling `chunkRequestReady` until it answers -- so a plugin that
/// asks and never polls, or that is unloaded mid-poll, used to pin a chunk for
/// the life of the server. Requests older than the timeout are swept whenever
/// either native runs.
static CHUNK_REQUESTS: OnceLock<SyncMutex<FxHashMap<Uuid, (Instant, ChunkRequestHandle)>>> =
    OnceLock::new();

/// How long an unpolled chunk request keeps its ticket.
///
/// There is no vanilla counterpart to weigh this against -- the polling shape is
/// Foton's own, where Paper hands back a future that releases on completion or
/// cancellation. A request that has been ready for a minute without anyone
/// asking is one nobody is waiting for, and the cost of being wrong is that a
/// plugin has to ask again, against the cost of pinning a chunk forever.
const ABANDONED_CHUNK_REQUEST_TIMEOUT: Duration = Duration::from_secs(60);

/// Non-persistent bars created through Bukkit, keyed by the opaque handle Java owns.
static BOSS_BARS: OnceLock<SyncRwLock<FxHashMap<Uuid, Arc<ServerBossEvent>>>> = OnceLock::new();

/// Bukkit updates header and footer independently, while the protocol packet carries both.
static PLAYER_TAB_LISTS: OnceLock<SyncRwLock<FxHashMap<Uuid, (TextComponent, TextComponent)>>> =
    OnceLock::new();

/// Non-entity Bukkit projectile sources retained for the lifetime of their projectile.
///
/// Entity sources remain authoritative Rust UUID state. Paper also permits block and
/// plugin-defined sources, whose JVM object identity has no Rust gameplay counterpart;
/// those references live here rather than in Java-only shadow fields.
static PROJECTILE_SOURCES: OnceLock<SyncRwLock<FxHashMap<Uuid, GlobalRef>>> = OnceLock::new();

/// Plugin world-creation requests. Requests are polled from JVM threads;
/// actual construction and attachment remain owned by the server safe-point.
static WORLD_CREATION_REQUESTS: OnceLock<SyncMutex<FxHashMap<u64, WorldCreationRequest>>> =
    OnceLock::new();

/// Points the natives at a server.
///
/// The host can be torn down and started again in the same process, so this
/// must replace the previous weak reference instead of permanently retaining
/// the first binding.
pub(crate) fn bind(server: Weak<Server>) {
    if let Some(sources) = PROJECTILE_SOURCES.get() {
        let stale = {
            let mut sources = sources.write();
            mem::take(&mut *sources)
        };
        drop(stale);
    }
    let slot = SERVER.get_or_init(|| SyncRwLock::new(Weak::new()));
    *slot.write() = server;
}

extern "system" fn set_compass_target(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    world_name: JString<'_>,
    x: jint,
    y: jint,
    z: jint,
) {
    let Some(player) = player(&mut env, &uuid) else {
        return;
    };
    let Ok(name) = env.get_string(&world_name) else {
        return;
    };
    let Ok(key) = name
        .to_str()
        .ok()
        .and_then(|value| value.parse::<Identifier>().ok())
        .ok_or(())
    else {
        return;
    };
    let Some(world) = server().and_then(|server| server.worlds.get_owned(&key)) else {
        return;
    };
    player.send_packet(CSetDefaultSpawnPosition {
        global_pos: foton_utils::GlobalPos::new(
            world.dimension_type.key.clone(),
            BlockPos::new(x, y, z),
        ),
        yaw: 0.0,
        pitch: 0.0,
    });
}

/// The server, if there still is one.
pub(crate) fn server() -> Option<Arc<Server>> {
    SERVER.get().and_then(|slot| slot.read().upgrade())
}

fn chunk_requests() -> &'static SyncMutex<FxHashMap<Uuid, (Instant, ChunkRequestHandle)>> {
    CHUNK_REQUESTS.get_or_init(|| SyncMutex::new(FxHashMap::default()))
}

/// Drops chunk requests nobody has polled inside the timeout.
///
/// Dropping the handle is what releases its tickets, so removing the entry is
/// the whole release. Both natives call this before doing their own work, which
/// bounds the damage a plugin that stops polling can do to the requests it made
/// since it last called in.
fn sweep_abandoned_chunk_requests(requests: &mut FxHashMap<Uuid, (Instant, ChunkRequestHandle)>) {
    let now = Instant::now();
    requests.retain(|id, (requested_at, _)| {
        let alive = now.duration_since(*requested_at) < ABANDONED_CHUNK_REQUEST_TIMEOUT;
        if !alive {
            log::warn!("Releasing chunk request {id}: no plugin polled it within the timeout");
        }
        alive
    });
}

fn boss_bars() -> &'static SyncRwLock<FxHashMap<Uuid, Arc<ServerBossEvent>>> {
    BOSS_BARS.get_or_init(|| SyncRwLock::new(FxHashMap::default()))
}

fn player_tab_lists() -> &'static SyncRwLock<FxHashMap<Uuid, (TextComponent, TextComponent)>> {
    PLAYER_TAB_LISTS.get_or_init(|| SyncRwLock::new(FxHashMap::default()))
}

fn projectile_sources() -> &'static SyncRwLock<FxHashMap<Uuid, GlobalRef>> {
    PROJECTILE_SOURCES.get_or_init(|| SyncRwLock::new(FxHashMap::default()))
}

#[cfg(test)]
pub(crate) fn projectile_source_count() -> usize {
    projectile_sources().read().len()
}

pub(crate) fn remove_projectile_source(id: &Uuid) {
    let stale = { projectile_sources().write().remove(id) };
    drop(stale);
}

fn discard_unpublished_entity(entity: &SharedEntity) {
    entity.set_removed(RemovalReason::Discarded);
    remove_projectile_source(&entity.uuid());
}

fn world_creation_requests() -> &'static SyncMutex<FxHashMap<u64, WorldCreationRequest>> {
    WORLD_CREATION_REQUESTS.get_or_init(|| SyncMutex::new(FxHashMap::default()))
}

fn boss_bar(env: &mut JNIEnv<'_>, id: &JString<'_>) -> Option<Arc<ServerBossEvent>> {
    let text: String = env.get_string(id).ok()?.into();
    let id = Uuid::parse_str(&text).ok()?;
    boss_bars().read().get(&id).map(Arc::clone)
}

/// Resolves a Java-side handle back to a player who is still online.
pub(crate) fn player(env: &mut JNIEnv<'_>, uuid: &JString<'_>) -> Option<Arc<Player>> {
    let text: String = env.get_string(uuid).ok()?.into();
    let uuid = Uuid::parse_str(&text).ok()?;
    server()?.online_players().get_by_uuid(&uuid)
}

/// Returns a Java string, or Java's null when there is nothing to say.
pub(crate) fn to_java(env: &mut JNIEnv<'_>, value: Option<String>) -> jstring {
    value
        .and_then(|text| env.new_string(text).ok())
        .map_or_else(null_mut, JString::into_raw)
}

/// Returns a Java `String[]`, or null if the array could not be built.
pub(crate) fn string_array(env: &mut JNIEnv<'_>, values: &[String]) -> jobjectArray {
    let Ok(empty) = env.new_string("") else {
        return null_mut();
    };
    let Ok(array) = env.new_object_array(
        i32::try_from(values.len()).unwrap_or(0),
        "java/lang/String",
        &empty,
    ) else {
        return null_mut();
    };
    for (index, value) in values.iter().enumerate() {
        let Ok(text) = env.new_string(value) else {
            continue;
        };
        let _ = env.set_object_array_element(&array, i32::try_from(index).unwrap_or(0), text);
    }
    let array: JObjectArray<'_> = array;
    array.into_raw()
}

pub(crate) fn read_string_array(
    env: &mut JNIEnv<'_>,
    array: &JObjectArray<'_>,
) -> Option<Vec<String>> {
    let length = env.get_array_length(array).ok()?;
    let mut values = Vec::with_capacity(length as usize);
    for index in 0..length {
        let object = env.get_object_array_element(array, index).ok()?;
        let value = JString::from(object);
        values.push(env.get_string(&value).ok()?.into());
    }
    Some(values)
}

/// Returns a position as `{x, y, z, yaw, pitch}`, or Java's null.
///
/// One array rather than five calls. Five calls could each land on a different
/// tick, and a plugin that read x from one and z from the next would get a
/// point nothing was ever at.
fn to_position(env: &mut JNIEnv<'_>, at: Option<[f64; 5]>) -> jdoubleArray {
    let Some(at) = at else {
        return null_mut();
    };
    let Ok(array) = env.new_double_array(5) else {
        return null_mut();
    };
    if env.set_double_array_region(&array, 0, &at).is_err() {
        return null_mut();
    }
    let array: JDoubleArray<'_> = array;
    array.into_raw()
}

/// Resolves a world by the key a plugin holds it under.
fn world(env: &mut JNIEnv<'_>, name: &JString<'_>) -> Option<Arc<World>> {
    let text: String = env.get_string(name).ok()?.into();
    let key: Identifier = text.parse().ok()?;
    server()?.worlds.get_owned(&key)
}

/// `foton.Native.serverName`
extern "system" fn enchantment_can_enchant(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    enchantment: JString<'_>,
    item: JString<'_>,
) -> jboolean {
    let Ok(enchantment) = env.get_string(&enchantment) else {
        return 0;
    };
    let Ok(item) = env.get_string(&item) else {
        return 0;
    };
    let Some(registry) = REGISTRY.get() else {
        return 0;
    };
    let Some(enchantment) = registry.enchantments.by_key(&Identifier::vanilla(
        enchantment.to_str().unwrap_or_default().to_owned(),
    )) else {
        return 0;
    };
    let Some(item) = registry.items.by_key(&Identifier::vanilla(
        item.to_str().unwrap_or_default().to_owned(),
    )) else {
        return 0;
    };
    jboolean::from(enchantment.can_enchant(item))
}

fn enchantment_named(env: &mut JNIEnv<'_>, key: &JString<'_>) -> Option<EnchantmentRef> {
    let key: Identifier = env.get_string(key).ok()?.to_str().ok()?.parse().ok()?;
    REGISTRY.enchantments.by_key(&key)
}

/// `foton.Native.blockPropertyValues`
extern "system" fn block_property_values(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    block: JString<'_>,
    property: JString<'_>,
) -> jobjectArray {
    let Some(block) = env
        .get_string(&block)
        .ok()
        .and_then(|text| text.to_str().ok()?.parse::<Identifier>().ok())
        .and_then(|key| REGISTRY.blocks.by_key(&key))
    else {
        return null_mut();
    };
    let Ok(property) = env.get_string(&property) else {
        return null_mut();
    };
    let property = String::from(property);
    let Some(found) = block
        .properties
        .iter()
        .find(|candidate| candidate.get_name() == property)
    else {
        return null_mut();
    };
    let values: Vec<String> = found
        .get_possible_value_names()
        .iter()
        .map(|value| (*value).to_owned())
        .collect();
    string_array(&mut env, &values)
}

/// `foton.Native.enchantmentItems`: the item keys an enchantment's primary or
/// supported holder set names, or null when it names none.
extern "system" fn enchantment_items(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    enchantment: JString<'_>,
    primary: jboolean,
) -> jobjectArray {
    let Some(enchantment) = enchantment_named(&mut env, &enchantment) else {
        return null_mut();
    };
    let holders = if primary == 0 {
        Some(enchantment.supported_items)
    } else {
        enchantment.primary_items
    };
    let Some(holders) = holders else {
        return null_mut();
    };
    // The data pack writes these as a tag (`#minecraft:enchantable/sword`) or
    // a single item, never inline lists, which is also all Foton parses.
    let items: Vec<String> = match holders.strip_prefix('#') {
        Some(tag) => {
            let Ok(tag) = tag.parse::<Identifier>() else {
                return null_mut();
            };
            REGISTRY
                .items
                .iter_tag(&tag)
                .map(|item| item.key.to_string())
                .collect()
        }
        None => holders
            .parse::<Identifier>()
            .ok()
            .and_then(|key| REGISTRY.items.by_key(&key))
            .map(|item| item.key.to_string())
            .into_iter()
            .collect(),
    };
    string_array(&mut env, &items)
}

fn enchantments_conflict_state(first: &str, second: &str) -> bool {
    let Ok(first) = first.parse::<Identifier>() else {
        return false;
    };
    let Ok(second) = second.parse::<Identifier>() else {
        return false;
    };
    let Some(registry) = REGISTRY.get() else {
        return false;
    };
    let Some(first) = registry.enchantments.by_key(&first) else {
        return false;
    };
    let Some(second) = registry.enchantments.by_key(&second) else {
        return false;
    };
    !Enchantment::are_compatible(first, second)
}

extern "system" fn enchantments_conflict(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    first: JString<'_>,
    second: JString<'_>,
) -> jboolean {
    let Ok(first) = env.get_string(&first) else {
        return 0;
    };
    let Ok(second) = env.get_string(&second) else {
        return 0;
    };
    jboolean::from(enchantments_conflict_state(
        first.to_str().unwrap_or_default(),
        second.to_str().unwrap_or_default(),
    ))
}

fn parse_item_snbt_patch(input: &str) -> Option<NbtCompound> {
    let text = input.trim();
    let compound = if text.starts_with('{') {
        text
    } else {
        let open = text.find('{')?;
        if text[..open].trim().parse::<Identifier>().is_err() {
            return None;
        }
        &text[open..]
    };
    parse_snbt_compound(compound).ok()
}

extern "system" fn merge_item_snbt(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    existing: JString<'_>,
    patch: JString<'_>,
) -> jstring {
    let Ok(existing) = env.get_string(&existing) else {
        return null_mut();
    };
    let Ok(patch) = env.get_string(&patch) else {
        return null_mut();
    };
    let Ok(mut target) = parse_snbt_compound(existing.to_str().unwrap_or_default()) else {
        return null_mut();
    };
    // Vanilla accepts a bare compound or an item id followed by legacy SNBT,
    // which is what `parse_item_snbt_patch` knows and what its tests cover.
    let Some(source) = parse_item_snbt_patch(patch.to_str().unwrap_or_default()) else {
        return null_mut();
    };
    merge_nbt_compounds(&mut target, &source);
    let value = to_canonical_snbt(&NbtTag::Compound(target));
    to_java(&mut env, value)
}

extern "system" fn dye_firework_color(_env: JNIEnv<'_>, _class: JClass<'_>, ordinal: jint) -> jint {
    foton_registry::DyeColor::VALUES
        .get(ordinal.max(0) as usize)
        .map_or(0, |color| color.firework_color())
}

extern "system" fn server_name(mut env: JNIEnv<'_>, _class: JClass<'_>) -> jstring {
    to_java(&mut env, Some("Foton".to_owned()))
}

extern "system" fn server_motd(mut env: JNIEnv<'_>, _class: JClass<'_>) -> jstring {
    to_java(&mut env, server().map(|value| value.config.motd.clone()))
}

/// The members of `tag` in the registry a plugin names, or `None` when that
/// registry has no such tag.
///
/// Registries are named as Paper's `RegistryKey`s are (`item`,
/// `worldgen/biome`), and also as Bukkit's `Tag.REGISTRY_*` strings
/// (`items`, `blocks`, `fluids`, `entity_types`, `game_events`).
fn tag_members(registry: &str, tag: &Identifier) -> Option<Vec<String>> {
    fn keys<T: foton_registry::RegistryEntry + 'static>(
        entries: Option<Vec<&'static T>>,
    ) -> Option<Vec<String>> {
        entries.map(|entries| {
            entries
                .iter()
                .map(|entry| entry.key().to_string())
                .collect()
        })
    }
    match registry.strip_prefix("minecraft:").unwrap_or(registry) {
        "item" | "items" => keys(REGISTRY.items.get_tag(tag)),
        "block" | "blocks" => keys(REGISTRY.blocks.get_tag(tag)),
        "fluid" | "fluids" => keys(REGISTRY.fluids.get_tag(tag)),
        "entity_type" | "entity_types" => keys(REGISTRY.entity_types.get_tag(tag)),
        "game_event" | "game_events" => keys(REGISTRY.game_events.get_tag(tag)),
        "enchantment" => keys(REGISTRY.enchantments.get_tag(tag)),
        "banner_pattern" => keys(REGISTRY.banner_patterns.get_tag(tag)),
        "instrument" => keys(REGISTRY.instruments.get_tag(tag)),
        "worldgen/biome" => keys(REGISTRY.biomes.get_tag(tag)),
        "potion" => keys(REGISTRY.potions.get_tag(tag)),
        _ => None,
    }
}

extern "system" fn is_tagged(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    registry: JString<'_>,
    tag: JString<'_>,
    value: JString<'_>,
) -> jboolean {
    let Ok(registry) = env.get_string(&registry) else {
        return 0;
    };
    let Ok(tag) = env.get_string(&tag) else {
        return 0;
    };
    let Ok(value) = env.get_string(&value) else {
        return 0;
    };
    jboolean::from(is_tagged_state(
        &String::from(registry),
        &String::from(tag),
        &String::from(value),
    ))
}

fn is_tagged_state(registry_name: &str, tag: &str, value: &str) -> bool {
    let Ok(tag) = tag.parse::<Identifier>() else {
        return false;
    };
    let Ok(value) = value.parse::<Identifier>() else {
        return false;
    };
    let Some(registry) = REGISTRY.get() else {
        return false;
    };
    match registry_name {
        "minecraft:items" | "items" | "item" => registry
            .items
            .by_key(&value)
            .is_some_and(|item| registry.items.is_in_tag(item, &tag)),
        "minecraft:blocks" | "blocks" | "block" => registry
            .blocks
            .by_key(&value)
            .is_some_and(|block| registry.blocks.is_in_tag(block, &tag)),
        other => tag_members(other, &tag)
            .is_some_and(|members| members.iter().any(|member| *member == value.to_string())),
    }
}

/// `foton.Native.tagValues`: the tag's members, or null when the registry has
/// no such tag -- which is how `Registry#hasTag` tells absent from empty.
extern "system" fn tag_values(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    registry: JString<'_>,
    tag: JString<'_>,
) -> jobjectArray {
    let Ok(registry) = env.get_string(&registry) else {
        return null_mut();
    };
    let Ok(tag) = env.get_string(&tag) else {
        return null_mut();
    };
    let registry_name = String::from(registry);
    let Some(_registry) = REGISTRY.get() else {
        return string_array(&mut env, &[]);
    };
    let Ok(tag) = String::from(tag).parse::<Identifier>() else {
        return null_mut();
    };
    let Some(values) = tag_members(&registry_name, &tag) else {
        return null_mut();
    };
    string_array(&mut env, &values)
}

/// `foton.Native.serverVersion`
extern "system" fn server_version(mut env: JNIEnv<'_>, _class: JClass<'_>) -> jstring {
    to_java(&mut env, Some(env!("CARGO_PKG_VERSION").to_owned()))
}

/// `foton.Native.minecraftVersion`
extern "system" fn minecraft_version(mut env: JNIEnv<'_>, _class: JClass<'_>) -> jstring {
    to_java(&mut env, Some(foton_utils::MC_VERSION.to_owned()))
}

/// `foton.Native.onlinePlayerIds`
extern "system" fn online_player_ids(mut env: JNIEnv<'_>, _class: JClass<'_>) -> jobjectArray {
    let mut ids = Vec::new();
    if let Some(server) = server() {
        server.online_players().iter_players(|_uuid, player| {
            ids.push(player.gameprofile.id.to_string());
            true
        });
    }

    string_array(&mut env, &ids)
}

extern "system" fn known_player_ids(mut env: JNIEnv<'_>, _class: JClass<'_>) -> jobjectArray {
    let Some(server) = server() else {
        return null_mut();
    };
    let ids: Vec<String> = server
        .known_players()
        .entries()
        .iter()
        .map(|entry| entry.uuid().to_string())
        .collect();
    string_array(&mut env, &ids)
}

extern "system" fn known_player_id_by_name(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
) -> jstring {
    let Ok(name) = env.get_string(&name) else {
        return null_mut();
    };
    let Ok(needle) = name.to_str() else {
        return null_mut();
    };
    let Some(server) = server() else {
        return null_mut();
    };
    let id = server
        .known_players()
        .entries()
        .iter()
        .find(|entry| entry.last_known_name().eq_ignore_ascii_case(needle))
        .map(|entry| entry.uuid().to_string());
    to_java(&mut env, id)
}

/// `foton.Native.playerLocale`
extern "system" fn player_locale(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jstring {
    let locale = player(&mut env, &uuid).map(|player| player.client_information().language);
    to_java(&mut env, locale)
}

/// `foton.Native.playerName`
extern "system" fn player_name(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jstring {
    let name = player(&mut env, &uuid).map(|player| player.gameprofile.name.clone());
    to_java(&mut env, name)
}

extern "system" fn spellcaster_spell(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jstring {
    let Ok(text): Result<String, _> = env.get_string(&uuid).map(Into::into) else {
        return null_mut();
    };
    let Ok(id) = text.parse() else {
        return null_mut();
    };
    let Some((_, entity)) = entity_by_uuid(&id) else {
        return null_mut();
    };
    let Some(evoker) = entity.as_ref().downcast_ref::<EvokerEntity>() else {
        return null_mut();
    };
    let value = match evoker.current_spell() {
        IllagerSpell::None => "NONE",
        IllagerSpell::SummonVex => "SUMMON_VEX",
        IllagerSpell::Fangs => "FANGS",
        IllagerSpell::Wololo => "WOLOLO",
        IllagerSpell::Disappear => "DISAPPEAR",
        IllagerSpell::Blindness => "BLINDNESS",
    };
    to_java(&mut env, Some(value.to_owned()))
}

extern "system" fn set_spellcaster_spell(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    spell: JString<'_>,
) {
    let Ok(text): Result<String, _> = env.get_string(&uuid).map(Into::into) else {
        return;
    };
    let Ok(id) = text.parse() else {
        return;
    };
    let Ok(value): Result<String, _> = env.get_string(&spell).map(Into::into) else {
        return;
    };
    let spell = match value.as_str() {
        "SUMMON_VEX" => IllagerSpell::SummonVex,
        "FANGS" => IllagerSpell::Fangs,
        "WOLOLO" => IllagerSpell::Wololo,
        "DISAPPEAR" => IllagerSpell::Disappear,
        "BLINDNESS" => IllagerSpell::Blindness,
        _ => IllagerSpell::None,
    };
    let Some((_, entity)) = entity_by_uuid(&id) else {
        return;
    };
    if let Some(evoker) = entity.as_ref().downcast_ref::<EvokerEntity>() {
        evoker.set_is_casting_spell(spell);
    }
}

extern "system" fn set_hanging_facing(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    face: JString<'_>,
    force: jboolean,
) -> jboolean {
    let Ok(text): Result<String, _> = env.get_string(&uuid).map(Into::into) else {
        return 0;
    };
    let Ok(id) = text.parse() else {
        return 0;
    };
    let Ok(face): Result<String, _> = env.get_string(&face).map(Into::into) else {
        return 0;
    };
    let direction = match face.to_ascii_uppercase().as_str() {
        "DOWN" => foton_utils::Direction::Down,
        "UP" => foton_utils::Direction::Up,
        "NORTH" => foton_utils::Direction::North,
        "SOUTH" => foton_utils::Direction::South,
        "WEST" => foton_utils::Direction::West,
        "EAST" => foton_utils::Direction::East,
        _ => return 0,
    };
    let Some((_world, entity)) = entity_by_uuid(&id) else {
        return 0;
    };
    if let Some(frame) = entity.as_ref().downcast_ref::<ItemFrameEntity>() {
        let old = frame.direction();
        frame.set_direction(direction);
        if force != 0 || frame.survives() {
            return 1;
        }
        frame.set_direction(old);
        return 0;
    }
    if let Some(painting) = entity.as_ref().downcast_ref::<PaintingEntity>() {
        let old = painting.direction();
        painting.set_direction(direction);
        if force != 0 || painting.survives() {
            return 1;
        }
        painting.set_direction(old);
        return 0;
    }
    0
}

extern "system" fn painting_art(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jstring {
    let Ok(text) = env.get_string(&uuid) else {
        return null_mut();
    };
    let Ok(id) = text
        .to_str()
        .ok()
        .and_then(|value| value.parse().ok())
        .ok_or(())
    else {
        return null_mut();
    };
    let Some((_, entity)) = entity_by_uuid(&id) else {
        return null_mut();
    };
    let Some(painting) = entity.as_ref().downcast_ref::<PaintingEntity>() else {
        return null_mut();
    };
    to_java(&mut env, Some(painting.variant().key.path.to_string()))
}

extern "system" fn set_painting_art(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    art: JString<'_>,
    force: jboolean,
) -> jboolean {
    let Ok(text) = env.get_string(&uuid) else {
        return 0;
    };
    let Ok(id) = text
        .to_str()
        .ok()
        .and_then(|value| value.parse().ok())
        .ok_or(())
    else {
        return 0;
    };
    let Ok(art) = env.get_string(&art) else {
        return 0;
    };
    let Ok(key) = Identifier::from_str(&format!(
        "minecraft:{}",
        art.to_str().unwrap_or_default().to_ascii_lowercase()
    )) else {
        return 0;
    };
    let Some(variant) = REGISTRY.painting_variants.by_key(&key) else {
        return 0;
    };
    let Some((_, entity)) = entity_by_uuid(&id) else {
        return 0;
    };
    let Some(painting) = entity.as_ref().downcast_ref::<PaintingEntity>() else {
        return 0;
    };
    let old = painting.variant();
    painting.set_variant(variant);
    if force != 0 || painting.survives() {
        return 1;
    }
    painting.set_variant(old);
    0
}

extern "system" fn hanging_facing(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jstring {
    let Ok(text) = env.get_string(&uuid) else {
        return null_mut();
    };
    let Ok(id) = text
        .to_str()
        .ok()
        .and_then(|value| value.parse().ok())
        .ok_or(())
    else {
        return null_mut();
    };
    let Some((_, entity)) = entity_by_uuid(&id) else {
        return null_mut();
    };
    let direction = entity
        .as_ref()
        .downcast_ref::<PaintingEntity>()
        .map(PaintingEntity::direction)
        .or_else(|| {
            entity
                .as_ref()
                .downcast_ref::<ItemFrameEntity>()
                .map(ItemFrame::direction)
        });
    let Some(direction) = direction else {
        return null_mut();
    };
    let value = match direction {
        foton_utils::Direction::Down => "down",
        foton_utils::Direction::Up => "up",
        foton_utils::Direction::North => "north",
        foton_utils::Direction::South => "south",
        foton_utils::Direction::West => "west",
        foton_utils::Direction::East => "east",
    };
    to_java(&mut env, Some(value.to_owned()))
}

extern "system" fn enderman_carried_block(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jstring {
    let Ok(text) = env.get_string(&uuid) else {
        return null_mut();
    };
    let Ok(id) = text
        .to_str()
        .ok()
        .and_then(|value| value.parse().ok())
        .ok_or(())
    else {
        return null_mut();
    };
    let Some((_, entity)) = entity_by_uuid(&id) else {
        return null_mut();
    };
    let Some(enderman) = entity.as_ref().downcast_ref::<EndermanEntity>() else {
        return null_mut();
    };
    to_java(&mut env, enderman.carried_block().and_then(describe_state))
}

extern "system" fn set_enderman_carried_block(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    block: JString<'_>,
) {
    let Ok(text) = env.get_string(&uuid) else {
        return;
    };
    let Ok(id) = text
        .to_str()
        .ok()
        .and_then(|value| value.parse().ok())
        .ok_or(())
    else {
        return;
    };
    let Ok(block) = env.get_string(&block) else {
        return;
    };
    let Some((_, entity)) = entity_by_uuid(&id) else {
        return;
    };
    let Some(enderman) = entity.as_ref().downcast_ref::<EndermanEntity>() else {
        return;
    };
    enderman.set_carried_block(parse_state(block.to_str().unwrap_or_default()));
}

extern "system" fn send_sign_change(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    world_name: JString<'_>,
    x: jint,
    y: jint,
    z: jint,
    lines: JObjectArray<'_>,
    color: jint,
) {
    let Some(player) = player(&mut env, &uuid) else {
        return;
    };
    let Some(lines) = read_string_array(&mut env, &lines) else {
        return;
    };
    let Some(world) = server().and_then(|server| {
        env.get_string(&world_name)
            .ok()
            .and_then(|name| {
                name.to_str()
                    .ok()
                    .and_then(|key| key.parse::<Identifier>().ok())
            })
            .and_then(|key| server.worlds.get_owned(&key))
    }) else {
        return;
    };
    let mut front = SignText::new();
    for (index, line) in lines.iter().take(4).enumerate() {
        front.set_message(index, TextComponent::from(line.clone()));
    }
    if let Some(value) = foton_registry::DyeColor::VALUES
        .get(color as usize)
        .copied()
    {
        front.set_color(value);
    }
    let mut nbt = NbtCompound::new();
    front.save(&mut nbt);
    let mut root = NbtCompound::new();
    root.insert("front_text", nbt);
    let mut back = NbtCompound::new();
    SignText::new().save(&mut back);
    root.insert("back_text", back);
    root.insert("is_waxed", 0i8);
    player.send_packet(CBlockEntityData {
        pos: BlockPos::new(x, y, z),
        block_entity_type: SIGN.id() as i32,
        nbt: OptionalNbt(Some(root)),
    });
    let _ = world;
}

extern "system" fn send_block_change(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    _world_name: JString<'_>,
    x: jint,
    y: jint,
    z: jint,
    block: JString<'_>,
) {
    let Some(player) = player(&mut env, &uuid) else {
        return;
    };
    let Ok(block) = env.get_string(&block) else {
        return;
    };
    let Some(block_state) = parse_state(block.to_str().unwrap_or_default()) else {
        return;
    };
    player.send_packet(CBlockUpdate {
        pos: BlockPos::new(x, y, z),
        block_state,
    });
}

fn entity_by_uuid(uuid: &Uuid) -> Option<(Arc<World>, SharedEntity)> {
    let server = server()?;
    for snapshot in server.worlds.snapshots() {
        let world = snapshot.world();
        if let Some(entity) = world.get_entity_by_uuid(uuid) {
            return Some((Arc::clone(world), entity));
        }
    }
    lifecycle::pending_entity(uuid)
}

#[cfg(test)]
static PREPUBLICATION_TEST_ENTITIES: LazyLock<SyncMutex<FxHashMap<Uuid, SharedEntity>>> =
    LazyLock::new(|| SyncMutex::new(FxHashMap::default()));

#[cfg(test)]
pub(crate) fn register_prepublication_test_entity(entity: SharedEntity) {
    PREPUBLICATION_TEST_ENTITIES
        .lock()
        .insert(entity.uuid(), entity);
}

fn animal_entity_by_uuid(uuid: &Uuid) -> Option<SharedEntity> {
    if let Some((_, entity)) = entity_by_uuid(uuid) {
        return Some(entity);
    }
    #[cfg(test)]
    {
        return PREPUBLICATION_TEST_ENTITIES.lock().get(uuid).cloned();
    }
    #[cfg(not(test))]
    None
}

fn entity_by_text(text: &str) -> Option<(Arc<World>, SharedEntity)> {
    entity_by_uuid(&Uuid::parse_str(text).ok()?)
}

fn entity_handle(env: &mut JNIEnv<'_>, uuid: &JString<'_>) -> Option<(Arc<World>, SharedEntity)> {
    let text: String = env.get_string(uuid).ok()?.into();
    entity_by_text(&text)
}

extern "system" fn set_entity_custom_name_visible(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    visible: jboolean,
) {
    let Ok(text) = env.get_string(&uuid) else {
        return;
    };
    let Ok(id) = text
        .to_str()
        .ok()
        .and_then(|value| value.parse().ok())
        .ok_or(())
    else {
        return;
    };
    let Some((_, entity)) = entity_by_uuid(&id) else {
        return;
    };
    entity.set_custom_name_visible(visible != 0);
}

extern "system" fn experience_orb_experience(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jint {
    let Ok(text) = env.get_string(&uuid) else {
        return 0;
    };
    let Ok(id) = text
        .to_str()
        .ok()
        .and_then(|value| value.parse().ok())
        .ok_or(())
    else {
        return 0;
    };
    let Some((_, entity)) = entity_by_uuid(&id) else {
        return 0;
    };
    entity
        .as_ref()
        .downcast_ref::<ExperienceOrbEntity>()
        .map_or(0, ExperienceOrbEntity::value)
}

extern "system" fn set_experience_orb_experience(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    experience: jint,
) {
    let Ok(text) = env.get_string(&uuid) else {
        return;
    };
    let Ok(id) = text
        .to_str()
        .ok()
        .and_then(|value| value.parse().ok())
        .ok_or(())
    else {
        return;
    };
    let Some((_, entity)) = entity_by_uuid(&id) else {
        return;
    };
    if let Some(orb) = entity.as_ref().downcast_ref::<ExperienceOrbEntity>() {
        orb.set_value(experience);
    }
}

extern "system" fn wolf_angry(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jboolean {
    let Ok(text) = env.get_string(&uuid) else {
        return 0;
    };
    let Ok(id) = text
        .to_str()
        .ok()
        .and_then(|value| value.parse().ok())
        .ok_or(())
    else {
        return 0;
    };
    let Some((_, entity)) = entity_by_uuid(&id) else {
        return 0;
    };
    jboolean::from(
        entity
            .as_ref()
            .downcast_ref::<WolfEntity>()
            .is_some_and(NeutralMob::is_angry),
    )
}
extern "system" fn set_wolf_angry(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    angry: jboolean,
) {
    let Ok(text) = env.get_string(&uuid) else {
        return;
    };
    let Ok(id) = text
        .to_str()
        .ok()
        .and_then(|value| value.parse().ok())
        .ok_or(())
    else {
        return;
    };
    let Some((_, entity)) = entity_by_uuid(&id) else {
        return;
    };
    if let Some(wolf) = entity.as_ref().downcast_ref::<WolfEntity>() {
        if angry != 0 {
            wolf.start_persistent_anger_timer();
        } else {
            wolf.set_persistent_anger_end_time(0);
        }
    }
}

extern "system" fn entity_tnt_source(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jstring {
    let Ok(text) = env.get_string(&uuid) else {
        return null_mut();
    };
    let Ok(id) = Uuid::parse_str(text.to_str().unwrap_or_default()) else {
        return null_mut();
    };
    let Some((world, entity)) = entity_by_uuid(&id) else {
        return null_mut();
    };
    let Some(tnt) = entity.as_ref().downcast_ref::<PrimedTntEntity>() else {
        return null_mut();
    };
    let Some(source) = tnt
        .source_entity_id()
        .and_then(|source| world.get_entity_by_id(source))
    else {
        return null_mut();
    };
    to_java(&mut env, Some(source.uuid().to_string()))
}

extern "system" fn entity_item_stack(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jstring {
    let Ok(text) = env.get_string(&uuid) else {
        return null_mut();
    };
    let Ok(id) = text
        .to_str()
        .ok()
        .and_then(|value| value.parse().ok())
        .ok_or(())
    else {
        return null_mut();
    };
    let Some((_, entity)) = entity_by_uuid(&id) else {
        return null_mut();
    };
    if let Some(item) = entity.as_ref().downcast_ref::<ItemEntity>() {
        return to_java(&mut env, Some(describe_slot(&item.get_item())));
    }
    if let Some(frame) = entity.as_ref().downcast_ref::<ItemFrameEntity>() {
        return to_java(&mut env, Some(describe_slot(&frame.framed_item())));
    }
    if let Some(fireball) = entity.as_ref().downcast_ref::<LargeFireballEntity>() {
        return to_java(&mut env, Some(describe_slot(&fireball.item())));
    }
    if let Some(fireball) = entity.as_ref().downcast_ref::<SmallFireballEntity>() {
        return to_java(&mut env, Some(describe_slot(&fireball.item())));
    }
    null_mut()
}

extern "system" fn set_entity_item_stack(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    item: JString<'_>,
) {
    let Ok(text) = env.get_string(&uuid) else {
        return;
    };
    let Ok(id) = text
        .to_str()
        .ok()
        .and_then(|value| value.parse().ok())
        .ok_or(())
    else {
        return;
    };
    let Ok(encoded) = env.get_string(&item) else {
        return;
    };
    let Some(stack) = parse_slot(encoded.to_str().unwrap_or_default()) else {
        return;
    };
    let Some((_, entity)) = entity_by_uuid(&id) else {
        return;
    };
    if let Some(item) = entity.as_ref().downcast_ref::<ItemEntity>() {
        item.set_item(stack);
    } else if let Some(frame) = entity.as_ref().downcast_ref::<ItemFrameEntity>() {
        frame.set_item(stack);
    } else if let Some(fireball) = entity.as_ref().downcast_ref::<LargeFireballEntity>() {
        fireball.set_item(stack);
    } else if let Some(fireball) = entity.as_ref().downcast_ref::<SmallFireballEntity>() {
        fireball.set_item(stack);
    }
}

extern "system" fn set_item_unlimited_lifetime(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    unlimited: jboolean,
) {
    let Ok(text) = env.get_string(&uuid) else {
        return;
    };
    let Ok(id) = text
        .to_str()
        .ok()
        .and_then(|value| value.parse().ok())
        .ok_or(())
    else {
        return;
    };
    let Some((_, entity)) = entity_by_uuid(&id) else {
        return;
    };
    if let Some(item) = entity.as_ref().downcast_ref::<ItemEntity>() {
        item.set_age(if unlimited != 0 { i32::MAX } else { 0 });
    }
}

extern "system" fn item_age(mut env: JNIEnv<'_>, _class: JClass<'_>, uuid: JString<'_>) -> jint {
    let Ok(text) = env.get_string(&uuid) else {
        return 0;
    };
    let Ok(id) = text
        .to_str()
        .ok()
        .and_then(|value| value.parse().ok())
        .ok_or(())
    else {
        return 0;
    };
    entity_by_uuid(&id)
        .and_then(|(_, entity)| entity.downcast_ref::<ItemEntity>().map(ItemEntity::get_age))
        .unwrap_or(0)
}

extern "system" fn set_item_age(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    age: jint,
) {
    let Ok(text) = env.get_string(&uuid) else {
        return;
    };
    let Ok(id) = text
        .to_str()
        .ok()
        .and_then(|value| value.parse().ok())
        .ok_or(())
    else {
        return;
    };
    if let Some((_, entity)) = entity_by_uuid(&id)
        && let Some(item) = entity.downcast_ref::<ItemEntity>()
    {
        item.set_age(age.clamp(0, i32::MAX));
    }
}

extern "system" fn remove_entity(mut env: JNIEnv<'_>, _class: JClass<'_>, uuid: JString<'_>) {
    let Ok(text) = env.get_string(&uuid) else {
        return;
    };
    let Ok(id) = text
        .to_str()
        .ok()
        .and_then(|value| value.parse::<Uuid>().ok())
        .ok_or(())
    else {
        return;
    };
    if lifecycle::discard_pending(&id) {
        remove_projectile_source(&id);
        return;
    }
    let Some((_, entity)) = entity_by_uuid(&id) else {
        remove_projectile_source(&id);
        return;
    };
    entity.set_removed(RemovalReason::Discarded);
    remove_projectile_source(&id);
}

extern "system" fn entity_world(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jstring {
    let text: String = match env.get_string(&uuid) {
        Ok(v) => v.into(),
        Err(_) => return to_java(&mut env, None),
    };
    let Some(id) = Uuid::parse_str(&text).ok() else {
        return to_java(&mut env, None);
    };
    to_java(
        &mut env,
        entity_by_uuid(&id).map(|(world, _)| world.key.to_string()),
    )
}

extern "system" fn entity_eject(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jboolean {
    let Ok(text) = env.get_string(&uuid) else {
        return jboolean::from(false);
    };
    let Ok(id) = Uuid::parse_str(text.to_str().unwrap_or_default()) else {
        return jboolean::from(false);
    };
    let Some((_world, entity)) = entity_by_uuid(&id) else {
        return jboolean::from(false);
    };
    let passengers = entity.passengers();
    if passengers.is_empty() {
        return jboolean::from(false);
    }
    for passenger in passengers {
        passenger.stop_riding();
    }
    jboolean::from(true)
}

extern "system" fn entity_remove_passenger(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    vehicle: JString<'_>,
    passenger: JString<'_>,
) -> jboolean {
    let Ok(vehicle) = env.get_string(&vehicle) else {
        return 0;
    };
    let Ok(passenger) = env.get_string(&passenger) else {
        return 0;
    };
    let Ok(vehicle_id) = vehicle
        .to_str()
        .ok()
        .and_then(|v| v.parse::<Uuid>().ok())
        .ok_or(())
    else {
        return 0;
    };
    let Ok(passenger_id) = passenger
        .to_str()
        .ok()
        .and_then(|v| v.parse::<Uuid>().ok())
        .ok_or(())
    else {
        return 0;
    };
    let Some((_world, entity)) = entity_by_uuid(&vehicle_id) else {
        return 0;
    };
    let present = entity
        .passengers()
        .iter()
        .any(|entry| entry.uuid() == passenger_id);
    if !present {
        return 0;
    }
    if let Some((_world, passenger)) = entity_by_uuid(&passenger_id) {
        passenger.stop_riding();
        return 1;
    }
    0
}

extern "system" fn entity_vehicle(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jstring {
    let Ok(text) = env.get_string(&uuid) else {
        return null_mut();
    };
    let Ok(id) = text
        .to_str()
        .ok()
        .and_then(|value| value.parse::<Uuid>().ok())
        .ok_or(())
    else {
        return null_mut();
    };
    let vehicle = entity_by_uuid(&id)
        .and_then(|(_, entity)| entity.vehicle())
        .map(|vehicle| vehicle.uuid().to_string());
    to_java(&mut env, vehicle)
}

extern "system" fn entity_leave_vehicle(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jboolean {
    let Ok(text) = env.get_string(&uuid) else {
        return 0;
    };
    let Ok(id) = Uuid::parse_str(text.to_str().unwrap_or_default()) else {
        return 0;
    };
    let Some((_, entity)) = entity_by_uuid(&id) else {
        return 0;
    };
    let was_riding = entity.vehicle().is_some();
    entity.stop_riding();
    jboolean::from(was_riding && entity.vehicle().is_none())
}

extern "system" fn entity_passengers(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jstring {
    let Ok(text) = env.get_string(&uuid) else {
        return null_mut();
    };
    let Some(id) = text
        .to_str()
        .ok()
        .and_then(|value| value.parse::<Uuid>().ok())
    else {
        return null_mut();
    };
    let value = entity_by_uuid(&id)
        .map(|(_, entity)| {
            entity
                .passengers()
                .into_iter()
                .map(|passenger| passenger.uuid().to_string())
                .collect::<Vec<_>>()
                .join(",")
        })
        .unwrap_or_default();
    to_java(&mut env, Some(value))
}

extern "system" fn entity_add_passenger(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    vehicle: JString<'_>,
    passenger: JString<'_>,
) -> jboolean {
    let Ok(vehicle) = env.get_string(&vehicle) else {
        return jboolean::from(false);
    };
    let Ok(passenger) = env.get_string(&passenger) else {
        return jboolean::from(false);
    };
    let (Some(vehicle), Some(passenger)) = (
        vehicle.to_str().ok().and_then(|v| v.parse::<Uuid>().ok()),
        passenger.to_str().ok().and_then(|v| v.parse::<Uuid>().ok()),
    ) else {
        return jboolean::from(false);
    };
    let Some((_, vehicle)) = entity_by_uuid(&vehicle) else {
        return jboolean::from(false);
    };
    let Some((_, passenger)) = entity_by_uuid(&passenger) else {
        return jboolean::from(false);
    };
    jboolean::from(start_riding_entities(&passenger, &vehicle))
}
extern "system" fn entity_target(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jstring {
    let Ok(value) = env.get_string(&uuid) else {
        return null_mut();
    };
    let Ok(id) = Uuid::parse_str(value.to_str().unwrap_or("")) else {
        return null_mut();
    };
    let Some((_, entity)) = entity_by_uuid(&id) else {
        return null_mut();
    };
    let Some(mob) = entity.as_mob() else {
        return null_mut();
    };
    mob.target().map_or(null_mut(), |target| {
        to_java(&mut env, Some(target.uuid().to_string()))
    })
}

extern "system" fn set_entity_target(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    target: JString<'_>,
) {
    let Ok(value) = env.get_string(&uuid) else {
        return;
    };
    let Ok(id) = Uuid::parse_str(value.to_str().unwrap_or("")) else {
        return;
    };
    let Some((_, entity)) = entity_by_uuid(&id) else {
        return;
    };
    let Some(mob) = entity.as_mob() else {
        return;
    };
    if target.is_null() {
        mob.set_target(None);
        return;
    }
    let Ok(target_value) = env.get_string(&target) else {
        return;
    };
    let target_id = Uuid::parse_str(target_value.to_str().unwrap_or("")).ok();
    let target_entity = target_id
        .as_ref()
        .and_then(|target_id| entity_by_uuid(target_id).map(|(_, entity)| entity));
    mob.set_target(target_entity.as_ref());
}

extern "system" fn entity_is_living(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jboolean {
    let Ok(text) = env.get_string(&uuid) else {
        return jboolean::from(false);
    };
    let Ok(id) = Uuid::parse_str(text.to_str().unwrap_or_default()) else {
        return jboolean::from(false);
    };
    jboolean::from(
        entity_by_uuid(&id).is_some_and(|(_, entity)| entity.as_living_entity().is_some()),
    )
}

extern "system" fn entity_is_fall_flying(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jboolean {
    let Ok(text) = env.get_string(&uuid) else {
        return 0;
    };
    let Ok(id) = Uuid::parse_str(text.to_str().unwrap_or_default()) else {
        return 0;
    };
    jboolean::from(entity_by_uuid(&id).is_some_and(|(_, entity)| {
        entity
            .as_living_entity()
            .is_some_and(LivingEntity::is_fall_flying)
    }))
}

extern "system" fn entity_is_tamed(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jboolean {
    let Ok(text) = env.get_string(&uuid) else {
        return 0;
    };
    let Ok(id) = text
        .to_str()
        .ok()
        .and_then(|value| value.parse().ok())
        .ok_or(())
    else {
        return 0;
    };
    jboolean::from(entity_by_uuid(&id).is_some_and(|(_, entity)| is_tamed(entity.as_ref())))
}

extern "system" fn set_entity_tamed(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    tamed: jboolean,
) {
    let Ok(text) = env.get_string(&uuid) else {
        return;
    };
    let Ok(id) = text
        .to_str()
        .ok()
        .and_then(|value| value.parse().ok())
        .ok_or(())
    else {
        return;
    };
    if let Some((_, entity)) = entity_by_uuid(&id) {
        set_tamed(entity.as_ref(), tamed != 0);
    }
}

extern "system" fn entity_owner(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jstring {
    let Ok(text) = env.get_string(&uuid) else {
        return null_mut();
    };
    let Ok(id) = text
        .to_str()
        .ok()
        .and_then(|value| value.parse().ok())
        .ok_or(())
    else {
        return null_mut();
    };
    to_java(
        &mut env,
        entity_by_uuid(&id)
            .and_then(|(_, entity)| owner_uuid(entity.as_ref()))
            .map(|owner| owner.to_string()),
    )
}

extern "system" fn set_entity_owner(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    owner: JString<'_>,
) {
    let Ok(text) = env.get_string(&uuid) else {
        return;
    };
    let Ok(id) = text
        .to_str()
        .ok()
        .and_then(|value| value.parse().ok())
        .ok_or(())
    else {
        return;
    };
    let owner = env
        .get_string(&owner)
        .ok()
        .and_then(|value| value.to_str().ok().and_then(|value| value.parse().ok()));
    if let Some((_, entity)) = entity_by_uuid(&id) {
        set_owner_uuid(entity.as_ref(), owner);
    }
}

extern "system" fn villager_type(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jstring {
    let Ok(text) = env.get_string(&uuid) else {
        return null_mut();
    };
    let Ok(id) = text
        .to_str()
        .ok()
        .and_then(|value| value.parse().ok())
        .ok_or(())
    else {
        return null_mut();
    };
    let value = entity_by_uuid(&id).and_then(|(_, entity)| {
        if let Some(villager) = entity.as_ref().downcast_ref::<VillagerEntity>() {
            return Some(villager.villager_type().key.to_string());
        }
        entity
            .as_ref()
            .downcast_ref::<ZombieVillagerEntity>()
            .map(|villager| villager.villager_type().key.to_string())
    });
    to_java(&mut env, value)
}

extern "system" fn villager_memory(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    key: JString<'_>,
) -> jobjectArray {
    let Ok(uuid_text) = env.get_string(&uuid) else {
        return null_mut();
    };
    let Ok(id) = uuid_text
        .to_str()
        .ok()
        .and_then(|v| v.parse().ok())
        .ok_or(())
    else {
        return null_mut();
    };
    let Ok(key_text) = env.get_string(&key) else {
        return null_mut();
    };
    let key = key_text.to_str().unwrap_or_default();
    let Some((_, entity)) = entity_by_uuid(&id) else {
        return null_mut();
    };
    let Some(villager) = entity.downcast_ref::<VillagerEntity>() else {
        return null_mut();
    };
    let Some(memory) = villager.memory_global_pos(key) else {
        return null_mut();
    };
    let values = [
        memory.dimension.to_string(),
        memory.pos.x().to_string(),
        memory.pos.y().to_string(),
        memory.pos.z().to_string(),
    ];
    string_array(&mut env, &values)
}

extern "system" fn set_villager_memory(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    key: JString<'_>,
    world: JString<'_>,
    x: jint,
    y: jint,
    z: jint,
) -> jboolean {
    let Ok(uuid_text) = env.get_string(&uuid) else {
        return 0;
    };
    let Ok(id) = uuid_text
        .to_str()
        .ok()
        .and_then(|v| v.parse().ok())
        .ok_or(())
    else {
        return 0;
    };
    let Ok(key_text) = env.get_string(&key) else {
        return 0;
    };
    let Ok(world_text) = env.get_string(&world) else {
        return 0;
    };
    let Ok(dimension) = world_text
        .to_str()
        .unwrap_or_default()
        .parse::<Identifier>()
    else {
        return 0;
    };
    let Some((_, entity)) = entity_by_uuid(&id) else {
        return 0;
    };
    let Some(villager) = entity.downcast_ref::<VillagerEntity>() else {
        return 0;
    };
    jboolean::from(villager.set_memory_global_pos(
        key_text.to_str().unwrap_or_default(),
        Some(foton_utils::GlobalPos::new(
            dimension,
            BlockPos::new(x, y, z),
        )),
    ))
}

extern "system" fn clear_villager_memory(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    key: JString<'_>,
) {
    let Ok(uuid_text) = env.get_string(&uuid) else {
        return;
    };
    let Ok(id) = uuid_text
        .to_str()
        .ok()
        .and_then(|v| v.parse().ok())
        .ok_or(())
    else {
        return;
    };
    let Ok(key_text) = env.get_string(&key) else {
        return;
    };
    if let Some((_, entity)) = entity_by_uuid(&id)
        && let Some(villager) = entity.downcast_ref::<VillagerEntity>()
    {
        villager.set_memory_global_pos(key_text.to_str().unwrap_or_default(), None);
    }
}

extern "system" fn set_villager_type(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    kind: JString<'_>,
) {
    let Ok(text) = env.get_string(&uuid) else {
        return;
    };
    let Ok(id) = text
        .to_str()
        .ok()
        .and_then(|value| value.parse().ok())
        .ok_or(())
    else {
        return;
    };
    let Ok(kind) = env.get_string(&kind) else {
        return;
    };
    let Ok(kind) = kind.to_str() else {
        return;
    };
    let key = format!("minecraft:{}", kind.to_lowercase());
    if let Some((_, entity)) = entity_by_uuid(&id) {
        if let Some(villager) = entity.as_ref().downcast_ref::<VillagerEntity>() {
            if let Ok(key) = key.parse()
                && let Some(kind) = REGISTRY.villager_types.by_key(&key)
            {
                villager.set_villager_type(kind);
            }
        } else if let Some(villager) = entity.as_ref().downcast_ref::<ZombieVillagerEntity>()
            && let Ok(key) = key.parse()
            && let Some(kind) = REGISTRY.villager_types.by_key(&key)
        {
            villager.set_villager_type(kind);
        }
    }
}

extern "system" fn villager_profession(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jstring {
    let Ok(text) = env.get_string(&uuid) else {
        return null_mut();
    };
    let Some(id) = text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return null_mut();
    };
    let value = entity_by_uuid(&id).and_then(|(_, entity)| {
        entity
            .as_ref()
            .downcast_ref::<VillagerEntity>()
            .map(|villager| villager.profession().key.path.to_string())
    });
    to_java(&mut env, value)
}

extern "system" fn set_villager_level(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    level: jint,
) {
    let Ok(text) = env.get_string(&uuid) else {
        return;
    };
    let Some(id) = text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return;
    };
    if let Some((_, entity)) = entity_by_uuid(&id)
        && let Some(villager) = entity.downcast_ref::<VillagerEntity>()
    {
        villager.set_level(level);
    }
}

extern "system" fn villager_level(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jint {
    let Ok(text) = env.get_string(&uuid) else {
        return 1;
    };
    let Some(id) = text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return 1;
    };
    entity_by_uuid(&id)
        .and_then(|(_, entity)| {
            entity
                .downcast_ref::<VillagerEntity>()
                .map(VillagerEntity::villager_level)
        })
        .unwrap_or(1)
}

extern "system" fn set_villager_experience(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    experience: jint,
) {
    let Ok(text) = env.get_string(&uuid) else {
        return;
    };
    let Some(id) = text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return;
    };
    if let Some((_, entity)) = entity_by_uuid(&id)
        && let Some(villager) = entity.downcast_ref::<VillagerEntity>()
    {
        villager.set_villager_xp(experience);
    }
}

extern "system" fn villager_experience(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jint {
    let Ok(text) = env.get_string(&uuid) else {
        return 0;
    };
    let Some(id) = text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return 0;
    };
    entity_by_uuid(&id)
        .and_then(|(_, entity)| {
            entity
                .as_ref()
                .downcast_ref::<VillagerEntity>()
                .map(VillagerEntity::villager_xp)
        })
        .unwrap_or(0)
}

extern "system" fn set_villager_offers(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    offers: JObjectArray<'_>,
) {
    let Ok(text) = env.get_string(&uuid) else {
        return;
    };
    let Some(id) = text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return;
    };
    let Some(values) = read_string_array(&mut env, &offers) else {
        return;
    };
    let mut parsed = Vec::with_capacity(values.len());
    for value in values {
        let fields = value.split('|').collect::<Vec<_>>();
        if fields.len() < 6 {
            continue;
        }
        let Some(result) = parse_slot(fields[0]) else {
            continue;
        };
        let Some(cost_a) = parse_slot(fields[4]) else {
            continue;
        };
        let cost_b = if fields[5].is_empty() {
            None
        } else {
            parse_slot(fields[5])
        };
        let uses = fields[1].parse().unwrap_or(0).max(0);
        let max_uses = fields[2].parse().unwrap_or(1).max(1);
        let demand = fields[3].parse().unwrap_or(0);
        let first = ItemCost::new(cost_a.item(), cost_a.count());
        let second = cost_b.map(|stack| ItemCost::new(stack.item(), stack.count()));
        parsed.push(MerchantOffer::with_uses(
            first, second, result, uses, max_uses, 0, 0.05, demand,
        ));
    }
    if let Some((_, entity)) = entity_by_uuid(&id)
        && let Some(villager) = entity.as_ref().downcast_ref::<VillagerEntity>()
    {
        villager.merchant().set_offers(parsed.into());
    }
}

extern "system" fn reset_villager_offers(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) {
    let Ok(text) = env.get_string(&uuid) else {
        return;
    };
    let Some(id) = text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return;
    };
    if let Some((_, entity)) = entity_by_uuid(&id)
        && let Some(villager) = entity.as_ref().downcast_ref::<VillagerEntity>()
    {
        villager.merchant().clear_offers();
    }
}

extern "system" fn set_zombie_villager_profession(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    profession: JString<'_>,
) {
    let Ok(text) = env.get_string(&uuid) else {
        return;
    };
    let Some(id) = text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return;
    };
    let Ok(value) = env.get_string(&profession) else {
        return;
    };
    let Ok(key) = format!("minecraft:{}", value.to_string_lossy().to_lowercase()).parse() else {
        return;
    };
    let Some(profession) = REGISTRY.villager_professions.by_key(&key) else {
        return;
    };
    if let Some((_, entity)) = entity_by_uuid(&id)
        && let Some(villager) = entity.downcast_ref::<ZombieVillagerEntity>()
    {
        villager.set_profession(profession);
    }
}

/// Converts a regular zombie to a zombie villager.
extern "system" fn set_zombie_villager(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    villager: jboolean,
) {
    if villager == 0 {
        return;
    }
    let Ok(text) = env.get_string(&uuid) else {
        return;
    };
    let Some(id) = text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return;
    };
    let Some((_, entity)) = entity_by_uuid(&id) else {
        return;
    };
    let Some(zombie) = entity.as_ref().downcast_ref::<ZombieEntity>() else {
        return;
    };
    let _ = convert_to(
        zombie,
        ConversionParams::single(true, true),
        |new_id, position, world| {
            ZombieVillagerEntity::new(&vanilla_entities::ZOMBIE_VILLAGER, new_id, position, world)
        },
        |_| {},
    );
}

extern "system" fn zombie_villager_profession(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jstring {
    let Ok(text) = env.get_string(&uuid) else {
        return null_mut();
    };
    let Some(id) = text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return null_mut();
    };
    let value = entity_by_uuid(&id).and_then(|(_, entity)| {
        entity
            .as_ref()
            .downcast_ref::<ZombieVillagerEntity>()
            .map(|villager| villager.profession().key.path.to_string())
    });
    to_java(&mut env, value)
}

extern "system" fn area_effect_cloud_source(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jstring {
    let Ok(text): Result<String, _> = env.get_string(&uuid).map(Into::into) else {
        return null_mut();
    };
    let Ok(id) = Uuid::parse_str(&text) else {
        return null_mut();
    };
    let source = entity_by_uuid(&id)
        .and_then(|(_, entity)| entity.downcast_ref::<AreaEffectCloudEntity>()?.owner_uuid())
        .map(|owner| owner.to_string());
    to_java(&mut env, source)
}

extern "system" fn area_effect_cloud_base_potion_type(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jstring {
    let Ok(text): Result<String, _> = env.get_string(&uuid).map(Into::into) else {
        return null_mut();
    };
    let Ok(id) = Uuid::parse_str(&text) else {
        return null_mut();
    };
    let value = entity_by_uuid(&id)
        .and_then(|(_, entity)| {
            entity
                .downcast_ref::<AreaEffectCloudEntity>()?
                .base_potion()
        })
        .map(|potion| potion.key.path.to_ascii_uppercase());
    to_java(&mut env, value)
}

extern "system" fn area_effect_cloud_radius(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jfloat {
    let Ok(text): Result<String, _> = env.get_string(&uuid).map(Into::into) else {
        return 0.0;
    };
    let Ok(id) = text.parse() else {
        return 0.0;
    };
    entity_by_uuid(&id)
        .and_then(|(_, entity)| {
            entity
                .downcast_ref::<AreaEffectCloudEntity>()
                .map(AreaEffectCloudEntity::radius)
        })
        .unwrap_or(0.0)
}

extern "system" fn area_effect_cloud_effects(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jobjectArray {
    let Ok(text): Result<String, _> = env.get_string(&uuid).map(Into::into) else {
        return null_mut();
    };
    let Ok(id) = Uuid::parse_str(&text) else {
        return null_mut();
    };
    let Some((_, entity)) = entity_by_uuid(&id) else {
        return null_mut();
    };
    let Some(cloud) = entity.downcast_ref::<AreaEffectCloudEntity>() else {
        return null_mut();
    };
    let effects: Vec<String> = cloud
        .effects()
        .into_iter()
        .map(|effect| {
            format!(
                "{}|{}|{}|{}|{}|{}",
                effect.effect().key,
                effect.duration(),
                effect.amplifier(),
                effect.is_ambient(),
                effect.is_visible(),
                effect.show_icon()
            )
        })
        .collect();
    string_array(&mut env, &effects)
}

extern "system" fn add_area_effect_cloud_effect(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    type_name: JString<'_>,
    duration: jint,
    amplifier: jint,
    ambient: jboolean,
    particles: jboolean,
    icon: jboolean,
    override_existing: jboolean,
) -> jboolean {
    let Ok(text): Result<String, _> = env.get_string(&uuid).map(Into::into) else {
        return 0;
    };
    let Ok(name): Result<String, _> = env.get_string(&type_name).map(Into::into) else {
        return 0;
    };
    let Ok(id) = text.parse() else {
        return 0;
    };
    let Ok(key) = format!("minecraft:{name}").parse() else {
        return 0;
    };
    let Some(effect) = REGISTRY.mob_effects.by_key(&key) else {
        return 0;
    };
    let Some((_, entity)) = entity_by_uuid(&id) else {
        return 0;
    };
    let Some(cloud) = entity.downcast_ref::<AreaEffectCloudEntity>() else {
        return 0;
    };
    cloud
        .add_custom_effect(
            MobEffectInstance::with_duration(effect, duration, amplifier)
                .with_ambient(ambient != 0)
                .with_visible(particles != 0)
                .with_show_icon(icon != 0),
            override_existing != 0,
        )
        .into()
}

extern "system" fn clear_area_effect_cloud_effects(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) {
    let Ok(text): Result<String, _> = env.get_string(&uuid).map(Into::into) else {
        return;
    };
    let Ok(id) = text.parse() else {
        return;
    };
    let Some((_, entity)) = entity_by_uuid(&id) else {
        return;
    };
    let Some(cloud) = entity.downcast_ref::<AreaEffectCloudEntity>() else {
        return;
    };
    cloud.clear_custom_effects();
}

extern "system" fn set_area_effect_cloud_radius(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    radius: jfloat,
) {
    let Ok(text): Result<String, _> = env.get_string(&uuid).map(Into::into) else {
        return;
    };
    let Ok(id) = text.parse() else {
        return;
    };
    let Some((_, entity)) = entity_by_uuid(&id) else {
        return;
    };
    let Some(cloud) = entity.downcast_ref::<AreaEffectCloudEntity>() else {
        return;
    };
    cloud.set_radius(radius);
}

extern "system" fn area_effect_cloud_duration(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jint {
    let Ok(text): Result<String, _> = env.get_string(&uuid).map(Into::into) else {
        return 0;
    };
    let Ok(id) = text.parse() else {
        return 0;
    };
    entity_by_uuid(&id)
        .and_then(|(_, entity)| {
            entity
                .downcast_ref::<AreaEffectCloudEntity>()
                .map(AreaEffectCloudEntity::duration)
        })
        .unwrap_or(0)
}

extern "system" fn area_effect_cloud_wait_time(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jint {
    let Ok(text): Result<String, _> = env.get_string(&uuid).map(Into::into) else {
        return 0;
    };
    let Ok(id) = text.parse() else {
        return 0;
    };
    entity_by_uuid(&id)
        .and_then(|(_, entity)| {
            entity
                .downcast_ref::<AreaEffectCloudEntity>()
                .map(AreaEffectCloudEntity::wait_time)
        })
        .unwrap_or(0)
}

extern "system" fn area_effect_cloud_reapplication_delay(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jint {
    let Ok(text): Result<String, _> = env.get_string(&uuid).map(Into::into) else {
        return 0;
    };
    let Ok(id) = text.parse() else {
        return 0;
    };
    entity_by_uuid(&id)
        .and_then(|(_, entity)| {
            entity
                .downcast_ref::<AreaEffectCloudEntity>()
                .map(AreaEffectCloudEntity::reapplication_delay)
        })
        .unwrap_or(0)
}

extern "system" fn area_effect_cloud_radius_per_tick(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jfloat {
    let Ok(text): Result<String, _> = env.get_string(&uuid).map(Into::into) else {
        return 0.0;
    };
    let Ok(id) = text.parse() else {
        return 0.0;
    };
    entity_by_uuid(&id)
        .and_then(|(_, entity)| {
            entity
                .downcast_ref::<AreaEffectCloudEntity>()
                .map(AreaEffectCloudEntity::radius_per_tick)
        })
        .unwrap_or(0.0)
}

extern "system" fn area_effect_cloud_radius_on_use(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jfloat {
    let Ok(text): Result<String, _> = env.get_string(&uuid).map(Into::into) else {
        return 0.0;
    };
    let Ok(id) = text.parse() else {
        return 0.0;
    };
    entity_by_uuid(&id)
        .and_then(|(_, entity)| {
            entity
                .downcast_ref::<AreaEffectCloudEntity>()
                .map(AreaEffectCloudEntity::radius_on_use)
        })
        .unwrap_or(0.0)
}

extern "system" fn set_area_effect_cloud_duration(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    value: jint,
) {
    let Ok(text): Result<String, _> = env.get_string(&uuid).map(Into::into) else {
        return;
    };
    let Ok(id) = text.parse() else {
        return;
    };
    let Some((_, entity)) = entity_by_uuid(&id) else {
        return;
    };
    let Some(cloud) = entity.downcast_ref::<AreaEffectCloudEntity>() else {
        return;
    };
    cloud.set_duration(value);
}

extern "system" fn set_area_effect_cloud_wait_time(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    value: jint,
) {
    let Ok(text): Result<String, _> = env.get_string(&uuid).map(Into::into) else {
        return;
    };
    let Ok(id) = text.parse() else {
        return;
    };
    let Some((_, entity)) = entity_by_uuid(&id) else {
        return;
    };
    let Some(cloud) = entity.downcast_ref::<AreaEffectCloudEntity>() else {
        return;
    };
    cloud.set_wait_time(value);
}

extern "system" fn set_area_effect_cloud_reapplication_delay(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    value: jint,
) {
    let Ok(text): Result<String, _> = env.get_string(&uuid).map(Into::into) else {
        return;
    };
    let Ok(id) = text.parse() else {
        return;
    };
    let Some((_, entity)) = entity_by_uuid(&id) else {
        return;
    };
    let Some(cloud) = entity.downcast_ref::<AreaEffectCloudEntity>() else {
        return;
    };
    cloud.set_reapplication_delay(value);
}

extern "system" fn set_area_effect_cloud_radius_per_tick(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    value: jfloat,
) {
    let Ok(text): Result<String, _> = env.get_string(&uuid).map(Into::into) else {
        return;
    };
    let Ok(id) = text.parse() else {
        return;
    };
    let Some((_, entity)) = entity_by_uuid(&id) else {
        return;
    };
    let Some(cloud) = entity.downcast_ref::<AreaEffectCloudEntity>() else {
        return;
    };
    cloud.set_radius_per_tick(value);
}

extern "system" fn set_area_effect_cloud_radius_on_use(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    value: jfloat,
) {
    let Ok(text): Result<String, _> = env.get_string(&uuid).map(Into::into) else {
        return;
    };
    let Ok(id) = text.parse() else {
        return;
    };
    let Some((_, entity)) = entity_by_uuid(&id) else {
        return;
    };
    let Some(cloud) = entity.downcast_ref::<AreaEffectCloudEntity>() else {
        return;
    };
    cloud.set_radius_on_use(value);
}

extern "system" fn firework_meta(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jstring {
    let Ok(text): Result<String, _> = env.get_string(&uuid).map(Into::into) else {
        return null_mut();
    };
    let Ok(id) = text.parse() else {
        return null_mut();
    };
    let value = entity_by_uuid(&id).and_then(|(_, entity)| {
        let rocket = entity.downcast_ref::<FireworkRocketEntity>()?;
        let item = rocket.get_item();
        let component = item.get(FIREWORKS)?;
        let effects = component
            .explosions()
            .iter()
            .map(|e| {
                format!(
                    "{}|{}|{}|{}|{}",
                    e.shape().id(),
                    e.colors()
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(","),
                    e.fade_colors()
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(","),
                    e.has_trail(),
                    e.has_twinkle()
                )
            })
            .collect::<Vec<_>>()
            .join(";");
        Some(format!("{};{}", component.flight_duration(), effects))
    });
    to_java(&mut env, value)
}

extern "system" fn set_firework_meta(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    power: jint,
    effects: JString<'_>,
) {
    let Ok(uuid): Result<String, _> = env.get_string(&uuid).map(Into::into) else {
        return;
    };
    let Ok(effects): Result<String, _> = env.get_string(&effects).map(Into::into) else {
        return;
    };
    let explosions = effects
        .split(';')
        .filter(|value| !value.is_empty())
        .filter_map(|value| {
            let fields: Vec<&str> = value.split('|').collect();
            if fields.len() != 5 {
                return None;
            }
            let shape = match fields[0] {
                "BALL" => FireworkExplosionShape::SmallBall,
                "BALL_LARGE" => FireworkExplosionShape::LargeBall,
                "STAR" => FireworkExplosionShape::Star,
                "CREEPER" => FireworkExplosionShape::Creeper,
                "BURST" => FireworkExplosionShape::Burst,
                _ => return None,
            };
            let parse_colors = |text: &str| -> Option<Vec<i32>> {
                if text.is_empty() {
                    return Some(Vec::new());
                }
                text.split(',').map(|color| color.parse().ok()).collect()
            };
            Some(FireworkExplosion::new(
                shape,
                parse_colors(fields[1])?,
                parse_colors(fields[2])?,
                fields[3] == "true",
                fields[4] == "true",
            ))
        })
        .collect();
    let Ok(component) = Fireworks::new(power.clamp(0, jint::from(u8::MAX)), explosions) else {
        return;
    };
    let Ok(id) = uuid.parse() else {
        return;
    };
    let Some((_, entity)) = entity_by_uuid(&id) else {
        return;
    };
    let Some(rocket) = entity.as_ref().downcast_ref::<FireworkRocketEntity>() else {
        return;
    };
    let mut item = ItemStack::new(&vanilla_items::FIREWORK_ROCKET);
    item.set(FIREWORKS, component);
    rocket.set_item(item);
}

extern "system" fn fox_sitting(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jboolean {
    let Ok(text) = env.get_string(&uuid) else {
        return 0;
    };
    let Some(id) = text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return 0;
    };
    jboolean::from(
        entity_by_uuid(&id)
            .and_then(|(_, entity)| {
                entity
                    .downcast_ref::<FoxEntity>()
                    .map(FoxEntity::is_sitting)
            })
            .unwrap_or(false),
    )
}
extern "system" fn set_fox_sitting(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    value: jboolean,
) {
    let Ok(text) = env.get_string(&uuid) else {
        return;
    };
    let Some(id) = text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return;
    };
    if let Some((_, entity)) = entity_by_uuid(&id)
        && let Some(fox) = entity.downcast_ref::<FoxEntity>()
    {
        fox.set_sitting(value != 0);
    }
}

extern "system" fn fox_type(mut env: JNIEnv<'_>, _class: JClass<'_>, uuid: JString<'_>) -> jstring {
    let Ok(text) = env.get_string(&uuid) else {
        return null_mut();
    };
    let Some(id) = text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return null_mut();
    };
    let value = entity_by_uuid(&id).and_then(|(_, entity)| {
        entity.as_ref().downcast_ref::<FoxEntity>().map(|fox| {
            match fox.variant() {
                FoxVariant::Snow => "snow",
                FoxVariant::Red => "red",
            }
            .to_owned()
        })
    });
    to_java(&mut env, value)
}

extern "system" fn set_fox_type(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    kind: JString<'_>,
) {
    let Ok(uuid_text) = env.get_string(&uuid) else {
        return;
    };
    let Ok(kind_text) = env.get_string(&kind) else {
        return;
    };
    let Some(id) = uuid_text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return;
    };
    let Some((_, entity)) = entity_by_uuid(&id) else {
        return;
    };
    let Some(fox) = entity.as_ref().downcast_ref::<FoxEntity>() else {
        return;
    };
    let variant = match kind_text
        .to_str()
        .ok()
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("snow") => FoxVariant::Snow,
        Some("red") => FoxVariant::Red,
        _ => return,
    };
    fox.set_variant(variant);
}

extern "system" fn tropical_fish_pattern_color(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jint {
    let Ok(text) = env.get_string(&uuid) else {
        return -1;
    };
    let Some(id) = text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return -1;
    };
    entity_by_uuid(&id)
        .and_then(|(_, entity)| {
            entity
                .as_ref()
                .downcast_ref::<TropicalFishEntity>()
                .map(|fish| (fish.packed_variant() >> 24) & 0xff)
        })
        .unwrap_or(-1)
}

extern "system" fn set_tropical_fish_pattern_color(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    color: jint,
) {
    if !(0..16).contains(&color) {
        return;
    }
    let Ok(text) = env.get_string(&uuid) else {
        return;
    };
    let Some(id) = text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return;
    };
    let Some((_, entity)) = entity_by_uuid(&id) else {
        return;
    };
    let Some(fish) = entity.as_ref().downcast_ref::<TropicalFishEntity>() else {
        return;
    };
    let packed = (fish.packed_variant() & 0x00ff_ffff) | (color << 24);
    fish.set_packed_variant(packed);
}

extern "system" fn tropical_fish_body_color(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jint {
    let Ok(text) = env.get_string(&uuid) else {
        return -1;
    };
    let Some(id) = text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return -1;
    };
    entity_by_uuid(&id)
        .and_then(|(_, entity)| {
            entity
                .as_ref()
                .downcast_ref::<TropicalFishEntity>()
                .map(|fish| (fish.packed_variant() >> 16) & 0xff)
        })
        .unwrap_or(-1)
}

extern "system" fn set_tropical_fish_body_color(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    color: jint,
) {
    if !(0..16).contains(&color) {
        return;
    }
    let Ok(text) = env.get_string(&uuid) else {
        return;
    };
    let Some(id) = text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return;
    };
    let Some((_, entity)) = entity_by_uuid(&id) else {
        return;
    };
    let Some(fish) = entity.as_ref().downcast_ref::<TropicalFishEntity>() else {
        return;
    };
    fish.set_packed_variant((fish.packed_variant() & !0x00ff_0000) | (color << 16));
}

extern "system" fn slime_size(mut env: JNIEnv<'_>, _class: JClass<'_>, uuid: JString<'_>) -> jint {
    let Ok(text) = env.get_string(&uuid) else {
        return 0;
    };
    let Some(id) = text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return 0;
    };
    entity_by_uuid(&id)
        .and_then(|(_, entity)| {
            entity
                .as_ref()
                .downcast_ref::<SlimeEntity>()
                .map(SlimeEntity::cube_size)
        })
        .unwrap_or(0)
}

extern "system" fn set_slime_size(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    size: jint,
) {
    let Ok(text) = env.get_string(&uuid) else {
        return;
    };
    let Some(id) = text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return;
    };
    let Some((_, entity)) = entity_by_uuid(&id) else {
        return;
    };
    let Some(slime) = entity.as_ref().downcast_ref::<SlimeEntity>() else {
        return;
    };
    slime.set_cube_size(size, Entity::is_alive(slime));
}

extern "system" fn cube_mob_can_wander(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jboolean {
    let Ok(text) = env.get_string(&uuid) else {
        return 0;
    };
    let Some(id) = text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return 0;
    };
    entity_by_uuid(&id)
        .and_then(|(_, entity)| {
            entity
                .as_ref()
                .downcast_ref::<SlimeEntity>()
                .map(SlimeEntity::can_wander)
        })
        .is_some_and(|can_wander| can_wander)
        .into()
}

extern "system" fn set_cube_mob_wander(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    can_wander: jboolean,
) {
    let Ok(text) = env.get_string(&uuid) else {
        return;
    };
    let Some(id) = text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return;
    };
    let Some((_, entity)) = entity_by_uuid(&id) else {
        return;
    };
    let Some(slime) = entity.as_ref().downcast_ref::<SlimeEntity>() else {
        return;
    };
    slime.set_wander(can_wander != 0);
}

extern "system" fn set_creeper_powered(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    powered: jboolean,
) {
    let Ok(text) = env.get_string(&uuid) else {
        return;
    };
    let Some(id) = text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return;
    };
    let Some((_, entity)) = entity_by_uuid(&id) else {
        return;
    };
    let Some(creeper) = entity.as_ref().downcast_ref::<CreeperEntity>() else {
        return;
    };
    creeper.set_powered(powered != 0);
}

extern "system" fn creeper_powered(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jboolean {
    let Ok(text) = env.get_string(&uuid) else {
        return jboolean::from(false);
    };
    let Some(id) = text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return jboolean::from(false);
    };
    jboolean::from(
        entity_by_uuid(&id)
            .and_then(|(_, entity)| {
                entity
                    .as_ref()
                    .downcast_ref::<CreeperEntity>()
                    .map(CreeperEntity::is_powered)
            })
            .unwrap_or(false),
    )
}

extern "system" fn set_goat_screaming(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    screaming: jboolean,
) {
    let Ok(text) = env.get_string(&uuid) else {
        return;
    };
    let Some(id) = text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return;
    };
    let Some((_, entity)) = entity_by_uuid(&id) else {
        return;
    };
    let Some(goat) = entity.as_ref().downcast_ref::<GoatEntity>() else {
        return;
    };
    goat.set_screaming_goat(screaming != 0);
}

extern "system" fn goat_left_horn(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jboolean {
    let Ok(text) = env.get_string(&uuid) else {
        return 0;
    };
    let Some(id) = text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return 0;
    };
    jboolean::from(
        entity_by_uuid(&id)
            .and_then(|(_, entity)| {
                entity
                    .downcast_ref::<GoatEntity>()
                    .map(GoatEntity::has_left_horn)
            })
            .unwrap_or(false),
    )
}
extern "system" fn set_goat_left_horn(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    value: jboolean,
) {
    let Ok(text) = env.get_string(&uuid) else {
        return;
    };
    let Some(id) = text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return;
    };
    if let Some((_, entity)) = entity_by_uuid(&id)
        && let Some(goat) = entity.downcast_ref::<GoatEntity>()
    {
        goat.set_left_horn(value != 0);
    }
}
extern "system" fn goat_right_horn(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jboolean {
    let Ok(text) = env.get_string(&uuid) else {
        return 0;
    };
    let Some(id) = text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return 0;
    };
    jboolean::from(
        entity_by_uuid(&id)
            .and_then(|(_, entity)| {
                entity
                    .downcast_ref::<GoatEntity>()
                    .map(GoatEntity::has_right_horn)
            })
            .unwrap_or(false),
    )
}
extern "system" fn set_goat_right_horn(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    value: jboolean,
) {
    let Ok(text) = env.get_string(&uuid) else {
        return;
    };
    let Some(id) = text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return;
    };
    if let Some((_, entity)) = entity_by_uuid(&id)
        && let Some(goat) = entity.downcast_ref::<GoatEntity>()
    {
        goat.set_right_horn(value != 0);
    }
}

extern "system" fn sheep_color(mut env: JNIEnv<'_>, _class: JClass<'_>, uuid: JString<'_>) -> jint {
    let Ok(text) = env.get_string(&uuid) else {
        return -1;
    };
    let Some(id) = text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return -1;
    };
    entity_by_uuid(&id)
        .and_then(|(_, entity)| {
            entity
                .downcast_ref::<SheepEntity>()
                .map(|sheep| sheep.color() as jint)
        })
        .unwrap_or(-1)
}
extern "system" fn set_sheep_color(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    color: jint,
) {
    let Ok(text) = env.get_string(&uuid) else {
        return;
    };
    let Some(id) = text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return;
    };
    let Some(value) = foton_registry::DyeColor::VALUES
        .get(color as usize)
        .copied()
    else {
        return;
    };
    if let Some((_, entity)) = entity_by_uuid(&id)
        && let Some(sheep) = entity.downcast_ref::<SheepEntity>()
    {
        sheep.set_color(value);
    }
}
extern "system" fn sheep_sheared(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jboolean {
    let Ok(text) = env.get_string(&uuid) else {
        return 0;
    };
    let Some(id) = text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return 0;
    };
    jboolean::from(
        entity_by_uuid(&id)
            .and_then(|(_, entity)| {
                entity
                    .downcast_ref::<SheepEntity>()
                    .map(SheepEntity::is_sheared)
            })
            .unwrap_or(false),
    )
}
extern "system" fn set_sheep_sheared(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    value: jboolean,
) {
    let Ok(text) = env.get_string(&uuid) else {
        return;
    };
    let Some(id) = text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return;
    };
    if let Some((_, entity)) = entity_by_uuid(&id)
        && let Some(sheep) = entity.downcast_ref::<SheepEntity>()
    {
        sheep.set_sheared(value != 0);
    }
}

extern "system" fn goat_screaming(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jboolean {
    let Ok(text) = env.get_string(&uuid) else {
        return jboolean::from(false);
    };
    let Some(id) = text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return jboolean::from(false);
    };
    jboolean::from(
        entity_by_uuid(&id)
            .and_then(|(_, entity)| {
                entity
                    .as_ref()
                    .downcast_ref::<GoatEntity>()
                    .map(GoatEntity::is_screaming_goat)
            })
            .unwrap_or(false),
    )
}

extern "system" fn entity_can_pickup_items(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jboolean {
    let Ok(text) = env.get_string(&uuid) else {
        return 0;
    };
    let Some(id) = text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return 0;
    };
    jboolean::from(entity_by_uuid(&id).is_some_and(|(_, entity)| entity.entity_can_pick_up_loot()))
}
extern "system" fn set_entity_can_pickup_items(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    pickup: jboolean,
) {
    let Ok(text) = env.get_string(&uuid) else {
        return;
    };
    let Some(id) = text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return;
    };
    if let Some((_, entity)) = entity_by_uuid(&id) {
        entity.entity_set_can_pick_up_loot(pickup != 0);
    }
}

extern "system" fn entity_age(mut env: JNIEnv<'_>, _class: JClass<'_>, uuid: JString<'_>) -> jint {
    let Ok(text) = env.get_string(&uuid) else {
        return 0;
    };
    let Some(id) = text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return 0;
    };
    entity_by_uuid(&id).map_or(0, |(_, entity)| entity.entity_age())
}
extern "system" fn set_entity_age(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    age: jint,
) {
    let Ok(text) = env.get_string(&uuid) else {
        return;
    };
    let Some(id) = text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return;
    };
    if let Some((_, entity)) = entity_by_uuid(&id) {
        entity.set_entity_age(age);
    }
}

extern "system" fn entity_is_baby(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jboolean {
    let Ok(text) = env.get_string(&uuid) else {
        return jboolean::from(false);
    };
    let Some(id) = text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return jboolean::from(false);
    };
    jboolean::from(
        entity_by_uuid(&id)
            .and_then(|(_, entity)| {
                entity
                    .as_ref()
                    .as_living_entity()
                    .map(LivingEntity::is_baby)
            })
            .unwrap_or(false),
    )
}

extern "system" fn enchantment_max_level(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    key: JString<'_>,
) -> jint {
    let Ok(key) = env.get_string(&key) else {
        return 0;
    };
    let Some((namespace, path)) = key.to_str().unwrap_or_default().split_once(':') else {
        return 0;
    };
    let key = foton_utils::Identifier::new(namespace.to_owned(), path.to_owned());
    foton_registry::REGISTRY
        .enchantments
        .by_key(&key)
        .map_or(0, |enchantment| enchantment.max_level as jint)
}

extern "system" fn entity_age_lock(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jboolean {
    let Ok(text) = env.get_string(&uuid) else {
        return 0;
    };
    let Some(id) = text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return 0;
    };
    jboolean::from(entity_by_uuid(&id).is_some_and(|(_, entity)| entity.is_ageable_age_locked()))
}

extern "system" fn set_entity_age_lock(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    locked: jboolean,
) {
    let Ok(text) = env.get_string(&uuid) else {
        return;
    };
    let Some(id) = text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return;
    };
    if let Some((_, entity)) = entity_by_uuid(&id) {
        entity.set_ageable_age_locked(locked != 0);
    }
}

extern "system" fn entity_set_baby(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    baby: jboolean,
) {
    let Ok(text) = env.get_string(&uuid) else {
        return;
    };
    let Some(id) = text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return;
    };
    let Some((_, entity)) = entity_by_uuid(&id) else {
        return;
    };
    entity.as_ref().set_ageable_baby(baby != 0);
}

extern "system" fn pig_has_saddle(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jboolean {
    let Ok(text) = env.get_string(&uuid) else {
        return jboolean::from(false);
    };
    let Some(id) = text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return jboolean::from(false);
    };
    jboolean::from(entity_by_uuid(&id).is_some_and(|(_, entity)| {
        let Some(pig) = entity.as_ref().downcast_ref::<PigEntity>() else {
            return false;
        };
        let mut saddled = false;
        pig.with_equipment_slot(EquipmentSlot::Saddle, &mut |stack| {
            saddled = stack.is(&vanilla_items::SADDLE);
        });
        saddled
    }))
}

extern "system" fn pig_set_saddle(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    saddled: jboolean,
) {
    let Ok(text) = env.get_string(&uuid) else {
        return;
    };
    let Some(id) = text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return;
    };
    let Some((_, entity)) = entity_by_uuid(&id) else {
        return;
    };
    let Some(pig) = entity.as_ref().downcast_ref::<PigEntity>() else {
        return;
    };
    let saddle = if saddled != 0 {
        ItemStack::new(&vanilla_items::SADDLE)
    } else {
        ItemStack::empty()
    };
    pig.set_item_slot(EquipmentSlot::Saddle, saddle);
}

extern "system" fn mount_inventory_slot(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    slot: jint,
) -> jstring {
    let Ok(text) = env.get_string(&uuid) else {
        return null_mut();
    };
    let Some(id) = text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return null_mut();
    };
    let Some((_, entity)) = entity_by_uuid(&id) else {
        return null_mut();
    };
    let equipment_slot = if slot == 0 {
        EquipmentSlot::Saddle
    } else if slot == 1 {
        EquipmentSlot::Body
    } else {
        return null_mut();
    };
    let mut value = None;
    if let Some(mount) = entity.as_ref().downcast_ref::<HorseEntity>() {
        mount.with_equipment_slot(equipment_slot, &mut |item| {
            value = Some(describe_slot(item));
        });
    } else if let Some(mount) = entity.as_ref().downcast_ref::<NautilusEntity>() {
        mount.with_equipment_slot(equipment_slot, &mut |item| {
            value = Some(describe_slot(item));
        });
    } else if let Some(mount) = entity.as_ref().downcast_ref::<ZombieNautilusEntity>() {
        mount.with_equipment_slot(equipment_slot, &mut |item| {
            value = Some(describe_slot(item));
        });
    }
    to_java(&mut env, value)
}

extern "system" fn set_mount_inventory_slot(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    slot: jint,
    item: JString<'_>,
) {
    let Ok(text) = env.get_string(&uuid) else {
        return;
    };
    let Some(id) = text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return;
    };
    let Some((_, entity)) = entity_by_uuid(&id) else {
        return;
    };
    let Ok(encoded) = env.get_string(&item) else {
        return;
    };
    let Some(stack) = parse_slot(encoded.to_str().unwrap_or_default()) else {
        return;
    };
    let equipment_slot = if slot == 0 {
        EquipmentSlot::Saddle
    } else if slot == 1 {
        EquipmentSlot::Body
    } else {
        return;
    };
    if let Some(mount) = entity.as_ref().downcast_ref::<HorseEntity>() {
        mount.set_item_slot(equipment_slot, stack);
    } else if let Some(mount) = entity.as_ref().downcast_ref::<NautilusEntity>() {
        mount.set_item_slot(equipment_slot, stack);
    } else if let Some(mount) = entity.as_ref().downcast_ref::<ZombieNautilusEntity>() {
        mount.set_item_slot(equipment_slot, stack);
    }
}

extern "system" fn horse_inventory_slot(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    slot: jint,
) -> jstring {
    let Ok(text) = env.get_string(&uuid) else {
        return null_mut();
    };
    let Some(id) = text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return null_mut();
    };
    let Some((_, entity)) = entity_by_uuid(&id) else {
        return null_mut();
    };
    let Some(horse) = entity.as_ref().downcast_ref::<HorseEntity>() else {
        return null_mut();
    };
    let equipment_slot = if slot == 0 {
        EquipmentSlot::Saddle
    } else if slot == 1 {
        EquipmentSlot::Body
    } else {
        return null_mut();
    };
    let mut value = None;
    horse.with_equipment_slot(equipment_slot, &mut |item| {
        value = Some(describe_slot(item));
    });
    to_java(&mut env, value)
}

extern "system" fn set_horse_inventory_slot(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    slot: jint,
    item: JString<'_>,
) {
    let Ok(text) = env.get_string(&uuid) else {
        return;
    };
    let Some(id) = text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return;
    };
    let Some((_, entity)) = entity_by_uuid(&id) else {
        return;
    };
    let Some(horse) = entity.as_ref().downcast_ref::<HorseEntity>() else {
        return;
    };
    let Ok(encoded) = env.get_string(&item) else {
        return;
    };
    let Some(stack) = parse_slot(encoded.to_str().unwrap_or_default()) else {
        return;
    };
    let equipment_slot = if slot == 0 {
        EquipmentSlot::Saddle
    } else if slot == 1 {
        EquipmentSlot::Body
    } else {
        return;
    };
    horse.set_item_slot(equipment_slot, stack);
}

extern "system" fn entity_has_chest(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jboolean {
    let Ok(text) = env.get_string(&uuid) else {
        return jboolean::from(false);
    };
    let Some(id) = text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return jboolean::from(false);
    };
    jboolean::from(
        entity_by_uuid(&id)
            .and_then(|(_, entity)| entity.as_ref().has_carried_chest())
            .unwrap_or(false),
    )
}

extern "system" fn entity_set_chest(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    carrying: jboolean,
) {
    let Ok(text) = env.get_string(&uuid) else {
        return;
    };
    let Some(id) = text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return;
    };
    if let Some((_, entity)) = entity_by_uuid(&id) {
        entity.as_ref().set_carried_chest(carrying != 0);
    }
}

extern "system" fn set_tropical_fish_pattern(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    pattern: JString<'_>,
) {
    let Ok(uuid_text) = env.get_string(&uuid) else {
        return;
    };
    let Some(id) = uuid_text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return;
    };
    let Ok(pattern_text) = env.get_string(&pattern) else {
        return;
    };
    let Some(pattern) = TropicalFishPattern::VALUES.into_iter().find(|value| {
        value
            .serialized_name()
            .eq_ignore_ascii_case(pattern_text.to_str().unwrap_or_default())
    }) else {
        return;
    };
    let Some((_, entity)) = entity_by_uuid(&id) else {
        return;
    };
    let Some(fish) = entity.as_ref().downcast_ref::<TropicalFishEntity>() else {
        return;
    };
    fish.set_pattern(pattern);
}

extern "system" fn tropical_fish_pattern<'a>(
    mut env: JNIEnv<'a>,
    _class: JClass<'a>,
    uuid: JString<'a>,
) -> JString<'a> {
    let Ok(text) = env.get_string(&uuid) else {
        return JString::from(JObject::null());
    };
    let Some(id) = text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return JString::from(JObject::null());
    };
    let name = entity_by_uuid(&id)
        .and_then(|(_, entity)| {
            entity
                .as_ref()
                .downcast_ref::<TropicalFishEntity>()
                .map(|fish| fish.pattern().serialized_name())
        })
        .unwrap_or("");
    match env.new_string(name) {
        Ok(value) => value,
        Err(_) => JString::from(JObject::null()),
    }
}

extern "system" fn set_axolotl_variant(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    variant: JString<'_>,
) {
    let Ok(uuid_text) = env.get_string(&uuid) else {
        return;
    };
    let Some(id) = uuid_text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return;
    };
    let Ok(variant_text) = env.get_string(&variant) else {
        return;
    };
    let Some(name) = variant_text.to_str().ok() else {
        return;
    };
    let name = name.to_ascii_lowercase();
    let Some(value) = AxolotlVariant::from_serialized_name(&name) else {
        return;
    };
    let Some((_, entity)) = entity_by_uuid(&id) else {
        return;
    };
    let Some(axolotl) = entity.as_ref().downcast_ref::<AxolotlEntity>() else {
        return;
    };
    axolotl.set_variant(value);
}

extern "system" fn axolotl_variant<'a>(
    mut env: JNIEnv<'a>,
    _class: JClass<'a>,
    uuid: JString<'a>,
) -> JString<'a> {
    let Ok(text) = env.get_string(&uuid) else {
        return JString::from(JObject::null());
    };
    let Some(id) = text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return JString::from(JObject::null());
    };
    let name = entity_by_uuid(&id)
        .and_then(|(_, entity)| {
            entity
                .as_ref()
                .downcast_ref::<AxolotlEntity>()
                .map(|value| value.variant().serialized_name())
        })
        .unwrap_or("");
    match env.new_string(name) {
        Ok(value) => value,
        Err(_) => JString::from(JObject::null()),
    }
}

extern "system" fn set_parrot_variant(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    variant: JString<'_>,
) {
    let Ok(uuid_text) = env.get_string(&uuid) else {
        return;
    };
    let Some(id) = uuid_text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return;
    };
    let Ok(variant_text) = env.get_string(&variant) else {
        return;
    };
    let Some(name) = variant_text.to_str().ok() else {
        return;
    };
    let Some(value) = (match name.to_ascii_uppercase().as_str() {
        "RED_BLUE" => Some(ParrotVariant::RedBlue),
        "BLUE" => Some(ParrotVariant::Blue),
        "GREEN" => Some(ParrotVariant::Green),
        "YELLOW_BLUE" => Some(ParrotVariant::YellowBlue),
        "GRAY" => Some(ParrotVariant::Gray),
        _ => None,
    }) else {
        return;
    };
    let Some((_, entity)) = entity_by_uuid(&id) else {
        return;
    };
    let Some(parrot) = entity.as_ref().downcast_ref::<ParrotEntity>() else {
        return;
    };
    parrot.set_variant(value);
}

extern "system" fn parrot_variant<'a>(
    mut env: JNIEnv<'a>,
    _class: JClass<'a>,
    uuid: JString<'a>,
) -> JString<'a> {
    let Ok(text) = env.get_string(&uuid) else {
        return JString::from(JObject::null());
    };
    let Some(id) = text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return JString::from(JObject::null());
    };
    let name = entity_by_uuid(&id)
        .and_then(|(_, entity)| {
            entity
                .as_ref()
                .downcast_ref::<ParrotEntity>()
                .map(|value| match value.variant() {
                    ParrotVariant::RedBlue => "red_blue",
                    ParrotVariant::Blue => "blue",
                    ParrotVariant::Green => "green",
                    ParrotVariant::YellowBlue => "yellow_blue",
                    ParrotVariant::Gray => "gray",
                })
        })
        .unwrap_or("");
    match env.new_string(name) {
        Ok(value) => value,
        Err(_) => JString::from(JObject::null()),
    }
}

extern "system" fn set_mushroom_cow_variant(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    variant: JString<'_>,
) {
    let Ok(text) = env.get_string(&uuid) else {
        return;
    };
    let Some(id) = text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return;
    };
    let Ok(value) = env.get_string(&variant) else {
        return;
    };
    let variant = match value.to_str().ok().map(str::to_ascii_lowercase).as_deref() {
        Some("brown") => MushroomCowVariant::Brown,
        Some("red") => MushroomCowVariant::Red,
        _ => return,
    };
    let Some((_, entity)) = entity_by_uuid(&id) else {
        return;
    };
    let Some(mooshroom) = entity.as_ref().downcast_ref::<MushroomCowEntity>() else {
        return;
    };
    mooshroom.set_variant(variant);
}

extern "system" fn set_zombie_nautilus_variant(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    variant: JString<'_>,
) {
    let Ok(text) = env.get_string(&uuid) else {
        return;
    };
    let Some(id) = text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return;
    };
    let Ok(value) = env.get_string(&variant) else {
        return;
    };
    let Some(entry) = REGISTRY
        .zombie_nautilus_variants
        .by_key(&Identifier::vanilla(
            value.to_str().unwrap_or_default().to_ascii_lowercase(),
        ))
    else {
        return;
    };
    let Some((_, entity)) = entity_by_uuid(&id) else {
        return;
    };
    if let Some(nautilus) = entity.as_ref().downcast_ref::<ZombieNautilusEntity>() {
        nautilus.set_variant(entry);
    }
}

extern "system" fn zombie_nautilus_variant<'a>(
    mut env: JNIEnv<'a>,
    _class: JClass<'a>,
    uuid: JString<'a>,
) -> JString<'a> {
    let Ok(text) = env.get_string(&uuid) else {
        return JString::from(JObject::null());
    };
    let Some(id) = text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return JString::from(JObject::null());
    };
    let name = entity_by_uuid(&id)
        .and_then(|(_, entity)| {
            entity
                .as_ref()
                .downcast_ref::<ZombieNautilusEntity>()
                .map(|nautilus| nautilus.variant().key.path.as_ref().to_owned())
        })
        .unwrap_or_default();
    env.new_string(name)
        .unwrap_or_else(|_| JString::from(JObject::null()))
}

extern "system" fn set_pig_variant(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    variant: JString<'_>,
) {
    let Ok(text) = env.get_string(&uuid) else {
        return;
    };
    let Some(id) = text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return;
    };
    let Ok(value) = env.get_string(&variant) else {
        return;
    };
    let Some(entry) = REGISTRY.pig_variants.by_key(&Identifier::vanilla(
        value.to_str().unwrap_or_default().to_ascii_lowercase(),
    )) else {
        return;
    };
    let Some((_, entity)) = entity_by_uuid(&id) else {
        return;
    };
    let Some(pig) = entity.as_ref().downcast_ref::<PigEntity>() else {
        return;
    };
    pig.set_variant(entry);
}

extern "system" fn pig_variant<'a>(
    mut env: JNIEnv<'a>,
    _class: JClass<'a>,
    uuid: JString<'a>,
) -> JString<'a> {
    let Ok(text) = env.get_string(&uuid) else {
        return JString::from(JObject::null());
    };
    let Some(id) = text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return JString::from(JObject::null());
    };
    let name = entity_by_uuid(&id)
        .and_then(|(_, entity)| {
            entity
                .as_ref()
                .downcast_ref::<PigEntity>()
                .map(|pig| pig.variant().key.path.as_ref().to_owned())
        })
        .unwrap_or_default();
    env.new_string(name)
        .unwrap_or_else(|_| JString::from(JObject::null()))
}

extern "system" fn set_chicken_variant(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    variant: JString<'_>,
) {
    let Ok(text) = env.get_string(&uuid) else {
        return;
    };
    let Some(id) = text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return;
    };
    let Ok(value) = env.get_string(&variant) else {
        return;
    };
    let Some(entry) = REGISTRY.chicken_variants.by_key(&Identifier::vanilla(
        value.to_str().unwrap_or_default().to_ascii_lowercase(),
    )) else {
        return;
    };
    let Some((_, entity)) = entity_by_uuid(&id) else {
        return;
    };
    let Some(chicken) = entity.as_ref().downcast_ref::<ChickenEntity>() else {
        return;
    };
    chicken.set_variant(entry);
}

extern "system" fn chicken_variant<'a>(
    mut env: JNIEnv<'a>,
    _class: JClass<'a>,
    uuid: JString<'a>,
) -> JString<'a> {
    let Ok(text) = env.get_string(&uuid) else {
        return JString::from(JObject::null());
    };
    let Some(id) = text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return JString::from(JObject::null());
    };
    let name = entity_by_uuid(&id)
        .and_then(|(_, entity)| {
            entity
                .as_ref()
                .downcast_ref::<ChickenEntity>()
                .map(|chicken| chicken.variant().key.path.as_ref().to_owned())
        })
        .unwrap_or_default();
    env.new_string(name)
        .unwrap_or_else(|_| JString::from(JObject::null()))
}

extern "system" fn set_frog_variant(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    variant: JString<'_>,
) {
    let Ok(text) = env.get_string(&uuid) else {
        return;
    };
    let Some(id) = text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return;
    };
    let Ok(value) = env.get_string(&variant) else {
        return;
    };
    let Some(name) = value.to_str().ok() else {
        return;
    };
    let Some(variant) = foton_registry::REGISTRY
        .frog_variants
        .by_key(&foton_utils::Identifier::vanilla(name.to_ascii_lowercase()))
    else {
        return;
    };
    let Some((_, entity)) = entity_by_uuid(&id) else {
        return;
    };
    let Some(frog) = entity.as_ref().downcast_ref::<FrogEntity>() else {
        return;
    };
    frog.set_variant(variant);
}

extern "system" fn frog_variant<'a>(
    mut env: JNIEnv<'a>,
    _class: JClass<'a>,
    uuid: JString<'a>,
) -> JString<'a> {
    let Ok(text) = env.get_string(&uuid) else {
        return JString::from(JObject::null());
    };
    let Some(id) = text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return JString::from(JObject::null());
    };
    let name = entity_by_uuid(&id)
        .and_then(|(_, entity)| {
            entity
                .as_ref()
                .downcast_ref::<FrogEntity>()
                .map(|frog| frog.variant().key.path.as_ref().to_owned())
        })
        .unwrap_or_default();
    match env.new_string(name) {
        Ok(value) => value,
        Err(_) => JString::from(JObject::null()),
    }
}

const fn panda_gene_name(gene: PandaGene) -> &'static str {
    match gene {
        PandaGene::Normal => "normal",
        PandaGene::Lazy => "lazy",
        PandaGene::Worried => "worried",
        PandaGene::Playful => "playful",
        PandaGene::Brown => "brown",
        PandaGene::Weak => "weak",
        PandaGene::Aggressive => "aggressive",
    }
}

extern "system" fn llama_variant<'a>(
    mut env: JNIEnv<'a>,
    _class: JClass<'a>,
    uuid: JString<'a>,
) -> JString<'a> {
    let Ok(text) = env.get_string(&uuid) else {
        return JString::from(JObject::null());
    };
    let Some(id) = text.to_str().ok().and_then(|v| v.parse().ok()) else {
        return JString::from(JObject::null());
    };
    let value = entity_by_uuid(&id).and_then(|(_, e)| {
        e.as_ref()
            .as_llama()
            .map(|l| format!("{:?}", l.llama_variant()))
    });
    value
        .and_then(|v| env.new_string(v).ok())
        .unwrap_or_else(|| JString::from(JObject::null()))
}
extern "system" fn generate_tree(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    x: jint,
    y: jint,
    z: jint,
    tree_type: JString<'_>,
) -> jboolean {
    let Ok(_world_name) = env.get_string(&name) else {
        return 0;
    };
    let Ok(type_name) = env.get_string(&tree_type) else {
        return 0;
    };
    let Some(world) = world(&mut env, &name) else {
        return 0;
    };
    let key = match type_name.to_str().unwrap_or_default() {
        "TREE" | "BIG_TREE" => "oak",
        "REDWOOD" | "TALL_REDWOOD" | "MEGA_REDWOOD" => "spruce",
        "BIRCH" => "birch",
        "JUNGLE" | "SMALL_JUNGLE" | "COCOA_TREE" | "JUNGLE_BUSH" => "jungle_tree",
        "BROWN_MUSHROOM" => "brown_mushroom",
        "RED_MUSHROOM" => "red_mushroom",
        "ACACIA" => "acacia",
        "DARK_OAK" => "dark_oak",
        "AZALEA" => "azalea_tree",
        "MANGROVE" => "mangrove",
        "CHERRY" => "cherry",
        _ => return 0,
    };
    jboolean::from(grow_tree(&world, BlockPos::new(x, y, z), key))
}

extern "system" fn set_llama_variant(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    variant: JString<'_>,
) {
    let Ok(text) = env.get_string(&uuid) else {
        return;
    };
    let Some(id) = text.to_str().ok().and_then(|v| v.parse().ok()) else {
        return;
    };
    let Ok(value) = env.get_string(&variant) else {
        return;
    };
    let Some(v) = LlamaVariant::ALL
        .into_iter()
        .find(|v| format!("{v:?}").eq_ignore_ascii_case(value.to_str().unwrap_or_default()))
    else {
        return;
    };
    let Some((_, e)) = entity_by_uuid(&id) else {
        return;
    };
    if let Some(l) = e.as_ref().as_llama() {
        l.set_llama_variant(v);
    }
}

extern "system" fn phantom_size(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jint {
    let Ok(text) = env.get_string(&uuid) else {
        return 0;
    };
    let Some(id) = text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return 0;
    };
    entity_by_uuid(&id)
        .and_then(|(_, entity)| {
            entity
                .as_ref()
                .downcast_ref::<PhantomEntity>()
                .map(PhantomEntity::phantom_size)
        })
        .unwrap_or(0)
}

extern "system" fn set_phantom_size(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    size: jint,
) {
    let Ok(text) = env.get_string(&uuid) else {
        return;
    };
    let Some(id) = text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return;
    };
    let Some((_, entity)) = entity_by_uuid(&id) else {
        return;
    };
    if let Some(phantom) = entity.as_ref().downcast_ref::<PhantomEntity>() {
        phantom.set_phantom_size(size);
    }
}

extern "system" fn raider_patrol_leader<'a>(
    mut env: JNIEnv<'a>,
    _class: JClass<'a>,
    uuid: JString<'a>,
) -> jboolean {
    let Ok(text) = env.get_string(&uuid) else {
        return 0;
    };
    let Some(id) = text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return 0;
    };
    jboolean::from(
        entity_by_uuid(&id)
            .and_then(|(_, entity)| {
                entity
                    .as_ref()
                    .as_raider()
                    .map(PatrollingMonster::is_patrol_leader)
            })
            .unwrap_or(false),
    )
}

extern "system" fn set_raider_patrol_leader(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    leader: jboolean,
) {
    let Ok(text) = env.get_string(&uuid) else {
        return;
    };
    let Some(id) = text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return;
    };
    let Some((_, entity)) = entity_by_uuid(&id) else {
        return;
    };
    if let Some(raider) = entity.as_ref().as_raider() {
        raider.set_patrol_leader(leader != 0);
    }
}

extern "system" fn panda_main_gene<'a>(
    mut env: JNIEnv<'a>,
    _class: JClass<'a>,
    uuid: JString<'a>,
) -> JString<'a> {
    let Ok(text) = env.get_string(&uuid) else {
        return JString::from(JObject::null());
    };
    let Some(id) = text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return JString::from(JObject::null());
    };
    let value = entity_by_uuid(&id).and_then(|(_, entity)| {
        entity
            .as_ref()
            .downcast_ref::<PandaEntity>()
            .map(|p| panda_gene_name(p.main_gene()))
    });
    value
        .and_then(|value| env.new_string(value).ok())
        .unwrap_or_else(|| JString::from(JObject::null()))
}

extern "system" fn panda_hidden_gene<'a>(
    mut env: JNIEnv<'a>,
    _class: JClass<'a>,
    uuid: JString<'a>,
) -> JString<'a> {
    let Ok(text) = env.get_string(&uuid) else {
        return JString::from(JObject::null());
    };
    let Some(id) = text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return JString::from(JObject::null());
    };
    let value = entity_by_uuid(&id).and_then(|(_, entity)| {
        entity
            .as_ref()
            .downcast_ref::<PandaEntity>()
            .map(|p| panda_gene_name(p.hidden_gene()))
    });
    value
        .and_then(|value| env.new_string(value).ok())
        .unwrap_or_else(|| JString::from(JObject::null()))
}

fn set_panda_gene(uuid: JString<'_>, env: &mut JNIEnv<'_>, gene: JString<'_>, hidden: bool) {
    let Ok(text) = env.get_string(&uuid) else {
        return;
    };
    let Some(id) = text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return;
    };
    let Ok(value) = env.get_string(&gene) else {
        return;
    };
    let Some(gene) = [
        PandaGene::Normal,
        PandaGene::Lazy,
        PandaGene::Worried,
        PandaGene::Playful,
        PandaGene::Brown,
        PandaGene::Weak,
        PandaGene::Aggressive,
    ]
    .into_iter()
    .find(|g| panda_gene_name(*g).eq_ignore_ascii_case(value.to_str().unwrap_or_default())) else {
        return;
    };
    let Some((_, entity)) = entity_by_uuid(&id) else {
        return;
    };
    let Some(panda) = entity.as_ref().downcast_ref::<PandaEntity>() else {
        return;
    };
    if hidden {
        panda.set_hidden_gene(gene);
    } else {
        panda.set_main_gene(gene);
    }
}
extern "system" fn set_panda_main_gene(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    gene: JString<'_>,
) {
    set_panda_gene(uuid, &mut env, gene, false);
}
extern "system" fn set_panda_hidden_gene(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    gene: JString<'_>,
) {
    set_panda_gene(uuid, &mut env, gene, true);
}

extern "system" fn cat_variant<'a>(
    mut env: JNIEnv<'a>,
    _class: JClass<'a>,
    uuid: JString<'a>,
) -> JString<'a> {
    let Ok(text) = env.get_string(&uuid) else {
        return JString::from(JObject::null());
    };
    let Some(id) = text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return JString::from(JObject::null());
    };
    let value = entity_by_uuid(&id).and_then(|(_, entity)| {
        entity
            .as_ref()
            .downcast_ref::<CatEntity>()
            .map(|cat| cat.variant().key.path.as_ref().to_owned())
    });
    value
        .and_then(|value| env.new_string(value).ok())
        .unwrap_or_else(|| JString::from(JObject::null()))
}

extern "system" fn set_cat_variant(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    variant: JString<'_>,
) {
    let Ok(text) = env.get_string(&uuid) else {
        return;
    };
    let Some(id) = text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return;
    };
    let Ok(value) = env.get_string(&variant) else {
        return;
    };
    let Some(entry) = REGISTRY.cat_variants.by_key(&Identifier::vanilla(
        value.to_str().unwrap_or_default().to_ascii_lowercase(),
    )) else {
        return;
    };
    let Some((_, entity)) = entity_by_uuid(&id) else {
        return;
    };
    let Some(cat) = entity.as_ref().downcast_ref::<CatEntity>() else {
        return;
    };
    cat.set_variant(entry);
}

extern "system" fn cat_sitting(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jboolean {
    let Ok(text): Result<String, _> = env.get_string(&uuid).map(Into::into) else {
        return 0;
    };
    let Ok(id) = text.parse() else {
        return 0;
    };
    if let Some((_, entity)) = entity_by_uuid(&id)
        && let Some(cat) = entity.as_ref().downcast_ref::<CatEntity>()
    {
        return cat.is_in_sitting_pose().into();
    }
    0
}

extern "system" fn set_cat_sitting(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    sitting: jboolean,
) {
    let Ok(text): Result<String, _> = env.get_string(&uuid).map(Into::into) else {
        return;
    };
    let Ok(id) = text.parse() else {
        return;
    };
    if let Some((_, entity)) = entity_by_uuid(&id)
        && let Some(cat) = entity.as_ref().downcast_ref::<CatEntity>()
    {
        cat.set_in_sitting_pose(sitting != 0);
    }
}

extern "system" fn end_crystal_shows_bottom(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jboolean {
    let Ok(text): Result<String, _> = env.get_string(&uuid).map(Into::into) else {
        return 0;
    };
    let Ok(id) = text.parse() else {
        return 0;
    };
    if let Some((_, entity)) = entity_by_uuid(&id)
        && let Some(crystal) = entity.as_ref().downcast_ref::<EndCrystalEntity>()
    {
        return crystal.shows_bottom().into();
    }
    0
}

extern "system" fn set_end_crystal_shows_bottom(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    showing: jboolean,
) {
    let Ok(text): Result<String, _> = env.get_string(&uuid).map(Into::into) else {
        return;
    };
    let Ok(id) = text.parse() else {
        return;
    };
    if let Some((_, entity)) = entity_by_uuid(&id)
        && let Some(crystal) = entity.as_ref().downcast_ref::<EndCrystalEntity>()
    {
        crystal.set_show_bottom(showing != 0);
    }
}

extern "system" fn cat_collar_color(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jint {
    let Ok(text): Result<String, _> = env.get_string(&uuid).map(Into::into) else {
        return 0;
    };
    let Ok(id) = text.parse() else {
        return 0;
    };
    if let Some((_, entity)) = entity_by_uuid(&id)
        && let Some(cat) = entity.as_ref().downcast_ref::<CatEntity>()
    {
        return cat.collar_color().id() as jint;
    }
    0
}

extern "system" fn set_cat_collar_color(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    value: jint,
) {
    let Ok(text): Result<String, _> = env.get_string(&uuid).map(Into::into) else {
        return;
    };
    let Ok(id) = text.parse() else {
        return;
    };
    if let Some(color) = foton_registry::DyeColor::VALUES
        .get(value as usize)
        .copied()
        && let Some((_, entity)) = entity_by_uuid(&id)
        && let Some(cat) = entity.as_ref().downcast_ref::<CatEntity>()
    {
        cat.set_collar_color(color);
    }
}

extern "system" fn armor_stand_set_arms(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    value: jboolean,
) {
    let Ok(text): Result<String, _> = env.get_string(&uuid).map(Into::into) else {
        return;
    };
    let Ok(id) = text.parse() else {
        return;
    };
    if let Some((_, entity)) = entity_by_uuid(&id)
        && let Some(stand) = entity.as_ref().downcast_ref::<ArmorStandEntity>()
    {
        stand.set_show_arms(value != 0);
    }
}

extern "system" fn entity_can_breed(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jboolean {
    let Ok(text): Result<String, _> = env.get_string(&uuid).map(Into::into) else {
        return 0;
    };
    let Ok(id) = text.parse() else {
        return 0;
    };
    let Some((_, entity)) = entity_by_uuid(&id) else {
        return 0;
    };
    let Some(ageable) = entity.as_ref().as_ageable_mob() else {
        return 0;
    };
    (ageable.get_age() == 0).into()
}

extern "system" fn set_entity_breed(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    breed: jboolean,
) {
    let Ok(text): Result<String, _> = env.get_string(&uuid).map(Into::into) else {
        return;
    };
    let Ok(id) = text.parse() else {
        return;
    };
    let Some((_, entity)) = entity_by_uuid(&id) else {
        return;
    };
    let Some(ageable) = entity.as_ref().as_ageable_mob() else {
        return;
    };
    if breed != 0 {
        ageable.set_age(0);
    } else if ageable.get_age() >= 0 {
        ageable.set_age(6000);
    }
}

extern "system" fn animal_breed_cause(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jstring {
    let Ok(text): Result<String, _> = env.get_string(&uuid).map(Into::into) else {
        return null_mut();
    };
    let Ok(id) = text.parse() else {
        return null_mut();
    };
    let value = entity_by_uuid(&id)
        .and_then(|(_, entity)| {
            entity
                .as_ref()
                .as_animal()
                .and_then(Animal::love_cause_uuid)
        })
        .map(|cause| cause.to_string());
    to_java(&mut env, value)
}

extern "system" fn set_animal_breed_cause(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    cause: JString<'_>,
) {
    let Ok(uuid_text): Result<String, _> = env.get_string(&uuid).map(Into::into) else {
        return;
    };
    let Ok(id) = uuid_text.parse() else {
        return;
    };
    let Ok(cause_text): Result<String, _> = env.get_string(&cause).map(Into::into) else {
        return;
    };
    let cause = if cause_text.is_empty() {
        None
    } else {
        let Ok(cause) = cause_text.parse() else {
            return;
        };
        Some(cause)
    };
    let Some((_, entity)) = entity_by_uuid(&id) else {
        return;
    };
    let Some(animal) = entity.as_ref().as_animal() else {
        return;
    };
    animal.set_love_cause_uuid(cause);
}

extern "system" fn animal_love_ticks(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jint {
    let Ok(text): Result<String, _> = env.get_string(&uuid).map(Into::into) else {
        return 0;
    };
    let Ok(id) = text.parse() else {
        return 0;
    };
    entity_by_uuid(&id)
        .and_then(|(_, entity)| entity.as_ref().as_animal().map(Animal::in_love_time))
        .unwrap_or(0)
}

extern "system" fn set_animal_love_ticks(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    ticks: jint,
) {
    let Ok(text): Result<String, _> = env.get_string(&uuid).map(Into::into) else {
        return;
    };
    let Ok(id) = text.parse() else {
        return;
    };
    let Some((_, entity)) = entity_by_uuid(&id) else {
        return;
    };
    let Some(animal) = entity.as_ref().as_animal() else {
        return;
    };
    animal.set_in_love_time(ticks);
}

extern "system" fn animal_is_breed_item(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    item: JString<'_>,
) -> jboolean {
    let Ok(uuid_text): Result<String, _> = env.get_string(&uuid).map(Into::into) else {
        return 0;
    };
    let Ok(id) = uuid_text.parse() else {
        return 0;
    };
    let Ok(item_text): Result<String, _> = env.get_string(&item).map(Into::into) else {
        return 0;
    };
    let Ok(item_key) = Identifier::from_str(&item_text) else {
        return 0;
    };
    let Some(entity) = animal_entity_by_uuid(&id) else {
        return 0;
    };
    let Some(animal) = entity.as_ref().as_animal() else {
        return 0;
    };
    let Some(registry) = REGISTRY.get() else {
        return 0;
    };
    let Some(item) = registry.items.by_key(&item_key) else {
        return 0;
    };
    animal.is_food(&ItemStack::new(item)).into()
}

extern "system" fn cow_variant(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jstring {
    let Ok(text): Result<String, _> = env.get_string(&uuid).map(Into::into) else {
        return null_mut();
    };
    let Ok(id) = text.parse() else {
        return null_mut();
    };
    let value = entity_by_uuid(&id).and_then(|(_, entity)| {
        entity
            .as_ref()
            .downcast_ref::<CowEntity>()
            .map(|cow| cow.variant().key.to_string())
    });
    to_java(&mut env, value)
}

extern "system" fn set_cow_variant(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    variant: JString<'_>,
) {
    let Ok(uuid_text): Result<String, _> = env.get_string(&uuid).map(Into::into) else {
        return;
    };
    let Ok(id) = uuid_text.parse() else {
        return;
    };
    let Ok(variant_text): Result<String, _> = env.get_string(&variant).map(Into::into) else {
        return;
    };
    let Ok(key) = Identifier::from_str(&variant_text) else {
        return;
    };
    let Some((_, entity)) = entity_by_uuid(&id) else {
        return;
    };
    let Some(cow) = entity.as_ref().downcast_ref::<CowEntity>() else {
        return;
    };
    let Some(registry) = REGISTRY.get() else {
        return;
    };
    let Some(variant) = registry.cow_variants.by_key(&key) else {
        return;
    };
    cow.set_variant(variant);
}

extern "system" fn cow_sound_variant(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jstring {
    let Ok(text): Result<String, _> = env.get_string(&uuid).map(Into::into) else {
        return null_mut();
    };
    let Ok(id) = text.parse() else {
        return null_mut();
    };
    let value = entity_by_uuid(&id).and_then(|(_, entity)| {
        entity
            .as_ref()
            .downcast_ref::<CowEntity>()
            .map(|cow| cow.sound_variant().key.to_string())
    });
    to_java(&mut env, value)
}

extern "system" fn set_cow_sound_variant(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    variant: JString<'_>,
) {
    let Ok(uuid_text): Result<String, _> = env.get_string(&uuid).map(Into::into) else {
        return;
    };
    let Ok(id) = uuid_text.parse() else {
        return;
    };
    let Ok(variant_text): Result<String, _> = env.get_string(&variant).map(Into::into) else {
        return;
    };
    let Ok(key) = Identifier::from_str(&variant_text) else {
        return;
    };
    let Some((_, entity)) = entity_by_uuid(&id) else {
        return;
    };
    let Some(cow) = entity.as_ref().downcast_ref::<CowEntity>() else {
        return;
    };
    let Some(registry) = REGISTRY.get() else {
        return;
    };
    let Some(variant) = registry.cow_sound_variants.by_key(&key) else {
        return;
    };
    cow.set_sound_variant(variant);
}

extern "system" fn bee_anger(mut env: JNIEnv<'_>, _class: JClass<'_>, uuid: JString<'_>) -> jint {
    let Ok(text): Result<String, _> = env.get_string(&uuid).map(Into::into) else {
        return 0;
    };
    let Ok(id) = text.parse() else {
        return 0;
    };
    let Some((_, entity)) = entity_by_uuid(&id) else {
        return 0;
    };
    let Some(bee) = entity.as_ref().downcast_ref::<BeeEntity>() else {
        return 0;
    };
    let remaining =
        bee.persistent_anger_end_time() - bee.level().map_or(0, |world| world.game_time());
    remaining.max(0).min(i64::from(i32::MAX)) as jint
}

extern "system" fn set_bee_anger(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    anger: jint,
) {
    let Ok(text): Result<String, _> = env.get_string(&uuid).map(Into::into) else {
        return;
    };
    let Ok(id) = text.parse() else {
        return;
    };
    let Some((_, entity)) = entity_by_uuid(&id) else {
        return;
    };
    let Some(bee) = entity.as_ref().downcast_ref::<BeeEntity>() else {
        return;
    };
    bee.set_time_to_remain_angry(i64::from(anger.max(0)));
}

extern "system" fn bee_has_nectar(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jboolean {
    let Ok(text): Result<String, _> = env.get_string(&uuid).map(Into::into) else {
        return 0;
    };
    let Ok(id) = text.parse() else {
        return 0;
    };
    if let Some((_, entity)) = entity_by_uuid(&id)
        && let Some(bee) = entity.as_ref().downcast_ref::<BeeEntity>()
    {
        return bee.has_nectar().into();
    }
    0
}

extern "system" fn set_bee_has_nectar(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    value: jboolean,
) {
    let Ok(text): Result<String, _> = env.get_string(&uuid).map(Into::into) else {
        return;
    };
    let Ok(id) = text.parse() else {
        return;
    };
    if let Some((_, entity)) = entity_by_uuid(&id)
        && let Some(bee) = entity.as_ref().downcast_ref::<BeeEntity>()
    {
        bee.set_has_nectar(value != 0);
    }
}

extern "system" fn bee_has_stung(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jboolean {
    let Ok(text): Result<String, _> = env.get_string(&uuid).map(Into::into) else {
        return 0;
    };
    let Ok(id) = text.parse() else {
        return 0;
    };
    if let Some((_, entity)) = entity_by_uuid(&id)
        && let Some(bee) = entity.as_ref().downcast_ref::<BeeEntity>()
    {
        return bee.has_stung().into();
    }
    0
}

extern "system" fn set_bee_has_stung(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    value: jboolean,
) {
    let Ok(text): Result<String, _> = env.get_string(&uuid).map(Into::into) else {
        return;
    };
    let Ok(id) = text.parse() else {
        return;
    };
    if let Some((_, entity)) = entity_by_uuid(&id)
        && let Some(bee) = entity.as_ref().downcast_ref::<BeeEntity>()
    {
        bee.set_has_stung(value != 0);
    }
}

extern "system" fn horse_temper(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jint {
    let Ok(text): Result<String, _> = env.get_string(&uuid).map(Into::into) else {
        return 0;
    };
    let Ok(id) = text.parse() else {
        return 0;
    };
    if let Some((_, entity)) = entity_by_uuid(&id)
        && let Some(horse) = entity.as_abstract_horse()
    {
        return horse.temper();
    }
    0
}

extern "system" fn set_horse_temper(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    value: jint,
) {
    let Ok(text): Result<String, _> = env.get_string(&uuid).map(Into::into) else {
        return;
    };
    let Ok(id) = text.parse() else {
        return;
    };
    if let Some((_, entity)) = entity_by_uuid(&id)
        && let Some(horse) = entity.as_abstract_horse()
    {
        horse.set_temper(value);
    }
}

extern "system" fn horse_max_temper(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jint {
    let Ok(text): Result<String, _> = env.get_string(&uuid).map(Into::into) else {
        return 0;
    };
    let Ok(id) = text.parse() else {
        return 0;
    };
    if let Some((_, entity)) = entity_by_uuid(&id)
        && let Some(horse) = entity.as_abstract_horse()
    {
        return horse.max_temper();
    }
    0
}

extern "system" fn wolf_sitting(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jboolean {
    let Ok(text) = env.get_string(&uuid) else {
        return 0;
    };
    let Some(id) = text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return 0;
    };
    jboolean::from(
        entity_by_uuid(&id)
            .and_then(|(_, entity)| {
                entity
                    .downcast_ref::<WolfEntity>()
                    .map(TamableAnimal::is_in_sitting_pose)
            })
            .unwrap_or(false),
    )
}
extern "system" fn set_wolf_sitting(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    value: jboolean,
) {
    let Ok(text) = env.get_string(&uuid) else {
        return;
    };
    let Some(id) = text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return;
    };
    if let Some((_, entity)) = entity_by_uuid(&id)
        && let Some(wolf) = entity.downcast_ref::<WolfEntity>()
    {
        wolf.set_in_sitting_pose(value != 0);
    }
}

extern "system" fn wolf_collar_color(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jint {
    let Ok(text) = env.get_string(&uuid) else {
        return 0;
    };
    let Some(id) = text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return 0;
    };
    entity_by_uuid(&id)
        .and_then(|(_, entity)| {
            entity
                .downcast_ref::<WolfEntity>()
                .map(|wolf| wolf.collar_color().id() as jint)
        })
        .unwrap_or(0)
}
extern "system" fn set_wolf_collar_color(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    value: jint,
) {
    let Ok(text) = env.get_string(&uuid) else {
        return;
    };
    let Some(id) = text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return;
    };
    if let Some(color) = foton_registry::DyeColor::VALUES
        .get(value as usize)
        .copied()
        && let Some((_, entity)) = entity_by_uuid(&id)
        && let Some(wolf) = entity.downcast_ref::<WolfEntity>()
    {
        wolf.set_collar_color(color);
    }
}

extern "system" fn wolf_variant<'a>(
    mut env: JNIEnv<'a>,
    _class: JClass<'a>,
    uuid: JString<'a>,
) -> JString<'a> {
    let Ok(text) = env.get_string(&uuid) else {
        return JString::from(JObject::null());
    };
    let Some(id) = text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return JString::from(JObject::null());
    };
    let value = entity_by_uuid(&id).and_then(|(_, entity)| {
        entity
            .as_ref()
            .downcast_ref::<WolfEntity>()
            .map(|wolf| wolf.variant().key.path.as_ref().to_owned())
    });
    value
        .and_then(|value| env.new_string(value).ok())
        .unwrap_or_else(|| JString::from(JObject::null()))
}

extern "system" fn set_wolf_variant(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    variant: JString<'_>,
) {
    let Ok(text) = env.get_string(&uuid) else {
        return;
    };
    let Some(id) = text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return;
    };
    let Ok(value) = env.get_string(&variant) else {
        return;
    };
    let Some(entry) = REGISTRY.wolf_variants.by_key(&Identifier::vanilla(
        value.to_str().unwrap_or_default().to_ascii_lowercase(),
    )) else {
        return;
    };
    let Some((_, entity)) = entity_by_uuid(&id) else {
        return;
    };
    let Some(wolf) = entity.as_ref().downcast_ref::<WolfEntity>() else {
        return;
    };
    wolf.set_variant(entry);
}

extern "system" fn horse_variant<'a>(
    mut env: JNIEnv<'a>,
    _class: JClass<'a>,
    uuid: JString<'a>,
) -> JString<'a> {
    let Ok(text) = env.get_string(&uuid) else {
        return JString::from(JObject::null());
    };
    let Some(id) = text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return JString::from(JObject::null());
    };
    let value = entity_by_uuid(&id).and_then(|(_, entity)| {
        entity
            .as_ref()
            .downcast_ref::<HorseEntity>()
            .map(|horse| format!("{:?}", horse.variant()))
    });
    value
        .and_then(|value| env.new_string(value).ok())
        .unwrap_or_else(|| JString::from(JObject::null()))
}

extern "system" fn set_horse_variant(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    variant: JString<'_>,
) {
    let Ok(text) = env.get_string(&uuid) else {
        return;
    };
    let Some(id) = text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return;
    };
    let Ok(value) = env.get_string(&variant) else {
        return;
    };
    let Some(coat) = HorseVariant::ALL
        .into_iter()
        .find(|coat| format!("{coat:?}").eq_ignore_ascii_case(value.to_str().unwrap_or_default()))
    else {
        return;
    };
    let Some((_, entity)) = entity_by_uuid(&id) else {
        return;
    };
    let Some(horse) = entity.as_ref().downcast_ref::<HorseEntity>() else {
        return;
    };
    horse.set_variant(coat);
}

extern "system" fn horse_markings<'a>(
    mut env: JNIEnv<'a>,
    _class: JClass<'a>,
    uuid: JString<'a>,
) -> JString<'a> {
    let Ok(text) = env.get_string(&uuid) else {
        return JString::from(JObject::null());
    };
    let Some(id) = text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return JString::from(JObject::null());
    };
    let value = entity_by_uuid(&id)
        .and_then(|(_, entity)| {
            entity
                .as_ref()
                .downcast_ref::<HorseEntity>()
                .map(HorseEntity::markings)
        })
        .map(|markings| format!("{markings:?}"));
    value
        .and_then(|value| env.new_string(value).ok())
        .unwrap_or_else(|| JString::from(JObject::null()))
}

extern "system" fn set_horse_markings(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    markings: JString<'_>,
) {
    let Ok(text) = env.get_string(&uuid) else {
        return;
    };
    let Some(id) = text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return;
    };
    let Ok(markings) = env.get_string(&markings) else {
        return;
    };
    let Some(value) = HorseMarkings::ALL.into_iter().find(|value| {
        format!("{value:?}").eq_ignore_ascii_case(markings.to_str().unwrap_or_default())
    }) else {
        return;
    };
    let Some((_, entity)) = entity_by_uuid(&id) else {
        return;
    };
    let Some(horse) = entity.as_ref().downcast_ref::<HorseEntity>() else {
        return;
    };
    horse.set_variant_and_markings(horse.variant(), value);
}

extern "system" fn mushroom_cow_variant<'a>(
    mut env: JNIEnv<'a>,
    _class: JClass<'a>,
    uuid: JString<'a>,
) -> JString<'a> {
    let Ok(text) = env.get_string(&uuid) else {
        return JString::from(JObject::null());
    };
    let Some(id) = text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return JString::from(JObject::null());
    };
    let name = entity_by_uuid(&id)
        .and_then(|(_, entity)| {
            entity
                .as_ref()
                .downcast_ref::<MushroomCowEntity>()
                .map(|cow| cow.variant().serialized_name())
        })
        .unwrap_or("");
    match env.new_string(name) {
        Ok(value) => value,
        Err(_) => JString::from(JObject::null()),
    }
}

extern "system" fn set_block_display_block(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    state: JString<'_>,
) {
    let Ok(uuid_text) = env.get_string(&uuid) else {
        return;
    };
    let Ok(state_text) = env.get_string(&state) else {
        return;
    };
    let Some(id) = uuid_text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return;
    };
    let Some(block_state) = parse_state(state_text.to_str().unwrap_or_default()) else {
        return;
    };
    let Some((_, entity)) = entity_by_uuid(&id) else {
        return;
    };
    if let Some(display) = entity.as_ref().downcast_ref::<BlockDisplayEntity>() {
        display.set_block_state_id(block_state);
    }
}

fn boat_variant(name: &str) -> Option<(EntityTypeRef, bool)> {
    Some(match name.to_ascii_uppercase().as_str() {
        "OAK" => (&vanilla_entities::OAK_BOAT, false),
        "SPRUCE" => (&vanilla_entities::SPRUCE_BOAT, false),
        "BIRCH" => (&vanilla_entities::BIRCH_BOAT, false),
        "JUNGLE" => (&vanilla_entities::JUNGLE_BOAT, false),
        "ACACIA" => (&vanilla_entities::ACACIA_BOAT, false),
        "DARK_OAK" => (&vanilla_entities::DARK_OAK_BOAT, false),
        "MANGROVE" => (&vanilla_entities::MANGROVE_BOAT, false),
        "CHERRY" => (&vanilla_entities::CHERRY_BOAT, false),
        "PALE_OAK" => (&vanilla_entities::PALE_OAK_BOAT, false),
        "BAMBOO" => (&vanilla_entities::BAMBOO_RAFT, true),
        _ => return None,
    })
}

extern "system" fn boat_type(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jstring {
    let Ok(text): Result<String, _> = env.get_string(&uuid).map(Into::into) else {
        return null_mut();
    };
    let Ok(id) = text.parse() else {
        return null_mut();
    };
    let Some((_, entity)) = entity_by_uuid(&id) else {
        return null_mut();
    };
    let key = entity.entity_type().key.path.to_ascii_uppercase();
    let key = key
        .strip_suffix("_BOAT")
        .or_else(|| key.strip_suffix("_RAFT"));
    to_java(&mut env, key.map(str::to_owned))
}

extern "system" fn set_boat_type(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    kind: JString<'_>,
) {
    let Ok(uuid_text): Result<String, _> = env.get_string(&uuid).map(Into::into) else {
        return;
    };
    let Ok(kind_text): Result<String, _> = env.get_string(&kind).map(Into::into) else {
        return;
    };
    let Ok(id) = uuid_text.parse() else {
        return;
    };
    let Some((target, raft)) = boat_variant(&kind_text) else {
        return;
    };
    let Some((_, old)) = entity_by_uuid(&id) else {
        return;
    };
    if old.entity_type().key.path == target.key.path {
        return;
    }
    if raft {
        if old.as_ref().downcast_ref::<RaftEntity>().is_some() {
            let _ = replace_entity(&old, ConversionReason::Unknown, |new_id, pos, weak| {
                RaftEntity::new(target, new_id, pos, weak)
            });
        }
    } else if old.as_ref().downcast_ref::<BoatEntity>().is_some() {
        let _ = replace_entity(&old, ConversionReason::Unknown, |new_id, pos, weak| {
            BoatEntity::new(target, new_id, pos, weak)
        });
    }
}

extern "system" fn entity_type(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jstring {
    let text: String = match env.get_string(&uuid) {
        Ok(v) => v.into(),
        Err(_) => return to_java(&mut env, None),
    };
    let Some(id) = Uuid::parse_str(&text).ok() else {
        return to_java(&mut env, None);
    };
    to_java(
        &mut env,
        entity_by_uuid(&id).map(|(_, entity)| entity.entity_type().key.path.to_string()),
    )
}

extern "system" fn entity_spawn_category(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jstring {
    let Ok(text) = env.get_string(&uuid) else {
        return null_mut();
    };
    let Ok(id) = Uuid::parse_str(match text.to_str() {
        Ok(value) => value,
        Err(_) => return null_mut(),
    }) else {
        return null_mut();
    };
    let Some((_world, entity)) = entity_by_uuid(&id) else {
        return null_mut();
    };
    let category = format!("{:?}", entity.entity_type().mob_category);
    to_java(&mut env, Some(category))
}

extern "system" fn entity_spawn_reason(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jstring {
    let Ok(text) = env.get_string(&uuid) else {
        return null_mut();
    };
    let Ok(id) = Uuid::parse_str(match text.to_str() {
        Ok(value) => value,
        Err(_) => return null_mut(),
    }) else {
        return null_mut();
    };
    let entity = entity_by_uuid(&id);
    let reason = entity_spawn_reason_state(entity.as_ref().map(|(_, entity)| entity.as_ref()));
    to_java(&mut env, Some(reason.as_str().to_owned()))
}

fn entity_spawn_reason_state(entity: Option<&dyn Entity>) -> PluginSpawnReason {
    entity
        .and_then(|entity| entity.base().plugin_spawn_reason())
        .unwrap_or_default()
}

extern "system" fn entity_position(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jdoubleArray {
    let text: String = match env.get_string(&uuid) {
        Ok(v) => v.into(),
        Err(_) => return to_position(&mut env, None),
    };
    let Some(id) = Uuid::parse_str(&text).ok() else {
        return to_position(&mut env, None);
    };
    let entity = entity_by_uuid(&id);
    to_position(
        &mut env,
        entity_position_state(entity.as_ref().map(|(_, entity)| entity.as_ref())),
    )
}

fn entity_position_state(entity: Option<&dyn Entity>) -> Option<[f64; 5]> {
    entity.map(|entity| {
        let (position, (yaw, pitch)) = entity.base().position_and_rotation();
        [
            position.x,
            position.y,
            position.z,
            f64::from(yaw),
            f64::from(pitch),
        ]
    })
}

fn entity_scoreboard_tags_state(entity: Option<&dyn Entity>) -> Option<Vec<String>> {
    entity.map(Entity::tags)
}

fn add_entity_scoreboard_tag_state(entity: Option<&dyn Entity>, tag: String) -> bool {
    entity.is_some_and(|entity| entity.add_tag(tag))
}

fn remove_entity_scoreboard_tag_state(entity: Option<&dyn Entity>, tag: &str) -> bool {
    entity.is_some_and(|entity| entity.remove_tag(tag))
}

fn entity_has_gravity_state(entity: Option<&dyn Entity>) -> bool {
    entity.is_some_and(|entity| !entity.is_no_gravity())
}

fn set_entity_gravity_state(entity: Option<&dyn Entity>, gravity: bool) {
    if let Some(entity) = entity {
        entity.set_no_gravity(!gravity);
    }
}

fn entity_silent_state(entity: Option<&dyn Entity>) -> bool {
    entity.is_some_and(Entity::is_silent)
}

fn set_entity_silent_state(entity: Option<&dyn Entity>, silent: bool) {
    if let Some(entity) = entity {
        entity.set_silent(silent);
    }
}

fn set_entity_rotation_state(entity: Option<&dyn Entity>, yaw: f32, pitch: f32) -> bool {
    if !yaw.is_finite() || !pitch.is_finite() {
        return false;
    }
    let Some(entity) = entity else {
        return false;
    };
    entity.set_rotation((yaw, pitch));
    if let Some(living) = entity.as_living_entity() {
        living.set_y_head_rot(yaw);
    }
    true
}

fn entity_in_rain_state(entity: Option<&dyn Entity>) -> bool {
    entity.is_some_and(Entity::is_in_rain)
}

extern "system" fn entity_scoreboard_tags(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jobjectArray {
    let entity = entity_handle(&mut env, &uuid);
    let Some(tags) =
        entity_scoreboard_tags_state(entity.as_ref().map(|(_, entity)| entity.as_ref()))
    else {
        return null_mut();
    };
    string_array(&mut env, &tags)
}

extern "system" fn add_entity_scoreboard_tag(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    tag: JString<'_>,
) -> jboolean {
    let Some((_, entity)) = entity_handle(&mut env, &uuid) else {
        return 0;
    };
    let Ok(tag): Result<String, _> = env.get_string(&tag).map(Into::into) else {
        return 0;
    };
    jboolean::from(add_entity_scoreboard_tag_state(Some(entity.as_ref()), tag))
}

extern "system" fn remove_entity_scoreboard_tag(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    tag: JString<'_>,
) -> jboolean {
    let Some((_, entity)) = entity_handle(&mut env, &uuid) else {
        return 0;
    };
    let Ok(tag): Result<String, _> = env.get_string(&tag).map(Into::into) else {
        return 0;
    };
    jboolean::from(remove_entity_scoreboard_tag_state(
        Some(entity.as_ref()),
        &tag,
    ))
}

extern "system" fn entity_has_gravity(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jboolean {
    let entity = entity_handle(&mut env, &uuid);
    jboolean::from(entity_has_gravity_state(
        entity.as_ref().map(|(_, entity)| entity.as_ref()),
    ))
}

extern "system" fn set_entity_gravity(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    gravity: jboolean,
) {
    let entity = entity_handle(&mut env, &uuid);
    set_entity_gravity_state(
        entity.as_ref().map(|(_, entity)| entity.as_ref()),
        gravity != 0,
    );
}

extern "system" fn entity_silent(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jboolean {
    let entity = entity_handle(&mut env, &uuid);
    jboolean::from(entity_silent_state(
        entity.as_ref().map(|(_, entity)| entity.as_ref()),
    ))
}

extern "system" fn set_entity_silent(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    silent: jboolean,
) {
    let entity = entity_handle(&mut env, &uuid);
    set_entity_silent_state(
        entity.as_ref().map(|(_, entity)| entity.as_ref()),
        silent != 0,
    );
}

extern "system" fn set_entity_rotation(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    yaw: jfloat,
    pitch: jfloat,
) {
    let entity = entity_handle(&mut env, &uuid);
    set_entity_rotation_state(
        entity.as_ref().map(|(_, entity)| entity.as_ref()),
        yaw,
        pitch,
    );
}

extern "system" fn entity_in_rain(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jboolean {
    let Some((_, entity)) = entity_handle(&mut env, &uuid) else {
        return 0;
    };
    jboolean::from(entity_in_rain_state(Some(entity.as_ref())))
}

extern "system" fn entity_origin(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jdoubleArray {
    let text: String = match env.get_string(&uuid) {
        Ok(value) => value.into(),
        Err(_) => return to_position(&mut env, None),
    };
    let Some(id) = Uuid::parse_str(&text).ok() else {
        return to_position(&mut env, None);
    };
    to_position(
        &mut env,
        entity_by_uuid(&id).and_then(|(_, entity)| {
            entity.downcast_ref::<FallingBlockEntity>().map(|falling| {
                let origin = falling.start_pos();
                [
                    f64::from(origin.x()) + 0.5,
                    f64::from(origin.y()),
                    f64::from(origin.z()) + 0.5,
                    0.0,
                    0.0,
                ]
            })
        }),
    )
}

extern "system" fn entity_bounding_box(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jdoubleArray {
    let Ok(text) = env.get_string(&uuid) else {
        return null_mut();
    };
    let Some(id) = text.to_str().ok().and_then(|v| Uuid::parse_str(v).ok()) else {
        return null_mut();
    };
    let Some((_, entity)) = entity_by_uuid(&id) else {
        return null_mut();
    };
    let bounds = entity.bounding_box();
    let values = [
        bounds.min_x(),
        bounds.min_y(),
        bounds.min_z(),
        bounds.max_x(),
        bounds.max_y(),
        bounds.max_z(),
    ];
    let Ok(array) = env.new_double_array(6) else {
        return null_mut();
    };
    if env.set_double_array_region(&array, 0, &values).is_err() {
        return null_mut();
    }
    array.into_raw()
}

extern "system" fn entity_portal_cooldown(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jint {
    let Ok(text) = env.get_string(&uuid) else {
        return 0;
    };
    let Ok(id) = Uuid::parse_str(text.to_str().unwrap_or_default()) else {
        return 0;
    };
    entity_by_uuid(&id).map_or(0, |(_, entity)| entity.base().portal_cooldown())
}

extern "system" fn set_entity_portal_cooldown(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    ticks: jint,
) {
    let Ok(text) = env.get_string(&uuid) else {
        return;
    };
    let Ok(id) = Uuid::parse_str(text.to_str().unwrap_or_default()) else {
        return;
    };
    if let Some((_, entity)) = entity_by_uuid(&id) {
        entity.base().set_portal_cooldown(ticks.max(0));
    }
}

extern "system" fn entity_glowing(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jboolean {
    let Ok(text) = env.get_string(&uuid) else {
        return 0;
    };
    let Ok(id) = Uuid::parse_str(text.to_str().unwrap_or_default()) else {
        return 0;
    };
    jboolean::from(entity_by_uuid(&id).is_some_and(|(_, entity)| entity.has_glowing_tag()))
}

extern "system" fn set_entity_glowing(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    glowing: jboolean,
) {
    let Ok(text) = env.get_string(&uuid) else {
        return;
    };
    let Ok(id) = Uuid::parse_str(text.to_str().unwrap_or_default()) else {
        return;
    };
    if let Some((_, entity)) = entity_by_uuid(&id) {
        entity.set_glowing_tag(glowing != 0);
    }
}

extern "system" fn entity_invulnerable(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jboolean {
    let Ok(text) = env.get_string(&uuid) else {
        return 0;
    };
    let Ok(id) = Uuid::parse_str(text.to_str().unwrap_or_default()) else {
        return 0;
    };
    jboolean::from(entity_by_uuid(&id).is_some_and(|(_, entity)| entity.is_invulnerable()))
}

extern "system" fn set_entity_invulnerable(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    invulnerable: jboolean,
) {
    let Ok(text) = env.get_string(&uuid) else {
        return;
    };
    let Ok(id) = Uuid::parse_str(text.to_str().unwrap_or_default()) else {
        return;
    };
    if let Some((_, entity)) = entity_by_uuid(&id) {
        entity.set_invulnerable(invulnerable != 0);
    }
}

extern "system" fn entity_on_ground(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jboolean {
    let Ok(text) = env.get_string(&uuid) else {
        return 0;
    };
    let Ok(id) = Uuid::parse_str(text.to_str().unwrap_or_default()) else {
        return 0;
    };
    jboolean::from(entity_by_uuid(&id).is_some_and(|(_, entity)| entity.on_ground()))
}

extern "system" fn entity_in_water(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jboolean {
    let Ok(text) = env.get_string(&uuid) else {
        return 0;
    };
    let Ok(id) = Uuid::parse_str(text.to_str().unwrap_or_default()) else {
        return 0;
    };
    jboolean::from(entity_by_uuid(&id).is_some_and(|(_, entity)| entity.is_in_water()))
}

extern "system" fn entity_invisible(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jboolean {
    let Ok(text) = env.get_string(&uuid) else {
        return jboolean::from(false);
    };
    let Some(id) = text.to_str().ok().and_then(|v| Uuid::parse_str(v).ok()) else {
        return jboolean::from(false);
    };
    jboolean::from(entity_by_uuid(&id).is_some_and(|(_, entity)| entity.is_invisible()))
}

extern "system" fn entity_freeze_ticks(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jint {
    let Ok(text) = env.get_string(&uuid) else {
        return 0;
    };
    let Ok(id) = Uuid::parse_str(text.to_str().unwrap_or_default()) else {
        return 0;
    };
    entity_by_uuid(&id)
        .and_then(|(_, entity)| entity.as_living_entity().map(Entity::ticks_frozen))
        .unwrap_or(0)
}

extern "system" fn set_entity_freeze_ticks(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    ticks: jint,
) {
    let Ok(text) = env.get_string(&uuid) else {
        return;
    };
    let Ok(id) = Uuid::parse_str(text.to_str().unwrap_or_default()) else {
        return;
    };
    if let Some((_, entity)) = entity_by_uuid(&id)
        && let Some(living) = entity.as_living_entity()
    {
        living.set_ticks_frozen(ticks.max(0));
    }
}

extern "system" fn entity_no_damage_ticks(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jint {
    let Ok(text) = env.get_string(&uuid) else {
        return 0;
    };
    let Some(id) = text.to_str().ok().and_then(|v| Uuid::parse_str(v).ok()) else {
        return 0;
    };
    entity_by_uuid(&id).map_or(0, |(_, entity)| {
        entity
            .as_ref()
            .as_living_entity()
            .map_or(0, LivingEntity::no_damage_ticks)
    })
}

extern "system" fn entity_sprinting(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jboolean {
    let Ok(text) = env.get_string(&uuid) else {
        return jboolean::from(false);
    };
    let Some(id) = text.to_str().ok().and_then(|v| Uuid::parse_str(v).ok()) else {
        return jboolean::from(false);
    };
    jboolean::from(entity_by_uuid(&id).is_some_and(|(_, entity)| {
        entity
            .as_ref()
            .as_living_entity()
            .is_some_and(LivingEntity::is_sprinting)
    }))
}

extern "system" fn entity_swimming(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jboolean {
    let Ok(text) = env.get_string(&uuid) else {
        return jboolean::from(false);
    };
    let Some(id) = text.to_str().ok().and_then(|v| Uuid::parse_str(v).ok()) else {
        return jboolean::from(false);
    };
    jboolean::from(entity_by_uuid(&id).is_some_and(|(_, entity)| entity.is_swimming()))
}

extern "system" fn entity_is_using_item(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jboolean {
    let Ok(text) = env.get_string(&uuid) else {
        return jboolean::from(false);
    };
    let Some(id) = text.to_str().ok().and_then(|v| Uuid::parse_str(v).ok()) else {
        return jboolean::from(false);
    };
    jboolean::from(entity_by_uuid(&id).is_some_and(|(_, entity)| {
        entity
            .as_ref()
            .as_living_entity()
            .is_some_and(LivingEntity::is_using_item)
    }))
}

extern "system" fn entity_clear_active_item(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) {
    let Ok(text) = env.get_string(&uuid) else {
        return;
    };
    let Some(id) = text.to_str().ok().and_then(|v| Uuid::parse_str(v).ok()) else {
        return;
    };
    if let Some((_, entity)) = entity_by_uuid(&id)
        && let Some(living) = entity.as_ref().as_living_entity()
    {
        living.living_base().stop_using_item();
    }
}

extern "system" fn entity_set_no_damage_ticks(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    ticks: jint,
) {
    let Ok(text) = env.get_string(&uuid) else {
        return;
    };
    let Some(id) = text.to_str().ok().and_then(|v| Uuid::parse_str(v).ok()) else {
        return;
    };
    let Some((_, entity)) = entity_by_uuid(&id) else {
        return;
    };
    if let Some(living) = entity.as_ref().as_living_entity() {
        living.set_no_damage_ticks(ticks);
    }
}

extern "system" fn entity_nearby(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    x: jdouble,
    y: jdouble,
    z: jdouble,
) -> jobjectArray {
    let Ok(text) = env.get_string(&uuid) else {
        return null_mut();
    };
    let Some(id) = text.to_str().ok().and_then(|v| Uuid::parse_str(v).ok()) else {
        return null_mut();
    };
    let Some((world, entity)) = entity_by_uuid(&id) else {
        return null_mut();
    };
    let bounds = entity.bounding_box().inflate_xyz(x, y, z);
    let values: Vec<String> = world
        .get_entities_in_aabb(&bounds)
        .into_iter()
        .filter(|other| other.uuid() != id)
        .map(|other| other.uuid().to_string())
        .collect();
    string_array(&mut env, &values)
}

extern "system" fn entity_tracked_by(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jobjectArray {
    let Ok(text) = env.get_string(&uuid) else {
        return null_mut();
    };
    let Some(id) = text.to_str().ok().and_then(|v| Uuid::parse_str(v).ok()) else {
        return null_mut();
    };
    let Some((world, entity)) = entity_by_uuid(&id) else {
        return null_mut();
    };
    let values = world
        .entity_tracker()
        .tracking_player_ids(entity.id())
        .into_iter()
        .filter_map(|player_id| {
            server().and_then(|value| value.online_players().get_by_entity_id(player_id))
        })
        .map(|player| player.gameprofile.id.to_string())
        .collect::<Vec<_>>();
    string_array(&mut env, &values)
}

extern "system" fn world_nearby(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    world_name: JString<'_>,
    x: jdouble,
    y: jdouble,
    z: jdouble,
    radius_x: jdouble,
    radius_y: jdouble,
    radius_z: jdouble,
) -> jobjectArray {
    let Some(world) = world(&mut env, &world_name) else {
        return null_mut();
    };
    let bounds = WorldAabb::new(
        x - radius_x,
        y - radius_y,
        z - radius_z,
        x + radius_x,
        y + radius_y,
        z + radius_z,
    );
    let values: Vec<String> = world
        .get_entities_in_aabb(&bounds)
        .into_iter()
        .map(|entity| entity.uuid().to_string())
        .collect();
    string_array(&mut env, &values)
}

extern "system" fn player_hide_entity(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    player_uuid: JString<'_>,
    entity_uuid: JString<'_>,
    hidden: jboolean,
) {
    let Ok(player_text) = env.get_string(&player_uuid) else {
        return;
    };
    let Ok(entity_text) = env.get_string(&entity_uuid) else {
        return;
    };
    let Some(player_id) = player_text
        .to_str()
        .ok()
        .and_then(|v| Uuid::parse_str(v).ok())
    else {
        return;
    };
    let Some(entity_id) = entity_text
        .to_str()
        .ok()
        .and_then(|v| Uuid::parse_str(v).ok())
    else {
        return;
    };
    let Some(player) = server().and_then(|value| value.online_players().get_by_uuid(&player_id))
    else {
        return;
    };
    let world = player.get_world();
    let Some((entity_world, entity)) = entity_by_uuid(&entity_id) else {
        return;
    };
    if !Arc::ptr_eq(&world, &entity_world) {
        return;
    }
    world
        .entity_tracker()
        .set_hidden_for_player(entity.id(), player.id(), hidden != 0);
}

extern "system" fn player_can_see_entity(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    player_uuid: JString<'_>,
    entity_uuid: JString<'_>,
) -> jboolean {
    let Ok(player_text) = env.get_string(&player_uuid) else {
        return 0;
    };
    let Ok(entity_text) = env.get_string(&entity_uuid) else {
        return 0;
    };
    let Some(player_id) = player_text
        .to_str()
        .ok()
        .and_then(|v| Uuid::parse_str(v).ok())
    else {
        return 0;
    };
    let Some(entity_id) = entity_text
        .to_str()
        .ok()
        .and_then(|v| Uuid::parse_str(v).ok())
    else {
        return 0;
    };
    let Some(player) = server().and_then(|value| value.online_players().get_by_uuid(&player_id))
    else {
        return 0;
    };
    let Some((entity_world, entity)) = entity_by_uuid(&entity_id) else {
        return 0;
    };
    if !Arc::ptr_eq(&player.get_world(), &entity_world) {
        return 0;
    }
    jboolean::from(
        !entity_world
            .entity_tracker()
            .is_hidden_for_player(entity.id(), player.id()),
    )
}

extern "system" fn entity_eye_height(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jdouble {
    let Ok(text) = env.get_string(&uuid) else {
        return 1.62;
    };
    let Some(id) = text
        .to_str()
        .ok()
        .and_then(|value| Uuid::parse_str(value).ok())
    else {
        return 1.62;
    };
    entity_by_uuid(&id).map_or(1.62, |(_, entity)| entity.get_eye_height())
}

extern "system" fn entity_velocity(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jdoubleArray {
    let Ok(text) = env.get_string(&uuid) else {
        return null_mut();
    };
    let Some(id) = text
        .to_str()
        .ok()
        .and_then(|value| Uuid::parse_str(value).ok())
    else {
        return null_mut();
    };
    let Some((_, entity)) = entity_by_uuid(&id) else {
        return null_mut();
    };
    let velocity = entity.velocity();
    let Ok(array) = env.new_double_array(3) else {
        return null_mut();
    };
    let values = [velocity.x, velocity.y, velocity.z];
    if env.set_double_array_region(&array, 0, &values).is_err() {
        return null_mut();
    }
    array.into_raw()
}

extern "system" fn set_entity_velocity(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    x: jdouble,
    y: jdouble,
    z: jdouble,
) {
    let Ok(text) = env.get_string(&uuid) else {
        return;
    };
    let Some(id) = text
        .to_str()
        .ok()
        .and_then(|value| Uuid::parse_str(value).ok())
    else {
        return;
    };
    if let Some((_, entity)) = entity_by_uuid(&id) {
        entity.set_velocity(DVec3::new(x, y, z));
        entity.mark_velocity_sync();
    }
}

extern "system" fn entity_fire_ticks(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jint {
    let Ok(text) = env.get_string(&uuid) else {
        return 0;
    };
    let Some(id) = text
        .to_str()
        .ok()
        .and_then(|value| Uuid::parse_str(value).ok())
    else {
        return 0;
    };
    entity_by_uuid(&id).map_or(0, |(_, entity)| entity.remaining_fire_ticks())
}

extern "system" fn set_entity_fire_ticks(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    ticks: jint,
) {
    let Ok(text) = env.get_string(&uuid) else {
        return;
    };
    let Some(id) = text
        .to_str()
        .ok()
        .and_then(|value| Uuid::parse_str(value).ok())
    else {
        return;
    };
    if let Some((_, entity)) = entity_by_uuid(&id) {
        entity.set_remaining_fire_ticks(ticks.max(0));
    }
}

extern "system" fn entity_id(mut env: JNIEnv<'_>, _class: JClass<'_>, uuid: JString<'_>) -> jint {
    let text: String = match env.get_string(&uuid) {
        Ok(v) => v.into(),
        Err(_) => return -1,
    };
    let Some(id) = Uuid::parse_str(&text).ok() else {
        return -1;
    };
    entity_by_uuid(&id).map_or(-1, |(_, entity)| entity.id())
}

extern "system" fn entity_projectile_owner(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jstring {
    let Ok(text) = env.get_string(&uuid) else {
        return null_mut();
    };
    let Ok(id) = Uuid::parse_str(text.to_str().unwrap_or_default()) else {
        return null_mut();
    };
    let owner = {
        let sources = projectile_sources().read();
        entity_by_uuid(&id).and_then(|(_, entity)| {
            if entity.is_removed() || sources.contains_key(&id) {
                None
            } else {
                entity.projectile_owner_uuid()
            }
        })
    };
    to_java(&mut env, owner.map(|value| value.to_string()))
}

extern "system" fn entity_projectile_source(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jobject {
    let Ok(uuid): Result<String, _> = env.get_string(&uuid).map(Into::into) else {
        return null_mut();
    };
    let Ok(id) = Uuid::parse_str(&uuid) else {
        return null_mut();
    };
    let source = {
        let sources = projectile_sources().read();
        entity_by_uuid(&id)
            .filter(|(_, entity)| !entity.is_removed())
            .and_then(|_| sources.get(&id).cloned())
    };
    let Some(source) = source else {
        return null_mut();
    };
    env.new_local_ref(source.as_obj())
        .map_or(null_mut(), JObject::into_raw)
}

extern "system" fn entity_projectile_shooter(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jobject {
    let Ok(uuid): Result<String, _> = env.get_string(&uuid).map(Into::into) else {
        return null_mut();
    };
    let Ok(id) = Uuid::parse_str(&uuid) else {
        return null_mut();
    };
    let (source, owner) = {
        let sources = projectile_sources().read();
        let Some((_, entity)) = entity_by_uuid(&id) else {
            return null_mut();
        };
        if entity.is_removed() {
            return null_mut();
        }
        let source = sources.get(&id).cloned();
        let owner = if source.is_none() {
            entity.projectile_owner_uuid()
        } else {
            None
        };
        (source, owner)
    };
    if let Some(source) = source {
        return env
            .new_local_ref(source.as_obj())
            .map_or(null_mut(), JObject::into_raw);
    }
    owner
        .and_then(|owner| env.new_string(owner.to_string()).ok())
        .map_or(null_mut(), JString::into_raw)
        .cast()
}

extern "system" fn set_entity_projectile_source(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    owner: JString<'_>,
    source: JObject<'_>,
    reset_pickup: jboolean,
) -> jboolean {
    let Ok(uuid): Result<String, _> = env.get_string(&uuid).map(Into::into) else {
        return 0;
    };
    let Ok(id) = Uuid::parse_str(&uuid) else {
        return 0;
    };
    let Ok(owner): Result<String, _> = env.get_string(&owner).map(Into::into) else {
        return 0;
    };
    let owner = if owner.is_empty() {
        None
    } else {
        let Ok(owner) = Uuid::parse_str(&owner) else {
            return 0;
        };
        Some(owner)
    };
    let Some((_, entity)) = entity_by_uuid(&id) else {
        remove_projectile_source(&id);
        return 0;
    };
    #[cfg(test)]
    task_one_tests::after_source_resolution();
    let Some(projectile) = entity.as_projectile() else {
        remove_projectile_source(&id);
        return 0;
    };
    let source = if owner.is_none() && !source.is_null() {
        let Ok(source) = env.new_global_ref(&source) else {
            return 0;
        };
        Some(source)
    } else {
        None
    };

    let replaced = {
        let mut sources = projectile_sources().write();
        if entity.is_removed()
            || !entity_by_uuid(&id).is_some_and(|(_, current)| Arc::ptr_eq(&current, &entity))
        {
            return 0;
        }
        projectile.set_owner_uuid(owner);
        if let Some(source) = source {
            sources.insert(id, source)
        } else {
            sources.remove(&id)
        }
    };
    drop(replaced);
    if reset_pickup != 0
        && let Some(arrow) = entity.downcast_ref::<ArrowEntity>()
        && let Some(shooter) =
            owner.and_then(|owner| entity_by_uuid(&owner).map(|(_, entity)| entity))
    {
        arrow.apply_owner_pickup_reset(shooter.as_ref());
    }
    1
}

extern "system" fn mob_effect_instant(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    key: JString<'_>,
) -> jboolean {
    let Ok(key): Result<String, _> = env.get_string(&key).map(Into::into) else {
        return 0;
    };
    let Ok(key) = Identifier::from_str(&format!("minecraft:{key}")) else {
        return 0;
    };
    REGISTRY
        .get()
        .map_or_else(
            || MobEffect::is_instantaneous_key(&key),
            |registry| {
                registry
                    .mob_effects
                    .by_key(&key)
                    .is_some_and(MobEffect::is_instantaneous)
            },
        )
        .into()
}

extern "system" fn entity_potion_effects(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jobjectArray {
    let Ok(text) = env.get_string(&uuid) else {
        return null_mut();
    };
    let Ok(id) = Uuid::parse_str(text.to_str().unwrap_or_default()) else {
        return null_mut();
    };
    let Some((_, entity)) = entity_by_uuid(&id) else {
        return null_mut();
    };
    let Some(living) = entity.as_living_entity() else {
        return null_mut();
    };
    let effects: Vec<String> = living
        .active_mob_effects()
        .into_iter()
        .map(|effect| {
            format!(
                "{}|{}|{}|{}|{}|{}",
                effect.effect().key,
                effect.duration(),
                effect.amplifier(),
                effect.is_ambient(),
                effect.is_visible(),
                effect.show_icon()
            )
        })
        .collect();
    string_array(&mut env, &effects)
}

extern "system" fn entity_persistent(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jboolean {
    let Ok(text) = env.get_string(&uuid) else {
        return 0;
    };
    let Ok(id) = Uuid::parse_str(text.to_str().unwrap_or_default()) else {
        return 0;
    };
    jboolean::from(entity_by_uuid(&id).is_some_and(|(_, entity)| entity.is_persistent()))
}

extern "system" fn set_entity_persistent(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    persistent: jboolean,
) {
    let Ok(text) = env.get_string(&uuid) else {
        return;
    };
    let Ok(id) = Uuid::parse_str(text.to_str().unwrap_or_default()) else {
        return;
    };
    if let Some((_, entity)) = entity_by_uuid(&id) {
        entity.set_persistent(persistent != 0);
    }
}

extern "system" fn entity_remove_when_far_away(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jboolean {
    let Ok(text) = env.get_string(&uuid) else {
        return 0;
    };
    let Ok(id) = Uuid::parse_str(text.to_str().unwrap_or_default()) else {
        return 0;
    };
    jboolean::from(entity_by_uuid(&id).is_some_and(|(_, entity)| {
        entity
            .as_mob()
            .is_some_and(|mob| !mob.is_persistence_required())
    }))
}

extern "system" fn set_entity_remove_when_far_away(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    remove: jboolean,
) {
    let Ok(text) = env.get_string(&uuid) else {
        return;
    };
    let Ok(id) = Uuid::parse_str(text.to_str().unwrap_or_default()) else {
        return;
    };
    if let Some((_, entity)) = entity_by_uuid(&id)
        && let Some(mob) = entity.as_mob()
    {
        mob.set_persistence_required_value(remove == 0);
    }
}

const fn equipment_slot_from_index(slot: jint) -> Option<EquipmentSlot> {
    match slot {
        0 => Some(EquipmentSlot::MainHand),
        1 => Some(EquipmentSlot::OffHand),
        2 => Some(EquipmentSlot::Feet),
        3 => Some(EquipmentSlot::Legs),
        4 => Some(EquipmentSlot::Chest),
        5 => Some(EquipmentSlot::Head),
        6 => Some(EquipmentSlot::Body),
        7 => Some(EquipmentSlot::Saddle),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EquipmentSlotRequestError {
    InvalidIndex,
}

fn entity_equipment_slot_state(
    entity: Option<&dyn Entity>,
    slot: jint,
) -> Result<Option<String>, EquipmentSlotRequestError> {
    let Some(slot) = equipment_slot_from_index(slot) else {
        return Err(EquipmentSlotRequestError::InvalidIndex);
    };
    let Some(living) = entity.and_then(Entity::as_living_entity) else {
        return Ok(None);
    };
    let mut described = None;
    living.with_equipment_slot(slot, &mut |item| described = Some(describe_slot(item)));
    Ok(described)
}

fn set_entity_equipment_slot_state(
    entity: Option<&dyn Entity>,
    slot: jint,
    encoded: &str,
) -> Result<(), EquipmentSlotRequestError> {
    let Some(slot) = equipment_slot_from_index(slot) else {
        return Err(EquipmentSlotRequestError::InvalidIndex);
    };
    let Some(living) = entity.and_then(Entity::as_living_entity) else {
        return Ok(());
    };
    let Some(stack) = parse_slot(encoded) else {
        return Ok(());
    };
    let mut stack = stack;
    living.with_equipment_slot_mut(slot, &mut |item| mem::swap(item, &mut stack));
    Ok(())
}

fn clear_entity_equipment_state(entity: Option<&dyn Entity>) -> bool {
    let Some(living) = entity.and_then(Entity::as_living_entity) else {
        return false;
    };
    living.clear_equipment();
    true
}

fn throw_invalid_equipment_slot_index(env: &mut JNIEnv<'_>) {
    let _ = env.throw_new(
        "java/lang/IllegalArgumentException",
        "equipment slot index is invalid",
    );
}

extern "system" fn entity_equipment_slot(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    slot: jint,
) -> jstring {
    let entity = env
        .get_string(&uuid)
        .ok()
        .and_then(|text| Uuid::parse_str(text.to_str().ok()?).ok())
        .and_then(|id| entity_by_uuid(&id).map(|(_, entity)| entity));
    match entity_equipment_slot_state(entity.as_deref(), slot) {
        Ok(value) => to_java(&mut env, value),
        Err(EquipmentSlotRequestError::InvalidIndex) => {
            throw_invalid_equipment_slot_index(&mut env);
            null_mut()
        }
    }
}

extern "system" fn set_entity_equipment_slot(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    slot: jint,
    item: JString<'_>,
) {
    let Some(encoded) = env
        .get_string(&item)
        .ok()
        .and_then(|text| text.to_str().ok().map(str::to_owned))
    else {
        return;
    };
    let entity = env
        .get_string(&uuid)
        .ok()
        .and_then(|text| Uuid::parse_str(text.to_str().ok()?).ok())
        .and_then(|id| entity_by_uuid(&id).map(|(_, entity)| entity));
    if matches!(
        set_entity_equipment_slot_state(entity.as_deref(), slot, &encoded),
        Err(EquipmentSlotRequestError::InvalidIndex)
    ) {
        throw_invalid_equipment_slot_index(&mut env);
    }
}

extern "system" fn clear_entity_equipment(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) {
    let entity = env
        .get_string(&uuid)
        .ok()
        .and_then(|text| Uuid::parse_str(text.to_str().ok()?).ok())
        .and_then(|id| entity_by_uuid(&id).map(|(_, entity)| entity));
    clear_entity_equipment_state(entity.as_deref());
}

extern "system" fn entity_drop_chance(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    slot: jint,
) -> jfloat {
    let Ok(text) = env.get_string(&uuid) else {
        return -1.0;
    };
    let Ok(id) = Uuid::parse_str(text.to_str().unwrap_or_default()) else {
        return -1.0;
    };
    let Some(slot) = equipment_slot_from_index(slot) else {
        return -1.0;
    };
    entity_by_uuid(&id)
        .and_then(|(_, entity)| entity.as_mob().map(|mob| mob.drop_chance(slot)))
        .unwrap_or(-1.0)
}

extern "system" fn set_entity_drop_chance(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    slot: jint,
    chance: jfloat,
) {
    let Ok(text) = env.get_string(&uuid) else {
        return;
    };
    let Ok(id) = Uuid::parse_str(text.to_str().unwrap_or_default()) else {
        return;
    };
    let Some(slot) = equipment_slot_from_index(slot) else {
        return;
    };
    if chance.is_finite()
        && chance >= 0.0
        && let Some((_, entity)) = entity_by_uuid(&id)
        && let Some(mob) = entity.as_mob()
    {
        mob.set_drop_chance(slot, chance);
    }
}

extern "system" fn arrow_potion(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jstring {
    let Ok(text) = env.get_string(&uuid) else {
        return null_mut();
    };
    let Ok(id) = text.to_str().ok().and_then(|v| v.parse().ok()).ok_or(()) else {
        return null_mut();
    };
    let value = entity_by_uuid(&id).and_then(|(_, entity)| {
        entity
            .downcast_ref::<ArrowEntity>()
            .and_then(ArrowEntity::ammo_potion_contents)
            .and_then(|contents| {
                contents
                    .potion()
                    .map(|potion| potion.value().key.to_string())
            })
    });
    to_java(&mut env, value)
}

extern "system" fn set_arrow_potion(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    potion: JString<'_>,
) {
    let Ok(text) = env.get_string(&uuid) else {
        return;
    };
    let Some(id) = text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return;
    };
    let Ok(potion_text) = env.get_string(&potion) else {
        return;
    };
    let Some((_, entity)) = entity_by_uuid(&id) else {
        return;
    };
    let Some(arrow) = entity.as_ref().downcast_ref::<ArrowEntity>() else {
        return;
    };
    let Some(potion_text) = potion_text.to_str().ok() else {
        return;
    };
    let potion = if potion_text.is_empty() {
        None
    } else {
        let Some(registry) = REGISTRY.get() else {
            return;
        };
        let Ok(key) = Identifier::from_str(potion_text) else {
            return;
        };
        let Some(potion) = registry.potions.by_key(&key) else {
            return;
        };
        Some(potion)
    };
    arrow.set_base_potion(potion);
}

extern "system" fn arrow_potion_color(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jint {
    let Ok(text) = env.get_string(&uuid) else {
        return -1;
    };
    let Ok(id) = text.to_str().ok().and_then(|v| v.parse().ok()).ok_or(()) else {
        return -1;
    };
    entity_by_uuid(&id)
        .and_then(|(_, entity)| {
            entity
                .downcast_ref::<ArrowEntity>()
                .and_then(ArrowEntity::ammo_potion_color)
        })
        .unwrap_or(-1)
}

extern "system" fn arrow_custom_effects(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jobjectArray {
    let Ok(text) = env.get_string(&uuid) else {
        return null_mut();
    };
    let Ok(id) = Uuid::parse_str(text.to_str().unwrap_or_default()) else {
        return null_mut();
    };
    let Some((_, entity)) = entity_by_uuid(&id) else {
        return null_mut();
    };
    let Some(arrow) = entity.downcast_ref::<ArrowEntity>() else {
        return null_mut();
    };
    let effects: Vec<String> = arrow
        .ammo_potion_contents()
        .map(|contents| contents.custom_effects().to_vec())
        .unwrap_or_default()
        .into_iter()
        .map(|effect| encode_arrow_custom_effect(&effect))
        .collect();
    string_array(&mut env, &effects)
}

fn encode_arrow_custom_effect(effect: &RegistryMobEffectInstance) -> String {
    let mut encoded = format!(
        "{}|{}|{}|{}|{}|{}",
        effect.effect().key.path,
        effect.duration(),
        effect.amplifier(),
        effect.ambient(),
        effect.show_particles(),
        effect.show_icon()
    );
    let mut hidden = effect.hidden_effect();
    while let Some(details) = hidden {
        let _ = write!(
            encoded,
            "|{}|{}|{}|{}|{}",
            details.duration(),
            details.amplifier(),
            details.ambient(),
            details.show_particles(),
            details.show_icon()
        );
        hidden = details.hidden_effect();
    }
    encoded
}

fn parse_arrow_custom_effect(encoded: &str) -> Option<RegistryMobEffectInstance> {
    let mut fields = encoded.split('|');
    let effect_name = fields.next()?;
    let effect = REGISTRY
        .mob_effects
        .by_key(&Identifier::from_str(&format!("minecraft:{effect_name}")).ok()?)?;
    let duration = fields.next()?.parse::<i32>().ok()?;
    let amplifier = fields.next()?.parse::<i32>().ok()?;
    let ambient = fields.next()?.parse::<bool>().ok()?;
    let particles = fields.next()?.parse::<bool>().ok()?;
    let icon = fields.next()?.parse::<bool>().ok()?;
    if fields.next().is_some() {
        return None;
    }
    Some(RegistryMobEffectInstance::new(
        effect, duration, amplifier, ambient, particles, icon, None,
    ))
}

fn add_or_replace_arrow_custom_effect(
    effects: &mut Vec<RegistryMobEffectInstance>,
    replacement: RegistryMobEffectInstance,
    overwrite: bool,
) -> bool {
    let effect = replacement.effect();
    let already_present = effects.iter().any(|current| current.effect() == effect);
    if already_present && !overwrite {
        return false;
    }
    if already_present {
        effects.retain(|current| current.effect() != effect);
    }
    effects.push(replacement);
    true
}

extern "system" fn arrow_property(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    property: JString<'_>,
) -> jstring {
    let Ok(uuid_text): Result<String, _> = env.get_string(&uuid).map(Into::into) else {
        return null_mut();
    };
    let Ok(property): Result<String, _> = env.get_string(&property).map(Into::into) else {
        return null_mut();
    };
    let Some((_, entity)) = Uuid::parse_str(&uuid_text)
        .ok()
        .and_then(|id| entity_by_uuid(&id))
    else {
        return null_mut();
    };
    let Some(arrow) = entity.downcast_ref::<ArrowEntity>() else {
        return null_mut();
    };
    let value = match property.as_str() {
        "damage" => match arrow.base_damage() {
            f64::INFINITY => "Infinity".to_owned(),
            f64::NEG_INFINITY => "-Infinity".to_owned(),
            damage => damage.to_string(),
        },
        "pierce" => arrow.pierce_level().to_string(),
        "critical" => arrow.is_crit_arrow().to_string(),
        "in_block" => arrow.is_in_ground().to_string(),
        "attached" => arrow
            .attached_blocks()
            .into_iter()
            .map(|pos| format!("{},{},{}", pos.x(), pos.y(), pos.z()))
            .collect::<Vec<_>>()
            .join(";"),
        "pickup" => match arrow.pickup() {
            ArrowPickup::Disallowed => "DISALLOWED".to_owned(),
            ArrowPickup::Allowed => "ALLOWED".to_owned(),
            ArrowPickup::CreativeOnly => "CREATIVE_ONLY".to_owned(),
        },
        "crossbow" => arrow
            .weapon_item()
            .is_some_and(|weapon| weapon.is(&vanilla_items::CROSSBOW))
            .to_string(),
        "item" => describe_slot(&arrow.pickup_item()),
        "weapon" => {
            let Some(item) = arrow.weapon_item() else {
                return null_mut();
            };
            describe_slot(&item)
        }
        "lifetime" => arrow.lifetime_ticks().to_string(),
        "sound" => arrow.hit_sound().key.to_string(),
        _ => return null_mut(),
    };
    to_java(&mut env, Some(value))
}

extern "system" fn set_arrow_property(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    property: JString<'_>,
    value: JString<'_>,
) -> jboolean {
    let Ok(uuid_text): Result<String, _> = env.get_string(&uuid).map(Into::into) else {
        return 0;
    };
    let Ok(property): Result<String, _> = env.get_string(&property).map(Into::into) else {
        return 0;
    };
    let Ok(value): Result<String, _> = env.get_string(&value).map(Into::into) else {
        return 0;
    };
    let Some((_, entity)) = Uuid::parse_str(&uuid_text)
        .ok()
        .and_then(|id| entity_by_uuid(&id))
    else {
        return 0;
    };
    let Some(arrow) = entity.downcast_ref::<ArrowEntity>() else {
        return 0;
    };
    let changed = match property.as_str() {
        "damage" => match value.as_str() {
            "Infinity" => Some(f64::INFINITY),
            "-Infinity" => Some(f64::NEG_INFINITY),
            _ => value.parse::<f64>().ok(),
        }
        .filter(|damage| *damage >= 0.0)
        .is_some_and(|damage| {
            arrow.set_base_damage(damage);
            true
        }),
        "pierce" => value.parse::<i8>().is_ok_and(|level| {
            arrow.set_pierce_level(level);
            true
        }),
        "critical" => value.parse::<bool>().is_ok_and(|critical| {
            arrow.set_crit_arrow(critical);
            true
        }),
        "pickup" => match value.as_str() {
            "DISALLOWED" => {
                arrow.set_pickup(ArrowPickup::Disallowed);
                true
            }
            "ALLOWED" => {
                arrow.set_pickup(ArrowPickup::Allowed);
                true
            }
            "CREATIVE_ONLY" => {
                arrow.set_pickup(ArrowPickup::CreativeOnly);
                true
            }
            _ => false,
        },
        "item" => parse_slot(&value).is_some_and(|item| {
            arrow.set_ammo_item(item);
            true
        }),
        "weapon" => parse_slot(&value).is_some_and(|item| {
            arrow.set_weapon_item(Some(item));
            true
        }),
        "lifetime" => value.parse::<i32>().is_ok_and(|ticks| {
            arrow.set_lifetime_ticks(ticks);
            true
        }),
        "sound" => Identifier::from_str(&value)
            .ok()
            .and_then(|key| REGISTRY.sound_events.by_key(&key))
            .is_some_and(|sound| {
                arrow.set_hit_sound(sound);
                true
            }),
        _ => false,
    };
    jboolean::from(changed)
}

extern "system" fn set_arrow_potion_color(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    color: jint,
    present: jboolean,
) {
    let Ok(text) = env.get_string(&uuid) else {
        return;
    };
    let Some(id) = text
        .to_str()
        .ok()
        .and_then(|value| Uuid::parse_str(value).ok())
    else {
        return;
    };
    if let Some((_, entity)) = entity_by_uuid(&id)
        && let Some(arrow) = entity.downcast_ref::<ArrowEntity>()
    {
        let color = if present != 0 { color } else { -1 };
        arrow.set_potion_color(Some(color));
    }
}

extern "system" fn add_arrow_custom_effect(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    encoded_effect: JString<'_>,
    overwrite: jboolean,
) -> jboolean {
    let Ok(uuid_text): Result<String, _> = env.get_string(&uuid).map(Into::into) else {
        return 0;
    };
    let Ok(encoded_effect): Result<String, _> = env.get_string(&encoded_effect).map(Into::into)
    else {
        return 0;
    };
    let Some((_, entity)) = Uuid::parse_str(&uuid_text)
        .ok()
        .and_then(|id| entity_by_uuid(&id))
    else {
        return 0;
    };
    let Some(arrow) = entity.downcast_ref::<ArrowEntity>() else {
        return 0;
    };
    let Some(replacement) = parse_arrow_custom_effect(&encoded_effect) else {
        return 0;
    };
    let mut effects = arrow
        .ammo_potion_contents()
        .map(|contents| contents.custom_effects().to_vec())
        .unwrap_or_default();
    if !add_or_replace_arrow_custom_effect(&mut effects, replacement, overwrite != 0) {
        return 0;
    }
    arrow.set_custom_effects(effects);
    1
}

extern "system" fn remove_arrow_custom_effect(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    effect_name: JString<'_>,
) -> jboolean {
    let Ok(uuid_text): Result<String, _> = env.get_string(&uuid).map(Into::into) else {
        return 0;
    };
    let Ok(effect_name): Result<String, _> = env.get_string(&effect_name).map(Into::into) else {
        return 0;
    };
    let Some((_, entity)) = Uuid::parse_str(&uuid_text)
        .ok()
        .and_then(|id| entity_by_uuid(&id))
    else {
        return 0;
    };
    let Some(arrow) = entity.downcast_ref::<ArrowEntity>() else {
        return 0;
    };
    let mut effects = arrow
        .ammo_potion_contents()
        .map(|contents| contents.custom_effects().to_vec())
        .unwrap_or_default();
    let before = effects.len();
    effects.retain(|effect| effect.effect().key.path != effect_name);
    if effects.len() == before {
        return 0;
    }
    arrow.set_custom_effects(effects);
    1
}

extern "system" fn clear_arrow_custom_effects(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) {
    let Ok(text) = env.get_string(&uuid) else {
        return;
    };
    let Some(id) = text
        .to_str()
        .ok()
        .and_then(|value| Uuid::parse_str(value).ok())
    else {
        return;
    };
    if let Some((_, entity)) = entity_by_uuid(&id)
        && let Some(arrow) = entity.downcast_ref::<ArrowEntity>()
    {
        arrow.set_custom_effects(Vec::new());
    }
}

extern "system" fn mushroom_cow_stew_effects(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jobjectArray {
    let Ok(text) = env.get_string(&uuid) else {
        return null_mut();
    };
    let Some(id) = text
        .to_str()
        .ok()
        .and_then(|value| Uuid::parse_str(value).ok())
    else {
        return null_mut();
    };
    let Some((_, entity)) = entity_by_uuid(&id) else {
        return null_mut();
    };
    let Some(cow) = entity.downcast_ref::<MushroomCowEntity>() else {
        return null_mut();
    };
    let effects = cow
        .stew_effects()
        .map(|effects| {
            effects
                .effects()
                .iter()
                .map(|effect| format!("{}|{}", effect.effect().key.path, effect.duration()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    string_array(&mut env, &effects)
}

extern "system" fn set_mushroom_cow_stew_effects(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    encoded: JObjectArray<'_>,
) -> jboolean {
    let Ok(text) = env.get_string(&uuid) else {
        return 0;
    };
    let Some(id) = text
        .to_str()
        .ok()
        .and_then(|value| Uuid::parse_str(value).ok())
    else {
        return 0;
    };
    let Some(values) = read_string_array(&mut env, &encoded) else {
        return 0;
    };
    let Some((_, entity)) = entity_by_uuid(&id) else {
        return 0;
    };
    let Some(cow) = entity.downcast_ref::<MushroomCowEntity>() else {
        return 0;
    };
    let mut effects = Vec::with_capacity(values.len());
    for value in values {
        let Some((effect_name, duration)) = value.split_once('|') else {
            return 0;
        };
        let Ok(key) = Identifier::from_str(&format!("minecraft:{effect_name}")) else {
            return 0;
        };
        let Some(effect) = REGISTRY.mob_effects.by_key(&key) else {
            return 0;
        };
        let Ok(duration) = duration.parse::<i32>() else {
            return 0;
        };
        effects.push(SuspiciousStewEffect::new(effect, duration));
    }
    cow.set_stew_effects((!effects.is_empty()).then(|| SuspiciousStewEffects::new(effects)));
    1
}

extern "system" fn mushroom_cow_ready_to_shear(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jboolean {
    let Ok(text) = env.get_string(&uuid) else {
        return 0;
    };
    let Some(id) = text
        .to_str()
        .ok()
        .and_then(|value| Uuid::parse_str(value).ok())
    else {
        return 0;
    };
    jboolean::from(entity_by_uuid(&id).is_some_and(|(_, entity)| {
        entity
            .downcast_ref::<MushroomCowEntity>()
            .is_some_and(MushroomCowEntity::ready_for_shearing)
    }))
}

extern "system" fn shear_mushroom_cow(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    source: JString<'_>,
) {
    let Ok(text) = env.get_string(&uuid) else {
        return;
    };
    let Some(id) = text
        .to_str()
        .ok()
        .and_then(|value| Uuid::parse_str(value).ok())
    else {
        return;
    };
    let Ok(source): Result<String, _> = env.get_string(&source).map(Into::into) else {
        return;
    };
    let sound_source = match source.as_str() {
        "MASTER" => SoundSource::Master,
        "MUSIC" => SoundSource::Music,
        "RECORD" => SoundSource::Records,
        "WEATHER" => SoundSource::Weather,
        "BLOCK" => SoundSource::Blocks,
        "HOSTILE" => SoundSource::Hostile,
        "NEUTRAL" => SoundSource::Neutral,
        "PLAYER" => SoundSource::Players,
        "AMBIENT" => SoundSource::Ambient,
        "VOICE" => SoundSource::Voice,
        "UI" => SoundSource::Ui,
        _ => return,
    };
    let Some((world, entity)) = entity_by_uuid(&id) else {
        return;
    };
    let Some(cow) = entity.downcast_ref::<MushroomCowEntity>() else {
        return;
    };
    cow.shear_with_sound_source(
        world.as_ref(),
        &ItemStack::new(&vanilla_items::SHEARS),
        sound_source,
    );
}

extern "system" fn air_supply(mut env: JNIEnv<'_>, _class: JClass<'_>, uuid: JString<'_>) -> jint {
    let Ok(text) = env.get_string(&uuid) else {
        return 300;
    };
    let Some(id) = text
        .to_str()
        .ok()
        .and_then(|value| Uuid::parse_str(value).ok())
    else {
        return 300;
    };
    entity_by_uuid(&id).map_or(300, |(_, entity)| entity.air_supply())
}

extern "system" fn set_air_supply(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    ticks: jint,
) {
    let Ok(text) = env.get_string(&uuid) else {
        return;
    };
    let Some(id) = text
        .to_str()
        .ok()
        .and_then(|value| Uuid::parse_str(value).ok())
    else {
        return;
    };
    if let Some((_, entity)) = entity_by_uuid(&id) {
        entity.set_air_supply(ticks);
    }
}

extern "system" fn max_air_supply(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jint {
    let Ok(text) = env.get_string(&uuid) else {
        return 300;
    };
    let Some(id) = text
        .to_str()
        .ok()
        .and_then(|value| Uuid::parse_str(value).ok())
    else {
        return 300;
    };
    entity_by_uuid(&id).map_or(300, |(_, entity)| entity.max_air_supply())
}

fn mutate_villager_offer(
    uuid: uuid::Uuid,
    index: usize,
    mutate: impl FnOnce(&mut MerchantOffer),
) -> bool {
    let Some(server) = server() else {
        return false;
    };
    for snapshot in server.worlds.snapshots() {
        let world = snapshot.world();
        let Some(entity) = world.get_entity_by_uuid(&uuid) else {
            continue;
        };
        let Some(villager) = entity.as_ref().downcast_ref::<VillagerEntity>() else {
            continue;
        };
        let offers = villager.merchant().offers();
        let mut offers = offers.lock();
        let Some(offer) = offers.get_mut(index) else {
            return false;
        };
        mutate(offer);
        return true;
    }
    false
}

extern "system" fn entity_set_merchant_offer_uses(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    index: jint,
    uses: jint,
) -> jboolean {
    let Ok(text) = env.get_string(&uuid) else {
        return 0;
    };
    let Ok(uuid) = text
        .to_str()
        .ok()
        .and_then(|value| uuid::Uuid::parse_str(value).ok())
        .ok_or(())
    else {
        return 0;
    };
    let Ok(index) = usize::try_from(index) else {
        return 0;
    };
    jboolean::from(mutate_villager_offer(uuid, index, |offer| {
        offer.set_uses(uses);
    }))
}

extern "system" fn entity_set_merchant_offer_max_uses(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    index: jint,
    max_uses: jint,
) -> jboolean {
    let Ok(text) = env.get_string(&uuid) else {
        return 0;
    };
    let Ok(uuid) = text
        .to_str()
        .ok()
        .and_then(|value| uuid::Uuid::parse_str(value).ok())
        .ok_or(())
    else {
        return 0;
    };
    let Ok(index) = usize::try_from(index) else {
        return 0;
    };
    jboolean::from(mutate_villager_offer(uuid, index, |offer| {
        offer.set_max_uses(max_uses);
    }))
}

extern "system" fn entity_set_merchant_offer_demand(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    index: jint,
    demand: jint,
) -> jboolean {
    let Ok(text) = env.get_string(&uuid) else {
        return 0;
    };
    let Ok(uuid) = text
        .to_str()
        .ok()
        .and_then(|value| uuid::Uuid::parse_str(value).ok())
        .ok_or(())
    else {
        return 0;
    };
    let Ok(index) = usize::try_from(index) else {
        return 0;
    };
    jboolean::from(mutate_villager_offer(uuid, index, |offer| {
        offer.set_demand(demand);
    }))
}

extern "system" fn entity_merchant_recipes(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jobjectArray {
    let Ok(text) = env.get_string(&uuid) else {
        return null_mut();
    };
    let Ok(uuid) = text
        .to_str()
        .ok()
        .and_then(|value| uuid::Uuid::parse_str(value).ok())
        .ok_or(())
    else {
        return null_mut();
    };
    let Some(server) = server() else {
        return null_mut();
    };
    for snapshot in server.worlds.snapshots() {
        let world = snapshot.world();
        let Some(entity) = world.get_entity_by_uuid(&uuid) else {
            continue;
        };
        let Some(villager) = entity.as_ref().downcast_ref::<VillagerEntity>() else {
            continue;
        };
        let offers = villager.offers();
        let values = offers
            .iter()
            .map(|offer| {
                format!(
                    "{} {}|{}|{}|{}|{} {}|{} {}",
                    offer.result().item().key,
                    offer.result().count(),
                    offer.uses(),
                    offer.max_uses(),
                    offer.demand(),
                    offer.cost_a().item().key,
                    offer.cost_a().count(),
                    offer.cost_b().item().key,
                    offer.cost_b().count()
                )
            })
            .collect::<Vec<_>>();
        return string_array(&mut env, &values);
    }
    null_mut()
}

extern "system" fn iron_golem_player_created(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jboolean {
    let Ok(uuid_text) = env.get_string(&uuid) else {
        return 0;
    };
    let Some(id) = Uuid::parse_str(&String::from(uuid_text)).ok() else {
        return 0;
    };
    entity_by_uuid(&id)
        .and_then(|(_, entity)| {
            entity
                .downcast_ref::<IronGolemEntity>()
                .map(|golem| u8::from(golem.is_player_created()))
        })
        .unwrap_or(0)
}

extern "system" fn set_iron_golem_player_created(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    value: jboolean,
) {
    let Ok(uuid_text) = env.get_string(&uuid) else {
        return;
    };
    let Some(id) = Uuid::parse_str(&String::from(uuid_text)).ok() else {
        return;
    };
    if let Some((_, entity)) = entity_by_uuid(&id)
        && let Some(golem) = entity.downcast_ref::<IronGolemEntity>()
    {
        golem.set_player_created(value != 0);
    }
}

extern "system" fn entity_custom_name(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jstring {
    let text: String = match env.get_string(&uuid) {
        Ok(v) => v.into(),
        Err(_) => return to_java(&mut env, None),
    };
    let Some(id) = Uuid::parse_str(&text).ok() else {
        return to_java(&mut env, None);
    };
    to_java(
        &mut env,
        entity_by_uuid(&id)
            .and_then(|(_, entity)| entity.custom_name().map(|name| name.to_string())),
    )
}

extern "system" fn entity_custom_name_visible(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jboolean {
    let Ok(uuid_text) = env.get_string(&uuid) else {
        return 0;
    };
    let Some(id) = Uuid::parse_str(&String::from(uuid_text)).ok() else {
        return 0;
    };
    entity_by_uuid(&id).map_or(0, |(_, entity)| u8::from(entity.is_custom_name_visible()))
}

extern "system" fn set_entity_custom_name(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    name: JString<'_>,
) {
    let Ok(uuid_text) = env.get_string(&uuid) else {
        return;
    };
    let Some(id) = Uuid::parse_str(&String::from(uuid_text)).ok() else {
        return;
    };
    let Some((_, entity)) = entity_by_uuid(&id) else {
        return;
    };
    if name.is_null() {
        entity.set_custom_name(None);
        return;
    }
    let Ok(name_text) = env.get_string(&name) else {
        return;
    };
    entity.set_custom_name(Some(text_components::TextComponent::plain(String::from(
        name_text,
    ))));
}

extern "system" fn entity_send_message(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    message: JString<'_>,
) {
    let Ok(uuid_text) = env.get_string(&uuid) else {
        return;
    };
    let Ok(message_text) = env.get_string(&message) else {
        return;
    };
    let Some(id) = Uuid::parse_str(&String::from(uuid_text)).ok() else {
        return;
    };
    if let Some((_, entity)) = entity_by_uuid(&id)
        && let Some(player) = entity.as_player()
    {
        player.send_message(&text_components::TextComponent::plain(String::from(
            message_text,
        )));
    }
}

extern "system" fn player_killer(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jstring {
    let Ok(text) = env.get_string(&uuid) else {
        return null_mut();
    };
    let Ok(id) = Uuid::parse_str(text.to_str().unwrap_or_default()) else {
        return null_mut();
    };
    let Some((_, entity)) = entity_by_uuid(&id) else {
        return null_mut();
    };
    let Some(living) = entity.as_living_entity() else {
        return null_mut();
    };
    let Some(killer) = living.last_hurt_by_player_uuid() else {
        return null_mut();
    };
    to_java(&mut env, Some(killer.to_string()))
}

extern "system" fn global_player_timestamp(
    mut env: JNIEnv<'_>,
    uuid: JString<'_>,
    first: bool,
) -> jlong {
    let Ok(text) = env.get_string(&uuid) else {
        return 0;
    };
    let Ok(uuid) = Uuid::parse_str(text.to_str().unwrap_or_default()) else {
        return 0;
    };
    let Some(server) = server() else {
        return 0;
    };
    let Some(data) = server.global_player_data(uuid) else {
        return 0;
    };
    if first {
        data.first_played
    } else {
        data.last_played
    }
}

extern "system" fn first_played(env: JNIEnv<'_>, _class: JClass<'_>, uuid: JString<'_>) -> jlong {
    global_player_timestamp(env, uuid, true)
}

extern "system" fn last_played(env: JNIEnv<'_>, _class: JClass<'_>, uuid: JString<'_>) -> jlong {
    global_player_timestamp(env, uuid, false)
}

extern "system" fn has_played_before(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jboolean {
    let Ok(text) = env.get_string(&uuid) else {
        return 0;
    };
    let text: String = text.into();
    let Ok(uuid) = Uuid::parse_str(&text) else {
        return 0;
    };
    u8::from(server().is_some_and(|server| server.known_players().by_uuid(uuid).is_some()))
}

/// `foton.Native.customName`
extern "system" fn custom_name(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jstring {
    let name = player(&mut env, &uuid)
        .and_then(|player| player.custom_name().map(|name| name.to_string()));
    to_java(&mut env, name)
}

/// `foton.Native.setCustomName`
extern "system" fn set_custom_name(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    name: JString<'_>,
) {
    let Some(player) = player(&mut env, &uuid) else {
        return;
    };
    if name.is_null() {
        player.set_custom_name(None);
        return;
    }
    let Ok(name) = env.get_string(&name) else {
        return;
    };
    let name = String::from(name);
    player.set_custom_name((!name.is_empty()).then(|| TextComponent::from(name)));
}

/// `foton.Native.health`
extern "system" fn world_seed(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    world_name: JString<'_>,
) -> jlong {
    let Ok(text) = env.get_string(&world_name) else {
        return 0;
    };
    let Some(key) = text
        .to_str()
        .ok()
        .and_then(|value| value.parse::<Identifier>().ok())
    else {
        return 0;
    };
    let Some(world) = server().and_then(|server| server.worlds.get_owned(&key)) else {
        return 0;
    };
    world.level_data.read().data().seed
}

extern "system" fn world_coordinate_scale(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    world_name: JString<'_>,
) -> jdouble {
    let Ok(text) = env.get_string(&world_name) else {
        return 1.0;
    };
    let Some(key) = text
        .to_str()
        .ok()
        .and_then(|value| value.parse::<Identifier>().ok())
    else {
        return 1.0;
    };
    server()
        .and_then(|server| server.worlds.get_owned(&key))
        .map_or(1.0, |world| world.dimension_type.coordinate_scale)
}

extern "system" fn world_can_generate_structures(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    world_name: JString<'_>,
) -> jboolean {
    jboolean::from(world(&mut env, &world_name).is_some_and(|value| {
        value
            .chunk_map
            .world_gen_context
            .generator
            .structure_generator()
            .is_some()
    }))
}

extern "system" fn world_allow_monsters(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    world_name: JString<'_>,
) -> jboolean {
    jboolean::from(world(&mut env, &world_name).is_some_and(|value| value.allow_monsters()))
}

extern "system" fn set_world_allow_monsters(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    world_name: JString<'_>,
    value: jboolean,
) {
    if let Some(world) = world(&mut env, &world_name) {
        world.set_allow_monsters(value != 0);
    }
}

extern "system" fn world_allow_animals(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    world_name: JString<'_>,
) -> jboolean {
    jboolean::from(world(&mut env, &world_name).is_some_and(|value| value.allow_animals()))
}

extern "system" fn set_world_allow_animals(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    world_name: JString<'_>,
    value: jboolean,
) {
    if let Some(world) = world(&mut env, &world_name) {
        world.set_allow_animals(value != 0);
    }
}

extern "system" fn world_pvp(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    world_name: JString<'_>,
) -> jboolean {
    jboolean::from(world(&mut env, &world_name).is_some_and(|value| value.is_pvp()))
}

extern "system" fn set_world_pvp(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    world_name: JString<'_>,
    enabled: jboolean,
) {
    if let Some(value) = world(&mut env, &world_name) {
        value.set_pvp(enabled != 0);
    }
}

extern "system" fn world_difficulty(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    world_name: JString<'_>,
) -> jstring {
    let Ok(text) = env.get_string(&world_name) else {
        return null_mut();
    };
    let Some(key) = text
        .to_str()
        .ok()
        .and_then(|value| value.parse::<Identifier>().ok())
    else {
        return null_mut();
    };
    let value = server().and_then(|server| {
        server
            .worlds
            .get(&key)
            .map(|world| format!("{:?}", world.level_data.read().data().difficulty))
    });
    to_java(&mut env, value)
}

extern "system" fn player_food_level(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jint {
    player(&mut env, &uuid).map_or(20, |player| player.food_data.lock().food_level)
}

extern "system" fn entity_fall_distance(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jfloat {
    let Ok(text) = env.get_string(&uuid) else {
        return 0.0;
    };
    let Some(id) = text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return 0.0;
    };
    entity_by_uuid(&id).map_or(0.0, |(_, entity)| entity.fall_distance() as f32)
}

extern "system" fn set_entity_fall_distance(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    distance: jfloat,
) {
    let Ok(text) = env.get_string(&uuid) else {
        return;
    };
    let Ok(id) = text
        .to_str()
        .ok()
        .and_then(|value| value.parse().ok())
        .ok_or(())
    else {
        return;
    };
    let Some((_, entity)) = entity_by_uuid(&id) else {
        return;
    };
    entity.set_fall_distance(f64::from(distance));
}

extern "system" fn player_food_saturation(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jfloat {
    player(&mut env, &uuid).map_or(5.0, |player| player.food_data.lock().saturation_level)
}

extern "system" fn player_food_exhaustion(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jfloat {
    player(&mut env, &uuid).map_or(0.0, |player| player.food_data.lock().exhaustion_level)
}

extern "system" fn set_player_food(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    food: jint,
    saturation: jfloat,
    exhaustion: jfloat,
) {
    let Some(player) = player(&mut env, &uuid) else {
        return;
    };
    let mut data = player.food_data.lock();
    data.food_level = food.clamp(0, 20);
    data.saturation_level = saturation.clamp(0.0, data.food_level as f32);
    data.exhaustion_level = exhaustion.clamp(0.0, 40.0);
}

extern "system" fn player_ping(mut env: JNIEnv<'_>, _class: JClass<'_>, uuid: JString<'_>) -> jint {
    player(&mut env, &uuid).map_or(0, |player| player.connection.latency())
}

extern "system" fn set_player_operator(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    operator: jboolean,
) {
    let Some(server) = server() else {
        return;
    };
    let Ok(text): Result<String, _> = env.get_string(&uuid).map(Into::into) else {
        return;
    };
    let Ok(uuid) = Uuid::parse_str(&text) else {
        return;
    };
    server.queue_player_operator_update(uuid, operator != 0);
}

extern "system" fn player_walk_speed(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jfloat {
    player(&mut env, &uuid).map_or(0.1, |player| player.get_walking_speed())
}

extern "system" fn set_player_walk_speed(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    speed: jfloat,
) {
    if let Some(player) = player(&mut env, &uuid) {
        player.set_walking_speed(speed.clamp(-1.0, 1.0));
    }
}

extern "system" fn player_fly_speed(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jfloat {
    player(&mut env, &uuid).map_or(0.1, |player| player.get_flying_speed())
}

extern "system" fn set_player_fly_speed(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    speed: jfloat,
) {
    if let Some(player) = player(&mut env, &uuid) {
        player.set_flying_speed(speed.clamp(-1.0, 1.0));
    }
}

extern "system" fn add_potion_effect(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    type_name: JString<'_>,
    duration: jint,
    amplifier: jint,
) -> jboolean {
    let Ok(name): Result<String, _> = env.get_string(&type_name).map(Into::into) else {
        return false.into();
    };
    let Ok(key) = format!("minecraft:{name}").parse() else {
        return false.into();
    };
    let Some(effect) = REGISTRY.mob_effects.by_key(&key) else {
        return false.into();
    };
    let Ok(text): Result<String, _> = env.get_string(&uuid).map(Into::into) else {
        return 0;
    };
    let Ok(id) = text.parse() else {
        return 0;
    };
    if let Some((_, entity)) = entity_by_uuid(&id)
        && let Some(living) = entity.as_living_entity()
    {
        return living
            .add_mob_effect(MobEffectInstance::with_duration(
                effect, duration, amplifier,
            ))
            .into();
    }
    0
}

extern "system" fn remove_potion_effect(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    type_name: JString<'_>,
) {
    let Ok(name): Result<String, _> = env.get_string(&type_name).map(Into::into) else {
        return;
    };
    let Ok(key) = format!("minecraft:{name}").parse() else {
        return;
    };
    let Some(effect) = REGISTRY.mob_effects.by_key(&key) else {
        return;
    };
    let Ok(text): Result<String, _> = env.get_string(&uuid).map(Into::into) else {
        return;
    };
    let Ok(id) = text.parse() else {
        return;
    };
    if let Some((_, entity)) = entity_by_uuid(&id)
        && let Some(living) = entity.as_living_entity()
    {
        living.remove_mob_effect(effect);
    }
}

extern "system" fn health(mut env: JNIEnv<'_>, _class: JClass<'_>, uuid: JString<'_>) -> jdouble {
    player(&mut env, &uuid).map_or(0.0, |player| f64::from(player.get_health()))
}

/// `foton.Native.setHealth`
extern "system" fn set_health(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    health: jdouble,
) {
    if let Some(player) = player(&mut env, &uuid) {
        player.set_health(health as f32);
    }
}

/// `foton.Native.maxHealth`
extern "system" fn max_health(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jdouble {
    player(&mut env, &uuid).map_or(20.0, |player| f64::from(player.get_max_health()))
}

fn attribute_ref_from_name(name: &str) -> Option<AttributeRef> {
    let key_name = name
        .strip_prefix("GENERIC_")
        .or_else(|| name.strip_prefix("PLAYER_"))
        .unwrap_or(name)
        .to_ascii_lowercase();
    let key = Identifier::from_str(&key_name).ok()?;
    REGISTRY.attributes.by_key(&key)
}

extern "system" fn set_attribute_base(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    attribute: JString<'_>,
    value: jdouble,
) {
    let Ok(uuid_text): Result<String, _> = env.get_string(&uuid).map(Into::into) else {
        return;
    };
    let Ok(attribute): Result<String, _> = env.get_string(&attribute).map(Into::into) else {
        return;
    };
    let Some(id) = Uuid::parse_str(&uuid_text).ok() else {
        return;
    };
    let Some(attribute) = attribute_ref_from_name(&attribute) else {
        return;
    };
    let Some((_, entity)) = entity_by_uuid(&id) else {
        return;
    };
    let Some(living) = entity.as_living_entity() else {
        return;
    };
    living.attributes().lock().set_base_value(attribute, value);
}

/// `foton.Native.playerRespawnWorld`
extern "system" fn player_respawn_world(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jstring {
    let world = player(&mut env, &uuid)
        .and_then(|player| player.respawn_config())
        .map(|config| config.respawn_data.dimension().to_string());
    to_java(&mut env, world)
}

extern "system" fn set_player_respawn_position(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    world_name: JString<'_>,
    x: jint,
    y: jint,
    z: jint,
    yaw: jfloat,
    pitch: jfloat,
) {
    let Ok(name) = env.get_string(&world_name) else {
        return;
    };
    let Ok(dimension) = name.to_str().unwrap_or_default().parse::<Identifier>() else {
        return;
    };
    let Some(player) = player(&mut env, &uuid) else {
        return;
    };
    player.set_bukkit_respawn_position(dimension, BlockPos::new(x, y, z), yaw, pitch);
}

/// `foton.Native.playerRespawnPosition`
extern "system" fn player_respawn_position(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jdoubleArray {
    let position = player(&mut env, &uuid)
        .and_then(|player| player.respawn_config())
        .map(|config| {
            let pos = config.respawn_data.pos();
            [
                f64::from(pos.x()) + 0.5,
                f64::from(pos.y()),
                f64::from(pos.z()) + 0.5,
                f64::from(config.respawn_data.yaw),
                f64::from(config.respawn_data.pitch),
            ]
        });
    to_position(&mut env, position)
}

/// `foton.Native.playerWorld`
extern "system" fn player_entity_effect(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    effect: JString<'_>,
) {
    let Ok(text) = env.get_string(&uuid) else {
        return;
    };
    let Ok(id) = Uuid::parse_str(text.to_str().unwrap_or_default()) else {
        return;
    };
    let Ok(value) = env.get_string(&effect) else {
        return;
    };
    let Some(status) = (match value.to_str().unwrap_or_default() {
        "PROTECTED_FROM_DEATH" => Some(EntityStatus::ProtectedFromDeath),
        _ => None,
    }) else {
        return;
    };
    if let Some((_, entity)) = entity_by_uuid(&id) {
        entity.broadcast_entity_event(status);
    }
}

extern "system" fn player_world(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jstring {
    let key = player(&mut env, &uuid).map(|player| player.get_world().key.to_string());
    to_java(&mut env, key)
}

extern "system" fn advancement_display(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    key: JString<'_>,
) -> jobjectArray {
    let Ok(value) = env.get_string(&key) else {
        return null_mut();
    };
    let Some(id) = value
        .to_str()
        .ok()
        .and_then(|text| text.parse::<Identifier>().ok())
    else {
        return null_mut();
    };
    let Some(advancement) = REGISTRY.advancements.by_key(&id) else {
        return null_mut();
    };
    let Some(display) = advancement.display.as_ref() else {
        return null_mut();
    };
    string_array(
        &mut env,
        &[
            display.title.to_string(),
            display.description.to_string(),
            display.hidden.to_string(),
            display.announce_chat.to_string(),
        ],
    )
}

extern "system" fn advancement_criteria(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    key: JString<'_>,
) -> jobjectArray {
    let Ok(key) = env.get_string(&key) else {
        return null_mut();
    };
    let Ok(key) = key.to_str() else {
        return null_mut();
    };
    let Ok(key) = key.parse::<Identifier>() else {
        return null_mut();
    };
    let Some(advancement) = REGISTRY.advancements.by_key(&key) else {
        return null_mut();
    };
    let values = advancement
        .criteria
        .iter()
        .map(|criterion| criterion.name.to_owned())
        .collect::<Vec<_>>();
    string_array(&mut env, &values)
}

/// `foton.Native.advancementKeys`: every advancement the server knows.
extern "system" fn advancement_keys(mut env: JNIEnv<'_>, _class: JClass<'_>) -> jobjectArray {
    let keys = REGISTRY
        .advancements
        .iter()
        .map(|advancement| advancement.key.to_string())
        .collect::<Vec<_>>();
    string_array(&mut env, &keys)
}

/// An online player and the tree node of an advancement, from their Java
/// names.
fn player_and_advancement(
    env: &mut JNIEnv<'_>,
    uuid: &JString<'_>,
    key: &JString<'_>,
) -> Option<(Arc<Player>, usize)> {
    let player = player(env, uuid)?;
    let key: String = env.get_string(key).ok()?.into();
    let node = ADVANCEMENT_TREE.index_of(&key.parse::<Identifier>().ok()?)?;
    Some((player, node))
}

/// `foton.Native.playerAdvancementProgress`: `1` or `0` for whether the
/// player has the advancement, then one entry per criterion -- its name, and
/// after a unit separator the epoch millisecond it was met, when it was.
/// Null for a player not online or an unknown advancement.
extern "system" fn player_advancement_progress(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    key: JString<'_>,
) -> jobjectArray {
    let Some((player, node)) = player_and_advancement(&mut env, &uuid, &key) else {
        return null_mut();
    };
    let mut values = vec![
        if player.has_advancement(node) {
            "1"
        } else {
            "0"
        }
        .to_owned(),
    ];
    values.extend(
        player
            .advancement_criteria(node)
            .into_iter()
            .map(|(name, obtained)| match obtained {
                Some(millis) => format!("{name}\u{1f}{millis}"),
                None => name.to_owned(),
            }),
    );
    string_array(&mut env, &values)
}

/// `foton.Native.playerAdvancementCriterion`: awards or revokes one
/// criterion, answering whether that changed anything.
///
/// Awarding can finish the advancement and hand out its rewards, which
/// touches the world, so it is done on the tick thread only.
extern "system" fn player_advancement_criterion(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    key: JString<'_>,
    criterion: JString<'_>,
    award: jboolean,
) -> jboolean {
    let Some((player, node)) = player_and_advancement(&mut env, &uuid, &key) else {
        return 0;
    };
    let Ok(criterion) = env.get_string(&criterion).map(String::from) else {
        return 0;
    };
    if award == 0 {
        return jboolean::from(player.revoke_advancement_criterion(node, &criterion));
    }
    if !on_tick() {
        return 0;
    }
    jboolean::from(player.award_advancement_criterion(node, &criterion).granted)
}

/// `foton.Native.whitelistEnabled`: whether admission is limited to the
/// whitelist.
extern "system" fn whitelist_enabled(_env: JNIEnv<'_>, _class: JClass<'_>) -> jboolean {
    jboolean::from(server().is_some_and(|server| server.config.whitelist_enabled))
}

extern "system" fn player_address(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jstring {
    let address = player(&mut env, &uuid)
        .and_then(|player| player.connection.remote_address())
        .map(|address| address.to_string());
    to_java(&mut env, address)
}

/// `foton.Native.sendMessage`
extern "system" fn send_message(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    message: JString<'_>,
) {
    let Some(player) = player(&mut env, &uuid) else {
        return;
    };
    let Ok(text) = env.get_string(&message) else {
        return;
    };
    let text: String = text.into();
    player.send_message(&text.into());
}

/// `foton.Native.kickPlayer`
extern "system" fn chat(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    message: JString<'_>,
) {
    let Some(player) = player(&mut env, &uuid) else {
        return;
    };
    let Ok(message) = env.get_string(&message) else {
        return;
    };
    player.chat_from_plugin(String::from(message));
}

extern "system" fn kick_player(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    message: JString<'_>,
) {
    let Some(player) = player(&mut env, &uuid) else {
        return;
    };
    let Ok(message) = env.get_string(&message) else {
        return;
    };
    player.disconnect(String::from(message));
}

fn set_player_tab_list(
    env: &mut JNIEnv<'_>,
    uuid: &JString<'_>,
    header: Option<JString<'_>>,
    footer: Option<JString<'_>>,
) {
    let Ok(id_text) = env.get_string(uuid) else {
        return;
    };
    let Ok(id) = String::from(id_text).parse::<Uuid>() else {
        return;
    };
    let Some(player) = player(env, uuid) else {
        return;
    };
    let mut lists = player_tab_lists().write();
    let entry = lists
        .entry(id)
        .or_insert_with(|| (TextComponent::plain(""), TextComponent::plain("")));
    if let Some(header) = header {
        let Ok(value) = env.get_string(&header) else {
            return;
        };
        entry.0 = String::from(value).into();
    }
    if let Some(footer) = footer {
        let Ok(value) = env.get_string(&footer) else {
            return;
        };
        entry.1 = String::from(value).into();
    }
    player.send_packet(CTabList::new(&entry.0, &entry.1, player.as_ref()));
}

/// `foton.Native.setPlayerListHeader`
extern "system" fn set_player_list_name(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    name: JString<'_>,
) {
    let Some(player) = player(&mut env, &uuid) else {
        return;
    };
    let Ok(name) = env.get_string(&name) else {
        return;
    };
    let name: String = name.into();
    let value = (!name.is_empty()).then(|| TextComponent::plain(name));
    player.set_tab_list_name(value);
}

extern "system" fn set_player_list_header(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    header: JString<'_>,
) {
    set_player_tab_list(&mut env, &uuid, Some(header), None);
}

/// `foton.Native.setPlayerListFooter`
extern "system" fn set_player_list_footer(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    footer: JString<'_>,
) {
    set_player_tab_list(&mut env, &uuid, None, Some(footer));
}

/// `foton.Native.setPlayerListHeaderFooter`
extern "system" fn set_player_list_header_footer(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    header: JString<'_>,
    footer: JString<'_>,
) {
    set_player_tab_list(&mut env, &uuid, Some(header), Some(footer));
}

/// `foton.Native.sendActionBar`
extern "system" fn send_action_bar(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    message: JString<'_>,
) {
    let Some(player) = player(&mut env, &uuid) else {
        return;
    };
    let Ok(message) = env.get_string(&message) else {
        return;
    };
    let message: TextComponent = String::from(message).into();
    player.send_packet(CSystemChat::new(&message, true, player.as_ref()));
}

/// `foton.Native.sendTitle`
extern "system" fn send_title(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    title: JString<'_>,
    subtitle: JString<'_>,
    fade_in: jint,
    stay: jint,
    fade_out: jint,
) {
    let Some(player) = player(&mut env, &uuid) else {
        return;
    };
    let Ok(title) = env.get_string(&title) else {
        return;
    };
    let Ok(subtitle) = env.get_string(&subtitle) else {
        return;
    };
    let title: TextComponent = String::from(title).into();
    let subtitle: TextComponent = String::from(subtitle).into();
    player.send_packet(CSetTitlesAnimation {
        fade_in,
        stay,
        fade_out,
    });
    player.send_packet(CSetTitleText::new(&title, player.as_ref()));
    player.send_packet(CSetSubtitleText::new(&subtitle, player.as_ref()));
}

/// `foton.Native.clearTitle`
extern "system" fn clear_title(mut env: JNIEnv<'_>, _class: JClass<'_>, uuid: JString<'_>) {
    let Some(player) = player(&mut env, &uuid) else {
        return;
    };
    player.send_packet(CClearTitles { reset_times: true });
}

/// `foton.Native.sendPluginMessage`
extern "system" fn send_plugin_message(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    channel: JString<'_>,
    message: JByteArray<'_>,
) {
    let Some(player) = player(&mut env, &uuid) else {
        return;
    };
    let Ok(channel) = env.get_string(&channel).map(String::from) else {
        return;
    };
    let Ok(channel) = channel.parse::<Identifier>() else {
        return;
    };
    let Ok(message) = env.convert_byte_array(&message) else {
        return;
    };
    player.send_packet(CCustomPayload::new(channel, message.into_boxed_slice()));
}

/// `foton.Native.hasPermission`
extern "system" fn has_permission(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    permission: JString<'_>,
) -> jboolean {
    let Some(player) = player(&mut env, &uuid) else {
        return u8::from(false);
    };
    let Ok(name) = env.get_string(&permission) else {
        return u8::from(false);
    };
    let name: String = name.into();
    // A key that does not parse is not a permission anyone holds, which is the
    // same answer as not holding it and a great deal calmer than a panic.
    let Ok(key) = PermissionKey::parse(name) else {
        return u8::from(false);
    };
    u8::from(player.has_permission(&PermissionExpr::key(key)))
}

/// The thread the game tick runs on, learned from the tick itself.
///
/// A plugin may write a block only from that thread: `World::set_block` says
/// its callers must be inside Foton's serialized world-mutation phase, and a
/// JVM thread is not. Knowing which thread that is means a write from an event
/// handler or a scheduled task -- which is where nearly every write comes
/// from -- can happen at once and read back immediately, while a write from a
/// plugin's own thread waits for the next tick instead of racing the palette.
static TICK_THREAD: SyncMutex<Option<ThreadId>> = SyncMutex::new(None);

/// Block writes that arrived from somewhere other than the tick.
static DEFERRED: SyncMutex<Vec<(Identifier, BlockPos, BlockStateId)>> = SyncMutex::new(Vec::new());

/// Records that this is the tick thread, and runs what was waiting for it.
pub(crate) fn begin_tick(server: &Arc<Server>) {
    *TICK_THREAD.lock() = Some(thread::current().id());

    let pending = mem::take(&mut *DEFERRED.lock());
    for (world, pos, state) in pending {
        if let Some(world) = server.worlds.get_owned(&world) {
            world.set_block(pos, state, UpdateFlags::UPDATE_ALL);
        }
    }
}

/// Whether the caller may write to the world right now.
fn on_tick() -> bool {
    *TICK_THREAD.lock() == Some(thread::current().id())
}

/// `foton.Native.isPrimaryThread`
extern "system" fn is_primary_thread(_env: JNIEnv<'_>, _class: JClass<'_>) -> jboolean {
    u8::from(on_tick())
}

/// `foton.Native.experienceLevel`
extern "system" fn set_experience_level(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    level: jint,
) {
    if let Some(player) = player(&mut env, &uuid) {
        player.experience.lock().set_levels(level);
    }
}

extern "system" fn experience_progress(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jfloat {
    player(&mut env, &uuid).map_or(0.0, |player| player.experience.lock().progress())
}

extern "system" fn set_experience_progress(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    progress: jfloat,
) {
    if progress.is_finite()
        && (0.0..=1.0).contains(&progress)
        && let Some(player) = player(&mut env, &uuid)
    {
        player.experience.lock().set_progress(progress);
    }
}

extern "system" fn total_experience(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jint {
    player(&mut env, &uuid).map_or(0, |value| value.total_experience())
}

extern "system" fn set_total_experience(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    total: jint,
) {
    if let Some(value) = player(&mut env, &uuid) {
        value.set_total_experience(total);
    }
}

extern "system" fn give_experience(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    amount: jint,
) {
    if let Some(player) = player(&mut env, &uuid) {
        player.experience.lock().add_points(amount);
    }
}

extern "system" fn experience_level(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jint {
    let Some(player) = player(&mut env, &uuid) else {
        return 0;
    };
    player.experience.lock().level()
}

/// `foton.Native.savePlayers`
extern "system" fn save_players(_env: JNIEnv<'_>, _class: JClass<'_>) {
    if let Some(server) = server() {
        server.request_save_players();
    }
}

/// `foton.Native.shutdown`
extern "system" fn shutdown(_env: JNIEnv<'_>, _class: JClass<'_>) {
    if let Some(server) = server() {
        server.cancel_token.cancel();
    }
}

/// One inventory slot, written the way `foton.Native.inventorySlot` promises.
///
/// An empty slot is the empty string and an unreadable one is Java's null, so
/// a plugin can tell "there is nothing here" from "this cannot be answered"
/// rather than reading a missing armor slot as bare feet.
fn hex_encode(bytes: &[u8]) -> String {
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut out, byte| {
            let _ = write!(out, "{byte:02x}");
            out
        })
}
fn hex_decode(value: &str) -> Vec<u8> {
    (0..value.len())
        .step_by(2)
        .filter_map(|index| u8::from_str_radix(value.get(index..index + 2)?, 16).ok())
        .collect()
}

fn append_potion_effects(value: &mut String, stack: &ItemStack) {
    let Some(contents) = stack.get(POTION_CONTENTS) else {
        return;
    };
    if let Some(potion) = contents.potion() {
        value.push_str("\u{1d}basepotionhex=");
        value.push_str(&hex_encode(potion.value().key.to_string().as_bytes()));
    }
    if contents.custom_effects().is_empty() {
        return;
    }

    value.push_str("\u{1d}potioneffects=");
    for effect in contents.custom_effects() {
        let _ = write!(
            value,
            "{},{},{},{},{},{};",
            effect.effect().key.path,
            effect.duration(),
            effect.amplifier(),
            effect.ambient(),
            effect.show_particles(),
            effect.show_icon()
        );
    }
}

pub(crate) fn describe_slot(stack: &ItemStack) -> String {
    if stack.is_empty() {
        return String::new();
    }
    let mut value = format!("{} {}", stack.item().key, stack.count());
    if let Some(damage) = stack.get(DAMAGE).copied().filter(|damage| *damage != 0) {
        value.push('\u{1d}');
        let _ = write!(value, "damage={damage}");
    }
    if let Some(opaque) = stack.opaque_nbt() {
        value.push('\u{1d}');
        value.push_str("nbthex=");
        value.push_str(&hex_encode(opaque.as_bytes()));
    }
    if stack.has(UNBREAKABLE) {
        value.push('\u{1d}');
        value.push_str("unbreakable");
    }
    if let Some(model) = stack.get(ITEM_MODEL) {
        value.push('\u{1d}');
        let _ = write!(
            value,
            "itemmodelhex={}",
            hex_encode(model.to_string().as_bytes())
        );
    }
    if let Some(style) = stack.get(TOOLTIP_STYLE) {
        value.push('\u{1d}');
        let _ = write!(
            value,
            "tooltipstylehex={}",
            hex_encode(style.to_string().as_bytes())
        );
    }
    if let Some(display) = stack.get(TOOLTIP_DISPLAY)
        && display.hide_tooltip
    {
        value.push('\u{1d}');
        value.push_str("hidetooltip");
    }
    if let Some(model) = stack.get(CUSTOM_MODEL_DATA) {
        for float in model.floats() {
            value.push('\u{1d}');
            let _ = write!(value, "modelfloat={float}");
        }
        for flag in model.flags() {
            value.push('\u{1d}');
            let _ = write!(value, "modelflag={flag}");
        }
        for string in model.strings() {
            value.push('\u{1d}');
            let _ = write!(value, "modelstrhex={}", hex_encode(string.as_bytes()));
        }
        for color in model.colors() {
            value.push('\u{1d}');
            let _ = write!(value, "modelcolor={color}");
        }
    }
    if let Some(enchantments) = stack.get(ENCHANTMENTS) {
        for (key, level) in enchantments.iter() {
            value.push('\u{1d}');
            let _ = write!(
                value,
                "enchhex={}:{}",
                hex_encode(key.to_string().as_bytes()),
                level
            );
        }
    }
    if let Some(enchantments) = stack.get(STORED_ENCHANTMENTS) {
        for (key, level) in enchantments.iter() {
            value.push('\u{1d}');
            let _ = write!(
                value,
                "storedenchhex={}:{}",
                hex_encode(key.to_string().as_bytes()),
                level
            );
        }
    }
    item_components::describe(stack, &mut value);
    append_potion_effects(&mut value, stack);
    value
}

extern "system" fn item_translation_key(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    item: JString<'_>,
) -> jstring {
    let key = env.get_string(&item).ok().and_then(|value| {
        let id: Identifier = value.to_str().ok()?.parse().ok()?;
        let item = REGISTRY.items.by_key(&id)?;
        let name = item.components.get(ITEM_NAME)?;
        match &name.content {
            TextContent::Translate(message) => Some(message.key.to_string()),
            _ => None,
        }
    });
    to_java(&mut env, key)
}

/// Reads back what `describe_slot` wrote.
#[expect(
    clippy::too_many_lines,
    reason = "one flat match over every component an item can carry; the length is \
              the component list, and splitting it would scatter one format across \
              several functions"
)]
pub(crate) fn parse_slot(text: &str) -> Option<ItemStack> {
    let mut encoded = text.trim().split('\u{1d}');
    let text = encoded.next().unwrap_or_default();
    let metadata = encoded.collect::<Vec<_>>();
    if metadata
        .iter()
        .filter(|value| value.starts_with("lorehex="))
        .count()
        > 256
    {
        return None;
    }
    if text.is_empty() {
        return Some(ItemStack::empty());
    }
    let (name, count) = text.rsplit_once(' ')?;
    let count: i32 = count.parse().ok()?;
    let key: Identifier = name.parse().ok()?;
    let item = REGISTRY.items.by_key(&key)?;
    if count <= 0 {
        return None;
    }
    let mut stack = ItemStack::with_count(item, count);
    if let Some(encoded) = metadata
        .iter()
        .find_map(|value| value.strip_prefix("nbthex="))
        && let Ok(raw) = String::from_utf8(hex_decode(encoded))
    {
        stack.set_opaque_nbt(Some(raw));
    }
    let damage = metadata
        .iter()
        .find_map(|value| value.strip_prefix("damage="))
        .and_then(|value| value.parse::<i32>().ok());
    if let Some(damage) = damage {
        stack.set_damage_value(damage);
    }
    if let Some(encoded) = metadata
        .iter()
        .find_map(|value| value.strip_prefix("namehex="))
    {
        let bytes = (0..encoded.len())
            .step_by(2)
            .filter_map(|index| u8::from_str_radix(encoded.get(index..index + 2)?, 16).ok())
            .collect::<Vec<_>>();
        if let Ok(name) = String::from_utf8(bytes) {
            use foton_registry::data_components::vanilla_components::CUSTOM_NAME;
            stack.set(CUSTOM_NAME, TextComponent::plain(name));
        }
    }
    let lore_lines = metadata
        .iter()
        .filter_map(|value| value.strip_prefix("lorehex="))
        .filter_map(|encoded| {
            let bytes = (0..encoded.len())
                .step_by(2)
                .filter_map(|index| u8::from_str_radix(encoded.get(index..index + 2)?, 16).ok())
                .collect::<Vec<_>>();
            String::from_utf8(bytes).ok().map(TextComponent::plain)
        })
        .collect::<Vec<_>>();
    if !lore_lines.is_empty()
        && let Ok(lore) = ItemLore::new(lore_lines)
    {
        stack.set(LORE, lore);
    }
    if metadata.contains(&"unbreakable") {
        stack.set(UNBREAKABLE, ());
    }
    if metadata.contains(&"hidetooltip") {
        stack.set(TOOLTIP_DISPLAY, TooltipDisplay::new(true));
    }
    if let Some(encoded) = metadata
        .iter()
        .find_map(|value| value.strip_prefix("tooltipstylehex="))
        && let Ok(style) = String::from_utf8(hex_decode(encoded))
        && let Ok(style) = style.parse()
    {
        stack.set(TOOLTIP_STYLE, style);
    }
    if let Some(encoded) = metadata
        .iter()
        .find_map(|value| value.strip_prefix("itemmodelhex="))
        && let Ok(model) = String::from_utf8(hex_decode(encoded))
        && let Ok(model) = model.parse()
    {
        stack.set(ITEM_MODEL, model);
    }
    let floats = metadata
        .iter()
        .filter_map(|value| value.strip_prefix("modelfloat="))
        .filter_map(|value| value.parse::<f32>().ok())
        .filter(|value| value.is_finite())
        .collect::<Vec<_>>();
    let flags = metadata
        .iter()
        .filter_map(|value| value.strip_prefix("modelflag="))
        .filter_map(|value| value.parse::<bool>().ok())
        .collect::<Vec<_>>();
    let strings = metadata
        .iter()
        .filter_map(|value| value.strip_prefix("modelstrhex="))
        .filter_map(|value| String::from_utf8(hex_decode(value)).ok())
        .collect::<Vec<_>>();
    let colors = metadata
        .iter()
        .filter_map(|value| value.strip_prefix("modelcolor="))
        .filter_map(|value| value.parse::<i32>().ok())
        .collect::<Vec<_>>();
    if !floats.is_empty() || !flags.is_empty() || !strings.is_empty() || !colors.is_empty() {
        stack.set(
            CUSTOM_MODEL_DATA,
            CustomModelData::new(floats, flags, strings, colors),
        );
    } else if let Some(model) = metadata
        .iter()
        .find_map(|value| value.strip_prefix("model="))
        .and_then(|value| value.parse::<f32>().ok())
        && model.is_finite()
    {
        stack.set(
            CUSTOM_MODEL_DATA,
            CustomModelData::new(vec![model], Vec::new(), Vec::new(), Vec::new()),
        );
    }
    let mut enchantments = ItemEnchantments::empty();
    for encoded in metadata
        .iter()
        .filter_map(|value| value.strip_prefix("enchhex="))
    {
        if let Some((key, level)) = encoded.rsplit_once(':') {
            let bytes = hex_decode(key);
            if let (Ok(name), Ok(level)) = (String::from_utf8(bytes), level.parse::<u32>())
                && let Ok(key) = name.parse()
            {
                enchantments.set(key, level);
            }
        }
    }
    if !enchantments.is_empty() {
        stack.set(ENCHANTMENTS, enchantments);
    }
    let mut stored = ItemEnchantments::empty();
    for encoded in metadata
        .iter()
        .filter_map(|value| value.strip_prefix("storedenchhex="))
    {
        if let Some((key, level)) = encoded.rsplit_once(':') {
            let bytes = hex_decode(key);
            if let (Ok(name), Ok(level)) = (String::from_utf8(bytes), level.parse::<u32>())
                && let Ok(key) = name.parse()
            {
                stored.set(key, level);
            }
        }
    }
    if !stored.is_empty() {
        stack.set(STORED_ENCHANTMENTS, stored);
    }
    item_components::parse(&mut stack, &metadata);
    let effects = metadata
        .iter()
        .find_map(|value| value.strip_prefix("potioneffects="))
        .or_else(|| {
            metadata.iter().copied().find(|value| {
                !value.contains('=') && *value != "hidetooltip" && *value != "unbreakable"
            })
        });
    let base_potion = metadata
        .iter()
        .find_map(|value| value.strip_prefix("basepotionhex="))
        .and_then(|encoded| String::from_utf8(hex_decode(encoded)).ok())
        .and_then(|name| name.parse::<Identifier>().ok())
        .and_then(|key| REGISTRY.potions.by_key(&key))
        .map(RegistryReference::new);
    let mut custom = Vec::new();
    if let Some(effects) = effects {
        use foton_registry::mob_effect::instance::MobEffectInstance;
        for field in effects.split(';') {
            let parts = field.split(',').collect::<Vec<_>>();
            let (name, duration, amplifier, ambient, show_particles, show_icon) = match parts[..] {
                [name, duration, amplifier] => (name, duration, amplifier, false, true, true),
                [
                    name,
                    duration,
                    amplifier,
                    ambient,
                    show_particles,
                    show_icon,
                ] => {
                    let (Ok(ambient), Ok(show_particles), Ok(show_icon)) = (
                        ambient.parse::<bool>(),
                        show_particles.parse::<bool>(),
                        show_icon.parse::<bool>(),
                    ) else {
                        continue;
                    };
                    (
                        name,
                        duration,
                        amplifier,
                        ambient,
                        show_particles,
                        show_icon,
                    )
                }
                _ => continue,
            };
            let (Ok(duration), Ok(amplifier)) = (duration.parse(), amplifier.parse()) else {
                continue;
            };
            let Ok(key) = format!("minecraft:{name}").parse() else {
                continue;
            };
            let Some(effect) = REGISTRY.mob_effects.by_key(&key) else {
                continue;
            };
            custom.push(MobEffectInstance::new(
                effect,
                duration,
                amplifier,
                ambient,
                show_particles,
                show_icon,
                None,
            ));
        }
    }
    if base_potion.is_some() || !custom.is_empty() {
        use foton_registry::data_components::components::PotionContents;
        stack.set(
            POTION_CONTENTS,
            PotionContents::new(base_potion, None, custom, None),
        );
    }
    Some(stack)
}

/// A block state as `minecraft:name[facing=north]`, the way `/setblock` writes it.
pub(crate) fn describe_state(state: BlockStateId) -> Option<String> {
    let block = REGISTRY.blocks.by_state_id(state)?;
    let properties = REGISTRY.blocks.get_properties(state);
    if properties.is_empty() {
        return Some(block.key.to_string());
    }
    let listed = properties
        .iter()
        .map(|(name, value)| format!("{name}={value}"))
        .collect::<Vec<_>>()
        .join(",");
    Some(format!("{}[{listed}]", block.key))
}

/// Reads back what `describe_state` wrote.
fn parse_state(text: &str) -> Option<BlockStateId> {
    let text = text.trim();
    let (name, rest) = match text.split_once('[') {
        Some((name, rest)) => (name, rest.strip_suffix(']')?),
        None => (text, ""),
    };
    let key: Identifier = name.parse().ok()?;
    let pairs: Vec<(&str, &str)> = if rest.is_empty() {
        Vec::new()
    } else {
        rest.split(',')
            .filter_map(|pair| pair.split_once('='))
            .map(|(name, value)| (name.trim(), value.trim()))
            .collect()
    };
    REGISTRY.blocks.state_id_from_properties(&key, &pairs)
}

/// `foton.Native.isOperator`
extern "system" fn statistic_value(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    statistic: JString<'_>,
) -> jint {
    let Ok(statistic) = env.get_string(&statistic) else {
        return 0;
    };
    let Some(player) = player(&mut env, &uuid) else {
        return 0;
    };
    let Some(stat) = statistic.to_str().ok().and_then(bukkit_custom_stat) else {
        return 0;
    };
    player.stat_value(Stat::custom(stat))
}

/// The vanilla custom statistic an untyped Bukkit `Statistic` names.
///
/// Bukkit keys one `minecraft:<lowercase name>`, which is vanilla's own key
/// except where vanilla renamed the statistic since; those go through the
/// rename vanilla's `DataFixers` applies to old saves.
fn bukkit_custom_stat(name: &str) -> Option<CustomStatRef> {
    let legacy = name.to_ascii_lowercase();
    let current = match legacy.as_str() {
        // Vanilla `DataFixers`: the `StatsRenameFix` pair
        // `minecraft:play_one_minute` -> `minecraft:play_time`.
        "play_one_minute" => "play_time",
        other => other,
    };
    REGISTRY
        .custom_stats
        .by_key(&Identifier::vanilla(current.to_owned()))
}

extern "system" fn offline_statistic(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    statistic: JString<'_>,
) -> jint {
    let Ok(uuid_text) = env.get_string(&uuid) else {
        return 0;
    };
    let Ok(statistic_text) = env.get_string(&statistic) else {
        return 0;
    };
    let Ok(uuid) = Uuid::parse_str(uuid_text.to_str().unwrap_or_default()) else {
        return 0;
    };
    let Some(stat) = bukkit_custom_stat(statistic_text.to_str().unwrap_or_default()) else {
        return 0;
    };
    server().map_or(0, |server| server.offline_statistic(uuid, &stat.key))
}

extern "system" fn is_operator(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jboolean {
    u8::from(player(&mut env, &uuid).is_some_and(|player| player.is_operator()))
}

extern "system" fn offline_is_operator(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jboolean {
    let Ok(value) = env.get_string(&uuid) else {
        return 0;
    };
    let Ok(uuid) = value
        .to_str()
        .ok()
        .and_then(|text| text.parse().ok())
        .ok_or(())
    else {
        return 0;
    };
    u8::from(server().is_some_and(|server| server.is_operator(uuid)))
}

extern "system" fn offline_is_whitelisted(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jboolean {
    let Ok(value) = env.get_string(&uuid) else {
        return 0;
    };
    let Ok(uuid) = value
        .to_str()
        .ok()
        .and_then(|text| text.parse().ok())
        .ok_or(())
    else {
        return 0;
    };
    u8::from(server().is_some_and(|server| {
        server
            .global_player_data(uuid)
            .is_some_and(|data| data.whitelisted)
    }))
}

extern "system" fn is_whitelisted(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jboolean {
    let Some(server) = server() else {
        return 0;
    };
    let Ok(value) = env.get_string(&uuid) else {
        return 0;
    };
    let Ok(uuid) = value
        .to_str()
        .ok()
        .and_then(|text| text.parse().ok())
        .ok_or(())
    else {
        return 0;
    };
    u8::from(
        server
            .global_player_data(uuid)
            .is_some_and(|data| data.whitelisted),
    )
}

extern "system" fn set_player_whitelisted(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    value: jboolean,
) {
    let Some(server) = server() else {
        return;
    };
    let Ok(text): Result<String, _> = env.get_string(&uuid).map(Into::into) else {
        return;
    };
    let Ok(uuid) = Uuid::parse_str(&text) else {
        return;
    };
    server.queue_player_whitelist_update(uuid, value != 0);
}

extern "system" fn effective_permissions(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jobjectArray {
    let Some(player) = player(&mut env, &uuid) else {
        return null_mut();
    };
    let values = player
        .permissions()
        .entries()
        .iter()
        .map(|entry| {
            format!(
                "{}|{}",
                entry.key().as_str(),
                u8::from(matches!(entry.state(), PermissionState::Allow))
            )
        })
        .collect::<Vec<_>>();
    string_array(&mut env, &values)
}

extern "system" fn is_permission_set(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    permission: JString<'_>,
) -> jboolean {
    let Some(player) = player(&mut env, &uuid) else {
        return 0;
    };
    let Ok(permission) = env.get_string(&permission) else {
        return 0;
    };
    let permission: String = permission.into();
    let Ok(key) = PermissionKey::parse(permission) else {
        return 0;
    };
    u8::from(player.permission_state(&PermissionExpr::key(key)).is_some())
}

/// `foton.Native.biomeKey`
extern "system" fn biome_key(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    world_name: JString<'_>,
    x: jint,
    y: jint,
    z: jint,
) -> jstring {
    let Some(world) = world(&mut env, &world_name) else {
        return null_mut();
    };
    to_java(&mut env, world.biome_key_at(BlockPos::new(x, y, z)))
}

extern "system" fn block_piston_reaction(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    x: jint,
    y: jint,
    z: jint,
) -> jstring {
    let Some(world) = world(&mut env, &name) else {
        return null_mut();
    };
    let state = world.get_block_state(BlockPos::new(x, y, z));
    let value = match state.get_block().config.push_reaction {
        PushReaction::Normal => "NORMAL",
        PushReaction::Destroy => "BREAK",
        PushReaction::Block => "BLOCK",
        PushReaction::Ignore => "IGNORE",
        PushReaction::PushOnly => "PUSH_ONLY",
    };
    to_java(&mut env, Some(value.to_owned()))
}

extern "system" fn block_state(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    x: jint,
    y: jint,
    z: jint,
) -> jstring {
    let described = world(&mut env, &name)
        .and_then(|world| describe_state(world.get_block_state(BlockPos::new(x, y, z))));
    to_java(&mut env, described)
}

extern "system" fn recipe_result(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    key: JString<'_>,
) -> jstring {
    let value = env.get_string(&key).ok().and_then(|text| {
        let key = text.to_str().ok()?.parse().ok()?;
        let recipe = foton_registry::REGISTRY.recipes.result_by_id(&key)?;
        Some(format!("{}|{}", recipe.item.key(), recipe.count))
    });
    to_java(&mut env, value)
}

extern "system" fn recipe_remove(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    key: JString<'_>,
) -> jboolean {
    let removed = env
        .get_string(&key)
        .ok()
        .and_then(|text| text.to_str().ok()?.parse().ok())
        .is_some_and(|key| foton_registry::REGISTRY.recipes.remove(&key));
    jboolean::from(removed)
}

extern "system" fn recipe_add_shapeless(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    key: JString<'_>,
    result: JString<'_>,
    count: jint,
    ingredients: JObjectArray<'_>,
) -> jboolean {
    let Some(key) = env
        .get_string(&key)
        .ok()
        .and_then(|v| v.to_str().ok()?.parse().ok())
    else {
        return jboolean::from(false);
    };
    let Some(result_key) = env
        .get_string(&result)
        .ok()
        .and_then(|v| v.to_str().ok()?.parse().ok())
    else {
        return jboolean::from(false);
    };
    let Some(result_item) = REGISTRY.items.by_key(&result_key) else {
        return jboolean::from(false);
    };
    if count <= 0 {
        return jboolean::from(false);
    }
    let Ok(length) = env.get_array_length(&ingredients) else {
        return jboolean::from(false);
    };
    let mut parsed = Vec::with_capacity(length as usize);
    for index in 0..length {
        let Ok(value) = env.get_object_array_element(&ingredients, index) else {
            return jboolean::from(false);
        };
        let value = JString::from(value);
        let Ok(text) = env.get_string(&value) else {
            return jboolean::from(false);
        };
        let Ok(item_key) = text.to_str().unwrap_or_default().parse::<Identifier>() else {
            return jboolean::from(false);
        };
        let Some(item) = REGISTRY.items.by_key(&item_key) else {
            return jboolean::from(false);
        };
        parsed.push(Ingredient::Item(item));
    }
    let ingredients: &'static [Ingredient] = Box::leak(parsed.into_boxed_slice());
    jboolean::from(
        REGISTRY
            .recipes
            .register_runtime_shapeless(ShapelessRecipe {
                id: key,
                category: CraftingCategory::Misc,
                ingredients,
                result: RecipeResult {
                    item: result_item,
                    count,
                },
                // Bukkit's defaults: a recipe that sets neither is ungrouped
                // and filed under misc, and it unlocks with a toast.
                show_notification: true,
                group: Cow::Borrowed(""),
            }),
    )
}

extern "system" fn recipe_add_shaped(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    key: JString<'_>,
    result: JString<'_>,
    count: jint,
    shape: JObjectArray<'_>,
    ingredients: JObjectArray<'_>,
) -> jboolean {
    let Some(key) = env
        .get_string(&key)
        .ok()
        .and_then(|v| v.to_str().ok()?.parse().ok())
    else {
        return jboolean::from(false);
    };
    let Some(result_key) = env
        .get_string(&result)
        .ok()
        .and_then(|v| v.to_str().ok()?.parse().ok())
    else {
        return jboolean::from(false);
    };
    let Some(result_item) = REGISTRY.items.by_key(&result_key) else {
        return jboolean::from(false);
    };
    if count <= 0 {
        return jboolean::from(false);
    }
    let Some(rows) = read_string_array(&mut env, &shape) else {
        return jboolean::from(false);
    };
    // Widths are counted in characters, because that is what the pattern below
    // is filled with. Counting bytes here instead let a non-ASCII key -- which
    // every guard accepted, since the character resolves fine -- record a
    // `width` larger than the pattern it describes. Nothing failed at
    // registration: the panic came later, out of `matches_at`, in the crafting
    // loop of whichever player happened to lay out two items in a row of three,
    // with nothing left to point at the plugin that caused it.
    if rows.is_empty()
        || rows.len() > 3
        || rows
            .iter()
            .any(|row| row.is_empty() || row.chars().count() > 3)
    {
        return jboolean::from(false);
    }
    let width = rows[0].chars().count();
    if rows.iter().any(|row| row.chars().count() != width) {
        return jboolean::from(false);
    }
    let Some(definitions) = read_string_array(&mut env, &ingredients) else {
        return jboolean::from(false);
    };
    let mut parsed = FxHashMap::default();
    for definition in definitions {
        let Some((character, item_name)) = definition.split_once('=') else {
            return jboolean::from(false);
        };
        let mut chars = character.chars();
        let Some(character) = chars.next() else {
            return jboolean::from(false);
        };
        if chars.next().is_some() || character == ' ' {
            return jboolean::from(false);
        }
        let Ok(item_key) = item_name.parse::<Identifier>() else {
            return jboolean::from(false);
        };
        let Some(item) = REGISTRY.items.by_key(&item_key) else {
            return jboolean::from(false);
        };
        if parsed.insert(character, Ingredient::Item(item)).is_some() {
            return jboolean::from(false);
        }
    }
    let mut pattern = Vec::with_capacity(width * rows.len());
    for row in &rows {
        for character in row.chars() {
            if character == ' ' {
                pattern.push(Ingredient::Empty);
            } else if let Some(ingredient) = parsed.get(&character) {
                pattern.push(ingredient.clone());
            } else {
                return jboolean::from(false);
            }
        }
    }
    let pattern: &'static [Ingredient] = Box::leak(pattern.into_boxed_slice());
    jboolean::from(REGISTRY.recipes.register_runtime_shaped(ShapedRecipe::new(
        key,
        CraftingCategory::Misc,
        width,
        rows.len(),
        pattern,
        RecipeResult {
            item: result_item,
            count,
        },
        true,
        Cow::Borrowed(""),
    )))
}

extern "system" fn recipe_list(mut env: JNIEnv<'_>, _class: JClass<'_>) -> jobjectArray {
    let values: Vec<String> = foton_registry::REGISTRY
        .recipes
        .iter_crafting()
        .map(|recipe| {
            format!(
                "{}|{}|{}",
                recipe.id(),
                recipe.result().item.key(),
                recipe.result().count
            )
        })
        .collect();
    string_array(&mut env, &values)
}

extern "system" fn block_indirectly_powered(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    x: jint,
    y: jint,
    z: jint,
) -> jboolean {
    jboolean::from(
        world(&mut env, &name)
            .is_some_and(|world| world.has_neighbor_signal(BlockPos::new(x, y, z))),
    )
}

extern "system" fn block_light(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    x: jint,
    y: jint,
    z: jint,
) -> jbyte {
    world(&mut env, &name).map_or(0, |world| {
        world.light_value_at(LightLayer::Block, BlockPos::new(x, y, z)) as jbyte
    })
}

extern "system" fn sky_light(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    x: jint,
    y: jint,
    z: jint,
) -> jbyte {
    world(&mut env, &name).map_or(0, |world| {
        world.light_value_at(LightLayer::Sky, BlockPos::new(x, y, z)) as jbyte
    })
}

extern "system" fn block_passable(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    x: jint,
    y: jint,
    z: jint,
) -> jboolean {
    world(&mut env, &name).map_or(jboolean::from(false), |world| {
        let pos = BlockPos::new(x, y, z);
        jboolean::from(
            world
                .get_block_state(pos)
                .get_collision_shape_at(pos)
                .is_empty(),
        )
    })
}

/// Returns the item currently stored in a lectern, without discarding its book type.
extern "system" fn lectern_book(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    x: jint,
    y: jint,
    z: jint,
) -> jstring {
    let value = world(&mut env, &name).and_then(|world| {
        let entity = world.get_block_entity(BlockPos::new(x, y, z))?;
        let lectern = entity.downcast_ref::<LecternBlockEntity>()?;
        let book = lectern.book();
        (!book.is_empty()).then(|| describe_slot(&book))
    });
    to_java(&mut env, value)
}

/// Returns the plain pages currently stored in a lectern book.
extern "system" fn lectern_book_pages(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    x: jint,
    y: jint,
    z: jint,
) -> jobjectArray {
    let Some(world) = world(&mut env, &name) else {
        return null_mut();
    };
    let Some(entity) = world.get_block_entity(BlockPos::new(x, y, z)) else {
        return null_mut();
    };
    let Some(lectern) = entity.downcast_ref::<LecternBlockEntity>() else {
        return null_mut();
    };
    let book = lectern.book();
    let pages: Vec<String> = if let Some(written) = book.get(WRITTEN_BOOK_CONTENT) {
        written
            .pages()
            .iter()
            .map(|page| page.get(false).to_plain(&DisplayResolutor))
            .collect()
    } else if let Some(writable) = book.get(WRITABLE_BOOK_CONTENT) {
        writable
            .pages()
            .iter()
            .map(|page| page.get(false).clone())
            .collect()
    } else {
        Vec::new()
    };
    string_array(&mut env, &pages)
}

/// Removes the book from a lectern and returns the slot to its vanilla empty state.
extern "system" fn lectern_clear_book(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    x: jint,
    y: jint,
    z: jint,
) {
    let Some(world) = world(&mut env, &name) else {
        return;
    };
    let Some(entity) = world.get_block_entity(BlockPos::new(x, y, z)) else {
        return;
    };
    let Some(lectern) = entity.downcast_ref::<LecternBlockEntity>() else {
        return;
    };
    let _ = lectern.take_book();
}

/// Sets a lectern book from the API's item encoding; non-book items are rejected.
extern "system" fn lectern_set_book(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    x: jint,
    y: jint,
    z: jint,
    item: JString<'_>,
) -> jboolean {
    let Ok(text) = env.get_string(&item) else {
        return 0;
    };
    let Some(book) = parse_slot(&String::from(text)) else {
        return 0;
    };
    if *book.item() != *vanilla_items::WRITABLE_BOOK && *book.item() != *vanilla_items::WRITTEN_BOOK
    {
        return 0;
    }
    let Some(world) = world(&mut env, &name) else {
        return 0;
    };
    let Some(entity) = world.get_block_entity(BlockPos::new(x, y, z)) else {
        return 0;
    };
    let Some(lectern) = entity.downcast_ref::<LecternBlockEntity>() else {
        return 0;
    };
    lectern.set_book(book);
    1
}

/// `foton.Native.setBlock`
extern "system" fn set_block(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    x: jint,
    y: jint,
    z: jint,
    state: JString<'_>,
) {
    let Some(world) = world(&mut env, &name) else {
        return;
    };
    let Ok(text) = env.get_string(&state) else {
        return;
    };
    let text: String = text.into();
    let Some(state) = parse_state(&text) else {
        return;
    };
    let pos = BlockPos::new(x, y, z);
    if on_tick() {
        world.set_block(pos, state, UpdateFlags::UPDATE_ALL);
    } else {
        // Off the tick. Writing here would race the palette, so it waits.
        DEFERRED.lock().push((world.key.clone(), pos, state));
    }
}

extern "system" fn break_block(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    x: jint,
    y: jint,
    z: jint,
) -> jboolean {
    if !on_tick() {
        return false.into();
    }
    world(&mut env, &name)
        .is_some_and(|world| world.destroy_block(BlockPos::new(x, y, z), true))
        .into()
}

/// `foton.Native.playSound`
extern "system" fn play_sound(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    x: jdouble,
    y: jdouble,
    z: jdouble,
    sound: JString<'_>,
    volume: jfloat,
    pitch: jfloat,
) {
    let Some(world) = world(&mut env, &name) else {
        return;
    };
    let Ok(text) = env.get_string(&sound) else {
        return;
    };
    let text: String = text.into();
    let Ok(key) = text.parse::<Identifier>() else {
        return;
    };
    let Some(sound) = REGISTRY.sound_events.by_key(&key) else {
        return;
    };
    // Reading and broadcasting is safe from any thread: this sends packets and
    // touches no block state.
    world.play_sound_at(
        sound,
        SoundSource::Master,
        DVec3::new(x, y, z),
        volume,
        pitch,
        None,
    );
}

/// `foton.Native.playSoundCategory`
extern "system" fn play_sound_category(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    x: jdouble,
    y: jdouble,
    z: jdouble,
    sound: JString<'_>,
    category: JString<'_>,
    volume: jfloat,
    pitch: jfloat,
) {
    let Some(world) = world(&mut env, &name) else {
        return;
    };
    let Ok(sound) = env.get_string(&sound) else {
        return;
    };
    let Ok(category) = env.get_string(&category) else {
        return;
    };
    let Ok(key) = String::from(sound).parse::<Identifier>() else {
        return;
    };
    let Some(sound) = REGISTRY.sound_events.by_key(&key) else {
        return;
    };
    let Ok(category) = category.to_str() else {
        return;
    };
    let source = match category {
        "MASTER" => SoundSource::Master,
        "MUSIC" => SoundSource::Music,
        "RECORDS" => SoundSource::Records,
        "WEATHER" => SoundSource::Weather,
        "BLOCKS" => SoundSource::Blocks,
        "HOSTILE" => SoundSource::Hostile,
        "NEUTRAL" => SoundSource::Neutral,
        "PLAYERS" => SoundSource::Players,
        "AMBIENT" => SoundSource::Ambient,
        "VOICE" => SoundSource::Voice,
        "UI" => SoundSource::Ui,
        _ => return,
    };
    world.play_sound_at(sound, source, DVec3::new(x, y, z), volume, pitch, None);
}

/// `foton.Native.stopSound`
extern "system" fn stop_sound(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    sound: JString<'_>,
    category: JString<'_>,
) {
    let Some(player) = player(&mut env, &uuid) else {
        return;
    };
    let Ok(sound) = env.get_string(&sound) else {
        return;
    };
    let Ok(category) = env.get_string(&category) else {
        return;
    };
    let sound: String = sound.into();
    let sound = if sound.is_empty() {
        None
    } else {
        sound.parse::<Identifier>().ok()
    };
    let source = match category.to_str().ok() {
        Some("MASTER") => Some(SoundSource::Master),
        Some("MUSIC") => Some(SoundSource::Music),
        Some("RECORDS") => Some(SoundSource::Records),
        Some("WEATHER") => Some(SoundSource::Weather),
        Some("BLOCKS") => Some(SoundSource::Blocks),
        Some("HOSTILE") => Some(SoundSource::Hostile),
        Some("NEUTRAL") => Some(SoundSource::Neutral),
        Some("PLAYERS") => Some(SoundSource::Players),
        Some("AMBIENT") => Some(SoundSource::Ambient),
        Some("VOICE") => Some(SoundSource::Voice),
        Some("UI") => Some(SoundSource::Ui),
        Some("") => None,
        _ => return,
    };
    player.send_packet(CStopSound { sound, source });
}

/// `foton.Native.openMenuSlotCount`
extern "system" fn open_menu_slot_count(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jint {
    player(&mut env, &uuid)
        .and_then(|player| player.open_container_slot_count())
        .and_then(|count| jint::try_from(count).ok())
        .unwrap_or(-1)
}

/// `foton.Native.openMenuSlot`
extern "system" fn open_menu_top_slot_count(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jint {
    player(&mut env, &uuid)
        .and_then(|player| player.open_container_top_slot_count())
        .and_then(|count| jint::try_from(count).ok())
        .unwrap_or(-1)
}

extern "system" fn open_menu_slot(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    slot: jint,
) -> jstring {
    let item = usize::try_from(slot).ok().and_then(|slot| {
        player(&mut env, &uuid).and_then(|player| player.open_container_item(slot))
    });
    to_java(&mut env, item.map(|stack| describe_slot(&stack)))
}

/// `foton.Native.openMenuType`
extern "system" fn set_open_menu_slot(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    slot: jint,
    item: JString<'_>,
) -> jboolean {
    let Ok(text) = env.get_string(&item) else {
        return 0;
    };
    let Some(stack) = parse_slot(&String::from(text)) else {
        return 0;
    };
    let Ok(slot) = usize::try_from(slot) else {
        return 0;
    };
    jboolean::from(
        player(&mut env, &uuid).is_some_and(|player| player.set_open_container_item(slot, stack)),
    )
}

/// Native open menu type
extern "system" fn open_menu_title(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jstring {
    let title = player(&mut env, &uuid).and_then(|player| player.open_container_title());
    to_java(&mut env, title)
}

extern "system" fn open_menu_type(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jstring {
    let menu_type = player(&mut env, &uuid).and_then(|player| player.open_container_menu_type());
    to_java(&mut env, menu_type)
}

/// `foton.Native.updateInventory`
extern "system" fn update_inventory(mut env: JNIEnv<'_>, _class: JClass<'_>, uuid: JString<'_>) {
    if let Some(player) = player(&mut env, &uuid) {
        player.broadcast_inventory_changes();
    }
}

extern "system" fn close_inventory(mut env: JNIEnv<'_>, _class: JClass<'_>, uuid: JString<'_>) {
    if let Some(player) = player(&mut env, &uuid) {
        player.close_container();
    }
}

/// `foton.Native.gameMode`
extern "system" fn game_mode(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jstring {
    let mode = player(&mut env, &uuid).map(|player| format!("{:?}", player.game_mode()));
    to_java(&mut env, mode)
}

/// `foton.Native.setGameMode`
extern "system" fn set_game_mode(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    mode: JString<'_>,
) -> jboolean {
    let requested: String = match env.get_string(&mode) {
        Ok(value) => value.into(),
        Err(_) => return 0,
    };
    let Some(game_mode) = (match requested.to_ascii_uppercase().as_str() {
        "CREATIVE" => Some(GameType::Creative),
        "SURVIVAL" => Some(GameType::Survival),
        "ADVENTURE" => Some(GameType::Adventure),
        "SPECTATOR" => Some(GameType::Spectator),
        _ => None,
    }) else {
        return 0;
    };
    jboolean::from(player(&mut env, &uuid).is_some_and(|player| player.set_game_mode(game_mode)))
}

extern "system" fn allow_flight(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jboolean {
    jboolean::from(player(&mut env, &uuid).is_some_and(|player| player.get_abilities().may_fly))
}

extern "system" fn is_flying(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jboolean {
    jboolean::from(player(&mut env, &uuid).is_some_and(|player| player.is_flying()))
}

extern "system" fn is_sleeping_ignored(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jboolean {
    jboolean::from(player(&mut env, &uuid).is_some_and(|player| player.is_sleeping_ignored()))
}

extern "system" fn set_sleeping_ignored(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    value: jboolean,
) {
    if let Some(player) = player(&mut env, &uuid) {
        player.set_sleeping_ignored(value != 0);
    }
}

extern "system" fn set_flying(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    value: jboolean,
) {
    if let Some(player) = player(&mut env, &uuid) {
        player.set_flying(value != 0);
        player.send_abilities();
    }
}

extern "system" fn open_generic_inventory(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    size: jint,
    title: JString<'_>,
    contents: JString<'_>,
) {
    let Ok(title) = env.get_string(&title) else {
        return;
    };
    let title: String = title.into();
    let Ok(contents) = env.get_string(&contents) else {
        return;
    };
    let Some(player) = player(&mut env, &uuid) else {
        return;
    };
    let items = contents
        .to_str()
        .unwrap_or_default()
        .split('\u{1e}')
        .map(parse_slot)
        .collect::<Option<Vec<_>>>();
    let Some(items) = items else {
        return;
    };
    let rows = usize::try_from(size)
        .ok()
        .filter(|size| *size % 9 == 0)
        .map_or(1, |size| size / 9);
    // JSON text, so a plugin's coloured title reaches the client coloured.
    player.open_generic_inventory(relay::component(&title), rows, items);
}

extern "system" fn open_smithing_table(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    _world: JString<'_>,
    x: jint,
    y: jint,
    z: jint,
) -> jboolean {
    let Some(player) = player(&mut env, &uuid) else {
        return 0;
    };
    let inventory = Arc::clone(&player.inventory);
    player.open_menu(
        TextComponent::translated(CONTAINER_UPGRADE.msg()),
        move |context| smithing(inventory, context.container_id, BlockPos::new(x, y, z)),
    );
    1
}

extern "system" fn open_loom(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    _world: JString<'_>,
    x: jint,
    y: jint,
    z: jint,
) -> jboolean {
    let Some(player) = player(&mut env, &uuid) else {
        return 0;
    };
    let inventory = Arc::clone(&player.inventory);
    player.open_menu(
        TextComponent::translated(CONTAINER_LOOM.msg()),
        move |context| loom(inventory, context.container_id, BlockPos::new(x, y, z)),
    );
    1
}

extern "system" fn open_cartography_table(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    _world: JString<'_>,
    x: jint,
    y: jint,
    z: jint,
) -> jboolean {
    let Some(player) = player(&mut env, &uuid) else {
        return 0;
    };
    let inventory = Arc::clone(&player.inventory);
    let world = player.get_world();
    let Some(server) = server() else {
        return 0;
    };
    let Some(maps) = server.map_data.for_world(&world).map(Arc::clone) else {
        return 0;
    };
    player.open_menu(
        TextComponent::translated(CONTAINER_CARTOGRAPHY_TABLE.msg()),
        move |context| {
            cartography(
                inventory,
                context.container_id,
                BlockPos::new(x, y, z),
                &world,
                maps,
            )
        },
    );
    1
}

extern "system" fn open_anvil(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    _world: JString<'_>,
    x: jint,
    y: jint,
    z: jint,
) -> jboolean {
    let Some(player) = player(&mut env, &uuid) else {
        return 0;
    };
    let inventory = Arc::clone(&player.inventory);
    let world = player.get_world();
    player.open_menu(
        TextComponent::translated(CONTAINER_REPAIR.msg()),
        move |context| {
            anvil(
                inventory,
                context.container_id,
                BlockPos::new(x, y, z),
                &world,
            )
        },
    );
    1
}

extern "system" fn open_stonecutter(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    _world: JString<'_>,
    x: jint,
    y: jint,
    z: jint,
) -> jboolean {
    let Some(player) = player(&mut env, &uuid) else {
        return 0;
    };
    let inventory = Arc::clone(&player.inventory);
    player.open_menu(
        TextComponent::translated(CONTAINER_STONECUTTER.msg()),
        move |context| stonecutter(inventory, context.container_id, BlockPos::new(x, y, z)),
    );
    1
}

extern "system" fn open_grindstone(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    _world: JString<'_>,
    x: jint,
    y: jint,
    z: jint,
) -> jboolean {
    let Some(player) = player(&mut env, &uuid) else {
        return 0;
    };
    let inventory = Arc::clone(&player.inventory);
    player.open_menu(
        TextComponent::translated(CONTAINER_GRINDSTONE_TITLE.msg()),
        move |context| {
            grindstone(
                inventory,
                context.container_id,
                BlockPos::new(x, y, z),
                context.world,
            )
        },
    );
    1
}

extern "system" fn open_workbench(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    _world: JString<'_>,
    x: jint,
    y: jint,
    z: jint,
) -> jboolean {
    let Some(player) = player(&mut env, &uuid) else {
        return 0;
    };
    let inventory = Arc::clone(&player.inventory);
    player.open_menu(
        TextComponent::translated(CONTAINER_CRAFTING.msg()),
        move |context| crafting(inventory, context.container_id, BlockPos::new(x, y, z)),
    );
    1
}

extern "system" fn set_allow_flight(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    value: jboolean,
) {
    if let Some(player) = player(&mut env, &uuid) {
        player.abilities.lock().may_fly = value != 0;
        player.send_abilities();
    }
}

/// `foton.Native.inventorySlot`
extern "system" fn inventory_slot(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    slot: jint,
) -> jstring {
    let described = player(&mut env, &uuid).and_then(|player| {
        let slot = usize::try_from(slot).ok()?;
        let inventory = player.inventory.lock();
        (slot < inventory.get_container_size()).then(|| describe_slot(inventory.get_item(slot)))
    });
    to_java(&mut env, described)
}

/// `foton.Native.setInventorySlot`
extern "system" fn set_inventory_slot(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    slot: jint,
    item: JString<'_>,
) {
    let Some(player) = player(&mut env, &uuid) else {
        return;
    };
    let Ok(text) = env.get_string(&item) else {
        return;
    };
    let text: String = text.into();
    let Some(stack) = parse_slot(&text) else {
        return;
    };
    let Ok(slot) = usize::try_from(slot) else {
        return;
    };
    let mut inventory = player.inventory.lock();
    if slot < inventory.get_container_size() {
        inventory.set_item(slot, stack);
    }
}

extern "system" fn ender_chest_slot(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    slot: jint,
) -> jstring {
    let described = player(&mut env, &uuid).and_then(|player| {
        let slot = usize::try_from(slot).ok()?;
        let inventory = player.ender_chest.lock();
        (slot < inventory.get_container_size()).then(|| describe_slot(inventory.get_item(slot)))
    });
    to_java(&mut env, described)
}

extern "system" fn set_ender_chest_slot(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    slot: jint,
    item: JString<'_>,
) {
    let Some(player) = player(&mut env, &uuid) else {
        return;
    };
    let Ok(text) = env.get_string(&item) else {
        return;
    };
    let text: String = text.into();
    let Some(stack) = parse_slot(&text) else {
        return;
    };
    let Ok(slot) = usize::try_from(slot) else {
        return;
    };
    let mut inventory = player.ender_chest.lock();
    if slot < inventory.get_container_size() {
        inventory.set_item(slot, stack);
    }
}

/// `foton.Native.heldSlot`
extern "system" fn held_slot(mut env: JNIEnv<'_>, _class: JClass<'_>, uuid: JString<'_>) -> jint {
    player(&mut env, &uuid).map_or(-1, |player| {
        jint::from(player.inventory.lock().get_selected_slot())
    })
}

/// `foton.Native.createBossBar`
extern "system" fn create_boss_bar(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    title: JString<'_>,
    color: jint,
    style: jint,
    flags: jint,
) -> jstring {
    let Ok(title) = env.get_string(&title).map(String::from) else {
        return null_mut();
    };
    let Some(color) = usize::try_from(color)
        .ok()
        .and_then(|index| BossBarColor::VALUES.get(index).copied())
    else {
        return null_mut();
    };
    let Some(style) = usize::try_from(style)
        .ok()
        .and_then(|index| BossBarOverlay::VALUES.get(index).copied())
    else {
        return null_mut();
    };
    let bar = Arc::new(ServerBossEvent::with_random_id(
        TextComponent::from(title),
        color,
        style,
    ));
    bar.set_darken_screen(flags & 1 != 0);
    bar.set_play_boss_music(flags & 2 != 0);
    bar.set_create_world_fog(flags & 4 != 0);
    let id = bar.id();
    boss_bars().write().insert(id, bar);
    to_java(&mut env, Some(id.to_string()))
}

extern "system" fn release_boss_bar(mut env: JNIEnv<'_>, _class: JClass<'_>, id: JString<'_>) {
    let Ok(text) = env.get_string(&id).map(String::from) else {
        return;
    };
    let Ok(id) = Uuid::parse_str(&text) else {
        return;
    };
    if let Some(bar) = boss_bars().write().remove(&id) {
        bar.remove_all_players();
    }
}

extern "system" fn boss_bar_set_title(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    id: JString<'_>,
    title: JString<'_>,
) {
    let Some(bar) = boss_bar(&mut env, &id) else {
        return;
    };
    let Ok(title) = env.get_string(&title).map(String::from) else {
        return;
    };
    bar.set_name(TextComponent::from(title));
}

extern "system" fn boss_bar_set_color(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    id: JString<'_>,
    color: jint,
) {
    let Some(bar) = boss_bar(&mut env, &id) else {
        return;
    };
    let Some(color) = usize::try_from(color)
        .ok()
        .and_then(|index| BossBarColor::VALUES.get(index).copied())
    else {
        return;
    };
    bar.set_color(color);
}

extern "system" fn boss_bar_set_style(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    id: JString<'_>,
    style: jint,
) {
    let Some(bar) = boss_bar(&mut env, &id) else {
        return;
    };
    let Some(style) = usize::try_from(style)
        .ok()
        .and_then(|index| BossBarOverlay::VALUES.get(index).copied())
    else {
        return;
    };
    bar.set_overlay(style);
}

extern "system" fn boss_bar_set_flags(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    id: JString<'_>,
    flags: jint,
) {
    let Some(bar) = boss_bar(&mut env, &id) else {
        return;
    };
    bar.set_darken_screen(flags & 1 != 0);
    bar.set_play_boss_music(flags & 2 != 0);
    bar.set_create_world_fog(flags & 4 != 0);
}

extern "system" fn boss_bar_set_progress(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    id: JString<'_>,
    progress: jdouble,
) {
    if let Some(bar) = boss_bar(&mut env, &id) {
        bar.set_progress(progress as f32);
    }
}

extern "system" fn boss_bar_add_player(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    id: JString<'_>,
    player_id: JString<'_>,
) {
    let Some(bar) = boss_bar(&mut env, &id) else {
        return;
    };
    if let Some(player) = player(&mut env, &player_id) {
        bar.add_player(&player);
    }
}

extern "system" fn boss_bar_remove_player(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    id: JString<'_>,
    player_id: JString<'_>,
) {
    let Some(bar) = boss_bar(&mut env, &id) else {
        return;
    };
    if let Some(player) = player(&mut env, &player_id) {
        bar.remove_player(&player);
    }
}

extern "system" fn boss_bar_remove_all(mut env: JNIEnv<'_>, _class: JClass<'_>, id: JString<'_>) {
    if let Some(bar) = boss_bar(&mut env, &id) {
        bar.remove_all_players();
    }
}

extern "system" fn boss_bar_player_ids(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    id: JString<'_>,
) -> jobjectArray {
    let ids = boss_bar(&mut env, &id).map_or_else(Vec::new, |bar| {
        bar.players()
            .into_iter()
            .map(|player| player.uuid().to_string())
            .collect()
    });
    string_array(&mut env, &ids)
}

extern "system" fn boss_bar_set_visible(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    id: JString<'_>,
    visible: jboolean,
) {
    if let Some(bar) = boss_bar(&mut env, &id) {
        bar.set_visible(visible != 0);
    }
}

/// `foton.Native.serverBrand`
extern "system" fn server_brand(mut env: JNIEnv<'_>, _class: JClass<'_>) -> jstring {
    let brand = format!(
        "Foton {} (MC: {})",
        env!("CARGO_PKG_VERSION"),
        foton_utils::MC_VERSION
    );
    to_java(&mut env, Some(brand))
}

/// `foton.Native.datapacks`
extern "system" fn datapacks(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    enabled_only: jboolean,
) -> jobjectArray {
    let records = server()
        .map(|server| server.datapack_records(enabled_only != 0))
        .unwrap_or_default();
    string_array(&mut env, &records)
}

/// `foton.Native.onlineMode`
extern "system" fn online_mode(_env: JNIEnv<'_>, _class: JClass<'_>) -> jboolean {
    u8::from(server().is_some_and(|server| server.config.online_mode))
}

/// `foton.Native.maxPlayers`
extern "system" fn max_players(_env: JNIEnv<'_>, _class: JClass<'_>) -> jint {
    server().map_or(0, |server| {
        i32::try_from(server.config.max_players).unwrap_or(i32::MAX)
    })
}

extern "system" fn server_allow_flight(_env: JNIEnv<'_>, _class: JClass<'_>) -> jboolean {
    jboolean::from(server().is_some_and(|value| value.config.allow_flight))
}

extern "system" fn server_default_game_mode(mut env: JNIEnv<'_>, _class: JClass<'_>) -> jstring {
    let value = server().and_then(|server| {
        server
            .worlds
            .values()
            .into_iter()
            .next()
            .map(|world| format!("{:?}", world.default_gamemode))
    });
    to_java(&mut env, value)
}

extern "system" fn server_view_distance(_env: JNIEnv<'_>, _class: JClass<'_>) -> jint {
    server().map_or(10, |value| i32::from(value.config.view_distance))
}

extern "system" fn server_simulation_distance(_env: JNIEnv<'_>, _class: JClass<'_>) -> jint {
    server().map_or(10, |value| i32::from(value.config.simulation_distance))
}

extern "system" fn server_tps(env: JNIEnv<'_>, _class: JClass<'_>) -> jdoubleArray {
    let Some(server) = server() else {
        return null_mut();
    };
    let values = server.tick_rate_manager.read().tps_averages();
    let Ok(array) = env.new_double_array(3) else {
        return null_mut();
    };
    if env.set_double_array_region(&array, 0, &values).is_err() {
        return null_mut();
    }
    array.into_raw()
}

extern "system" fn server_average_tick_time(_env: JNIEnv<'_>, _class: JClass<'_>) -> jdouble {
    server().map_or(50.0, |value| {
        f64::from(value.tick_rate_manager.read().get_average_mspt())
    })
}

/// `foton.Native.playerIdByName`
extern "system" fn player_id_by_name(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
) -> jstring {
    let Ok(wanted) = env.get_string(&name) else {
        return null_mut();
    };
    let wanted: String = wanted.into();
    let found = server().and_then(|server| {
        let mut found = None;
        server.online_players().iter_players(|_uuid, player| {
            if player.gameprofile.name == wanted {
                found = Some(player.gameprofile.id.to_string());
                return false;
            }
            true
        });
        found
    });
    to_java(&mut env, found)
}

/// `foton.Native.broadcast`
extern "system" fn broadcast(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    message: JString<'_>,
) -> jint {
    let Ok(text) = env.get_string(&message) else {
        return 0;
    };
    let text: String = text.into();
    let Some(server) = server() else {
        return 0;
    };
    let mut reached = 0;
    server.online_players().iter_players(|_uuid, player| {
        player.send_message(&text.clone().into());
        reached += 1;
        true
    });
    reached
}

/// `foton.Native.playerPosition`
extern "system" fn player_position(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jdoubleArray {
    let at = player(&mut env, &uuid).map(|player| {
        let position = player.position();
        let (yaw, pitch) = player.rotation();
        [
            position.x,
            position.y,
            position.z,
            f64::from(yaw),
            f64::from(pitch),
        ]
    });
    to_position(&mut env, at)
}

/// `foton.Native.worldNames`
/// unload world at the next serialized tick safe-point
extern "system" fn unload_world(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    save: jboolean,
) -> jboolean {
    let Ok(value) = env.get_string(&name) else {
        return 0;
    };
    let Some(key) = value
        .to_str()
        .ok()
        .and_then(|v| v.parse::<Identifier>().ok())
    else {
        return 0;
    };
    jboolean::from(
        server().is_some_and(|server| server.request_world_removal_with_save(key, save != 0)),
    )
}

extern "system" fn world_names(mut env: JNIEnv<'_>, _class: JClass<'_>) -> jobjectArray {
    let names = server().map_or_else(Vec::new, |server| {
        server
            .worlds
            .key_snapshots()
            .into_iter()
            .map(|key| key.to_string())
            .collect()
    });
    string_array(&mut env, &names)
}

/// Starts a validated world creation request. Status values are 0=pending,
/// 1=ready, 2=failed, and -1=unknown request.
extern "system" fn request_world_creation(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    generator: JString<'_>,
    seed: jlong,
    bonus_chest: jboolean,
) -> jlong {
    let Ok(name): Result<String, _> = env.get_string(&name).map(Into::into) else {
        return -1;
    };
    let Ok(generator): Result<String, _> = env.get_string(&generator).map(Into::into) else {
        return -1;
    };
    let Some(server) = server() else { return -1 };
    let Ok(generator) = generator.parse::<Identifier>() else {
        return -1;
    };
    let Ok(request) = server.request_world_creation(name, generator, seed, bonus_chest != 0) else {
        return -1;
    };
    let id = request.id();
    world_creation_requests().lock().insert(id, request);
    id as jlong
}

extern "system" fn world_creation_state(_env: JNIEnv<'_>, _class: JClass<'_>, id: jlong) -> jint {
    let Ok(id) = u64::try_from(id) else { return -1 };
    let mut requests = world_creation_requests().lock();
    let Some(request) = requests.get_mut(&id) else {
        return -1;
    };
    match request.poll() {
        WorldCreationState::Pending => 0,
        WorldCreationState::Ready => {
            requests.remove(&id);
            1
        }
        WorldCreationState::Failed(_) => {
            requests.remove(&id);
            2
        }
    }
}

/// `foton.Native.worldPlayerIds`
extern "system" fn world_player_ids(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
) -> jobjectArray {
    let ids = world(&mut env, &name).map_or_else(Vec::new, |world| {
        let mut ids = Vec::new();
        world.players.iter_players(|_uuid, player| {
            ids.push(player.gameprofile.id.to_string());
            true
        });
        ids
    });
    string_array(&mut env, &ids)
}

/// `foton.Native.worldEntityIds`
extern "system" fn world_entity_ids(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
) -> jobjectArray {
    let ids = world(&mut env, &name).map_or_else(Vec::new, |world| {
        world
            .accessible_entities()
            .into_iter()
            .map(|entity| entity.uuid().to_string())
            .collect()
    });
    string_array(&mut env, &ids)
}

extern "system" fn chunk_block_entities(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    x: jint,
    z: jint,
) -> jobjectArray {
    let values = world(&mut env, &name).map_or_else(Vec::new, |world| {
        world
            .block_entity_positions_in_chunk(x, z)
            .into_iter()
            .map(|(pos, state)| format!("{}|{}|{}|{}", pos.x(), pos.y(), pos.z(), state.0))
            .collect()
    });
    string_array(&mut env, &values)
}

/// `foton.Native.requestChunk`
extern "system" fn request_chunk(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    x: jint,
    z: jint,
) -> jstring {
    let Some(world) = world(&mut env, &name) else {
        return null_mut();
    };
    let pos = foton_utils::ChunkPos::new(x, z);
    let handle = world
        .chunk_map
        .request_chunk(pos, ChunkStatus::Full, ChunkTicketKind::Command);
    let id = Uuid::new_v4();
    {
        let mut requests = chunk_requests().lock();
        sweep_abandoned_chunk_requests(&mut requests);
        requests.insert(id, (Instant::now(), handle));
    }
    to_java(&mut env, Some(id.to_string()))
}

/// `foton.Native.chunkRequestReady`
extern "system" fn chunk_request_ready(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    id: JString<'_>,
) -> jboolean {
    let Ok(text) = env.get_string(&id) else {
        return 0;
    };
    let Ok(text) = text.to_str() else {
        return 0;
    };
    let Ok(id) = Uuid::parse_str(text) else {
        return 0;
    };
    let mut requests = chunk_requests().lock();
    sweep_abandoned_chunk_requests(&mut requests);
    let Some((_, handle)) = requests.get(&id) else {
        return 0;
    };
    match handle.poll() {
        ChunkRequestState::Ready => {
            requests.remove(&id);
            1
        }
        ChunkRequestState::Cancelled => {
            requests.remove(&id);
            0
        }
        ChunkRequestState::Pending { .. } => 0,
    }
}

/// `foton.Native.worldChunkLoaded`
extern "system" fn world_chunk_loaded(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    x: jint,
    z: jint,
) -> jboolean {
    jboolean::from(world(&mut env, &name).is_some_and(|world| world.is_chunk_loaded(x, z)))
}

extern "system" fn world_chunk_generated(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    x: jint,
    z: jint,
) -> jboolean {
    jboolean::from(world(&mut env, &name).is_some_and(|world| world.is_chunk_generated(x, z)))
}

extern "system" fn set_world_spawn_ticks(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    category: JString<'_>,
    ticks: jint,
) {
    let Ok(category) = env.get_string(&category) else {
        return;
    };
    let Some(world) = world(&mut env, &name) else {
        return;
    };
    let Some(category) = category.to_str().ok().map(str::to_ascii_uppercase) else {
        return;
    };
    let category = match category.as_str() {
        "MONSTER" => MobCategory::Monster,
        "CREATURE" | "ANIMAL" => MobCategory::Creature,
        "AMBIENT" => MobCategory::Ambient,
        "AXOLOTL" | "AXOLOTLS" => MobCategory::Axolotls,
        "WATER_CREATURE" | "WATER_ANIMAL" => MobCategory::WaterCreature,
        "WATER_AMBIENT" => MobCategory::WaterAmbient,
        "UNDERGROUND_WATER_CREATURE" => MobCategory::UndergroundWaterCreature,
        _ => return,
    };
    world.set_spawn_ticks(category, ticks);
}

extern "system" fn world_spawn_limit(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    category: JString<'_>,
) -> jint {
    let Ok(category) = env.get_string(&category) else {
        return 0;
    };
    let Some(world) = world(&mut env, &name) else {
        return 0;
    };
    let Some(category) = category.to_str().ok().map(str::to_ascii_uppercase) else {
        return 0;
    };
    let category = match category.as_str() {
        "MONSTER" => MobCategory::Monster,
        "CREATURE" | "ANIMAL" => MobCategory::Creature,
        "AMBIENT" => MobCategory::Ambient,
        "AXOLOTL" | "AXOLOTLS" => MobCategory::Axolotls,
        "WATER_CREATURE" | "WATER_ANIMAL" => MobCategory::WaterCreature,
        "WATER_AMBIENT" => MobCategory::WaterAmbient,
        "UNDERGROUND_WATER_CREATURE" => MobCategory::UndergroundWaterCreature,
        _ => return 0,
    };
    world
        .spawn_limit(category)
        .unwrap_or(category.max_instances_per_chunk()) as jint
}

extern "system" fn set_world_spawn_limit(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    category: JString<'_>,
    limit: jint,
) {
    let Ok(category) = env.get_string(&category) else {
        return;
    };
    let Some(world) = world(&mut env, &name) else {
        return;
    };
    let category = match category
        .to_str()
        .ok()
        .map(str::to_ascii_uppercase)
        .as_deref()
    {
        Some("MONSTER") => MobCategory::Monster,
        Some("CREATURE") => MobCategory::Creature,
        Some("AMBIENT") => MobCategory::Ambient,
        Some("AXOLOTL" | "AXOLOTLS") => MobCategory::Axolotls,
        Some("UNDERGROUND_WATER_CREATURE") => MobCategory::UndergroundWaterCreature,
        Some("WATER_CREATURE") => MobCategory::WaterCreature,
        Some("WATER_AMBIENT") => MobCategory::WaterAmbient,
        _ => return,
    };
    world.set_spawn_limit(category, limit);
}

extern "system" fn world_keep_spawn_in_memory(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
) -> jboolean {
    jboolean::from(world(&mut env, &name).is_some_and(|w| w.keep_spawn_in_memory()))
}
extern "system" fn set_world_keep_spawn_in_memory(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    value: jboolean,
) {
    if let Some(w) = world(&mut env, &name) {
        w.set_keep_spawn_in_memory(value != 0);
    }
}

extern "system" fn world_storm(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
) -> jboolean {
    world(&mut env, &name).map_or(0, |world| jboolean::from(world.is_raining()))
}

extern "system" fn set_world_storm(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    storm: jboolean,
) {
    if let Some(world) = world(&mut env, &name) {
        world.set_weather_parameters(0, 6000, storm != 0, false);
    }
}

extern "system" fn world_has_bonus_chest(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
) -> jboolean {
    jboolean::from(world(&mut env, &name).is_some_and(|world| world.has_bonus_chest()))
}

extern "system" fn world_weather_duration(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
) -> jint {
    world(&mut env, &name).map_or(0, |world| world.level_data.read().rain_time())
}

extern "system" fn set_world_weather_duration(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    ticks: jint,
) {
    if let Some(world) = world(&mut env, &name) {
        let raining = world.is_raining();
        let thunder = world.is_thundering();
        world.set_weather_parameters(0, ticks.max(0), raining, thunder);
    }
}

extern "system" fn set_world_difficulty(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    world_name: JString<'_>,
    difficulty: JString<'_>,
) {
    let Ok(value): Result<String, _> = env.get_string(&difficulty).map(Into::into) else {
        return;
    };
    let difficulty = match value.to_ascii_uppercase().as_str() {
        "PEACEFUL" => Difficulty::Peaceful,
        "EASY" => Difficulty::Easy,
        "NORMAL" => Difficulty::Normal,
        "HARD" => Difficulty::Hard,
        _ => return,
    };
    if let Some(world) = world(&mut env, &world_name) {
        world.set_difficulty(difficulty);
    }
}

extern "system" fn world_thunder_duration(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
) -> jint {
    world(&mut env, &name).map_or(0, |world| world.level_data.read().thunder_time())
}

extern "system" fn set_world_thunder_duration(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    ticks: jint,
) {
    if let Some(world) = world(&mut env, &name) {
        let raining = world.is_raining();
        let thundering = world.is_thundering();
        world.set_weather_parameters(0, ticks.max(0), raining, thundering);
    }
}

extern "system" fn world_thundering(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
) -> jboolean {
    world(&mut env, &name).map_or(0, |world| jboolean::from(world.is_thundering()))
}

extern "system" fn set_world_thundering(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    thundering: jboolean,
) {
    if let Some(world) = world(&mut env, &name) {
        let raining = world.is_raining();
        world.set_weather_parameters(0, 6000, raining, thundering != 0);
    }
}

extern "system" fn spawn_entity(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    world_name: JString<'_>,
    x: jdouble,
    y: jdouble,
    z: jdouble,
    type_name: JString<'_>,
    initialization: JString<'_>,
) -> jstring {
    let Ok(world_text): Result<String, _> = env.get_string(&world_name).map(Into::into) else {
        return null_mut();
    };
    let Ok(type_text): Result<String, _> = env.get_string(&type_name).map(Into::into) else {
        return null_mut();
    };
    let Ok(initialization_text): Result<String, _> =
        env.get_string(&initialization).map(Into::into)
    else {
        return null_mut();
    };
    let Ok(world_key) = Identifier::from_str(&world_text) else {
        return null_mut();
    };
    let Ok(type_key) = Identifier::from_str(&format!("minecraft:{type_text}")) else {
        return null_mut();
    };
    let Some(world) = server().and_then(|server| server.worlds.get_owned(&world_key)) else {
        return null_mut();
    };
    let position = DVec3::new(x, y, z);
    let entity = if initialization_text.is_empty() {
        create_entity_at(&world, &type_key, position)
    } else {
        let Some(registry) = REGISTRY.get() else {
            return null_mut();
        };
        let Ok(potion_key) = Identifier::from_str(&initialization_text) else {
            return null_mut();
        };
        let Some(potion) = registry.potions.by_key(&potion_key) else {
            return null_mut();
        };
        create_entity_at_initialized(
            &world,
            &type_key,
            position,
            SpawnEntityInitialization::ArrowPotion(potion),
        )
    };
    let Some(entity) = entity else {
        return null_mut();
    };
    match prepare_then_publish(&world, entity, |entity| {
        env.new_string(entity.uuid().to_string())
    }) {
        Ok(uuid) => uuid.into_raw(),
        Err(_) => null_mut(),
    }
}

extern "system" fn spawn_entity_pending(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    world_name: JString<'_>,
    x: jdouble,
    y: jdouble,
    z: jdouble,
    type_name: JString<'_>,
    initialization: JString<'_>,
) -> jstring {
    let Ok(world_text): Result<String, _> = env.get_string(&world_name).map(Into::into) else {
        return null_mut();
    };
    let Ok(type_text): Result<String, _> = env.get_string(&type_name).map(Into::into) else {
        return null_mut();
    };
    let Ok(initialization_text): Result<String, _> =
        env.get_string(&initialization).map(Into::into)
    else {
        return null_mut();
    };
    let Ok(world_key) = Identifier::from_str(&world_text) else {
        return null_mut();
    };
    let Ok(type_key) = Identifier::from_str(&format!("minecraft:{type_text}")) else {
        return null_mut();
    };
    let Some(world) = server().and_then(|server| server.worlds.get_owned(&world_key)) else {
        return null_mut();
    };
    let position = DVec3::new(x, y, z);
    let entity = if initialization_text.is_empty() {
        create_entity_at(&world, &type_key, position)
    } else {
        let Some(registry) = REGISTRY.get() else {
            return null_mut();
        };
        let Ok(potion_key) = Identifier::from_str(&initialization_text) else {
            return null_mut();
        };
        let Some(potion) = registry.potions.by_key(&potion_key) else {
            return null_mut();
        };
        create_entity_at_initialized(
            &world,
            &type_key,
            position,
            SpawnEntityInitialization::ArrowPotion(potion),
        )
    };
    let Some(entity) = entity else {
        return null_mut();
    };
    let Ok(uuid) = env.new_string(entity.uuid().to_string()) else {
        return null_mut();
    };
    world.begin_pending_spawn(entity);
    uuid.into_raw()
}

extern "system" fn finish_pending_spawn(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    world_name: JString<'_>,
    entity_uuid: JString<'_>,
    publish: jboolean,
) -> jboolean {
    let Some(world) = world(&mut env, &world_name) else {
        return 0;
    };
    let Ok(uuid_text): Result<String, _> = env.get_string(&entity_uuid).map(Into::into) else {
        return 0;
    };
    let Ok(uuid) = Uuid::parse_str(&uuid_text) else {
        return 0;
    };
    let Some(entity) = world.take_pending_spawn(&uuid) else {
        return 0;
    };
    if publish == 0 {
        discard_unpublished_entity(&entity);
        return 1;
    }
    let published = world.try_add_entity(Arc::clone(&entity)).is_ok();
    if !published {
        discard_unpublished_entity(&entity);
    }
    jboolean::from(published)
}

extern "system" fn set_world_game_rule(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    world_name: JString<'_>,
    rule_name: JString<'_>,
    value: JString<'_>,
) -> jboolean {
    let Ok(world_text): Result<String, _> = env.get_string(&world_name).map(Into::into) else {
        return 0;
    };
    let Ok(rule_text): Result<String, _> = env.get_string(&rule_name).map(Into::into) else {
        return 0;
    };
    let Ok(value_text): Result<String, _> = env.get_string(&value).map(Into::into) else {
        return 0;
    };
    let Ok(world_key) = Identifier::from_str(&world_text) else {
        return 0;
    };
    let Ok(rule_key) = Identifier::from_str(&rule_text) else {
        return 0;
    };
    let Some(world) = server().and_then(|server| server.worlds.get_owned(&world_key)) else {
        return 0;
    };
    let Some(rule) = foton_registry::REGISTRY.game_rules.by_key(&rule_key) else {
        return 0;
    };
    let parsed = match rule.value_type() {
        GameRuleType::Bool => value_text.parse::<bool>().ok().map(GameRuleValue::new),
        GameRuleType::Int => value_text.parse::<i32>().ok().map(GameRuleValue::new),
    };
    jboolean::from(parsed.is_some_and(|value| world.set_erased_game_rule(rule, value)))
}

extern "system" fn world_game_rule_default(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    world_name: JString<'_>,
    rule_name: JString<'_>,
) -> jstring {
    let Ok(world_text): Result<String, _> = env.get_string(&world_name).map(Into::into) else {
        return null_mut();
    };
    let Ok(rule_text): Result<String, _> = env.get_string(&rule_name).map(Into::into) else {
        return null_mut();
    };
    let Ok(world_key) = Identifier::from_str(&world_text) else {
        return null_mut();
    };
    let Ok(rule_key) = Identifier::from_str(&rule_text) else {
        return null_mut();
    };
    // The answer is the registry's default rather than this world's value, so
    // the world is only here to be checked: asking about a world that is not
    // loaded gets nothing back, not a number that looks like an answer.
    if server().is_none_or(|server| server.worlds.get_owned(&world_key).is_none()) {
        return null_mut();
    }
    let Some(rule) = foton_registry::REGISTRY.game_rules.by_key(&rule_key) else {
        return null_mut();
    };
    to_java(&mut env, Some(rule.default_erased_value().to_string()))
}

extern "system" fn world_game_rule(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    world_name: JString<'_>,
    rule_name: JString<'_>,
) -> jstring {
    let Ok(world_text): Result<String, _> = env.get_string(&world_name).map(Into::into) else {
        return null_mut();
    };
    let Ok(rule_text): Result<String, _> = env.get_string(&rule_name).map(Into::into) else {
        return null_mut();
    };
    let Ok(world_key) = Identifier::from_str(&world_text) else {
        return null_mut();
    };
    let Some(world) = server().and_then(|server| server.worlds.get_owned(&world_key)) else {
        return null_mut();
    };
    let Ok(key) = Identifier::from_str(&rule_text) else {
        return null_mut();
    };
    let Some(rule) = foton_registry::REGISTRY.game_rules.by_key(&key) else {
        return null_mut();
    };
    to_java(&mut env, Some(world.get_erased_game_rule(rule).to_string()))
}

extern "system" fn hopper_custom_name(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    x: jint,
    y: jint,
    z: jint,
) -> jstring {
    let value = world(&mut env, &name)
        .and_then(|world| world.get_block_entity(BlockPos::new(x, y, z)))
        .and_then(|entity| entity.downcast_ref::<HopperBlockEntity>()?.custom_name())
        .map(|text| text.to_string());
    to_java(&mut env, value)
}

extern "system" fn hopper_set_custom_name(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    x: jint,
    y: jint,
    z: jint,
    value: JString<'_>,
) {
    let Ok(value): Result<String, _> = env.get_string(&value).map(Into::into) else {
        return;
    };
    let Some(world) = world(&mut env, &name) else {
        return;
    };
    let Some(entity) = world.get_block_entity(BlockPos::new(x, y, z)) else {
        return;
    };
    let Some(hopper) = entity.downcast_ref::<HopperBlockEntity>() else {
        return;
    };
    hopper.set_custom_name((!value.is_empty()).then(|| TextComponent::plain(value)));
}

extern "system" fn jukebox_is_playing(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    x: jint,
    y: jint,
    z: jint,
) -> jboolean {
    let playing = world(&mut env, &name)
        .and_then(|world| world.get_block_entity(BlockPos::new(x, y, z)))
        .map(|entity| {
            entity
                .downcast_ref::<JukeboxBlockEntity>()
                .is_some_and(JukeboxBlockEntity::is_playing)
        });
    let playing = playing.unwrap_or(false);
    u8::from(playing)
}

extern "system" fn jukebox_record(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    x: jint,
    y: jint,
    z: jint,
) -> jstring {
    let value = world(&mut env, &name)
        .and_then(|world| world.get_block_entity(BlockPos::new(x, y, z)))
        .and_then(|entity| {
            entity
                .downcast_ref::<JukeboxBlockEntity>()
                .map(|jukebox| describe_slot(&jukebox.item()))
        });
    to_java(&mut env, value)
}

extern "system" fn jukebox_set_record(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    x: jint,
    y: jint,
    z: jint,
    encoded: JString<'_>,
) {
    let Ok(encoded): Result<String, _> = env.get_string(&encoded).map(Into::into) else {
        return;
    };
    let Some(world) = world(&mut env, &name) else {
        return;
    };
    let Some(entity) = world.get_block_entity(BlockPos::new(x, y, z)) else {
        return;
    };
    let Some(jukebox) = entity.downcast_ref::<JukeboxBlockEntity>() else {
        return;
    };
    let item = parse_slot(&encoded).unwrap_or_else(ItemStack::empty);
    jukebox.insert(&world, item);
}

extern "system" fn hopper_inventory_slot(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    x: jint,
    y: jint,
    z: jint,
    slot: jint,
) -> jstring {
    let value = world(&mut env, &name).and_then(|world| {
        let entity = world.get_block_entity(BlockPos::new(x, y, z))?;
        let container = ContainerRef::from_block_entity(entity)?;
        let guard = ContainerLockGuard::lock_all(&[&container]);
        guard.get(container.container_id()).and_then(|c| {
            (slot >= 0 && (slot as usize) < c.get_container_size())
                .then(|| describe_slot(c.get_item(slot as usize)))
        })
    });
    to_java(&mut env, value)
}

/// `foton.Native.craftingRecipe`: the key of the recipe a square crafting
/// grid `width` wide makes, the stacks row by row, or null when none does.
extern "system" fn crafting_recipe(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    items: JString<'_>,
    width: jint,
) -> jstring {
    let items: Option<String> = env.get_string(&items).ok().map(Into::into);
    let value = items.and_then(|items| {
        let width = usize::try_from(width)
            .ok()
            .filter(|width| matches!(width, 2 | 3))?;
        let stacks = items
            .split('\u{1e}')
            .map(parse_slot)
            .collect::<Option<Vec<_>>>()?;
        if stacks.len() != width * width {
            return None;
        }
        let input = CraftingInput::positioned(width, width, stacks).input;
        let recipe = if width == 2 {
            REGISTRY.recipes.find_crafting_recipe_2x2(&input)
        } else {
            REGISTRY.recipes.find_crafting_recipe(&input)
        }?;
        Some(recipe.id().to_string())
    });
    to_java(&mut env, value)
}

/// `foton.Native.isFuel`: whether a furnace burns the encoded stack.
extern "system" fn is_fuel(mut env: JNIEnv<'_>, _class: JClass<'_>, item: JString<'_>) -> jboolean {
    let Ok(text) = env.get_string(&item) else {
        return 0;
    };
    let text: String = text.into();
    jboolean::from(parse_slot(&text).is_some_and(|stack| fuel::is_fuel(&stack)))
}

/// `foton.Native.cookingRecipe`: the recipe by which a furnace, blast furnace
/// or smoker (`block`) cooks the encoded stack, described as
/// [`describe_cooking`] does, or null when none does.
extern "system" fn cooking_recipe(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    block: JString<'_>,
    item: JString<'_>,
) -> jstring {
    let block: Option<String> = env.get_string(&block).ok().map(Into::into);
    let item: Option<String> = env.get_string(&item).ok().map(Into::into);
    let value = block.zip(item).and_then(|(block, item)| {
        let kind = cooking_kind(&block)?;
        let stack = parse_slot(&item).filter(|stack| !stack.is_empty())?;
        let recipe = REGISTRY.recipes.find_cooking_recipe(kind, &stack)?;
        Some(describe_cooking(recipe))
    });
    to_java(&mut env, value)
}

/// `foton.Native.furnaceTimes`: `{burn, cook, cookTotal}`, or null where no
/// furnace, smoker or blast furnace stands.
extern "system" fn furnace_times(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    x: jint,
    y: jint,
    z: jint,
) -> jintArray {
    let Some(times) = world(&mut env, &name).and_then(|world| {
        let entity = world.get_block_entity(BlockPos::new(x, y, z))?;
        let furnace = entity.downcast_ref::<FurnaceBlockEntity>()?;
        Some(furnace.times())
    }) else {
        return null_mut();
    };
    let Ok(array) = env.new_int_array(3) else {
        return null_mut();
    };
    if env.set_int_array_region(&array, 0, &times).is_err() {
        return null_mut();
    }
    array.into_raw()
}

/// `foton.Native.setFurnaceTimes`: writes a `Furnace` snapshot's times back,
/// lighting or putting out the block to match the burn, as Bukkit does.
extern "system" fn set_furnace_times(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    x: jint,
    y: jint,
    z: jint,
    times: JIntArray<'_>,
) {
    let mut values = [0; 3];
    if env.get_int_array_region(&times, 0, &mut values).is_err() {
        return;
    }
    let Some(world) = world(&mut env, &name) else {
        return;
    };
    let pos = BlockPos::new(x, y, z);
    let Some(entity) = world.get_block_entity(pos) else {
        return;
    };
    let Some(furnace) = entity.downcast_ref::<FurnaceBlockEntity>() else {
        return;
    };
    furnace.set_times(values);
    let state = world.get_block_state(pos);
    let lit = state.set_value(&BlockStateProperties::LIT, values[0] > 0);
    if lit == state {
        return;
    }
    if on_tick() {
        world.set_block(pos, lit, UpdateFlags::UPDATE_ALL);
    } else {
        DEFERRED.lock().push((world.key.clone(), pos, lit));
    }
}

const BREWING_SNAPSHOT_HEADER: &[u8; 4] = b"FBS\x02";
const MAX_BREWING_SNAPSHOT_BYTES: usize = 1 << 20;
const MAX_BREWING_ITEM_BYTES: usize = 256 * 1024;
const MAX_BREWING_COMPONENTS: usize = 256;
const MAX_BREWING_OPAQUE_NBT_BYTES: usize = 256 * 1024;
const MAX_BREWING_TEXT_BYTES: usize = 64 * 1024;
const MAX_BREWING_LOCK_BYTES: usize = 64 * 1024;

const fn brewing_payload_length_is_valid(length: usize) -> bool {
    length <= MAX_BREWING_SNAPSHOT_BYTES
}

const fn brewing_item_payload_length_is_valid(length: usize) -> bool {
    length <= MAX_BREWING_ITEM_BYTES
}

struct BrewingBridgeSnapshot {
    identity: u64,
    items: Vec<ItemStack>,
    brew_time: i32,
    recipe_brew_time: i32,
    fuel: i32,
    custom_name: Option<TextComponent>,
    lock: LockCode,
}

struct BrewingPayloadWriter {
    bytes: Vec<u8>,
    limit: usize,
    length: usize,
    counting: bool,
}

impl BrewingPayloadWriter {
    const fn new(limit: usize) -> Self {
        Self {
            bytes: Vec::new(),
            limit,
            length: 0,
            counting: false,
        }
    }

    fn begin_blob(&mut self) -> Option<usize> {
        let offset = self.length;
        self.write_all(&0_u32.to_be_bytes()).ok()?;
        Some(offset)
    }

    fn finish_blob(&mut self, offset: usize, field_limit: usize) -> Option<()> {
        let body_start = offset.checked_add(4)?;
        let length = self.length.checked_sub(body_start)?;
        if length > field_limit {
            return None;
        }
        let length = u32::try_from(length).ok()?;
        if !self.counting {
            self.bytes
                .get_mut(offset..body_start)?
                .copy_from_slice(&length.to_be_bytes());
        }
        Some(())
    }

    fn write_optional_blob(&mut self, value: Option<&[u8]>, field_limit: usize) -> Option<()> {
        let Some(value) = value else {
            return self.write_all(&u32::MAX.to_be_bytes()).ok();
        };
        if value.len() > field_limit {
            return None;
        }
        let length = u32::try_from(value.len()).ok()?;
        self.write_all(&length.to_be_bytes()).ok()?;
        self.write_all(value).ok()
    }

    fn into_inner(self) -> Vec<u8> {
        self.bytes
    }
}

impl IoWrite for BrewingPayloadWriter {
    fn write(&mut self, bytes: &[u8]) -> IoResult<usize> {
        let Some(length) = self.length.checked_add(bytes.len()) else {
            return Err(IoError::other("brewing payload length overflow"));
        };
        if length > self.limit {
            return Err(IoError::other("brewing payload exceeds its byte limit"));
        }
        self.length = length;
        if self.counting {
            return Ok(bytes.len());
        }
        if length > self.bytes.capacity() {
            let capacity = self
                .bytes
                .capacity()
                .saturating_mul(2)
                .max(64)
                .max(length)
                .min(self.limit);
            self.bytes
                .try_reserve_exact(capacity - self.bytes.len())
                .map_err(IoError::other)?;
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> IoResult<()> {
        Ok(())
    }
}

fn write_brewing_item(output: &mut BrewingPayloadWriter, item: &ItemStack) -> Option<()> {
    use foton_registry::data_components::ComponentPatchEntry;
    if item.components_patch().len() > MAX_BREWING_COMPONENTS {
        return None;
    }
    let mut added = Vec::new();
    let mut removed = Vec::new();
    for (key, value) in item.components_patch().iter() {
        let id = REGISTRY.data_components.id_from_key(key)?;
        match value {
            ComponentPatchEntry::Set(value) => added.push((id, value)),
            ComponentPatchEntry::Removed => removed.push(id),
        }
    }
    added.sort_unstable_by_key(|(id, _)| *id);
    removed.sort_unstable();
    VarInt(item.count).write(output).ok()?;
    VarInt(i32::try_from(item.item.id()).ok()?)
        .write(output)
        .ok()?;
    VarInt(i32::try_from(added.len()).ok()?)
        .write(output)
        .ok()?;
    VarInt(i32::try_from(removed.len()).ok()?)
        .write(output)
        .ok()?;
    for (id, value) in added {
        VarInt(i32::try_from(id).ok()?).write(output).ok()?;
        let remaining = output.limit.checked_sub(output.length)?;
        REGISTRY
            .data_components
            .by_id(id)?
            .write_network_bounded(value, remaining, output)
            .ok()?;
    }
    for id in removed {
        VarInt(i32::try_from(id).ok()?).write(output).ok()?;
    }
    output.write_optional_blob(
        item.opaque_nbt().map(str::as_bytes),
        MAX_BREWING_OPAQUE_NBT_BYTES,
    )
}

fn append_brewing_item(output: &mut BrewingPayloadWriter, item: &ItemStack) -> Option<()> {
    let offset = output.begin_blob()?;
    let limit = output.limit;
    output.limit = limit.min(output.length.checked_add(MAX_BREWING_ITEM_BYTES)?);
    let result = write_brewing_item(output, item);
    output.limit = limit;
    result?;
    output.finish_blob(offset, MAX_BREWING_ITEM_BYTES)
}

fn encode_brewing_item(item: &ItemStack) -> Option<Vec<u8>> {
    let mut output = BrewingPayloadWriter::new(MAX_BREWING_ITEM_BYTES);
    write_brewing_item(&mut output, item)?;
    Some(output.into_inner())
}

#[cfg(test)]
fn encode_brewing_snapshot(snapshot: &BrewingBridgeSnapshot) -> Option<Vec<u8>> {
    encode_brewing_state(&BrewingStandStateView {
        identity: snapshot.identity,
        items: &snapshot.items,
        brew_time: snapshot.brew_time,
        recipe_brew_time: snapshot.recipe_brew_time,
        fuel: snapshot.fuel,
        custom_name: snapshot.custom_name.as_ref(),
        lock: &snapshot.lock,
    })
}

fn encode_brewing_state(snapshot: &BrewingStandStateView<'_>) -> Option<Vec<u8>> {
    // Reject aggregate/field overflow before retaining a snapshot-sized output.
    let mut count = BrewingPayloadWriter::new(MAX_BREWING_SNAPSHOT_BYTES);
    count.counting = true;
    write_brewing_snapshot(snapshot, &mut count)?;
    let mut output = BrewingPayloadWriter::new(MAX_BREWING_SNAPSHOT_BYTES);
    write_brewing_snapshot(snapshot, &mut output)?;
    Some(output.into_inner())
}

fn write_brewing_snapshot(
    snapshot: &BrewingStandStateView<'_>,
    output: &mut BrewingPayloadWriter,
) -> Option<()> {
    if snapshot.items.len() != BREWING_STAND_SLOTS {
        return None;
    }
    output.write_all(BREWING_SNAPSHOT_HEADER).ok()?;
    output.write_all(&snapshot.identity.to_be_bytes()).ok()?;
    output.write_all(&snapshot.brew_time.to_be_bytes()).ok()?;
    output
        .write_all(&snapshot.recipe_brew_time.to_be_bytes())
        .ok()?;
    output.write_all(&snapshot.fuel.to_be_bytes()).ok()?;
    for item in snapshot.items {
        append_brewing_item(output, item)?;
    }
    match &snapshot.custom_name {
        Some(name) => {
            let offset = output.begin_blob()?;
            write_bounded(
                name,
                MAX_BREWING_TEXT_BYTES.min(output.limit.checked_sub(output.length)?),
                output,
            )
            .ok()?;
            output.finish_blob(offset, MAX_BREWING_TEXT_BYTES)?;
        }
        None => output.write_all(&u32::MAX.to_be_bytes()).ok()?,
    }
    let offset = output.begin_blob()?;
    snapshot
        .lock
        .write_bounded(
            MAX_BREWING_LOCK_BYTES.min(output.limit.checked_sub(output.length)?),
            output,
        )
        .ok()?;
    output.finish_blob(offset, MAX_BREWING_LOCK_BYTES)?;
    Some(())
}

struct BrewingPayloadReader<'a> {
    bytes: &'a [u8],
    cursor: usize,
}

impl<'a> BrewingPayloadReader<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, cursor: 0 }
    }

    fn take(&mut self, length: usize) -> Option<&'a [u8]> {
        let end = self.cursor.checked_add(length)?;
        let value = self.bytes.get(self.cursor..end)?;
        self.cursor = end;
        Some(value)
    }

    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_be_bytes(self.take(4)?.try_into().ok()?))
    }

    fn i32(&mut self) -> Option<i32> {
        Some(i32::from_be_bytes(self.take(4)?.try_into().ok()?))
    }

    fn u64(&mut self) -> Option<u64> {
        Some(u64::from_be_bytes(self.take(8)?.try_into().ok()?))
    }

    fn blob(&mut self, field_limit: usize) -> Option<&'a [u8]> {
        let length = usize::try_from(self.u32()?).ok()?;
        if length > field_limit {
            return None;
        }
        self.take(length)
    }

    const fn is_finished(&self) -> bool {
        self.cursor == self.bytes.len()
    }
}

fn read_brewing_item_after_length_check(
    length: usize,
    copy: impl FnOnce() -> Option<Vec<u8>>,
) -> Option<ItemStack> {
    if !brewing_item_payload_length_is_valid(length) {
        return None;
    }
    let bytes = copy()?;
    if bytes.len() != length {
        return None;
    }
    read_brewing_item(&bytes)
}

fn read_brewing_component_count(input: &mut Cursor<&[u8]>) -> Option<usize> {
    usize::try_from(VarInt::read(input).ok()?.0).ok()
}

fn read_brewing_item(bytes: &[u8]) -> Option<ItemStack> {
    DecodeBudget::new(512 * 1024)
        .decode(|| {
            read_brewing_item_inner(bytes).ok_or_else(|| IoError::other("Invalid brewing item"))
        })
        .ok()
}

fn read_brewing_item_inner(bytes: &[u8]) -> Option<ItemStack> {
    use foton_registry::data_components::ComponentPatchEntry;
    if bytes.len() > MAX_BREWING_ITEM_BYTES {
        return None;
    }
    let mut input = Cursor::new(bytes);
    let count = VarInt::read(&mut input).ok()?.0;
    let mut item = {
        let item_id = usize::try_from(VarInt::read(&mut input).ok()?.0).ok()?;
        let item_type = REGISTRY.items.by_id(item_id)?;
        let added_count = read_brewing_component_count(&mut input)?;
        let removed_count = read_brewing_component_count(&mut input)?;
        if added_count.checked_add(removed_count)? > MAX_BREWING_COMPONENTS {
            return None;
        }

        budget::charge_map::<Identifier, ComponentPatchEntry>(added_count + removed_count).ok()?;
        budget::charge_map::<Identifier, ()>(added_count + removed_count).ok()?;
        let mut patch = DataComponentPatch::new();
        let mut seen = FxHashSet::default();
        for _ in 0..added_count {
            let component_id = usize::try_from(VarInt::read(&mut input).ok()?.0).ok()?;
            let key = REGISTRY
                .data_components
                .get_key_by_id(component_id)?
                .clone();
            if !seen.insert(key.clone()) {
                return None;
            }
            let component = REGISTRY.data_components.by_id(component_id)?;
            let data = component.read_network(&mut input).ok()?;
            if !patch.set_raw(key, data) {
                return None;
            }
        }
        for _ in 0..removed_count {
            let component_id = usize::try_from(VarInt::read(&mut input).ok()?.0).ok()?;
            let key = REGISTRY
                .data_components
                .get_key_by_id(component_id)?
                .clone();
            if !seen.insert(key.clone()) || !patch.remove_raw(key) {
                return None;
            }
        }
        ItemStack::from_raw_parts(item_type, count, patch)
    };

    let opaque_length = u32::from_be_bytes({
        let start = usize::try_from(input.position()).ok()?;
        let end = start.checked_add(4)?;
        let bytes: [u8; 4] = input.get_ref().get(start..end)?.try_into().ok()?;
        input.set_position(u64::try_from(end).ok()?);
        bytes
    });
    if opaque_length != u32::MAX {
        let opaque_length = usize::try_from(opaque_length).ok()?;
        if opaque_length > MAX_BREWING_OPAQUE_NBT_BYTES {
            return None;
        }
        let start = usize::try_from(input.position()).ok()?;
        let end = start.checked_add(opaque_length)?;
        let opaque = str::from_utf8(input.get_ref().get(start..end)?).ok()?;
        budget::charge::<u8>(opaque.len()).ok()?;
        item.set_opaque_nbt(Some(opaque.to_owned()));
        input.set_position(u64::try_from(end).ok()?);
    }
    if usize::try_from(input.position()).ok()? != bytes.len() {
        return None;
    }
    if encode_brewing_item(&item)?.as_slice() != bytes {
        return None;
    }
    Some(item)
}

fn decode_brewing_snapshot(bytes: &[u8]) -> Option<BrewingBridgeSnapshot> {
    DecodeBudget::new(512 * 1024)
        .decode(|| {
            decode_brewing_snapshot_inner(bytes)
                .ok_or_else(|| IoError::other("Invalid brewing snapshot"))
        })
        .ok()
}

fn decode_brewing_snapshot_inner(bytes: &[u8]) -> Option<BrewingBridgeSnapshot> {
    if !brewing_payload_length_is_valid(bytes.len()) {
        return None;
    }
    let mut input = BrewingPayloadReader::new(bytes);
    if input.take(BREWING_SNAPSHOT_HEADER.len())? != BREWING_SNAPSHOT_HEADER {
        return None;
    }
    let identity = input.u64()?;
    let brew_time = input.i32()?;
    let recipe_brew_time = input.i32()?;
    let fuel = input.i32()?;
    if recipe_brew_time <= 0 {
        return None;
    }
    budget::charge::<ItemStack>(BREWING_STAND_SLOTS).ok()?;
    let mut items = Vec::with_capacity(BREWING_STAND_SLOTS);
    for _ in 0..BREWING_STAND_SLOTS {
        items.push(read_brewing_item(input.blob(MAX_BREWING_ITEM_BYTES)?)?);
    }
    let custom_name = match input.u32()? {
        u32::MAX => None,
        length => {
            let length = usize::try_from(length).ok()?;
            if length > MAX_BREWING_TEXT_BYTES {
                return None;
            }
            let encoded = input.take(length)?;
            let mut cursor = Cursor::new(encoded);
            let name = TextComponent::read(&mut cursor).ok()?;
            let consumed = usize::try_from(cursor.position()).ok()?;
            if consumed != encoded.len() {
                return None;
            }
            let mut canonical_name = BrewingPayloadWriter::new(MAX_BREWING_TEXT_BYTES);
            write_bounded(&name, MAX_BREWING_TEXT_BYTES, &mut canonical_name).ok()?;
            if canonical_name.bytes != encoded {
                return None;
            }
            Some(name)
        }
    };
    let encoded_lock = input.blob(MAX_BREWING_LOCK_BYTES)?;
    let mut cursor = Cursor::new(encoded_lock);
    let lock = LockCode::read(&mut cursor).ok()?;
    if usize::try_from(cursor.position()).ok()? != encoded_lock.len() || !input.is_finished() {
        return None;
    }
    let mut canonical_lock = BrewingPayloadWriter::new(MAX_BREWING_LOCK_BYTES);
    lock.write_bounded(MAX_BREWING_LOCK_BYTES, &mut canonical_lock)
        .ok()?;
    if canonical_lock.bytes != encoded_lock {
        return None;
    }
    Some(BrewingBridgeSnapshot {
        identity,
        items,
        brew_time,
        recipe_brew_time,
        fuel,
        custom_name,
        lock,
    })
}

const fn brewing_update_flags(apply_physics: bool) -> UpdateFlags {
    if apply_physics {
        UpdateFlags::UPDATE_ALL
    } else {
        UpdateFlags::UPDATE_CLIENTS
    }
}

fn brewing_apply_matches(
    current_state: BlockStateId,
    current_identity: Option<u64>,
    desired_state: BlockStateId,
    snapshot_identity: u64,
    force: bool,
) -> bool {
    desired_state.get_block() == &vanilla_blocks::BREWING_STAND
        && (force
            || (current_state.get_block() == desired_state.get_block()
                && current_identity == Some(snapshot_identity)))
}

const fn brewing_identity_from_java(identity: jlong) -> u64 {
    u64::from_ne_bytes(identity.to_ne_bytes())
}

fn apply_brewing_snapshot(
    world: &Arc<World>,
    pos: BlockPos,
    state: BlockStateId,
    snapshot: BrewingBridgeSnapshot,
    force: bool,
    apply_physics: bool,
) -> bool {
    if snapshot.items.len() != BREWING_STAND_SLOTS || snapshot.recipe_brew_time <= 0 {
        return false;
    }
    let current_state = world.get_block_state(pos);
    let current_entity = world.get_block_entity(pos);
    let current = current_entity
        .as_deref()
        .and_then(|entity| entity.downcast_ref::<BrewingStandBlockEntity>());
    let current_identity = current.map(BrewingStandBlockEntity::identity);
    if !brewing_apply_matches(
        current_state,
        current_identity,
        state,
        snapshot.identity,
        force,
    ) {
        return false;
    }
    if current_state.get_block() == state.get_block() {
        let Some(brewing) = current else {
            return false;
        };
        let target = BrewingStandStateSnapshot {
            identity: if force {
                brewing.identity()
            } else {
                snapshot.identity
            },
            items: snapshot.items,
            brew_time: snapshot.brew_time,
            recipe_brew_time: snapshot.recipe_brew_time,
            fuel: snapshot.fuel,
            custom_name: snapshot.custom_name,
            lock: snapshot.lock,
        };
        if current_state == state {
            return brewing.apply_state_snapshot(target);
        }
        let Some(current_entity) = current_entity.as_ref() else {
            return false;
        };
        return brewing.apply_state_snapshot_with_state(
            world,
            current_entity,
            state,
            target,
            brewing_update_flags(apply_physics),
        );
    }
    if !BLOCK_ENTITIES.has_factory(&BREWING_STAND_BLOCK_ENTITY) {
        return false;
    }
    let brewing = Arc::new(BrewingStandBlockEntity::new(
        Arc::downgrade(world),
        pos,
        state,
    ));
    let mut target = brewing.state_snapshot();
    target.items = snapshot.items;
    target.brew_time = snapshot.brew_time;
    target.recipe_brew_time = snapshot.recipe_brew_time;
    target.fuel = snapshot.fuel;
    target.custom_name = snapshot.custom_name;
    target.lock = snapshot.lock;
    brewing.apply_replacement_snapshot(target);
    let replacement: SharedBlockEntity = brewing;
    world.replace_block_with_entity_if_unchanged(
        pos,
        current_state,
        current_entity.as_ref(),
        state,
        replacement,
        brewing_update_flags(apply_physics),
    )
}

extern "system" fn brewing_stand_snapshot(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    x: jint,
    y: jint,
    z: jint,
) -> jbyteArray {
    let encoded = world(&mut env, &name)
        .and_then(|world| world.get_block_entity(BlockPos::new(x, y, z)))
        .and_then(|entity| {
            let brewing = entity.downcast_ref::<BrewingStandBlockEntity>()?;
            brewing.with_state(|snapshot| encode_brewing_state(&snapshot))
        });
    let Some(encoded) = encoded else {
        return null_mut();
    };
    env.byte_array_from_slice(&encoded)
        .map_or_else(|_| null_mut(), JByteArray::into_raw)
}

extern "system" fn brewing_stand_live_item(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    x: jint,
    y: jint,
    z: jint,
    identity: jlong,
    slot: jint,
) -> jbyteArray {
    let value = world(&mut env, &name)
        .and_then(|world| world.get_block_entity(BlockPos::new(x, y, z)))
        .and_then(|entity| {
            let brewing = entity.downcast_ref::<BrewingStandBlockEntity>()?;
            let slot = usize::try_from(slot).ok()?;
            brewing
                .with_live_item(
                    brewing_identity_from_java(identity),
                    slot,
                    encode_brewing_item,
                )
                .flatten()
        });
    let Some(value) = value else {
        return null_mut();
    };
    env.byte_array_from_slice(&value)
        .map_or_else(|_| null_mut(), JByteArray::into_raw)
}

extern "system" fn brewing_stand_set_live_item(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    x: jint,
    y: jint,
    z: jint,
    identity: jlong,
    slot: jint,
    item: JByteArray<'_>,
) -> jboolean {
    if !on_tick() {
        return 0;
    }
    let Some(slot) = usize::try_from(slot).ok() else {
        return 0;
    };
    let Ok(item_length) = env.get_array_length(&item) else {
        return 0;
    };
    let Ok(item_length) = usize::try_from(item_length) else {
        return 0;
    };
    let Some(item) =
        read_brewing_item_after_length_check(item_length, || env.convert_byte_array(&item).ok())
    else {
        return 0;
    };
    let changed = world(&mut env, &name)
        .and_then(|world| world.get_block_entity(BlockPos::new(x, y, z)))
        .and_then(|entity| {
            entity
                .downcast_ref::<BrewingStandBlockEntity>()
                .map(|brewing| {
                    brewing.set_live_item(brewing_identity_from_java(identity), slot, item)
                })
        })
        .unwrap_or(false);
    jboolean::from(changed)
}

extern "system" fn brewing_stand_apply(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    x: jint,
    y: jint,
    z: jint,
    state: JString<'_>,
    payload: JByteArray<'_>,
    force: jboolean,
    apply_physics: jboolean,
) -> jboolean {
    if !on_tick() {
        return 0;
    }
    let Ok(state): Result<String, _> = env.get_string(&state).map(Into::into) else {
        return 0;
    };
    let Some(state) = parse_state(&state) else {
        return 0;
    };
    let Ok(payload_length) = env.get_array_length(&payload) else {
        return 0;
    };
    let Ok(payload_length) = usize::try_from(payload_length) else {
        return 0;
    };
    if !brewing_payload_length_is_valid(payload_length) {
        return 0;
    }
    let Ok(payload) = env.convert_byte_array(&payload) else {
        return 0;
    };
    let Some(snapshot) = decode_brewing_snapshot(&payload) else {
        return 0;
    };
    let Some(world) = world(&mut env, &name) else {
        return 0;
    };
    jboolean::from(apply_brewing_snapshot(
        &world,
        BlockPos::new(x, y, z),
        state,
        snapshot,
        force != 0,
        apply_physics != 0,
    ))
}

extern "system" fn brewing_stand_state(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    x: jint,
    y: jint,
    z: jint,
) -> jstring {
    let value = world(&mut env, &name)
        .and_then(|world| world.get_block_entity(BlockPos::new(x, y, z)))
        .and_then(|entity| {
            let brewing = entity.downcast_ref::<BrewingStandBlockEntity>()?;
            let (brew_time, fuel) = brewing.with_state(|state| (state.brew_time, state.fuel));
            Some(format!("{brew_time}\u{001f}{fuel}"))
        });
    to_java(&mut env, value)
}

extern "system" fn hopper_set_inventory_slot(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    x: jint,
    y: jint,
    z: jint,
    slot: jint,
    item: JString<'_>,
) {
    let Ok(item): Result<String, _> = env.get_string(&item).map(Into::into) else {
        return;
    };
    let Some(stack) = parse_slot(&item) else {
        return;
    };
    let Some(world) = world(&mut env, &name) else {
        return;
    };
    let Some(entity) = world.get_block_entity(BlockPos::new(x, y, z)) else {
        return;
    };
    let Some(container) = ContainerRef::from_block_entity(entity) else {
        return;
    };
    let mut guard = ContainerLockGuard::lock_all(&[&container]);
    if slot >= 0
        && let Some(c) = guard.get_mut(container.container_id())
        && (slot as usize) < c.get_container_size()
    {
        c.set_item(slot as usize, stack);
    }
}

/// `foton.Native.signLines`
extern "system" fn sign_lines(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    x: jint,
    y: jint,
    z: jint,
) -> jobjectArray {
    let lines = world(&mut env, &name)
        .and_then(|world| {
            let entity = world.get_block_entity(BlockPos::new(x, y, z))?;
            let sign = entity.downcast_ref::<SignBlockEntity>()?;
            let text = sign.get_text(true);
            Some(
                (0..4)
                    .map(|index| {
                        text.get_message(index)
                            .map_or_else(String::new, |line| line.to_plain(&DisplayResolutor))
                    })
                    .collect::<Vec<_>>(),
            )
        })
        .unwrap_or_default();
    string_array(&mut env, &lines)
}

extern "system" fn sign_set_waxed(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    x: jint,
    y: jint,
    z: jint,
    waxed: jboolean,
) {
    let Some(world) = world(&mut env, &name) else {
        return;
    };
    let Some(entity) = world.get_block_entity(BlockPos::new(x, y, z)) else {
        return;
    };
    let Some(sign) = entity.downcast_ref::<SignBlockEntity>() else {
        return;
    };
    sign.set_waxed(waxed != 0);
}

extern "system" fn sign_is_waxed(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    x: jint,
    y: jint,
    z: jint,
) -> jboolean {
    let waxed = world(&mut env, &name)
        .and_then(|world| world.get_block_entity(BlockPos::new(x, y, z)))
        .and_then(|entity| {
            entity
                .downcast_ref::<SignBlockEntity>()
                .map(SignBlockEntity::is_waxed)
        })
        .unwrap_or(false);
    u8::from(waxed)
}

extern "system" fn spawner_delay(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    x: jint,
    y: jint,
    z: jint,
) -> jint {
    world(&mut env, &name)
        .and_then(|world| world.get_block_entity(BlockPos::new(x, y, z)))
        .and_then(|entity| {
            entity
                .downcast_ref::<SpawnerBlockEntity>()
                .map(|spawner| spawner.spawner().spawn_delay())
        })
        .unwrap_or(0)
}
extern "system" fn set_spawner_delay(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    x: jint,
    y: jint,
    z: jint,
    delay: jint,
) {
    let Some(world) = world(&mut env, &name) else {
        return;
    };
    let Some(entity) = world.get_block_entity(BlockPos::new(x, y, z)) else {
        return;
    };
    let Some(spawner) = entity.downcast_ref::<SpawnerBlockEntity>() else {
        return;
    };
    spawner.spawner().set_spawn_delay(delay);
}

extern "system" fn spawner_min_spawn_delay(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    x: jint,
    y: jint,
    z: jint,
) -> jint {
    world(&mut env, &name)
        .and_then(|world| world.get_block_entity(BlockPos::new(x, y, z)))
        .and_then(|entity| {
            entity
                .downcast_ref::<SpawnerBlockEntity>()
                .map(|spawner| spawner.spawner().min_spawn_delay())
        })
        .unwrap_or(0)
}

extern "system" fn set_spawner_min_spawn_delay(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    x: jint,
    y: jint,
    z: jint,
    delay: jint,
) {
    let Some(world) = world(&mut env, &name) else {
        return;
    };
    let Some(entity) = world.get_block_entity(BlockPos::new(x, y, z)) else {
        return;
    };
    let Some(spawner) = entity.downcast_ref::<SpawnerBlockEntity>() else {
        return;
    };
    spawner.spawner().set_min_spawn_delay(delay);
}

extern "system" fn spawner_max_spawn_delay(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    x: jint,
    y: jint,
    z: jint,
) -> jint {
    world(&mut env, &name)
        .and_then(|world| world.get_block_entity(BlockPos::new(x, y, z)))
        .and_then(|entity| {
            entity
                .downcast_ref::<SpawnerBlockEntity>()
                .map(|spawner| spawner.spawner().max_spawn_delay())
        })
        .unwrap_or(0)
}

extern "system" fn set_spawner_max_spawn_delay(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    x: jint,
    y: jint,
    z: jint,
    delay: jint,
) {
    let Some(world) = world(&mut env, &name) else {
        return;
    };
    let Some(entity) = world.get_block_entity(BlockPos::new(x, y, z)) else {
        return;
    };
    let Some(spawner) = entity.downcast_ref::<SpawnerBlockEntity>() else {
        return;
    };
    spawner.spawner().set_max_spawn_delay(delay);
}

extern "system" fn spawner_entity_type<'a>(
    mut env: JNIEnv<'a>,
    _class: JClass<'a>,
    name: JString<'a>,
    x: jint,
    y: jint,
    z: jint,
) -> JString<'a> {
    let key = world(&mut env, &name)
        .and_then(|world| world.get_block_entity(BlockPos::new(x, y, z)))
        .and_then(|entity| {
            entity
                .downcast_ref::<SpawnerBlockEntity>()
                .and_then(|spawner| spawner.spawner().next_entity_type_key())
        })
        .map(|key| key.path.to_string());
    key.and_then(|value| env.new_string(value).ok())
        .unwrap_or_else(|| JString::from(JObject::null()))
}

extern "system" fn set_spawner_entity_type(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    x: jint,
    y: jint,
    z: jint,
    type_name: JString<'_>,
) {
    let Ok(type_text) = env.get_string(&type_name) else {
        return;
    };
    let Some(entity_type) = REGISTRY.entity_types.by_key(&Identifier::vanilla(
        type_text.to_str().unwrap_or_default().to_ascii_lowercase(),
    )) else {
        return;
    };
    let Some(world) = world(&mut env, &name) else {
        return;
    };
    let Some(entity) = world.get_block_entity(BlockPos::new(x, y, z)) else {
        return;
    };
    let Some(spawner) = entity.downcast_ref::<SpawnerBlockEntity>() else {
        return;
    };
    spawner.set_spawner_entity_id(entity_type);
}

extern "system" fn sign_set_line(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    x: jint,
    y: jint,
    z: jint,
    line: JString<'_>,
    index: jint,
) {
    let Ok(line): Result<String, _> = env.get_string(&line).map(Into::into) else {
        return;
    };
    if !(0..4).contains(&index) {
        return;
    }
    let Some(world) = world(&mut env, &name) else {
        return;
    };
    let Some(entity) = world.get_block_entity(BlockPos::new(x, y, z)) else {
        return;
    };
    let Some(sign) = entity.downcast_ref::<SignBlockEntity>() else {
        return;
    };
    sign.update_text(
        |text| {
            text.set_message(index as usize, TextComponent::plain(line));
            true
        },
        true,
    );
}

extern "system" fn sign_glowing(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    x: jint,
    y: jint,
    z: jint,
    front: jboolean,
) -> jboolean {
    let value = world(&mut env, &name)
        .and_then(|world| world.get_block_entity(BlockPos::new(x, y, z)))
        .and_then(|entity| {
            entity
                .downcast_ref::<SignBlockEntity>()
                .map(|sign| sign.get_text(front != 0).has_glowing_text)
        })
        .unwrap_or(false);
    jboolean::from(value)
}

extern "system" fn sign_set_glowing(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    x: jint,
    y: jint,
    z: jint,
    front: jboolean,
    glowing: jboolean,
) {
    let Some(world) = world(&mut env, &name) else {
        return;
    };
    let Some(entity) = world.get_block_entity(BlockPos::new(x, y, z)) else {
        return;
    };
    let Some(sign) = entity.downcast_ref::<SignBlockEntity>() else {
        return;
    };
    sign.update_text(|text| text.set_has_glowing_text(glowing != 0), front != 0);
}

extern "system" fn sign_color(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    x: jint,
    y: jint,
    z: jint,
    front: jboolean,
) -> jint {
    let Some(world) = world(&mut env, &name) else {
        return -1;
    };
    let Some(entity) = world.get_block_entity(BlockPos::new(x, y, z)) else {
        return -1;
    };
    let Some(sign) = entity.downcast_ref::<SignBlockEntity>() else {
        return -1;
    };
    foton_registry::DyeColor::VALUES
        .iter()
        .position(|color| *color == sign.get_text(front != 0).color)
        .map_or(-1, |index| index as jint)
}

extern "system" fn sign_set_color(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    x: jint,
    y: jint,
    z: jint,
    color: jint,
) {
    let Some(world) = world(&mut env, &name) else {
        return;
    };
    let Some(entity) = world.get_block_entity(BlockPos::new(x, y, z)) else {
        return;
    };
    let Some(sign) = entity.downcast_ref::<SignBlockEntity>() else {
        return;
    };
    let Some(color) = foton_registry::DyeColor::VALUES
        .get(color as usize)
        .copied()
    else {
        return;
    };
    sign.update_text(|text| text.set_color(color), true);
}

extern "system" fn sign_side_lines(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    x: jint,
    y: jint,
    z: jint,
    front: jboolean,
) -> jobjectArray {
    let lines = world(&mut env, &name)
        .and_then(|world| {
            let entity = world.get_block_entity(BlockPos::new(x, y, z))?;
            let sign = entity.downcast_ref::<SignBlockEntity>()?;
            let text = sign.get_text(front != 0);
            Some(
                (0..4)
                    .map(|index| {
                        text.get_message(index)
                            .map_or_else(String::new, |line| line.to_plain(&DisplayResolutor))
                    })
                    .collect::<Vec<_>>(),
            )
        })
        .unwrap_or_default();
    string_array(&mut env, &lines)
}

extern "system" fn sign_side_set_line(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    x: jint,
    y: jint,
    z: jint,
    line: JString<'_>,
    index: jint,
    front: jboolean,
) {
    let Ok(line): Result<String, _> = env.get_string(&line).map(Into::into) else {
        return;
    };
    if !(0..4).contains(&index) {
        return;
    }
    let Some(world) = world(&mut env, &name) else {
        return;
    };
    let Some(entity) = world.get_block_entity(BlockPos::new(x, y, z)) else {
        return;
    };
    let Some(sign) = entity.downcast_ref::<SignBlockEntity>() else {
        return;
    };
    sign.update_text(
        |text| {
            text.set_message(index as usize, TextComponent::plain(line));
            true
        },
        front != 0,
    );
}

/// `foton.Native.bannerPatterns`
extern "system" fn banner_patterns(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    x: jint,
    y: jint,
    z: jint,
) -> jobjectArray {
    let patterns = world(&mut env, &name)
        .and_then(|world| {
            let entity = world.get_block_entity(BlockPos::new(x, y, z))?;
            let banner = entity.downcast_ref::<BannerBlockEntity>()?;
            Some(
                banner
                    .pattern_descriptions()
                    .into_iter()
                    .map(|(key, color)| format!("{key}|{color}"))
                    .collect::<Vec<_>>(),
            )
        })
        .unwrap_or_default();
    string_array(&mut env, &patterns)
}

/// `foton.Native.setBannerPatterns`
extern "system" fn set_banner_patterns(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    x: jint,
    y: jint,
    z: jint,
    encoded: JString<'_>,
) -> jboolean {
    let Some(world) = world(&mut env, &name) else {
        return 0;
    };
    let Ok(encoded) = env.get_string(&encoded) else {
        return 0;
    };
    let Some(entity) = world.get_block_entity(BlockPos::new(x, y, z)) else {
        return 0;
    };
    let Some(banner) = entity.downcast_ref::<BannerBlockEntity>() else {
        return 0;
    };
    let mut descriptions = Vec::new();
    for item in encoded
        .to_str()
        .unwrap_or_default()
        .split(';')
        .filter(|item| !item.is_empty())
    {
        let Some((key, color)) = item.split_once('|') else {
            return 0;
        };
        let Ok(key) = Identifier::from_str(key) else {
            return 0;
        };
        let Ok(color) = color.parse::<usize>() else {
            return 0;
        };
        let Some(color) = foton_registry::DyeColor::VALUES.get(color).copied() else {
            return 0;
        };
        descriptions.push((key, color));
    }
    jboolean::from(banner.set_pattern_descriptions(descriptions))
}

/// `foton.Native.worldLoadedChunkCoords`
extern "system" fn world_loaded_chunk_coords(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
) -> jobjectArray {
    let coords = world(&mut env, &name).map_or_else(Vec::new, |world| {
        world
            .loaded_chunk_positions()
            .into_iter()
            .map(|pos| format!("{},{}", pos.0.x, pos.0.y))
            .collect()
    });
    string_array(&mut env, &coords)
}

/// `foton.Native.worldDropItem`
extern "system" fn world_drop_item(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    x: jdouble,
    y: jdouble,
    z: jdouble,
    item: JString<'_>,
) -> jstring {
    let Some(world) = world(&mut env, &name) else {
        return null_mut();
    };
    let Ok(item) = env.get_string(&item) else {
        return null_mut();
    };
    let Ok(item) = item.to_str() else {
        return null_mut();
    };
    let Some(stack) = parse_slot(item) else {
        return null_mut();
    };
    let Some(entity) = world.spawn_item(glam::DVec3::new(x, y, z), stack) else {
        return null_mut();
    };
    to_java(&mut env, Some(entity.uuid().to_string()))
}

/// `foton.Native.worldAutoSave`
extern "system" fn world_auto_save(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
) -> jboolean {
    jboolean::from(world(&mut env, &name).is_some_and(|world| world.is_auto_save()))
}

/// `foton.Native.setWorldAutoSave`
extern "system" fn set_world_auto_save(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    value: jboolean,
) {
    if let Some(world) = world(&mut env, &name) {
        world.set_auto_save(value != 0);
    }
}

/// `foton.Native.saveWorld`
extern "system" fn save_world(mut env: JNIEnv<'_>, _class: JClass<'_>, name: JString<'_>) {
    if let Some(world) = world(&mut env, &name) {
        world.request_save();
    }
}

/// `foton.Native.worldFolder`
extern "system" fn world_folder(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
) -> jstring {
    let Some(path) = world(&mut env, &name).and_then(|world| world.world_folder()) else {
        return null_mut();
    };
    let value = path.to_string_lossy();
    let Ok(value) = env.new_string(value.as_ref()) else {
        return null_mut();
    };
    value.into_raw()
}

/// `foton.Native.scoreboardTeamEntries`
extern "system" fn scoreboard_team_entries(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    world_name: JString<'_>,
    team_name: JString<'_>,
) -> jobjectArray {
    let Ok(world_name): Result<String, _> = env.get_string(&world_name).map(Into::into) else {
        return string_array(&mut env, &[]);
    };
    let Ok(team_name): Result<String, _> = env.get_string(&team_name).map(Into::into) else {
        return string_array(&mut env, &[]);
    };
    let entries = server()
        .and_then(|server| {
            let key: Identifier = world_name.parse().ok()?;
            let world = server.worlds.get_owned(&key)?;
            server.scoreboards.get(world.domain()).map(|scoreboard| {
                scoreboard
                    .team(&team_name)
                    .map(|team| scoreboard.team_entries(&team))
                    .unwrap_or_default()
            })
        })
        .unwrap_or_default();
    string_array(&mut env, &entries)
}

/// `foton.Native.scoreboardEntryTeam`
extern "system" fn scoreboard_entry_team(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    world_name: JString<'_>,
    entry: JString<'_>,
) -> jstring {
    let Ok(world_name): Result<String, _> = env.get_string(&world_name).map(Into::into) else {
        return null_mut();
    };
    let Ok(entry): Result<String, _> = env.get_string(&entry).map(Into::into) else {
        return null_mut();
    };
    let team = server().and_then(|server| {
        let key: Identifier = world_name.parse().ok()?;
        let world = server.worlds.get_owned(&key)?;
        server
            .scoreboards
            .get(world.domain())
            .and_then(|scoreboard| scoreboard.holder_team_name(&ScoreHolder::new(entry)))
    });
    to_java(&mut env, team)
}

/// `foton.Native.setWorldSpawn`
extern "system" fn set_world_spawn(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    x: jint,
    y: jint,
    z: jint,
) -> jboolean {
    let Some(world) = world(&mut env, &name) else {
        return 0;
    };
    world
        .level_data
        .write()
        .data_mut()
        .set_spawn_pos(BlockPos::new(x, y, z));
    1
}

/// `foton.Native.worldSpawn`
extern "system" fn world_spawn(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
) -> jdoubleArray {
    let at = world(&mut env, &name).map(|world| {
        let spawn = world.level_data.read().data().spawn_pos();
        // The center of the block, which is where vanilla puts a player
        // standing on it rather than in its corner.
        [
            f64::from(spawn.0.x) + 0.5,
            f64::from(spawn.0.y),
            f64::from(spawn.0.z) + 0.5,
            0.0,
            0.0,
        ]
    });
    to_position(&mut env, at)
}

/// `foton.Native.worldTime`
extern "system" fn reset_world_border(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    world_name: JString<'_>,
) {
    if let Some(world) = world(&mut env, &world_name) {
        world.reset_world_border();
    }
}

extern "system" fn world_border(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
) -> jdoubleArray {
    let Some(world) = world(&mut env, &name) else {
        return null_mut();
    };
    let (x, z, size) = world.world_border_center_size();
    let Ok(array) = env.new_double_array(3) else {
        return null_mut();
    };
    let _ = env.set_double_array_region(&array, 0, &[x, z, size]);
    array.into_raw()
}
extern "system" fn set_world_border_center(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    x: jdouble,
    z: jdouble,
) {
    if let Some(world) = world(&mut env, &name) {
        let _ = world.set_world_border_center(x, z);
    }
}
extern "system" fn world_border_warning_distance(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    world_name: JString<'_>,
) -> jint {
    world(&mut env, &world_name).map_or(5, |world| world.world_border_warning_blocks())
}

extern "system" fn set_world_border_warning_distance(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    world_name: JString<'_>,
    distance: jint,
) {
    if let Some(world) = world(&mut env, &world_name) {
        world.set_world_border_warning_blocks(distance.max(0));
    }
}

extern "system" fn world_border_warning_time(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    world_name: JString<'_>,
) -> jint {
    world(&mut env, &world_name).map_or(300, |world| world.world_border_warning_time())
}

extern "system" fn set_world_border_warning_time(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    world_name: JString<'_>,
    ticks: jint,
) {
    if let Some(world) = world(&mut env, &world_name) {
        world.set_world_border_warning_time(ticks.max(0));
    }
}

extern "system" fn world_border_damage_amount(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    world_name: JString<'_>,
) -> jdouble {
    world(&mut env, &world_name).map_or(0.2, |value| value.world_border_damage_per_block())
}

extern "system" fn set_world_border_damage_amount(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    world_name: JString<'_>,
    amount: jdouble,
) {
    if let Some(value) = world(&mut env, &world_name) {
        let _ = value.set_world_border_damage_per_block(amount);
    }
}

extern "system" fn world_border_damage_buffer(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    world_name: JString<'_>,
) -> jdouble {
    world(&mut env, &world_name).map_or(0.0, |world| world.world_border_safe_zone())
}

extern "system" fn set_world_border_damage_buffer(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    world_name: JString<'_>,
    distance: jdouble,
) {
    if let Some(world) = world(&mut env, &world_name) {
        let _ = world.set_world_border_safe_zone(distance);
    }
}

extern "system" fn set_world_border_lerp(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    old_size: jdouble,
    new_size: jdouble,
    ticks: jlong,
) {
    if let Some(world) = world(&mut env, &name) {
        let _ = world.lerp_world_border_size_between(old_size, new_size, ticks);
    }
}
extern "system" fn set_world_border_size(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    size: jdouble,
) {
    if let Some(world) = world(&mut env, &name) {
        let _ = world.set_world_border_size(size);
    }
}
extern "system" fn create_explosion_advanced(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    x: jdouble,
    y: jdouble,
    z: jdouble,
    power: jfloat,
    fire: jboolean,
    break_blocks: jboolean,
) -> jboolean {
    let Some(world) = world(&mut env, &name) else {
        return 0;
    };
    if !power.is_finite() || power < 0.0 {
        return 0;
    }
    let interaction = if break_blocks != 0 {
        world.explosion_destroy_type(&TNT_EXPLOSION_DROP_DECAY)
    } else {
        ExplosionBlockInteraction::Keep
    };
    let spec = ExplosionSpec::new(None, None, None, power, fire != 0, interaction);
    world.explode(spec, glam::DVec3::new(x, y, z));
    1
}

extern "system" fn world_time(mut env: JNIEnv<'_>, _class: JClass<'_>, name: JString<'_>) -> jlong {
    world(&mut env, &name).map_or(-1, |world| world.game_time())
}

extern "system" fn set_world_time(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    time: jlong,
) {
    if let Some(world) = world(&mut env, &name) {
        world.level_data.write().set_game_time(time);
        world.broadcast_time_sync();
    }
}

extern "system" fn create_explosion(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    x: jdouble,
    y: jdouble,
    z: jdouble,
    power: jfloat,
) -> jboolean {
    let Some(world) = world(&mut env, &name) else {
        return 0;
    };
    if !power.is_finite() || power < 0.0 {
        return 0;
    }
    let interaction = world.explosion_destroy_type(&TNT_EXPLOSION_DROP_DECAY);
    let spec = ExplosionSpec::new(None, None, None, power, false, interaction);
    world.explode(spec, glam::DVec3::new(x, y, z));
    1
}

extern "system" fn world_min_height(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
) -> jint {
    world(&mut env, &name).map_or(0, |world| world.get_min_y())
}

extern "system" fn world_max_height(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
) -> jint {
    world(&mut env, &name).map_or(0, |world| world.max_build_height())
}

extern "system" fn is_sneaking(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jboolean {
    jboolean::from(player(&mut env, &uuid).is_some_and(|player| player.is_crouching()))
}

/// Parses the recipe keys a plugin handed over, dropping any that do not parse.
///
/// Paper drops them the same way: `bukkitKeysToMinecraftRecipes` keeps only
/// the keys the recipe manager resolves, and a malformed key resolves to
/// nothing.
fn recipe_keys(env: &mut JNIEnv<'_>, keys: &JObjectArray<'_>) -> Vec<Identifier> {
    read_string_array(env, keys)
        .unwrap_or_default()
        .iter()
        .filter_map(|key| key.parse().ok())
        .collect()
}

/// `foton.Native.discoverRecipes` -- Paper's `CraftHumanEntity.discoverRecipes`,
/// which is `ServerPlayer.awardRecipes`.
extern "system" fn discover_recipes(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    keys: JObjectArray<'_>,
) -> jint {
    let Some(player) = player(&mut env, &uuid) else {
        return 0;
    };
    let keys = recipe_keys(&mut env, &keys);
    i32::try_from(player.award_recipes(&keys)).unwrap_or(i32::MAX)
}

/// `foton.Native.undiscoverRecipes` -- `ServerPlayer.resetRecipes`.
extern "system" fn undiscover_recipes(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    keys: JObjectArray<'_>,
) -> jint {
    let Some(player) = player(&mut env, &uuid) else {
        return 0;
    };
    let keys = recipe_keys(&mut env, &keys);
    i32::try_from(player.reset_recipes(&keys)).unwrap_or(i32::MAX)
}

/// `foton.Native.hasDiscoveredRecipe` -- `ServerRecipeBook.contains`.
extern "system" fn has_discovered_recipe(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    key: JString<'_>,
) -> jboolean {
    let Some(player) = player(&mut env, &uuid) else {
        return u8::from(false);
    };
    let known = env
        .get_string(&key)
        .ok()
        .and_then(|key| key.to_str().ok()?.parse::<Identifier>().ok())
        .is_some_and(|key| player.has_recipe(&key));
    u8::from(known)
}

/// `foton.Native.discoveredRecipes` -- every key the book knows.
extern "system" fn discovered_recipes(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jobjectArray {
    let keys: Vec<String> = player(&mut env, &uuid)
        .map(|player| {
            player
                .known_recipes()
                .iter()
                .map(ToString::to_string)
                .collect()
        })
        .unwrap_or_default();
    string_array(&mut env, &keys)
}

extern "system" fn open_book(mut env: JNIEnv<'_>, _class: JClass<'_>, uuid: JString<'_>) {
    let Some(player) = player(&mut env, &uuid) else {
        return;
    };
    let inventory = player.inventory.lock();
    let hand = if inventory
        .get_item_in_hand(InteractionHand::MainHand)
        .is(&vanilla_items::WRITTEN_BOOK)
        || inventory
            .get_item_in_hand(InteractionHand::MainHand)
            .is(&vanilla_items::WRITABLE_BOOK)
    {
        InteractionHand::MainHand
    } else if inventory
        .get_item_in_hand(InteractionHand::OffHand)
        .is(&vanilla_items::WRITTEN_BOOK)
        || inventory
            .get_item_in_hand(InteractionHand::OffHand)
            .is(&vanilla_items::WRITABLE_BOOK)
    {
        InteractionHand::OffHand
    } else {
        return;
    };
    drop(inventory);
    player.send_packet(COpenBook { hand });
}

extern "system" fn teleport_entity(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    world_name: JString<'_>,
    x: jdouble,
    y: jdouble,
    z: jdouble,
    yaw: jfloat,
    pitch: jfloat,
) -> jboolean {
    let Ok(world_name) = env.get_string(&world_name) else {
        return 0;
    };
    let Ok(text) = env.get_string(&uuid) else {
        return 0;
    };
    let Ok(text) = text.to_str() else {
        return 0;
    };
    let Ok(id) = Uuid::parse_str(text) else {
        return 0;
    };
    let Some((world, entity)) = entity_by_uuid(&id) else {
        return 0;
    };
    if world.key.to_string() != String::from(world_name) {
        return 0;
    }
    if entity.try_set_position(DVec3::new(x, y, z)).is_err() {
        return 0;
    }
    entity.set_rotation((yaw, pitch));
    1
}

extern "system" fn teleport(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    world_name: JString<'_>,
    x: jdouble,
    y: jdouble,
    z: jdouble,
    yaw: jfloat,
    pitch: jfloat,
) -> jboolean {
    let Some(player) = player(&mut env, &uuid) else {
        return 0;
    };
    let Ok(world_name) = env.get_string(&world_name) else {
        return 0;
    };
    // `FotonPlayer.teleport` has already asked `PlayerTeleportEvent`.
    u8::from(player.teleport_announced(&TeleportPoint {
        world: world_name.into(),
        position: DVec3::new(x, y, z),
        rotation: (yaw, pitch),
    }))
}

/// Every native, with the descriptor the JVM matches it by.
///
/// A descriptor that disagrees with the Java declaration is not a compile
/// error on either side -- it is a `NoSuchMethodError` the first time a plugin
/// calls it, which is why they sit next to each other here.
#[expect(
    clippy::too_many_lines,
    reason = "one flat list of every native and its descriptor; splitting it would               put a name and the signature it must match in different functions"
)]
pub(crate) fn bindings() -> Vec<jni::NativeMethod> {
    use std::ffi::c_void;

    fn method(name: &str, signature: &str, pointer: *mut c_void) -> jni::NativeMethod {
        jni::NativeMethod {
            name: name.into(),
            sig: signature.into(),
            fn_ptr: pointer,
        }
    }

    let mut bindings = vec![
        method(
            "mergeItemSnbt",
            "(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
            merge_item_snbt as *mut c_void,
        ),
        method(
            "enchantmentCanEnchant",
            "(Ljava/lang/String;Ljava/lang/String;)Z",
            enchantment_can_enchant as *mut c_void,
        ),
        method(
            "blockPropertyValues",
            "(Ljava/lang/String;Ljava/lang/String;)[Ljava/lang/String;",
            block_property_values as *mut c_void,
        ),
        method(
            "enchantmentsConflict",
            "(Ljava/lang/String;Ljava/lang/String;)Z",
            enchantments_conflict as *mut c_void,
        ),
        method(
            "enchantmentItems",
            "(Ljava/lang/String;Z)[Ljava/lang/String;",
            enchantment_items as *mut c_void,
        ),
        method(
            "dyeFireworkColor",
            "(I)I",
            dye_firework_color as *mut c_void,
        ),
        method(
            "serverName",
            "()Ljava/lang/String;",
            server_name as *mut c_void,
        ),
        method(
            "serverMotd",
            "()Ljava/lang/String;",
            server_motd as *mut c_void,
        ),
        method(
            "isTagged",
            "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)Z",
            is_tagged as *mut c_void,
        ),
        method(
            "tagValues",
            "(Ljava/lang/String;Ljava/lang/String;)[Ljava/lang/String;",
            tag_values as *mut c_void,
        ),
        method(
            "serverVersion",
            "()Ljava/lang/String;",
            server_version as *mut c_void,
        ),
        method(
            "minecraftVersion",
            "()Ljava/lang/String;",
            minecraft_version as *mut c_void,
        ),
        method(
            "serverBrand",
            "()Ljava/lang/String;",
            server_brand as *mut c_void,
        ),
        method(
            "datapacks",
            "(Z)[Ljava/lang/String;",
            datapacks as *mut c_void,
        ),
        method("onlineMode", "()Z", online_mode as *mut c_void),
        method("maxPlayers", "()I", max_players as *mut c_void),
        method(
            "serverAllowFlight",
            "()Z",
            server_allow_flight as *mut c_void,
        ),
        method(
            "serverDefaultGameMode",
            "()Ljava/lang/String;",
            server_default_game_mode as *mut c_void,
        ),
        method(
            "serverViewDistance",
            "()I",
            server_view_distance as *mut c_void,
        ),
        method(
            "serverSimulationDistance",
            "()I",
            server_simulation_distance as *mut c_void,
        ),
        method("serverTps", "()[D", server_tps as *mut c_void),
        method(
            "serverAverageTickTime",
            "()D",
            server_average_tick_time as *mut c_void,
        ),
        method("isPrimaryThread", "()Z", is_primary_thread as *mut c_void),
        method("shutdown", "()V", shutdown as *mut c_void),
        method("savePlayers", "()V", save_players as *mut c_void),
        method(
            "experienceProgress",
            "(Ljava/lang/String;)F",
            experience_progress as *mut c_void,
        ),
        method(
            "setExperienceProgress",
            "(Ljava/lang/String;F)V",
            set_experience_progress as *mut c_void,
        ),
        method(
            "setExperienceLevel",
            "(Ljava/lang/String;I)V",
            set_experience_level as *mut c_void,
        ),
        method(
            "totalExperience",
            "(Ljava/lang/String;)I",
            total_experience as *mut c_void,
        ),
        method(
            "setTotalExperience",
            "(Ljava/lang/String;I)V",
            set_total_experience as *mut c_void,
        ),
        method(
            "giveExperience",
            "(Ljava/lang/String;I)V",
            give_experience as *mut c_void,
        ),
        method(
            "experienceLevel",
            "(Ljava/lang/String;)I",
            experience_level as *mut c_void,
        ),
        method(
            "playerIdByName",
            "(Ljava/lang/String;)Ljava/lang/String;",
            player_id_by_name as *mut c_void,
        ),
        method(
            "broadcast",
            "(Ljava/lang/String;)I",
            broadcast as *mut c_void,
        ),
        method(
            "playerPosition",
            "(Ljava/lang/String;)[D",
            player_position as *mut c_void,
        ),
        method(
            "unloadWorld",
            "(Ljava/lang/String;Z)Z",
            unload_world as *mut c_void,
        ),
        method(
            "worldNames",
            "()[Ljava/lang/String;",
            world_names as *mut c_void,
        ),
        method(
            "requestWorldCreation",
            "(Ljava/lang/String;Ljava/lang/String;JZ)J",
            request_world_creation as *mut c_void,
        ),
        method(
            "worldCreationState",
            "(J)I",
            world_creation_state as *mut c_void,
        ),
        method(
            "worldPlayerIds",
            "(Ljava/lang/String;)[Ljava/lang/String;",
            world_player_ids as *mut c_void,
        ),
        method(
            "worldEntityIds",
            "(Ljava/lang/String;)[Ljava/lang/String;",
            world_entity_ids as *mut c_void,
        ),
        method(
            "requestChunk",
            "(Ljava/lang/String;II)Ljava/lang/String;",
            request_chunk as *mut c_void,
        ),
        method(
            "chunkRequestReady",
            "(Ljava/lang/String;)Z",
            chunk_request_ready as *mut c_void,
        ),
        method(
            "worldChunkLoaded",
            "(Ljava/lang/String;II)Z",
            world_chunk_loaded as *mut c_void,
        ),
        method(
            "worldChunkGenerated",
            "(Ljava/lang/String;II)Z",
            world_chunk_generated as *mut c_void,
        ),
        method(
            "chunkBlockEntities",
            "(Ljava/lang/String;II)[Ljava/lang/String;",
            chunk_block_entities as *mut c_void,
        ),
        method(
            "worldHasBonusChest",
            "(Ljava/lang/String;)Z",
            world_has_bonus_chest as *mut c_void,
        ),
        method(
            "worldWeatherDuration",
            "(Ljava/lang/String;)I",
            world_weather_duration as *mut c_void,
        ),
        method(
            "setWorldWeatherDuration",
            "(Ljava/lang/String;I)V",
            set_world_weather_duration as *mut c_void,
        ),
        method(
            "worldThunderDuration",
            "(Ljava/lang/String;)I",
            world_thunder_duration as *mut c_void,
        ),
        method(
            "setWorldThunderDuration",
            "(Ljava/lang/String;I)V",
            set_world_thunder_duration as *mut c_void,
        ),
        method(
            "setWorldSpawnLimit",
            "(Ljava/lang/String;Ljava/lang/String;I)V",
            set_world_spawn_limit as *mut c_void,
        ),
        method(
            "setWorldSpawnTicks",
            "(Ljava/lang/String;Ljava/lang/String;I)V",
            set_world_spawn_ticks as *mut c_void,
        ),
        method(
            "worldSpawnLimit",
            "(Ljava/lang/String;Ljava/lang/String;)I",
            world_spawn_limit as *mut c_void,
        ),
        method(
            "worldKeepSpawnInMemory",
            "(Ljava/lang/String;)Z",
            world_keep_spawn_in_memory as *mut c_void,
        ),
        method(
            "setWorldKeepSpawnInMemory",
            "(Ljava/lang/String;Z)V",
            set_world_keep_spawn_in_memory as *mut c_void,
        ),
        method(
            "worldStorm",
            "(Ljava/lang/String;)Z",
            world_storm as *mut c_void,
        ),
        method(
            "setWorldStorm",
            "(Ljava/lang/String;Z)V",
            set_world_storm as *mut c_void,
        ),
        method(
            "worldThundering",
            "(Ljava/lang/String;)Z",
            world_thundering as *mut c_void,
        ),
        method(
            "setWorldThundering",
            "(Ljava/lang/String;Z)V",
            set_world_thundering as *mut c_void,
        ),
        method(
            "spawnEntity",
            "(Ljava/lang/String;DDDLjava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
            spawn_entity as *mut c_void,
        ),
        method(
            "spawnEntityPending",
            "(Ljava/lang/String;DDDLjava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
            spawn_entity_pending as *mut c_void,
        ),
        method(
            "finishPendingSpawn",
            "(Ljava/lang/String;Ljava/lang/String;Z)Z",
            finish_pending_spawn as *mut c_void,
        ),
        method(
            "worldGameRule",
            "(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
            world_game_rule as *mut c_void,
        ),
        method(
            "worldGameRuleDefault",
            "(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
            world_game_rule_default as *mut c_void,
        ),
        method(
            "setWorldGameRule",
            "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)Z",
            set_world_game_rule as *mut c_void,
        ),
        method(
            "signLines",
            "(Ljava/lang/String;III)[Ljava/lang/String;",
            sign_lines as *mut c_void,
        ),
        method(
            "jukeboxIsPlaying",
            "(Ljava/lang/String;III)Z",
            jukebox_is_playing as *mut c_void,
        ),
        method(
            "jukeboxSetRecord",
            "(Ljava/lang/String;IIILjava/lang/String;)V",
            jukebox_set_record as *mut c_void,
        ),
        method(
            "jukeboxRecord",
            "(Ljava/lang/String;III)Ljava/lang/String;",
            jukebox_record as *mut c_void,
        ),
        method(
            "hopperInventorySlot",
            "(Ljava/lang/String;IIII)Ljava/lang/String;",
            hopper_inventory_slot as *mut c_void,
        ),
        method("isFuel", "(Ljava/lang/String;)Z", is_fuel as *mut c_void),
        method(
            "craftingRecipe",
            "(Ljava/lang/String;I)Ljava/lang/String;",
            crafting_recipe as *mut c_void,
        ),
        method(
            "cookingRecipe",
            "(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
            cooking_recipe as *mut c_void,
        ),
        method(
            "furnaceTimes",
            "(Ljava/lang/String;III)[I",
            furnace_times as *mut c_void,
        ),
        method(
            "setFurnaceTimes",
            "(Ljava/lang/String;III[I)V",
            set_furnace_times as *mut c_void,
        ),
        method(
            "brewingStandState",
            "(Ljava/lang/String;III)Ljava/lang/String;",
            brewing_stand_state as *mut c_void,
        ),
        method(
            "brewingStandSnapshot",
            "(Ljava/lang/String;III)[B",
            brewing_stand_snapshot as *mut c_void,
        ),
        method(
            "brewingStandLiveItem",
            "(Ljava/lang/String;IIIJI)[B",
            brewing_stand_live_item as *mut c_void,
        ),
        method(
            "brewingStandSetLiveItem",
            "(Ljava/lang/String;IIIJI[B)Z",
            brewing_stand_set_live_item as *mut c_void,
        ),
        method(
            "brewingStandApply",
            "(Ljava/lang/String;IIILjava/lang/String;[BZZ)Z",
            brewing_stand_apply as *mut c_void,
        ),
        method(
            "hopperSetInventorySlot",
            "(Ljava/lang/String;IIIILjava/lang/String;)V",
            hopper_set_inventory_slot as *mut c_void,
        ),
        method(
            "hopperCustomName",
            "(Ljava/lang/String;III)Ljava/lang/String;",
            hopper_custom_name as *mut c_void,
        ),
        method(
            "hopperSetCustomName",
            "(Ljava/lang/String;IIILjava/lang/String;)V",
            hopper_set_custom_name as *mut c_void,
        ),
        method(
            "signIsWaxed",
            "(Ljava/lang/String;III)Z",
            sign_is_waxed as *mut c_void,
        ),
        method(
            "signSetWaxed",
            "(Ljava/lang/String;IIIZ)V",
            sign_set_waxed as *mut c_void,
        ),
        method(
            "spawnerDelay",
            "(Ljava/lang/String;III)I",
            spawner_delay as *mut c_void,
        ),
        method(
            "setSpawnerDelay",
            "(Ljava/lang/String;IIII)V",
            set_spawner_delay as *mut c_void,
        ),
        method(
            "spawnerMinSpawnDelay",
            "(Ljava/lang/String;III)I",
            spawner_min_spawn_delay as *mut c_void,
        ),
        method(
            "setSpawnerMinSpawnDelay",
            "(Ljava/lang/String;IIII)V",
            set_spawner_min_spawn_delay as *mut c_void,
        ),
        method(
            "spawnerMaxSpawnDelay",
            "(Ljava/lang/String;III)I",
            spawner_max_spawn_delay as *mut c_void,
        ),
        method(
            "setSpawnerMaxSpawnDelay",
            "(Ljava/lang/String;IIII)V",
            set_spawner_max_spawn_delay as *mut c_void,
        ),
        method(
            "spawnerEntityType",
            "(Ljava/lang/String;III)Ljava/lang/String;",
            spawner_entity_type as *mut c_void,
        ),
        method(
            "setSpawnerEntityType",
            "(Ljava/lang/String;IIILjava/lang/String;)V",
            set_spawner_entity_type as *mut c_void,
        ),
        method(
            "signSetLine",
            "(Ljava/lang/String;IIILjava/lang/String;I)V",
            sign_set_line as *mut c_void,
        ),
        method(
            "signSetColor",
            "(Ljava/lang/String;IIII)V",
            sign_set_color as *mut c_void,
        ),
        method(
            "signColor",
            "(Ljava/lang/String;IIIZ)I",
            sign_color as *mut c_void,
        ),
        method(
            "signSetGlowing",
            "(Ljava/lang/String;IIIZZ)V",
            sign_set_glowing as *mut c_void,
        ),
        method(
            "signGlowing",
            "(Ljava/lang/String;IIIZ)Z",
            sign_glowing as *mut c_void,
        ),
        method(
            "signSideLines",
            "(Ljava/lang/String;IIIZ)[Ljava/lang/String;",
            sign_side_lines as *mut c_void,
        ),
        method(
            "signSideSetLine",
            "(Ljava/lang/String;IIILjava/lang/String;IZ)V",
            sign_side_set_line as *mut c_void,
        ),
        method(
            "bannerPatterns",
            "(Ljava/lang/String;III)[Ljava/lang/String;",
            banner_patterns as *mut c_void,
        ),
        method(
            "setBannerPatterns",
            "(Ljava/lang/String;IIILjava/lang/String;)Z",
            set_banner_patterns as *mut c_void,
        ),
        method(
            "worldLoadedChunkCoords",
            "(Ljava/lang/String;)[Ljava/lang/String;",
            world_loaded_chunk_coords as *mut c_void,
        ),
        method(
            "worldFolder",
            "(Ljava/lang/String;)Ljava/lang/String;",
            world_folder as *mut c_void,
        ),
        method(
            "worldAutoSave",
            "(Ljava/lang/String;)Z",
            world_auto_save as *mut c_void,
        ),
        method(
            "setWorldAutoSave",
            "(Ljava/lang/String;Z)V",
            set_world_auto_save as *mut c_void,
        ),
        method(
            "saveWorld",
            "(Ljava/lang/String;)V",
            save_world as *mut c_void,
        ),
        method(
            "worldDropItem",
            "(Ljava/lang/String;DDDLjava/lang/String;)Ljava/lang/String;",
            world_drop_item as *mut c_void,
        ),
        method(
            "scoreboardTeamEntries",
            "(Ljava/lang/String;Ljava/lang/String;)[Ljava/lang/String;",
            scoreboard_team_entries as *mut c_void,
        ),
        method(
            "scoreboardTeamNames",
            "(Ljava/lang/String;)[Ljava/lang/String;",
            scoreboard_natives::team_names as *mut c_void,
        ),
        method(
            "scoreboardRegisterTeam",
            "(Ljava/lang/String;Ljava/lang/String;)Z",
            scoreboard_natives::register_team as *mut c_void,
        ),
        method(
            "scoreboardUnregisterTeam",
            "(Ljava/lang/String;Ljava/lang/String;)Z",
            scoreboard_natives::unregister_team as *mut c_void,
        ),
        method(
            "scoreboardAddTeamEntry",
            "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)Z",
            scoreboard_natives::add_team_entry as *mut c_void,
        ),
        method(
            "scoreboardRemoveTeamEntry",
            "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)Z",
            scoreboard_natives::remove_team_entry as *mut c_void,
        ),
        method(
            "scoreboardTeamProperty",
            "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
            scoreboard_natives::team_property as *mut c_void,
        ),
        method(
            "scoreboardSetTeamProperty",
            "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)Z",
            scoreboard_natives::set_team_property as *mut c_void,
        ),
        method(
            "scoreboardEntryTeam",
            "(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
            scoreboard_entry_team as *mut c_void,
        ),
        method(
            "worldSpawn",
            "(Ljava/lang/String;)[D",
            world_spawn as *mut c_void,
        ),
        method(
            "setWorldSpawn",
            "(Ljava/lang/String;III)Z",
            set_world_spawn as *mut c_void,
        ),
        method(
            "resetWorldBorder",
            "(Ljava/lang/String;)V",
            reset_world_border as *mut c_void,
        ),
        method(
            "worldBorder",
            "(Ljava/lang/String;)[D",
            world_border as *mut c_void,
        ),
        method(
            "setWorldBorderCenter",
            "(Ljava/lang/String;DD)V",
            set_world_border_center as *mut c_void,
        ),
        method(
            "worldBorderWarningDistance",
            "(Ljava/lang/String;)I",
            world_border_warning_distance as *mut c_void,
        ),
        method(
            "setWorldBorderWarningDistance",
            "(Ljava/lang/String;I)V",
            set_world_border_warning_distance as *mut c_void,
        ),
        method(
            "worldBorderWarningTime",
            "(Ljava/lang/String;)I",
            world_border_warning_time as *mut c_void,
        ),
        method(
            "setWorldBorderWarningTime",
            "(Ljava/lang/String;I)V",
            set_world_border_warning_time as *mut c_void,
        ),
        method(
            "worldBorderDamageAmount",
            "(Ljava/lang/String;)D",
            world_border_damage_amount as *mut c_void,
        ),
        method(
            "setWorldBorderDamageAmount",
            "(Ljava/lang/String;D)V",
            set_world_border_damage_amount as *mut c_void,
        ),
        method(
            "worldBorderDamageBuffer",
            "(Ljava/lang/String;)D",
            world_border_damage_buffer as *mut c_void,
        ),
        method(
            "setWorldBorderDamageBuffer",
            "(Ljava/lang/String;D)V",
            set_world_border_damage_buffer as *mut c_void,
        ),
        method(
            "setWorldBorderSize",
            "(Ljava/lang/String;D)V",
            set_world_border_size as *mut c_void,
        ),
        method(
            "setWorldBorderLerp",
            "(Ljava/lang/String;DDJ)V",
            set_world_border_lerp as *mut c_void,
        ),
        method(
            "worldTime",
            "(Ljava/lang/String;)J",
            world_time as *mut c_void,
        ),
        method(
            "setWorldTime",
            "(Ljava/lang/String;J)V",
            set_world_time as *mut c_void,
        ),
        method(
            "createExplosion",
            "(Ljava/lang/String;DDDF)Z",
            create_explosion as *mut c_void,
        ),
        method(
            "createExplosionAdvanced",
            "(Ljava/lang/String;DDDFZZ)Z",
            create_explosion_advanced as *mut c_void,
        ),
        method(
            "isSneaking",
            "(Ljava/lang/String;)Z",
            is_sneaking as *mut c_void,
        ),
        method(
            "openBook",
            "(Ljava/lang/String;)V",
            open_book as *mut c_void,
        ),
        method(
            "discoverRecipes",
            "(Ljava/lang/String;[Ljava/lang/String;)I",
            discover_recipes as *mut c_void,
        ),
        method(
            "undiscoverRecipes",
            "(Ljava/lang/String;[Ljava/lang/String;)I",
            undiscover_recipes as *mut c_void,
        ),
        method(
            "hasDiscoveredRecipe",
            "(Ljava/lang/String;Ljava/lang/String;)Z",
            has_discovered_recipe as *mut c_void,
        ),
        method(
            "discoveredRecipes",
            "(Ljava/lang/String;)[Ljava/lang/String;",
            discovered_recipes as *mut c_void,
        ),
        method(
            "teleport",
            "(Ljava/lang/String;Ljava/lang/String;DDDFF)Z",
            teleport as *mut c_void,
        ),
        method(
            "teleportEntity",
            "(Ljava/lang/String;Ljava/lang/String;DDDFF)Z",
            teleport_entity as *mut c_void,
        ),
        method(
            "worldMinHeight",
            "(Ljava/lang/String;)I",
            world_min_height as *mut c_void,
        ),
        method(
            "worldMaxHeight",
            "(Ljava/lang/String;)I",
            world_max_height as *mut c_void,
        ),
        method(
            "entityWorld",
            "(Ljava/lang/String;)Ljava/lang/String;",
            entity_world as *mut c_void,
        ),
        method(
            "removeEntity",
            "(Ljava/lang/String;)V",
            remove_entity as *mut c_void,
        ),
        method(
            "experienceOrbExperience",
            "(Ljava/lang/String;)I",
            experience_orb_experience as *mut c_void,
        ),
        method(
            "setExperienceOrbExperience",
            "(Ljava/lang/String;I)V",
            set_experience_orb_experience as *mut c_void,
        ),
        method(
            "wolfAngry",
            "(Ljava/lang/String;)Z",
            wolf_angry as *mut c_void,
        ),
        method(
            "setWolfAngry",
            "(Ljava/lang/String;Z)V",
            set_wolf_angry as *mut c_void,
        ),
        method(
            "entityTarget",
            "(Ljava/lang/String;)Ljava/lang/String;",
            entity_target as *mut c_void,
        ),
        method(
            "setEntityTarget",
            "(Ljava/lang/String;Ljava/lang/String;)V",
            set_entity_target as *mut c_void,
        ),
        method(
            "entityIsLiving",
            "(Ljava/lang/String;)Z",
            entity_is_living as *mut c_void,
        ),
        method(
            "entityIsFallFlying",
            "(Ljava/lang/String;)Z",
            entity_is_fall_flying as *mut c_void,
        ),
        method(
            "entityIsTamed",
            "(Ljava/lang/String;)Z",
            entity_is_tamed as *mut c_void,
        ),
        method(
            "setEntityTamed",
            "(Ljava/lang/String;Z)V",
            set_entity_tamed as *mut c_void,
        ),
        method(
            "entityOwner",
            "(Ljava/lang/String;)Ljava/lang/String;",
            entity_owner as *mut c_void,
        ),
        method(
            "setEntityOwner",
            "(Ljava/lang/String;Ljava/lang/String;)V",
            set_entity_owner as *mut c_void,
        ),
        method(
            "villagerType",
            "(Ljava/lang/String;)Ljava/lang/String;",
            villager_type as *mut c_void,
        ),
        method(
            "setVillagerType",
            "(Ljava/lang/String;Ljava/lang/String;)V",
            set_villager_type as *mut c_void,
        ),
        method(
            "villagerMemory",
            "(Ljava/lang/String;Ljava/lang/String;)[Ljava/lang/String;",
            villager_memory as *mut c_void,
        ),
        method(
            "setVillagerMemory",
            "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;III)Z",
            set_villager_memory as *mut c_void,
        ),
        method(
            "clearVillagerMemory",
            "(Ljava/lang/String;Ljava/lang/String;)V",
            clear_villager_memory as *mut c_void,
        ),
        method(
            "villagerProfession",
            "(Ljava/lang/String;)Ljava/lang/String;",
            villager_profession as *mut c_void,
        ),
        method(
            "villagerExperience",
            "(Ljava/lang/String;)I",
            villager_experience as *mut c_void,
        ),
        method(
            "setVillagerExperience",
            "(Ljava/lang/String;I)V",
            set_villager_experience as *mut c_void,
        ),
        method(
            "villagerLevel",
            "(Ljava/lang/String;)I",
            villager_level as *mut c_void,
        ),
        method(
            "setVillagerLevel",
            "(Ljava/lang/String;I)V",
            set_villager_level as *mut c_void,
        ),
        method(
            "resetVillagerOffers",
            "(Ljava/lang/String;)V",
            reset_villager_offers as *mut c_void,
        ),
        method(
            "zombieVillagerProfession",
            "(Ljava/lang/String;)Ljava/lang/String;",
            zombie_villager_profession as *mut c_void,
        ),
        method(
            "setZombieVillagerProfession",
            "(Ljava/lang/String;Ljava/lang/String;)V",
            set_zombie_villager_profession as *mut c_void,
        ),
        method(
            "setZombieVillager",
            "(Ljava/lang/String;Z)V",
            set_zombie_villager as *mut c_void,
        ),
        method(
            "foxType",
            "(Ljava/lang/String;)Ljava/lang/String;",
            fox_type as *mut c_void,
        ),
        method(
            "foxSitting",
            "(Ljava/lang/String;)Z",
            fox_sitting as *mut c_void,
        ),
        method(
            "setFoxSitting",
            "(Ljava/lang/String;Z)V",
            set_fox_sitting as *mut c_void,
        ),
        method(
            "setFoxType",
            "(Ljava/lang/String;Ljava/lang/String;)V",
            set_fox_type as *mut c_void,
        ),
        method(
            "tropicalFishPatternColor",
            "(Ljava/lang/String;)I",
            tropical_fish_pattern_color as *mut c_void,
        ),
        method(
            "setTropicalFishPatternColor",
            "(Ljava/lang/String;I)V",
            set_tropical_fish_pattern_color as *mut c_void,
        ),
        method(
            "setTropicalFishPattern",
            "(Ljava/lang/String;Ljava/lang/String;)V",
            set_tropical_fish_pattern as *mut c_void,
        ),
        method(
            "tropicalFishPattern",
            "(Ljava/lang/String;)Ljava/lang/String;",
            tropical_fish_pattern as *mut c_void,
        ),
        method(
            "setAxolotlVariant",
            "(Ljava/lang/String;Ljava/lang/String;)V",
            set_axolotl_variant as *mut c_void,
        ),
        method(
            "axolotlVariant",
            "(Ljava/lang/String;)Ljava/lang/String;",
            axolotl_variant as *mut c_void,
        ),
        method(
            "setParrotVariant",
            "(Ljava/lang/String;Ljava/lang/String;)V",
            set_parrot_variant as *mut c_void,
        ),
        method(
            "parrotVariant",
            "(Ljava/lang/String;)Ljava/lang/String;",
            parrot_variant as *mut c_void,
        ),
        method(
            "setMushroomCowVariant",
            "(Ljava/lang/String;Ljava/lang/String;)V",
            set_mushroom_cow_variant as *mut c_void,
        ),
        method(
            "mushroomCowVariant",
            "(Ljava/lang/String;)Ljava/lang/String;",
            mushroom_cow_variant as *mut c_void,
        ),
        method(
            "mushroomCowStewEffects",
            "(Ljava/lang/String;)[Ljava/lang/String;",
            mushroom_cow_stew_effects as *mut c_void,
        ),
        method(
            "setMushroomCowStewEffects",
            "(Ljava/lang/String;[Ljava/lang/String;)Z",
            set_mushroom_cow_stew_effects as *mut c_void,
        ),
        method(
            "mushroomCowReadyToShear",
            "(Ljava/lang/String;)Z",
            mushroom_cow_ready_to_shear as *mut c_void,
        ),
        method(
            "shearMushroomCow",
            "(Ljava/lang/String;Ljava/lang/String;)V",
            shear_mushroom_cow as *mut c_void,
        ),
        method(
            "setZombieNautilusVariant",
            "(Ljava/lang/String;Ljava/lang/String;)V",
            set_zombie_nautilus_variant as *mut c_void,
        ),
        method(
            "zombieNautilusVariant",
            "(Ljava/lang/String;)Ljava/lang/String;",
            zombie_nautilus_variant as *mut c_void,
        ),
        method(
            "setPigVariant",
            "(Ljava/lang/String;Ljava/lang/String;)V",
            set_pig_variant as *mut c_void,
        ),
        method(
            "pigVariant",
            "(Ljava/lang/String;)Ljava/lang/String;",
            pig_variant as *mut c_void,
        ),
        method(
            "setChickenVariant",
            "(Ljava/lang/String;Ljava/lang/String;)V",
            set_chicken_variant as *mut c_void,
        ),
        method(
            "chickenVariant",
            "(Ljava/lang/String;)Ljava/lang/String;",
            chicken_variant as *mut c_void,
        ),
        method(
            "setFrogVariant",
            "(Ljava/lang/String;Ljava/lang/String;)V",
            set_frog_variant as *mut c_void,
        ),
        method(
            "frogVariant",
            "(Ljava/lang/String;)Ljava/lang/String;",
            frog_variant as *mut c_void,
        ),
        method(
            "horseMarkings",
            "(Ljava/lang/String;)Ljava/lang/String;",
            horse_markings as *mut c_void,
        ),
        method(
            "setHorseMarkings",
            "(Ljava/lang/String;Ljava/lang/String;)V",
            set_horse_markings as *mut c_void,
        ),
        method(
            "horseVariant",
            "(Ljava/lang/String;)Ljava/lang/String;",
            horse_variant as *mut c_void,
        ),
        method(
            "setHorseVariant",
            "(Ljava/lang/String;Ljava/lang/String;)V",
            set_horse_variant as *mut c_void,
        ),
        method(
            "wolfVariant",
            "(Ljava/lang/String;)Ljava/lang/String;",
            wolf_variant as *mut c_void,
        ),
        method(
            "wolfSitting",
            "(Ljava/lang/String;)Z",
            wolf_sitting as *mut c_void,
        ),
        method(
            "setWolfSitting",
            "(Ljava/lang/String;Z)V",
            set_wolf_sitting as *mut c_void,
        ),
        method(
            "wolfCollarColor",
            "(Ljava/lang/String;)I",
            wolf_collar_color as *mut c_void,
        ),
        method(
            "setWolfCollarColor",
            "(Ljava/lang/String;I)V",
            set_wolf_collar_color as *mut c_void,
        ),
        method(
            "setWolfVariant",
            "(Ljava/lang/String;Ljava/lang/String;)V",
            set_wolf_variant as *mut c_void,
        ),
        method(
            "catVariant",
            "(Ljava/lang/String;)Ljava/lang/String;",
            cat_variant as *mut c_void,
        ),
        method(
            "setCatVariant",
            "(Ljava/lang/String;Ljava/lang/String;)V",
            set_cat_variant as *mut c_void,
        ),
        method(
            "catSitting",
            "(Ljava/lang/String;)Z",
            cat_sitting as *mut c_void,
        ),
        method(
            "catCollarColor",
            "(Ljava/lang/String;)I",
            cat_collar_color as *mut c_void,
        ),
        method(
            "setCatCollarColor",
            "(Ljava/lang/String;I)V",
            set_cat_collar_color as *mut c_void,
        ),
        method(
            "setCatSitting",
            "(Ljava/lang/String;Z)V",
            set_cat_sitting as *mut c_void,
        ),
        method(
            "endCrystalShowsBottom",
            "(Ljava/lang/String;)Z",
            end_crystal_shows_bottom as *mut c_void,
        ),
        method(
            "setEndCrystalShowsBottom",
            "(Ljava/lang/String;Z)V",
            set_end_crystal_shows_bottom as *mut c_void,
        ),
        method(
            "entityCanBreed",
            "(Ljava/lang/String;)Z",
            entity_can_breed as *mut c_void,
        ),
        method(
            "setEntityBreed",
            "(Ljava/lang/String;Z)V",
            set_entity_breed as *mut c_void,
        ),
        method(
            "animalBreedCause",
            "(Ljava/lang/String;)Ljava/lang/String;",
            animal_breed_cause as *mut c_void,
        ),
        method(
            "setAnimalBreedCause",
            "(Ljava/lang/String;Ljava/lang/String;)V",
            set_animal_breed_cause as *mut c_void,
        ),
        method(
            "animalLoveTicks",
            "(Ljava/lang/String;)I",
            animal_love_ticks as *mut c_void,
        ),
        method(
            "setAnimalLoveTicks",
            "(Ljava/lang/String;I)V",
            set_animal_love_ticks as *mut c_void,
        ),
        method(
            "animalIsBreedItem",
            "(Ljava/lang/String;Ljava/lang/String;)Z",
            animal_is_breed_item as *mut c_void,
        ),
        method(
            "cowVariant",
            "(Ljava/lang/String;)Ljava/lang/String;",
            cow_variant as *mut c_void,
        ),
        method(
            "setCowVariant",
            "(Ljava/lang/String;Ljava/lang/String;)V",
            set_cow_variant as *mut c_void,
        ),
        method(
            "cowSoundVariant",
            "(Ljava/lang/String;)Ljava/lang/String;",
            cow_sound_variant as *mut c_void,
        ),
        method(
            "setCowSoundVariant",
            "(Ljava/lang/String;Ljava/lang/String;)V",
            set_cow_sound_variant as *mut c_void,
        ),
        method(
            "beeAnger",
            "(Ljava/lang/String;)I",
            bee_anger as *mut c_void,
        ),
        method(
            "setBeeAnger",
            "(Ljava/lang/String;I)V",
            set_bee_anger as *mut c_void,
        ),
        method(
            "beeHasNectar",
            "(Ljava/lang/String;)Z",
            bee_has_nectar as *mut c_void,
        ),
        method(
            "setBeeHasNectar",
            "(Ljava/lang/String;Z)V",
            set_bee_has_nectar as *mut c_void,
        ),
        method(
            "armorStandSetArms",
            "(Ljava/lang/String;Z)V",
            armor_stand_set_arms as *mut c_void,
        ),
        method(
            "beeHasStung",
            "(Ljava/lang/String;)Z",
            bee_has_stung as *mut c_void,
        ),
        method(
            "setBeeHasStung",
            "(Ljava/lang/String;Z)V",
            set_bee_has_stung as *mut c_void,
        ),
        method(
            "horseTemper",
            "(Ljava/lang/String;)I",
            horse_temper as *mut c_void,
        ),
        method(
            "setHorseTemper",
            "(Ljava/lang/String;I)V",
            set_horse_temper as *mut c_void,
        ),
        method(
            "horseMaxTemper",
            "(Ljava/lang/String;)I",
            horse_max_temper as *mut c_void,
        ),
        method(
            "pandaMainGene",
            "(Ljava/lang/String;)Ljava/lang/String;",
            panda_main_gene as *mut c_void,
        ),
        method(
            "setPandaMainGene",
            "(Ljava/lang/String;Ljava/lang/String;)V",
            set_panda_main_gene as *mut c_void,
        ),
        method(
            "pandaHiddenGene",
            "(Ljava/lang/String;)Ljava/lang/String;",
            panda_hidden_gene as *mut c_void,
        ),
        method(
            "setPandaHiddenGene",
            "(Ljava/lang/String;Ljava/lang/String;)V",
            set_panda_hidden_gene as *mut c_void,
        ),
        method(
            "raiderPatrolLeader",
            "(Ljava/lang/String;)Z",
            raider_patrol_leader as *mut c_void,
        ),
        method(
            "setRaiderPatrolLeader",
            "(Ljava/lang/String;Z)V",
            set_raider_patrol_leader as *mut c_void,
        ),
        method(
            "phantomSize",
            "(Ljava/lang/String;)I",
            phantom_size as *mut c_void,
        ),
        method(
            "setPhantomSize",
            "(Ljava/lang/String;I)V",
            set_phantom_size as *mut c_void,
        ),
        method(
            "llamaVariant",
            "(Ljava/lang/String;)Ljava/lang/String;",
            llama_variant as *mut c_void,
        ),
        method(
            "setLlamaVariant",
            "(Ljava/lang/String;Ljava/lang/String;)V",
            set_llama_variant as *mut c_void,
        ),
        method(
            "generateTree",
            "(Ljava/lang/String;IIILjava/lang/String;)Z",
            generate_tree as *mut c_void,
        ),
        method(
            "tropicalFishBodyColor",
            "(Ljava/lang/String;)I",
            tropical_fish_body_color as *mut c_void,
        ),
        method(
            "setTropicalFishBodyColor",
            "(Ljava/lang/String;I)V",
            set_tropical_fish_body_color as *mut c_void,
        ),
        method(
            "slimeSize",
            "(Ljava/lang/String;)I",
            slime_size as *mut c_void,
        ),
        method(
            "setSlimeSize",
            "(Ljava/lang/String;I)V",
            set_slime_size as *mut c_void,
        ),
        method(
            "cubeMobCanWander",
            "(Ljava/lang/String;)Z",
            cube_mob_can_wander as *mut c_void,
        ),
        method(
            "setCubeMobWander",
            "(Ljava/lang/String;Z)V",
            set_cube_mob_wander as *mut c_void,
        ),
        method(
            "setCreeperPowered",
            "(Ljava/lang/String;Z)V",
            set_creeper_powered as *mut c_void,
        ),
        method(
            "creeperPowered",
            "(Ljava/lang/String;)Z",
            creeper_powered as *mut c_void,
        ),
        method(
            "setGoatScreaming",
            "(Ljava/lang/String;Z)V",
            set_goat_screaming as *mut c_void,
        ),
        method(
            "goatScreaming",
            "(Ljava/lang/String;)Z",
            goat_screaming as *mut c_void,
        ),
        method(
            "sheepColor",
            "(Ljava/lang/String;)I",
            sheep_color as *mut c_void,
        ),
        method(
            "setSheepColor",
            "(Ljava/lang/String;I)V",
            set_sheep_color as *mut c_void,
        ),
        method(
            "sheepSheared",
            "(Ljava/lang/String;)Z",
            sheep_sheared as *mut c_void,
        ),
        method(
            "setSheepSheared",
            "(Ljava/lang/String;Z)V",
            set_sheep_sheared as *mut c_void,
        ),
        method(
            "goatLeftHorn",
            "(Ljava/lang/String;)Z",
            goat_left_horn as *mut c_void,
        ),
        method(
            "setGoatLeftHorn",
            "(Ljava/lang/String;Z)V",
            set_goat_left_horn as *mut c_void,
        ),
        method(
            "goatRightHorn",
            "(Ljava/lang/String;)Z",
            goat_right_horn as *mut c_void,
        ),
        method(
            "setGoatRightHorn",
            "(Ljava/lang/String;Z)V",
            set_goat_right_horn as *mut c_void,
        ),
        method(
            "entityIsBaby",
            "(Ljava/lang/String;)Z",
            entity_is_baby as *mut c_void,
        ),
        method(
            "entityAge",
            "(Ljava/lang/String;)I",
            entity_age as *mut c_void,
        ),
        method(
            "setEntityAge",
            "(Ljava/lang/String;I)V",
            set_entity_age as *mut c_void,
        ),
        method(
            "entityCanPickupItems",
            "(Ljava/lang/String;)Z",
            entity_can_pickup_items as *mut c_void,
        ),
        method(
            "setEntityCanPickupItems",
            "(Ljava/lang/String;Z)V",
            set_entity_can_pickup_items as *mut c_void,
        ),
        method(
            "enchantmentMaxLevel",
            "(Ljava/lang/String;)I",
            enchantment_max_level as *mut c_void,
        ),
        method(
            "entityHasChest",
            "(Ljava/lang/String;)Z",
            entity_has_chest as *mut c_void,
        ),
        method(
            "entitySetChest",
            "(Ljava/lang/String;Z)V",
            entity_set_chest as *mut c_void,
        ),
        method(
            "entitySetBaby",
            "(Ljava/lang/String;Z)V",
            entity_set_baby as *mut c_void,
        ),
        method(
            "entityAgeLock",
            "(Ljava/lang/String;)Z",
            entity_age_lock as *mut c_void,
        ),
        method(
            "setEntityAgeLock",
            "(Ljava/lang/String;Z)V",
            set_entity_age_lock as *mut c_void,
        ),
        method(
            "pigHasSaddle",
            "(Ljava/lang/String;)Z",
            pig_has_saddle as *mut c_void,
        ),
        method(
            "pigSetSaddle",
            "(Ljava/lang/String;Z)V",
            pig_set_saddle as *mut c_void,
        ),
        method(
            "mountInventorySlot",
            "(Ljava/lang/String;I)Ljava/lang/String;",
            mount_inventory_slot as *mut c_void,
        ),
        method(
            "setMountInventorySlot",
            "(Ljava/lang/String;ILjava/lang/String;)V",
            set_mount_inventory_slot as *mut c_void,
        ),
        method(
            "horseInventorySlot",
            "(Ljava/lang/String;I)Ljava/lang/String;",
            horse_inventory_slot as *mut c_void,
        ),
        method(
            "setHorseInventorySlot",
            "(Ljava/lang/String;ILjava/lang/String;)V",
            set_horse_inventory_slot as *mut c_void,
        ),
        method(
            "setBlockDisplayBlock",
            "(Ljava/lang/String;Ljava/lang/String;)V",
            set_block_display_block as *mut c_void,
        ),
        method(
            "boatType",
            "(Ljava/lang/String;)Ljava/lang/String;",
            boat_type as *mut c_void,
        ),
        method(
            "setBoatType",
            "(Ljava/lang/String;Ljava/lang/String;)V",
            set_boat_type as *mut c_void,
        ),
        method(
            "areaEffectCloudRadius",
            "(Ljava/lang/String;)F",
            area_effect_cloud_radius as *mut c_void,
        ),
        method(
            "areaEffectCloudSource",
            "(Ljava/lang/String;)Ljava/lang/String;",
            area_effect_cloud_source as *mut c_void,
        ),
        method(
            "areaEffectCloudEffects",
            "(Ljava/lang/String;)[Ljava/lang/String;",
            area_effect_cloud_effects as *mut c_void,
        ),
        method(
            "addAreaEffectCloudEffect",
            "(Ljava/lang/String;Ljava/lang/String;IIZZZZ)Z",
            add_area_effect_cloud_effect as *mut c_void,
        ),
        method(
            "clearAreaEffectCloudEffects",
            "(Ljava/lang/String;)V",
            clear_area_effect_cloud_effects as *mut c_void,
        ),
        method(
            "setAreaEffectCloudRadius",
            "(Ljava/lang/String;F)V",
            set_area_effect_cloud_radius as *mut c_void,
        ),
        method(
            "areaEffectCloudDuration",
            "(Ljava/lang/String;)I",
            area_effect_cloud_duration as *mut c_void,
        ),
        method(
            "setAreaEffectCloudDuration",
            "(Ljava/lang/String;I)V",
            set_area_effect_cloud_duration as *mut c_void,
        ),
        method(
            "areaEffectCloudWaitTime",
            "(Ljava/lang/String;)I",
            area_effect_cloud_wait_time as *mut c_void,
        ),
        method(
            "setAreaEffectCloudWaitTime",
            "(Ljava/lang/String;I)V",
            set_area_effect_cloud_wait_time as *mut c_void,
        ),
        method(
            "areaEffectCloudReapplicationDelay",
            "(Ljava/lang/String;)I",
            area_effect_cloud_reapplication_delay as *mut c_void,
        ),
        method(
            "setAreaEffectCloudReapplicationDelay",
            "(Ljava/lang/String;I)V",
            set_area_effect_cloud_reapplication_delay as *mut c_void,
        ),
        method(
            "areaEffectCloudRadiusPerTick",
            "(Ljava/lang/String;)F",
            area_effect_cloud_radius_per_tick as *mut c_void,
        ),
        method(
            "setAreaEffectCloudRadiusPerTick",
            "(Ljava/lang/String;F)V",
            set_area_effect_cloud_radius_per_tick as *mut c_void,
        ),
        method(
            "areaEffectCloudRadiusOnUse",
            "(Ljava/lang/String;)F",
            area_effect_cloud_radius_on_use as *mut c_void,
        ),
        method(
            "setAreaEffectCloudRadiusOnUse",
            "(Ljava/lang/String;F)V",
            set_area_effect_cloud_radius_on_use as *mut c_void,
        ),
        method(
            "fireworkMeta",
            "(Ljava/lang/String;)Ljava/lang/String;",
            firework_meta as *mut c_void,
        ),
        method(
            "setFireworkMeta",
            "(Ljava/lang/String;ILjava/lang/String;)V",
            set_firework_meta as *mut c_void,
        ),
        method(
            "entityType",
            "(Ljava/lang/String;)Ljava/lang/String;",
            entity_type as *mut c_void,
        ),
        method(
            "hangingFacing",
            "(Ljava/lang/String;)Ljava/lang/String;",
            hanging_facing as *mut c_void,
        ),
        method(
            "paintingArt",
            "(Ljava/lang/String;)Ljava/lang/String;",
            painting_art as *mut c_void,
        ),
        method(
            "setPaintingArt",
            "(Ljava/lang/String;Ljava/lang/String;Z)Z",
            set_painting_art as *mut c_void,
        ),
        method(
            "endermanCarriedBlock",
            "(Ljava/lang/String;)Ljava/lang/String;",
            enderman_carried_block as *mut c_void,
        ),
        method(
            "setEndermanCarriedBlock",
            "(Ljava/lang/String;Ljava/lang/String;)V",
            set_enderman_carried_block as *mut c_void,
        ),
        method(
            "entityTntSource",
            "(Ljava/lang/String;)Ljava/lang/String;",
            entity_tnt_source as *mut c_void,
        ),
        method(
            "entityItemStack",
            "(Ljava/lang/String;)Ljava/lang/String;",
            entity_item_stack as *mut c_void,
        ),
        method(
            "setEntityItemStack",
            "(Ljava/lang/String;Ljava/lang/String;)V",
            set_entity_item_stack as *mut c_void,
        ),
        method(
            "setItemUnlimitedLifetime",
            "(Ljava/lang/String;Z)V",
            set_item_unlimited_lifetime as *mut c_void,
        ),
        method("itemAge", "(Ljava/lang/String;)I", item_age as *mut c_void),
        method(
            "setItemAge",
            "(Ljava/lang/String;I)V",
            set_item_age as *mut c_void,
        ),
        method(
            "entityEject",
            "(Ljava/lang/String;)Z",
            entity_eject as *mut c_void,
        ),
        method(
            "entityVehicle",
            "(Ljava/lang/String;)Ljava/lang/String;",
            entity_vehicle as *mut c_void,
        ),
        method(
            "entityLeaveVehicle",
            "(Ljava/lang/String;)Z",
            entity_leave_vehicle as *mut c_void,
        ),
        method(
            "entityPassengers",
            "(Ljava/lang/String;)Ljava/lang/String;",
            entity_passengers as *mut c_void,
        ),
        method(
            "entityAddPassenger",
            "(Ljava/lang/String;Ljava/lang/String;)Z",
            entity_add_passenger as *mut c_void,
        ),
        method(
            "entityRemovePassenger",
            "(Ljava/lang/String;Ljava/lang/String;)Z",
            entity_remove_passenger as *mut c_void,
        ),
        method(
            "entitySpawnCategory",
            "(Ljava/lang/String;)Ljava/lang/String;",
            entity_spawn_category as *mut c_void,
        ),
        method(
            "entitySpawnReason",
            "(Ljava/lang/String;)Ljava/lang/String;",
            entity_spawn_reason as *mut c_void,
        ),
        method(
            "entityPosition",
            "(Ljava/lang/String;)[D",
            entity_position as *mut c_void,
        ),
        method(
            "entityOrigin",
            "(Ljava/lang/String;)[D",
            entity_origin as *mut c_void,
        ),
        method(
            "entityBoundingBox",
            "(Ljava/lang/String;)[D",
            entity_bounding_box as *mut c_void,
        ),
        method(
            "entityScoreboardTags",
            "(Ljava/lang/String;)[Ljava/lang/String;",
            entity_scoreboard_tags as *mut c_void,
        ),
        method(
            "addEntityScoreboardTag",
            "(Ljava/lang/String;Ljava/lang/String;)Z",
            add_entity_scoreboard_tag as *mut c_void,
        ),
        method(
            "removeEntityScoreboardTag",
            "(Ljava/lang/String;Ljava/lang/String;)Z",
            remove_entity_scoreboard_tag as *mut c_void,
        ),
        method(
            "entityHasGravity",
            "(Ljava/lang/String;)Z",
            entity_has_gravity as *mut c_void,
        ),
        method(
            "setEntityGravity",
            "(Ljava/lang/String;Z)V",
            set_entity_gravity as *mut c_void,
        ),
        method(
            "entitySilent",
            "(Ljava/lang/String;)Z",
            entity_silent as *mut c_void,
        ),
        method(
            "setEntitySilent",
            "(Ljava/lang/String;Z)V",
            set_entity_silent as *mut c_void,
        ),
        method(
            "setEntityRotation",
            "(Ljava/lang/String;FF)V",
            set_entity_rotation as *mut c_void,
        ),
        method(
            "entityInRain",
            "(Ljava/lang/String;)Z",
            entity_in_rain as *mut c_void,
        ),
        method(
            "entityInvulnerable",
            "(Ljava/lang/String;)Z",
            entity_invulnerable as *mut c_void,
        ),
        method(
            "setEntityInvulnerable",
            "(Ljava/lang/String;Z)V",
            set_entity_invulnerable as *mut c_void,
        ),
        method(
            "entityOnGround",
            "(Ljava/lang/String;)Z",
            entity_on_ground as *mut c_void,
        ),
        method(
            "entityInWater",
            "(Ljava/lang/String;)Z",
            entity_in_water as *mut c_void,
        ),
        method(
            "entityInvisible",
            "(Ljava/lang/String;)Z",
            entity_invisible as *mut c_void,
        ),
        method(
            "entityPortalCooldown",
            "(Ljava/lang/String;)I",
            entity_portal_cooldown as *mut c_void,
        ),
        method(
            "setEntityPortalCooldown",
            "(Ljava/lang/String;I)V",
            set_entity_portal_cooldown as *mut c_void,
        ),
        method(
            "entityGlowing",
            "(Ljava/lang/String;)Z",
            entity_glowing as *mut c_void,
        ),
        method(
            "setEntityGlowing",
            "(Ljava/lang/String;Z)V",
            set_entity_glowing as *mut c_void,
        ),
        method(
            "entityFreezeTicks",
            "(Ljava/lang/String;)I",
            entity_freeze_ticks as *mut c_void,
        ),
        method(
            "setEntityFreezeTicks",
            "(Ljava/lang/String;I)V",
            set_entity_freeze_ticks as *mut c_void,
        ),
        method(
            "entityNoDamageTicks",
            "(Ljava/lang/String;)I",
            entity_no_damage_ticks as *mut c_void,
        ),
        method(
            "entitySetNoDamageTicks",
            "(Ljava/lang/String;I)V",
            entity_set_no_damage_ticks as *mut c_void,
        ),
        method(
            "entityFallDistance",
            "(Ljava/lang/String;)F",
            entity_fall_distance as *mut c_void,
        ),
        method(
            "setEntityFallDistance",
            "(Ljava/lang/String;F)V",
            set_entity_fall_distance as *mut c_void,
        ),
        method(
            "setCompassTarget",
            "(Ljava/lang/String;Ljava/lang/String;III)V",
            set_compass_target as *mut c_void,
        ),
        method(
            "entitySprinting",
            "(Ljava/lang/String;)Z",
            entity_sprinting as *mut c_void,
        ),
        method(
            "entitySwimming",
            "(Ljava/lang/String;)Z",
            entity_swimming as *mut c_void,
        ),
        method(
            "entityIsUsingItem",
            "(Ljava/lang/String;)Z",
            entity_is_using_item as *mut c_void,
        ),
        method(
            "entityClearActiveItem",
            "(Ljava/lang/String;)V",
            entity_clear_active_item as *mut c_void,
        ),
        method(
            "entityNearby",
            "(Ljava/lang/String;DDD)[Ljava/lang/String;",
            entity_nearby as *mut c_void,
        ),
        method(
            "entityTrackedBy",
            "(Ljava/lang/String;)[Ljava/lang/String;",
            entity_tracked_by as *mut c_void,
        ),
        method(
            "worldNearby",
            "(Ljava/lang/String;DDDDDD)[Ljava/lang/String;",
            world_nearby as *mut c_void,
        ),
        method(
            "playerHideEntity",
            "(Ljava/lang/String;Ljava/lang/String;Z)V",
            player_hide_entity as *mut c_void,
        ),
        method(
            "playerCanSeeEntity",
            "(Ljava/lang/String;Ljava/lang/String;)Z",
            player_can_see_entity as *mut c_void,
        ),
        method(
            "entityEyeHeight",
            "(Ljava/lang/String;)D",
            entity_eye_height as *mut c_void,
        ),
        method(
            "entityVelocity",
            "(Ljava/lang/String;)[D",
            entity_velocity as *mut c_void,
        ),
        method(
            "setEntityVelocity",
            "(Ljava/lang/String;DDD)V",
            set_entity_velocity as *mut c_void,
        ),
        method(
            "entityFireTicks",
            "(Ljava/lang/String;)I",
            entity_fire_ticks as *mut c_void,
        ),
        method(
            "setEntityFireTicks",
            "(Ljava/lang/String;I)V",
            set_entity_fire_ticks as *mut c_void,
        ),
        method(
            "entityId",
            "(Ljava/lang/String;)I",
            entity_id as *mut c_void,
        ),
        method(
            "entityProjectileOwner",
            "(Ljava/lang/String;)Ljava/lang/String;",
            entity_projectile_owner as *mut c_void,
        ),
        method(
            "entityProjectileSource",
            "(Ljava/lang/String;)Lorg/bukkit/projectiles/ProjectileSource;",
            entity_projectile_source as *mut c_void,
        ),
        method(
            "entityProjectileShooter",
            "(Ljava/lang/String;)Ljava/lang/Object;",
            entity_projectile_shooter as *mut c_void,
        ),
        method(
            "setEntityProjectileSource",
            "(Ljava/lang/String;Ljava/lang/String;Lorg/bukkit/projectiles/ProjectileSource;Z)Z",
            set_entity_projectile_source as *mut c_void,
        ),
        method(
            "entityPotionEffects",
            "(Ljava/lang/String;)[Ljava/lang/String;",
            entity_potion_effects as *mut c_void,
        ),
        method(
            "mobEffectInstant",
            "(Ljava/lang/String;)Z",
            mob_effect_instant as *mut c_void,
        ),
        method(
            "entityPersistent",
            "(Ljava/lang/String;)Z",
            entity_persistent as *mut c_void,
        ),
        method(
            "setEntityPersistent",
            "(Ljava/lang/String;Z)V",
            set_entity_persistent as *mut c_void,
        ),
        method(
            "entityRemoveWhenFarAway",
            "(Ljava/lang/String;)Z",
            entity_remove_when_far_away as *mut c_void,
        ),
        method(
            "setEntityRemoveWhenFarAway",
            "(Ljava/lang/String;Z)V",
            set_entity_remove_when_far_away as *mut c_void,
        ),
        method(
            "entityDropChance",
            "(Ljava/lang/String;I)F",
            entity_drop_chance as *mut c_void,
        ),
        method(
            "setEntityDropChance",
            "(Ljava/lang/String;IF)V",
            set_entity_drop_chance as *mut c_void,
        ),
        method(
            "entityEquipmentSlot",
            "(Ljava/lang/String;I)Ljava/lang/String;",
            entity_equipment_slot as *mut c_void,
        ),
        method(
            "setEntityEquipmentSlot",
            "(Ljava/lang/String;ILjava/lang/String;)V",
            set_entity_equipment_slot as *mut c_void,
        ),
        method(
            "clearEntityEquipment",
            "(Ljava/lang/String;)V",
            clear_entity_equipment as *mut c_void,
        ),
        method(
            "arrowPotion",
            "(Ljava/lang/String;)Ljava/lang/String;",
            arrow_potion as *mut c_void,
        ),
        method(
            "setArrowPotion",
            "(Ljava/lang/String;Ljava/lang/String;)V",
            set_arrow_potion as *mut c_void,
        ),
        method(
            "arrowPotionColor",
            "(Ljava/lang/String;)I",
            arrow_potion_color as *mut c_void,
        ),
        method(
            "arrowCustomEffects",
            "(Ljava/lang/String;)[Ljava/lang/String;",
            arrow_custom_effects as *mut c_void,
        ),
        method(
            "arrowProperty",
            "(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
            arrow_property as *mut c_void,
        ),
        method(
            "setArrowProperty",
            "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)Z",
            set_arrow_property as *mut c_void,
        ),
        method(
            "setArrowPotionColor",
            "(Ljava/lang/String;IZ)V",
            set_arrow_potion_color as *mut c_void,
        ),
        method(
            "addArrowCustomEffect",
            "(Ljava/lang/String;Ljava/lang/String;Z)Z",
            add_arrow_custom_effect as *mut c_void,
        ),
        method(
            "removeArrowCustomEffect",
            "(Ljava/lang/String;Ljava/lang/String;)Z",
            remove_arrow_custom_effect as *mut c_void,
        ),
        method(
            "clearArrowCustomEffects",
            "(Ljava/lang/String;)V",
            clear_arrow_custom_effects as *mut c_void,
        ),
        method(
            "airSupply",
            "(Ljava/lang/String;)I",
            air_supply as *mut c_void,
        ),
        method(
            "setAirSupply",
            "(Ljava/lang/String;I)V",
            set_air_supply as *mut c_void,
        ),
        method(
            "maxAirSupply",
            "(Ljava/lang/String;)I",
            max_air_supply as *mut c_void,
        ),
        method(
            "entityCustomName",
            "(Ljava/lang/String;)Ljava/lang/String;",
            entity_custom_name as *mut c_void,
        ),
        method(
            "entityCustomNameVisible",
            "(Ljava/lang/String;)Z",
            entity_custom_name_visible as *mut c_void,
        ),
        method(
            "setEntityCustomNameVisible",
            "(Ljava/lang/String;Z)V",
            set_entity_custom_name_visible as *mut c_void,
        ),
        method(
            "ironGolemPlayerCreated",
            "(Ljava/lang/String;)Z",
            iron_golem_player_created as *mut c_void,
        ),
        method(
            "setIronGolemPlayerCreated",
            "(Ljava/lang/String;Z)V",
            set_iron_golem_player_created as *mut c_void,
        ),
        method(
            "entityMerchantRecipes",
            "(Ljava/lang/String;)[Ljava/lang/String;",
            entity_merchant_recipes as *mut c_void,
        ),
        method(
            "setVillagerOffers",
            "(Ljava/lang/String;[Ljava/lang/String;)V",
            set_villager_offers as *mut c_void,
        ),
        method(
            "entitySetMerchantOfferUses",
            "(Ljava/lang/String;II)Z",
            entity_set_merchant_offer_uses as *mut c_void,
        ),
        method(
            "entitySetMerchantOfferMaxUses",
            "(Ljava/lang/String;II)Z",
            entity_set_merchant_offer_max_uses as *mut c_void,
        ),
        method(
            "entitySetMerchantOfferDemand",
            "(Ljava/lang/String;II)Z",
            entity_set_merchant_offer_demand as *mut c_void,
        ),
        method(
            "setEntityCustomName",
            "(Ljava/lang/String;Ljava/lang/String;)V",
            set_entity_custom_name as *mut c_void,
        ),
        method(
            "entitySendMessage",
            "(Ljava/lang/String;Ljava/lang/String;)V",
            entity_send_message as *mut c_void,
        ),
        method(
            "openMenuSlotCount",
            "(Ljava/lang/String;)I",
            open_menu_slot_count as *mut c_void,
        ),
        method(
            "openMenuSlot",
            "(Ljava/lang/String;I)Ljava/lang/String;",
            open_menu_slot as *mut c_void,
        ),
        method(
            "openMenuTopSlotCount",
            "(Ljava/lang/String;)I",
            open_menu_top_slot_count as *mut c_void,
        ),
        method(
            "setOpenMenuSlot",
            "(Ljava/lang/String;ILjava/lang/String;)Z",
            set_open_menu_slot as *mut c_void,
        ),
        method(
            "openMenuType",
            "(Ljava/lang/String;)Ljava/lang/String;",
            open_menu_type as *mut c_void,
        ),
        method(
            "openMenuTitle",
            "(Ljava/lang/String;)Ljava/lang/String;",
            open_menu_title as *mut c_void,
        ),
        method(
            "updateInventory",
            "(Ljava/lang/String;)V",
            update_inventory as *mut c_void,
        ),
        method(
            "closeInventory",
            "(Ljava/lang/String;)V",
            close_inventory as *mut c_void,
        ),
        method(
            "gameMode",
            "(Ljava/lang/String;)Ljava/lang/String;",
            game_mode as *mut c_void,
        ),
        method(
            "setGameMode",
            "(Ljava/lang/String;Ljava/lang/String;)Z",
            set_game_mode as *mut c_void,
        ),
        method(
            "allowFlight",
            "(Ljava/lang/String;)Z",
            allow_flight as *mut c_void,
        ),
        method(
            "isFlying",
            "(Ljava/lang/String;)Z",
            is_flying as *mut c_void,
        ),
        method(
            "setFlying",
            "(Ljava/lang/String;Z)V",
            set_flying as *mut c_void,
        ),
        method(
            "isSleepingIgnored",
            "(Ljava/lang/String;)Z",
            is_sleeping_ignored as *mut c_void,
        ),
        method(
            "setSleepingIgnored",
            "(Ljava/lang/String;Z)V",
            set_sleeping_ignored as *mut c_void,
        ),
        method(
            "openGenericInventory",
            "(Ljava/lang/String;ILjava/lang/String;Ljava/lang/String;)V",
            open_generic_inventory as *mut c_void,
        ),
        method(
            "openSmithingTable",
            "(Ljava/lang/String;Ljava/lang/String;III)Z",
            open_smithing_table as *mut c_void,
        ),
        method(
            "openLoom",
            "(Ljava/lang/String;Ljava/lang/String;III)Z",
            open_loom as *mut c_void,
        ),
        method(
            "openCartographyTable",
            "(Ljava/lang/String;Ljava/lang/String;III)Z",
            open_cartography_table as *mut c_void,
        ),
        method(
            "openAnvil",
            "(Ljava/lang/String;Ljava/lang/String;III)Z",
            open_anvil as *mut c_void,
        ),
        method(
            "openStonecutter",
            "(Ljava/lang/String;Ljava/lang/String;III)Z",
            open_stonecutter as *mut c_void,
        ),
        method(
            "openGrindstone",
            "(Ljava/lang/String;Ljava/lang/String;III)Z",
            open_grindstone as *mut c_void,
        ),
        method(
            "openWorkbench",
            "(Ljava/lang/String;Ljava/lang/String;III)Z",
            open_workbench as *mut c_void,
        ),
        method(
            "setAllowFlight",
            "(Ljava/lang/String;Z)V",
            set_allow_flight as *mut c_void,
        ),
        method(
            "inventorySlot",
            "(Ljava/lang/String;I)Ljava/lang/String;",
            inventory_slot as *mut c_void,
        ),
        method(
            "setInventorySlot",
            "(Ljava/lang/String;ILjava/lang/String;)V",
            set_inventory_slot as *mut c_void,
        ),
        method(
            "enderChestSlot",
            "(Ljava/lang/String;I)Ljava/lang/String;",
            ender_chest_slot as *mut c_void,
        ),
        method(
            "setEnderChestSlot",
            "(Ljava/lang/String;ILjava/lang/String;)V",
            set_ender_chest_slot as *mut c_void,
        ),
        method(
            "spellcasterSpell",
            "(Ljava/lang/String;)Ljava/lang/String;",
            spellcaster_spell as *mut c_void,
        ),
        method(
            "setSpellcasterSpell",
            "(Ljava/lang/String;Ljava/lang/String;)V",
            set_spellcaster_spell as *mut c_void,
        ),
        method(
            "setHangingFacing",
            "(Ljava/lang/String;Ljava/lang/String;Z)Z",
            set_hanging_facing as *mut c_void,
        ),
        method(
            "areaEffectCloudBasePotionType",
            "(Ljava/lang/String;)Ljava/lang/String;",
            area_effect_cloud_base_potion_type as *mut c_void,
        ),
        method(
            "heldSlot",
            "(Ljava/lang/String;)I",
            held_slot as *mut c_void,
        ),
        method(
            "statisticValue",
            "(Ljava/lang/String;Ljava/lang/String;)I",
            statistic_value as *mut c_void,
        ),
        method(
            "isOperator",
            "(Ljava/lang/String;)Z",
            is_operator as *mut c_void,
        ),
        method(
            "offlineStatistic",
            "(Ljava/lang/String;Ljava/lang/String;)I",
            offline_statistic as *mut c_void,
        ),
        method(
            "offlineIsOperator",
            "(Ljava/lang/String;)Z",
            offline_is_operator as *mut c_void,
        ),
        method(
            "offlineIsWhitelisted",
            "(Ljava/lang/String;)Z",
            offline_is_whitelisted as *mut c_void,
        ),
        method(
            "isWhitelisted",
            "(Ljava/lang/String;)Z",
            is_whitelisted as *mut c_void,
        ),
        method(
            "setPlayerWhitelisted",
            "(Ljava/lang/String;Z)V",
            set_player_whitelisted as *mut c_void,
        ),
        method(
            "isPermissionSet",
            "(Ljava/lang/String;Ljava/lang/String;)Z",
            is_permission_set as *mut c_void,
        ),
        method(
            "createBossBar",
            "(Ljava/lang/String;III)Ljava/lang/String;",
            create_boss_bar as *mut c_void,
        ),
        method(
            "releaseBossBar",
            "(Ljava/lang/String;)V",
            release_boss_bar as *mut c_void,
        ),
        method(
            "bossBarSetTitle",
            "(Ljava/lang/String;Ljava/lang/String;)V",
            boss_bar_set_title as *mut c_void,
        ),
        method(
            "bossBarSetColor",
            "(Ljava/lang/String;I)V",
            boss_bar_set_color as *mut c_void,
        ),
        method(
            "bossBarSetStyle",
            "(Ljava/lang/String;I)V",
            boss_bar_set_style as *mut c_void,
        ),
        method(
            "bossBarSetFlags",
            "(Ljava/lang/String;I)V",
            boss_bar_set_flags as *mut c_void,
        ),
        method(
            "bossBarSetProgress",
            "(Ljava/lang/String;D)V",
            boss_bar_set_progress as *mut c_void,
        ),
        method(
            "bossBarAddPlayer",
            "(Ljava/lang/String;Ljava/lang/String;)V",
            boss_bar_add_player as *mut c_void,
        ),
        method(
            "bossBarRemovePlayer",
            "(Ljava/lang/String;Ljava/lang/String;)V",
            boss_bar_remove_player as *mut c_void,
        ),
        method(
            "bossBarRemoveAll",
            "(Ljava/lang/String;)V",
            boss_bar_remove_all as *mut c_void,
        ),
        method(
            "bossBarPlayerIds",
            "(Ljava/lang/String;)[Ljava/lang/String;",
            boss_bar_player_ids as *mut c_void,
        ),
        method(
            "bossBarSetVisible",
            "(Ljava/lang/String;Z)V",
            boss_bar_set_visible as *mut c_void,
        ),
        method(
            "lecternBook",
            "(Ljava/lang/String;III)Ljava/lang/String;",
            lectern_book as *mut c_void,
        ),
        method(
            "lecternBookPages",
            "(Ljava/lang/String;III)[Ljava/lang/String;",
            lectern_book_pages as *mut c_void,
        ),
        method(
            "lecternClearBook",
            "(Ljava/lang/String;III)V",
            lectern_clear_book as *mut c_void,
        ),
        method(
            "lecternSetBook",
            "(Ljava/lang/String;IIILjava/lang/String;)Z",
            lectern_set_book as *mut c_void,
        ),
        method(
            "biomeKey",
            "(Ljava/lang/String;III)Ljava/lang/String;",
            biome_key as *mut c_void,
        ),
        method(
            "blockPistonReaction",
            "(Ljava/lang/String;III)Ljava/lang/String;",
            block_piston_reaction as *mut c_void,
        ),
        method(
            "blockState",
            "(Ljava/lang/String;III)Ljava/lang/String;",
            block_state as *mut c_void,
        ),
        method(
            "recipeResult",
            "(Ljava/lang/String;)Ljava/lang/String;",
            recipe_result as *mut c_void,
        ),
        method(
            "recipeList",
            "()[Ljava/lang/String;",
            recipe_list as *mut c_void,
        ),
        method(
            "itemTranslationKey",
            "(Ljava/lang/String;)Ljava/lang/String;",
            item_translation_key as *mut c_void,
        ),
        method(
            "recipeRemove",
            "(Ljava/lang/String;)Z",
            recipe_remove as *mut c_void,
        ),
        method(
            "recipeAddShapeless",
            "(Ljava/lang/String;Ljava/lang/String;I[Ljava/lang/String;)Z",
            recipe_add_shapeless as *mut c_void,
        ),
        method(
            "recipeAddShaped",
            "(Ljava/lang/String;Ljava/lang/String;I[Ljava/lang/String;[Ljava/lang/String;)Z",
            recipe_add_shaped as *mut c_void,
        ),
        method(
            "blockLight",
            "(Ljava/lang/String;III)B",
            block_light as *mut c_void,
        ),
        method(
            "blockIndirectlyPowered",
            "(Ljava/lang/String;III)Z",
            block_indirectly_powered as *mut c_void,
        ),
        method(
            "skyLight",
            "(Ljava/lang/String;III)B",
            sky_light as *mut c_void,
        ),
        method(
            "blockPassable",
            "(Ljava/lang/String;III)Z",
            block_passable as *mut c_void,
        ),
        method(
            "setBlock",
            "(Ljava/lang/String;IIILjava/lang/String;)V",
            set_block as *mut c_void,
        ),
        method(
            "breakBlock",
            "(Ljava/lang/String;III)Z",
            break_block as *mut c_void,
        ),
        method(
            "playSound",
            "(Ljava/lang/String;DDDLjava/lang/String;FF)V",
            play_sound as *mut c_void,
        ),
        method(
            "playSoundCategory",
            "(Ljava/lang/String;DDDLjava/lang/String;Ljava/lang/String;FF)V",
            play_sound_category as *mut c_void,
        ),
        method(
            "stopSound",
            "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)V",
            stop_sound as *mut c_void,
        ),
        method(
            "onlinePlayerIds",
            "()[Ljava/lang/String;",
            online_player_ids as *mut c_void,
        ),
        method(
            "knownPlayerIds",
            "()[Ljava/lang/String;",
            known_player_ids as *mut c_void,
        ),
        method(
            "knownPlayerIdByName",
            "(Ljava/lang/String;)Ljava/lang/String;",
            known_player_id_by_name as *mut c_void,
        ),
        method(
            "playerName",
            "(Ljava/lang/String;)Ljava/lang/String;",
            player_name as *mut c_void,
        ),
        method(
            "playerLocale",
            "(Ljava/lang/String;)Ljava/lang/String;",
            player_locale as *mut c_void,
        ),
        method(
            "playerKiller",
            "(Ljava/lang/String;)Ljava/lang/String;",
            player_killer as *mut c_void,
        ),
        method(
            "hasPlayedBefore",
            "(Ljava/lang/String;)Z",
            has_played_before as *mut c_void,
        ),
        method(
            "firstPlayed",
            "(Ljava/lang/String;)J",
            first_played as *mut c_void,
        ),
        method(
            "lastPlayed",
            "(Ljava/lang/String;)J",
            last_played as *mut c_void,
        ),
        method(
            "customName",
            "(Ljava/lang/String;)Ljava/lang/String;",
            custom_name as *mut c_void,
        ),
        method(
            "setCustomName",
            "(Ljava/lang/String;Ljava/lang/String;)V",
            set_custom_name as *mut c_void,
        ),
        method(
            "playerFoodLevel",
            "(Ljava/lang/String;)I",
            player_food_level as *mut c_void,
        ),
        method(
            "worldSeed",
            "(Ljava/lang/String;)J",
            world_seed as *mut c_void,
        ),
        method(
            "worldCoordinateScale",
            "(Ljava/lang/String;)D",
            world_coordinate_scale as *mut c_void,
        ),
        method(
            "worldCanGenerateStructures",
            "(Ljava/lang/String;)Z",
            world_can_generate_structures as *mut c_void,
        ),
        method(
            "setWorldDifficulty",
            "(Ljava/lang/String;Ljava/lang/String;)V",
            set_world_difficulty as *mut c_void,
        ),
        method(
            "worldDifficulty",
            "(Ljava/lang/String;)Ljava/lang/String;",
            world_difficulty as *mut c_void,
        ),
        method(
            "worldAllowMonsters",
            "(Ljava/lang/String;)Z",
            world_allow_monsters as *mut c_void,
        ),
        method(
            "setWorldAllowMonsters",
            "(Ljava/lang/String;Z)V",
            set_world_allow_monsters as *mut c_void,
        ),
        method(
            "worldAllowAnimals",
            "(Ljava/lang/String;)Z",
            world_allow_animals as *mut c_void,
        ),
        method(
            "setWorldAllowAnimals",
            "(Ljava/lang/String;Z)V",
            set_world_allow_animals as *mut c_void,
        ),
        method(
            "worldPvp",
            "(Ljava/lang/String;)Z",
            world_pvp as *mut c_void,
        ),
        method(
            "setWorldPvp",
            "(Ljava/lang/String;Z)V",
            set_world_pvp as *mut c_void,
        ),
        method(
            "playerFoodSaturation",
            "(Ljava/lang/String;)F",
            player_food_saturation as *mut c_void,
        ),
        method(
            "playerFoodExhaustion",
            "(Ljava/lang/String;)F",
            player_food_exhaustion as *mut c_void,
        ),
        method(
            "setPlayerFood",
            "(Ljava/lang/String;IFF)V",
            set_player_food as *mut c_void,
        ),
        method(
            "playerPing",
            "(Ljava/lang/String;)I",
            player_ping as *mut c_void,
        ),
        method(
            "setPlayerOperator",
            "(Ljava/lang/String;Z)V",
            set_player_operator as *mut c_void,
        ),
        method(
            "playerWalkSpeed",
            "(Ljava/lang/String;)F",
            player_walk_speed as *mut c_void,
        ),
        method(
            "setPlayerWalkSpeed",
            "(Ljava/lang/String;F)V",
            set_player_walk_speed as *mut c_void,
        ),
        method(
            "playerFlySpeed",
            "(Ljava/lang/String;)F",
            player_fly_speed as *mut c_void,
        ),
        method(
            "setPlayerFlySpeed",
            "(Ljava/lang/String;F)V",
            set_player_fly_speed as *mut c_void,
        ),
        method(
            "addPotionEffect",
            "(Ljava/lang/String;Ljava/lang/String;II)Z",
            add_potion_effect as *mut c_void,
        ),
        method(
            "removePotionEffect",
            "(Ljava/lang/String;Ljava/lang/String;)V",
            remove_potion_effect as *mut c_void,
        ),
        method("health", "(Ljava/lang/String;)D", health as *mut c_void),
        method(
            "setHealth",
            "(Ljava/lang/String;D)V",
            set_health as *mut c_void,
        ),
        method(
            "maxHealth",
            "(Ljava/lang/String;)D",
            max_health as *mut c_void,
        ),
        method(
            "setAttributeBase",
            "(Ljava/lang/String;Ljava/lang/String;D)V",
            set_attribute_base as *mut c_void,
        ),
        method(
            "playerEntityEffect",
            "(Ljava/lang/String;Ljava/lang/String;)V",
            player_entity_effect as *mut c_void,
        ),
        method(
            "playerWorld",
            "(Ljava/lang/String;)Ljava/lang/String;",
            player_world as *mut c_void,
        ),
        method(
            "playerAddress",
            "(Ljava/lang/String;)Ljava/lang/String;",
            player_address as *mut c_void,
        ),
        method(
            "advancementDisplay",
            "(Ljava/lang/String;)[Ljava/lang/String;",
            advancement_display as *mut c_void,
        ),
        method(
            "advancementCriteria",
            "(Ljava/lang/String;)[Ljava/lang/String;",
            advancement_criteria as *mut c_void,
        ),
        method(
            "advancementKeys",
            "()[Ljava/lang/String;",
            advancement_keys as *mut c_void,
        ),
        method(
            "playerAdvancementProgress",
            "(Ljava/lang/String;Ljava/lang/String;)[Ljava/lang/String;",
            player_advancement_progress as *mut c_void,
        ),
        method(
            "playerAdvancementCriterion",
            "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;Z)Z",
            player_advancement_criterion as *mut c_void,
        ),
        method("whitelistEnabled", "()Z", whitelist_enabled as *mut c_void),
        method(
            "playerRespawnWorld",
            "(Ljava/lang/String;)Ljava/lang/String;",
            player_respawn_world as *mut c_void,
        ),
        method(
            "setPlayerRespawnPosition",
            "(Ljava/lang/String;Ljava/lang/String;IIIFF)V",
            set_player_respawn_position as *mut c_void,
        ),
        method(
            "playerRespawnPosition",
            "(Ljava/lang/String;)[D",
            player_respawn_position as *mut c_void,
        ),
        method(
            "sendMessage",
            "(Ljava/lang/String;Ljava/lang/String;)V",
            send_message as *mut c_void,
        ),
        method(
            "chat",
            "(Ljava/lang/String;Ljava/lang/String;)V",
            chat as *mut c_void,
        ),
        method(
            "kickPlayer",
            "(Ljava/lang/String;Ljava/lang/String;)V",
            kick_player as *mut c_void,
        ),
        method(
            "setPlayerListName",
            "(Ljava/lang/String;Ljava/lang/String;)V",
            set_player_list_name as *mut c_void,
        ),
        method(
            "setPlayerListHeader",
            "(Ljava/lang/String;Ljava/lang/String;)V",
            set_player_list_header as *mut c_void,
        ),
        method(
            "setPlayerListFooter",
            "(Ljava/lang/String;Ljava/lang/String;)V",
            set_player_list_footer as *mut c_void,
        ),
        method(
            "setPlayerListHeaderFooter",
            "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)V",
            set_player_list_header_footer as *mut c_void,
        ),
        method(
            "sendActionBar",
            "(Ljava/lang/String;Ljava/lang/String;)V",
            send_action_bar as *mut c_void,
        ),
        method(
            "sendTitle",
            "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;III)V",
            send_title as *mut c_void,
        ),
        method(
            "clearTitle",
            "(Ljava/lang/String;)V",
            clear_title as *mut c_void,
        ),
        method(
            "sendSignChange",
            "(Ljava/lang/String;Ljava/lang/String;III[Ljava/lang/String;I)V",
            send_sign_change as *mut c_void,
        ),
        method(
            "sendBlockChange",
            "(Ljava/lang/String;Ljava/lang/String;IIILjava/lang/String;)V",
            send_block_change as *mut c_void,
        ),
        method(
            "sendPluginMessage",
            "(Ljava/lang/String;Ljava/lang/String;[B)V",
            send_plugin_message as *mut c_void,
        ),
        method(
            "hasPermission",
            "(Ljava/lang/String;Ljava/lang/String;)Z",
            has_permission as *mut c_void,
        ),
        method(
            "effectivePermissions",
            "(Ljava/lang/String;)[Ljava/lang/String;",
            effective_permissions as *mut c_void,
        ),
    ];
    // Entities, players and world queries live in their own modules.
    bindings.extend(attributes::bindings());
    bindings.extend(blocks::bindings());
    bindings.extend(particles::bindings());
    bindings.extend(entities::bindings());
    bindings.extend(displays::bindings());
    bindings.extend(lifecycle::bindings());
    bindings.extend(merchants::bindings());
    bindings.extend(players::bindings());
    // Beside the table rather than in it: the packet tap is its own module.
    bindings.extend(packet_tap::bindings());
    bindings
}

#[cfg(test)]
mod integration_union_tests {
    #[test]
    fn text_codec_combines_damage_potions_json_and_extended_components() {
        use foton_registry::data_components::vanilla_components::*;
        use foton_registry::{init_vanilla_registry, item_stack::ItemStack};
        init_vanilla_registry();
        let fields = [
            "minecraft:diamond_sword 1",
            "damage=37",
            "basepotionhex=6d696e6563726166743a7761746572",
            "potioneffects=speed,120,2,true,false,true;",
            "namejsonhex=7b2274657874223a22556e696f6e227d",
            "customhex=7b756e696f6e3a317d",
            "cooldown=2.5",
            "cooldowngroup=minecraft:union",
            "trim=minecraft:iron,minecraft:sentry",
        ];
        let original = super::parse_slot(&fields.join("\u{1d}")).unwrap();
        assert_eq!(original.get_damage_value(), 37);
        let potion = original.get(POTION_CONTENTS).unwrap();
        assert!(potion.potion().is_some());
        assert!(original.get(CUSTOM_NAME).is_some());
        assert!(original.get(CUSTOM_DATA).is_some());
        assert!(original.get(TRIM).is_some());
        assert_eq!(original.get(USE_COOLDOWN).unwrap().seconds, 2.5);
        let encoded = super::describe_slot(&original);
        assert!(encoded.contains("speed,120,2,true,false,true;"));
        assert!(encoded.contains("namejsonhex="));
        assert!(encoded.contains("customhex="));
        assert!(encoded.contains("trim="));
        let restored = super::parse_slot(&encoded).unwrap();
        assert!(ItemStack::matches(&restored, &original));
    }

    #[test]
    fn native_registration_has_one_implementation_per_java_method() {
        let mut seen = std::collections::BTreeSet::new();
        for method in super::bindings() {
            let key = (
                method.name.to_str().unwrap().to_owned(),
                method.sig.to_str().unwrap().to_owned(),
            );
            assert!(seen.insert(key.clone()), "ambiguous JNI binding: {key:?}");
        }
    }
}

#[cfg(test)]
pub(crate) mod entity_bridge_tests {
    use std::sync::{Arc, Weak};
    use std::thread;
    use std::time::Duration;

    use foton_core::chunk::chunk_request::{ChunkRequestState, ChunkTicketKind};
    use foton_core::chunk::status::ChunkStatus;
    use foton_core::config::RuntimeConfig;
    use foton_core::entity::Entity;
    use foton_core::entity::entities::{ArmorStandEntity, HorseEntity, RawEntity};
    use foton_core::inventory::container::Container as _;
    use foton_core::inventory::equipment::EquipmentSlot;
    use foton_core::level_data::WorldGenerationSettings;
    use foton_core::player::connection::NetworkConnection;
    use foton_core::player::{ClientInformation, GameProfile, Player, PlayerConnection};
    use foton_core::world::{World, WorldConfig, WorldStorageConfig};
    use foton_core::worldgen::{ChunkGeneratorType, EmptyChunkGenerator};
    use foton_protocol::packet_traits::{CompressionInfo, EncodedPacket};
    use foton_registry::{init_vanilla_registry, vanilla_dimension_types, vanilla_entities};
    use foton_utils::types::{Difficulty, GameType};
    use foton_utils::{ChunkPos, Identifier};
    use glam::DVec3;
    use text_components::TextComponent;
    use tokio::runtime::{Builder as RuntimeBuilder, Runtime};
    use toml::Value as TomlValue;
    use toml::map::Map as TomlMap;
    use uuid::Uuid;

    use super::{
        EquipmentSlotRequestError, add_entity_scoreboard_tag_state, bindings,
        clear_entity_equipment_state, enchantments_conflict_state, entity_by_text,
        entity_equipment_slot_state, entity_has_gravity_state, entity_in_rain_state,
        entity_position_state, entity_scoreboard_tags_state, entity_silent_state,
        equipment_slot_from_index, is_tagged_state, remove_entity_scoreboard_tag_state,
        set_entity_equipment_slot_state, set_entity_gravity_state, set_entity_rotation_state,
        set_entity_silent_state,
    };

    fn entity() -> RawEntity {
        RawEntity::new(
            7,
            DVec3::new(1.25, 64.5, -3.75),
            Weak::new(),
            &vanilla_entities::ITEM,
        )
    }

    struct EquipmentTestConnection;

    impl NetworkConnection for EquipmentTestConnection {
        fn compression(&self) -> Option<CompressionInfo> {
            None
        }

        fn send_encoded(&self, _packet: EncodedPacket) {}

        fn send_encoded_bundle(&self, _packets: Vec<EncodedPacket>) {}

        fn disconnect_with_reason(&self, _reason: TextComponent) {}

        fn tick(&self) {}

        fn latency(&self) -> i32 {
            0
        }

        fn close(&self) {}

        fn closed(&self) -> bool {
            false
        }
    }

    pub(crate) fn equipment_test_config() -> Arc<RuntimeConfig> {
        Arc::new(RuntimeConfig {
            max_players: 1,
            view_distance: 2,
            simulation_distance: 2,
            max_chained_neighbor_updates: 1_000_000,
            online_mode: false,
            whitelist_enabled: false,
            auth_server: None,
            allow_insecure_auth_server: false,
            profile_server: None,
            services_server: None,
            encryption: false,
            allow_flight: false,
            motd: String::new(),
            use_favicon: false,
            favicon: String::new(),
            enforce_secure_chat: false,
            chat_spam_threshold_seconds: 10,
            command_spam_threshold_seconds: 10,
            compression: None,
            server_links: None,
            packet_workers: Some(1),
            chunk_generation_threads: Some(1),
            chunk_encoding_threads: Some(1),
            bug_report_webhook: None,
        })
    }

    fn equipment_test_player() -> Arc<Player> {
        let world = rain_test_world();
        Arc::new_cyclic(|player| {
            Player::new(
                GameProfile {
                    id: Uuid::new_v4(),
                    name: "EquipmentBridge".to_owned(),
                    properties: Vec::new(),
                    profile_actions: None,
                },
                Arc::new(PlayerConnection::Other(Box::new(EquipmentTestConnection))),
                world,
                Weak::new(),
                equipment_test_config(),
                10,
                ClientInformation::default(),
                player,
            )
        })
    }

    pub(crate) fn rain_test_world() -> Arc<World> {
        rain_test_world_with_worker_observer(|_, _| {})
    }

    pub(crate) fn rain_test_world_with_worker_observer(
        observe: impl FnOnce(&Runtime, &rayon::ThreadPool),
    ) -> Arc<World> {
        init_vanilla_registry();
        let runtime = Arc::new(
            match RuntimeBuilder::new_multi_thread()
                .worker_threads(1)
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime,
                Err(error) => panic!("rain test runtime should start: {error}"),
            },
        );
        let generation_pool = Arc::new(
            match rayon::ThreadPoolBuilder::new()
                .num_threads(1)
                .thread_name(|index| format!("foton-plugin-rain-test-{index}"))
                .build()
            {
                Ok(pool) => pool,
                Err(error) => panic!("rain test generation pool should start: {error}"),
            },
        );
        observe(&runtime, &generation_pool);
        let dimension_type = &vanilla_dimension_types::OVERWORLD;
        let generator_config = TomlValue::Table(TomlMap::new());
        let generation_settings = WorldGenerationSettings::from_generator_config(
            Identifier::vanilla_static("empty"),
            &generator_config,
            dimension_type.key.clone(),
            dimension_type.min_y,
            dimension_type.height,
        );
        let world = match runtime.block_on(World::new_with_config(
            Arc::clone(&runtime),
            Identifier::vanilla_static("plugin_rain_binding"),
            dimension_type,
            0,
            WorldConfig {
                storage: WorldStorageConfig::RamOnly,
                level_data_path: None,
                generator: Arc::new(ChunkGeneratorType::Empty(EmptyChunkGenerator::new())),
                generation_settings,
                view_distance: 2,
                simulation_distance: 2,
                max_chained_neighbor_updates: 1_000_000,
                compression: None,
                is_flat: false,
                sea_level: 63,
                default_gamemode: GameType::Survival,
                difficulty: Difficulty::Normal,
                bonus_chest: false,
            },
            generation_pool,
        )) {
            Ok(world) => world,
            Err(error) => panic!("rain test world should initialize: {error}"),
        };
        let request = world.chunk_map.request_square(
            ChunkPos::new(0, -1),
            1,
            ChunkStatus::Full,
            ChunkTicketKind::Command,
        );
        for _ in 0..10_000 {
            world.chunk_map.advance_scheduling();
            if request.poll() == ChunkRequestState::Ready {
                return world;
            }
            thread::sleep(Duration::from_millis(1));
        }
        panic!("rain test chunks did not become ready");
    }

    #[test]
    fn spawn_reason_query_preserves_override_and_defaults_without_custom() {
        use foton_core::entity::{EntitySpawnReason, PluginSpawnReason};
        let entity = entity();
        assert_eq!(
            super::entity_spawn_reason_state(None),
            PluginSpawnReason::Default
        );
        assert_eq!(
            super::entity_spawn_reason_state(Some(&entity)),
            PluginSpawnReason::Default
        );
        for (vanilla, expected) in [
            (
                EntitySpawnReason::ChunkGeneration,
                PluginSpawnReason::ChunkGen,
            ),
            (
                EntitySpawnReason::TrialSpawner,
                PluginSpawnReason::TrialSpawner,
            ),
            (EntitySpawnReason::Load, PluginSpawnReason::Default),
        ] {
            entity.base().set_spawn_reason(vanilla);
            assert_eq!(super::entity_spawn_reason_state(Some(&entity)), expected);
        }
        entity
            .base()
            .set_plugin_spawn_reason(PluginSpawnReason::Beehive);
        assert_eq!(
            super::entity_spawn_reason_state(Some(&entity)),
            PluginSpawnReason::Beehive
        );
        assert_eq!(entity.base().spawn_reason(), Some(EntitySpawnReason::Load));
    }

    #[test]
    fn entity_lookup_rejects_invalid_and_missing_uuids() {
        assert!(entity_by_text("not-a-uuid").is_none());
        assert!(entity_by_text("00000000-0000-0000-0000-000000000000").is_none());
    }

    #[test]
    fn equipment_bridge_reads_and_writes_body_and_saddle_on_a_horse() {
        init_vanilla_registry();
        let horse = HorseEntity::new(&vanilla_entities::HORSE, 8, DVec3::ZERO, Weak::new());
        let horse = &horse as &dyn Entity;

        for (index, slot) in [
            EquipmentSlot::MainHand,
            EquipmentSlot::OffHand,
            EquipmentSlot::Feet,
            EquipmentSlot::Legs,
            EquipmentSlot::Chest,
            EquipmentSlot::Head,
            EquipmentSlot::Body,
            EquipmentSlot::Saddle,
        ]
        .into_iter()
        .enumerate()
        {
            assert_eq!(equipment_slot_from_index(index as i32), Some(slot));
        }

        assert_eq!(
            set_entity_equipment_slot_state(Some(horse), 6, "minecraft:leather_horse_armor 1"),
            Ok(())
        );
        assert_eq!(
            set_entity_equipment_slot_state(Some(horse), 7, "minecraft:saddle 1"),
            Ok(())
        );
        assert!(matches!(
            entity_equipment_slot_state(Some(horse), 6),
            Ok(Some(value)) if value.starts_with("minecraft:leather_horse_armor 1")
        ));
        assert!(matches!(
            entity_equipment_slot_state(Some(horse), 7),
            Ok(Some(value)) if value.starts_with("minecraft:saddle 1")
        ));
    }

    #[test]
    fn equipment_bridge_crosses_player_inventory_for_selected_hand_offhand_and_armor() {
        init_vanilla_registry();
        let player = equipment_test_player();
        player.inventory.lock().set_selected_slot(4);
        let entity = player.as_ref() as &dyn Entity;

        for (bridge_slot, inventory_slot, encoded) in [
            (0, 4, "minecraft:diamond_sword 1"),
            (1, 40, "minecraft:shield 1"),
            (2, 36, "minecraft:diamond_boots 1"),
            (3, 37, "minecraft:diamond_leggings 1"),
            (4, 38, "minecraft:diamond_chestplate 1"),
            (5, 39, "minecraft:diamond_helmet 1"),
        ] {
            assert_eq!(
                set_entity_equipment_slot_state(Some(entity), bridge_slot, encoded),
                Ok(())
            );
            assert!(
                super::describe_slot(player.inventory.lock().get_item(inventory_slot))
                    .starts_with(encoded)
            );
        }

        let changes_before_idempotent_write = player.inventory.lock().get_times_changed();
        assert_eq!(
            set_entity_equipment_slot_state(Some(entity), 0, "minecraft:diamond_sword 1"),
            Ok(())
        );
        assert_eq!(
            player.inventory.lock().get_times_changed(),
            changes_before_idempotent_write
        );

        for (bridge_slot, inventory_slot, encoded) in [
            (0, 4, "minecraft:iron_sword 1"),
            (1, 40, "minecraft:totem_of_undying 1"),
            (2, 36, "minecraft:iron_boots 1"),
            (3, 37, "minecraft:iron_leggings 1"),
            (4, 38, "minecraft:iron_chestplate 1"),
            (5, 39, "minecraft:iron_helmet 1"),
        ] {
            let stack = super::parse_slot(encoded).expect("test item should parse");
            player.inventory.lock().set_item(inventory_slot, stack);
            assert!(matches!(
                entity_equipment_slot_state(Some(entity), bridge_slot),
                Ok(Some(value)) if value.starts_with(encoded)
            ));
        }
    }

    #[test]
    fn equipment_bridge_uses_storage_for_slots_the_entity_cannot_naturally_use() {
        init_vanilla_registry();
        let stand =
            ArmorStandEntity::new(&vanilla_entities::ARMOR_STAND, 9, DVec3::ZERO, Weak::new());
        let entity = &stand as &dyn Entity;

        assert_eq!(
            set_entity_equipment_slot_state(Some(entity), 6, "minecraft:stone 2"),
            Ok(())
        );
        assert_eq!(
            set_entity_equipment_slot_state(Some(entity), 7, "minecraft:saddle 1"),
            Ok(())
        );
        assert!(matches!(
            entity_equipment_slot_state(Some(entity), 6),
            Ok(Some(value)) if value.starts_with("minecraft:stone 2")
        ));
        assert!(matches!(
            entity_equipment_slot_state(Some(entity), 7),
            Ok(Some(value)) if value.starts_with("minecraft:saddle 1")
        ));
    }

    #[test]
    fn equipment_bridge_clear_is_atomic_and_validates_before_mutating() {
        init_vanilla_registry();
        let stand =
            ArmorStandEntity::new(&vanilla_entities::ARMOR_STAND, 11, DVec3::ZERO, Weak::new());
        let entity = &stand as &dyn Entity;

        for slot in 0..8 {
            assert_eq!(
                set_entity_equipment_slot_state(
                    Some(entity),
                    slot,
                    &format!("minecraft:stone {}", slot + 1),
                ),
                Ok(())
            );
        }
        let before = (0..8)
            .map(|slot| entity_equipment_slot_state(Some(entity), slot))
            .collect::<Vec<_>>();

        assert_eq!(
            set_entity_equipment_slot_state(Some(entity), 8, "minecraft:diamond 1"),
            Err(EquipmentSlotRequestError::InvalidIndex)
        );
        assert_eq!(
            entity_equipment_slot_state(Some(entity), -1),
            Err(EquipmentSlotRequestError::InvalidIndex)
        );
        assert!(!clear_entity_equipment_state(None));
        assert_eq!(
            (0..8)
                .map(|slot| entity_equipment_slot_state(Some(entity), slot))
                .collect::<Vec<_>>(),
            before
        );

        assert!(clear_entity_equipment_state(Some(entity)));
        assert!(bindings().iter().any(|method| {
            method.name.to_str().ok() == Some("clearEntityEquipment")
                && method.sig.to_str().ok() == Some("(Ljava/lang/String;)V")
        }));
        for slot in 0..8 {
            assert_eq!(
                entity_equipment_slot_state(Some(entity), slot),
                Ok(Some(String::new()))
            );
        }
    }

    #[test]
    fn equipment_bridge_clear_updates_player_inventory_once() {
        init_vanilla_registry();
        let player = equipment_test_player();
        let entity = player.as_ref() as &dyn Entity;
        let living = entity
            .as_living_entity()
            .expect("a player should expose living equipment");
        living.clear_equipment();

        for (bridge_slot, inventory_slot) in [
            (0, 0),
            (1, 40),
            (2, 36),
            (3, 37),
            (4, 38),
            (5, 39),
            (6, 41),
            (7, 42),
        ] {
            if bridge_slot % 2 == 0 {
                assert_eq!(
                    set_entity_equipment_slot_state(Some(entity), bridge_slot, "minecraft:stone 1",),
                    Ok(())
                );
            } else {
                let stack =
                    super::parse_slot("minecraft:stone 1").expect("test equipment should parse");
                player.inventory.lock().set_item(inventory_slot, stack);
            }
            assert!(!player.inventory.lock().get_item(inventory_slot).is_empty());
            assert!(matches!(
                entity_equipment_slot_state(Some(entity), bridge_slot),
                Ok(Some(value)) if value.starts_with("minecraft:stone 1")
            ));
        }
        let changes_before_clear = player.inventory.lock().get_times_changed();

        assert!(clear_entity_equipment_state(Some(entity)));

        assert_eq!(
            player.inventory.lock().get_times_changed(),
            changes_before_clear + 1
        );
        for (bridge_slot, inventory_slot) in [
            (0, 0),
            (1, 40),
            (2, 36),
            (3, 37),
            (4, 38),
            (5, 39),
            (6, 41),
            (7, 42),
        ] {
            assert!(player.inventory.lock().get_item(inventory_slot).is_empty());
            assert_eq!(
                entity_equipment_slot_state(Some(entity), bridge_slot),
                Ok(Some(String::new()))
            );
        }
    }

    #[test]
    fn scoreboard_tag_bindings_delegate_and_absence_stays_absent() {
        let entity = entity();

        assert_eq!(entity_scoreboard_tags_state(None), None);
        assert!(!add_entity_scoreboard_tag_state(None, "alpha".to_owned()));
        assert!(!remove_entity_scoreboard_tag_state(None, "alpha"));

        assert!(add_entity_scoreboard_tag_state(
            Some(&entity),
            "beta".to_owned()
        ));
        assert!(add_entity_scoreboard_tag_state(
            Some(&entity),
            "alpha".to_owned()
        ));
        assert!(!add_entity_scoreboard_tag_state(
            Some(&entity),
            "alpha".to_owned()
        ));
        assert_eq!(
            entity_scoreboard_tags_state(Some(&entity)),
            Some(vec!["alpha".to_owned(), "beta".to_owned()])
        );
        assert!(remove_entity_scoreboard_tag_state(Some(&entity), "alpha"));
        assert!(!remove_entity_scoreboard_tag_state(Some(&entity), "alpha"));
    }

    #[test]
    fn live_registry_bridge_resolves_namespaced_tags_and_enchantment_conflicts() {
        init_vanilla_registry();

        assert!(is_tagged_state(
            "items",
            "minecraft:trimmable_armor",
            "minecraft:diamond_chestplate"
        ));
        assert!(!is_tagged_state(
            "items",
            "minecraft:trimmable_armor",
            "minecraft:elytra"
        ));

        assert!(enchantments_conflict_state(
            "minecraft:infinity",
            "minecraft:infinity"
        ));
        assert!(enchantments_conflict_state(
            "minecraft:infinity",
            "minecraft:mending"
        ));
        assert!(enchantments_conflict_state(
            "minecraft:mending",
            "minecraft:infinity"
        ));
        assert!(!enchantments_conflict_state(
            "minecraft:sharpness",
            "minecraft:unbreaking"
        ));
        assert!(!enchantments_conflict_state(
            "example:infinity",
            "minecraft:mending"
        ));
        assert!(!enchantments_conflict_state(
            "minecraft:missing",
            "minecraft:mending"
        ));
        assert!(bindings().iter().any(|method| {
            method.name.to_str().ok() == Some("enchantmentsConflict")
                && method.sig.to_str().ok() == Some("(Ljava/lang/String;Ljava/lang/String;)Z")
        }));
    }

    #[test]
    fn gravity_bindings_invert_the_native_no_gravity_flag() {
        let entity = entity();

        assert!(!entity_has_gravity_state(None));
        set_entity_gravity_state(None, true);
        assert!(entity_has_gravity_state(Some(&entity)));

        set_entity_gravity_state(Some(&entity), false);
        assert!(entity.is_no_gravity());
        assert!(!entity_has_gravity_state(Some(&entity)));
        set_entity_gravity_state(Some(&entity), true);
        assert!(!entity.is_no_gravity());
    }

    #[test]
    fn silent_bindings_delegate_and_ignore_a_missing_entity() {
        let entity = entity();

        assert!(!entity_silent_state(None));
        set_entity_silent_state(None, true);
        assert!(!entity_silent_state(Some(&entity)));

        set_entity_silent_state(Some(&entity), true);
        assert!(entity_silent_state(Some(&entity)));
        set_entity_silent_state(Some(&entity), false);
        assert!(!entity_silent_state(Some(&entity)));
    }

    #[test]
    fn rotation_binding_delegates_only_finite_values() {
        let entity = entity();

        assert!(!set_entity_rotation_state(None, 45.0, 20.0));
        assert!(set_entity_rotation_state(Some(&entity), 450.0, 120.0));
        assert_eq!(entity.rotation(), (90.0, 90.0));

        assert!(!set_entity_rotation_state(Some(&entity), f32::NAN, 0.0));
        assert!(!set_entity_rotation_state(
            Some(&entity),
            0.0,
            f32::INFINITY
        ));
        assert_eq!(entity.rotation(), (90.0, 90.0));
    }

    #[test]
    fn entity_position_keeps_the_rotation_from_the_same_snapshot() {
        let entity = entity();
        entity.set_rotation((120.0, -35.0));

        assert_eq!(entity_position_state(None), None);
        assert_eq!(
            entity_position_state(Some(&entity)),
            Some([1.25, 64.5, -3.75, 120.0, -35.0])
        );
    }

    #[test]
    fn rain_binding_observes_dry_to_raining_world_transition() {
        let world = rain_test_world();
        let entity = RawEntity::new(
            8,
            DVec3::new(1.25, 64.5, -3.75),
            Arc::downgrade(&world),
            &vanilla_entities::ITEM,
        );

        assert!(!entity_in_rain_state(None));
        assert!(!entity_in_rain_state(Some(&entity)));

        world.level_data.write().set_raining(true);
        world.weather.lock().rain_level = 1.0;

        assert!(world.can_see_sky(entity.block_position()));
        assert!(world.is_raining_at(entity.block_position()));
        assert!(entity_in_rain_state(Some(&entity)));
    }

    #[test]
    fn every_entity_state_native_has_the_java_descriptor_it_serves() {
        let expected = [
            (
                "entityScoreboardTags",
                "(Ljava/lang/String;)[Ljava/lang/String;",
            ),
            (
                "addEntityScoreboardTag",
                "(Ljava/lang/String;Ljava/lang/String;)Z",
            ),
            (
                "removeEntityScoreboardTag",
                "(Ljava/lang/String;Ljava/lang/String;)Z",
            ),
            ("entityHasGravity", "(Ljava/lang/String;)Z"),
            ("setEntityGravity", "(Ljava/lang/String;Z)V"),
            ("entitySilent", "(Ljava/lang/String;)Z"),
            ("setEntitySilent", "(Ljava/lang/String;Z)V"),
            ("setEntityRotation", "(Ljava/lang/String;FF)V"),
            ("entityInRain", "(Ljava/lang/String;)Z"),
            ("animalBreedCause", "(Ljava/lang/String;)Ljava/lang/String;"),
            (
                "setAnimalBreedCause",
                "(Ljava/lang/String;Ljava/lang/String;)V",
            ),
            ("animalLoveTicks", "(Ljava/lang/String;)I"),
            ("setAnimalLoveTicks", "(Ljava/lang/String;I)V"),
            (
                "animalIsBreedItem",
                "(Ljava/lang/String;Ljava/lang/String;)Z",
            ),
            ("cowVariant", "(Ljava/lang/String;)Ljava/lang/String;"),
            ("setCowVariant", "(Ljava/lang/String;Ljava/lang/String;)V"),
            ("cowSoundVariant", "(Ljava/lang/String;)Ljava/lang/String;"),
            (
                "setCowSoundVariant",
                "(Ljava/lang/String;Ljava/lang/String;)V",
            ),
            ("setArrowPotion", "(Ljava/lang/String;Ljava/lang/String;)V"),
        ];
        let methods = bindings();

        for (name, signature) in expected {
            let method = methods
                .iter()
                .find(|method| method.name.to_str().ok() == Some(name));
            assert_eq!(
                method.and_then(|method| method.sig.to_str().ok()),
                Some(signature)
            );
        }
    }
}

#[cfg(test)]
mod snbt_tests {
    use super::parse_item_snbt_patch;
    #[test]
    fn prefixed_item_snbt_is_accepted() {
        let value = parse_item_snbt_patch("minecraft:stone{foo:1}").expect("valid");
        assert_eq!(value.int("foo"), Some(1));
    }
    #[test]
    fn malformed_or_unknown_prefix_is_rejected() {
        assert!(parse_item_snbt_patch("not valid{foo:1}").is_none());
        assert!(parse_item_snbt_patch("minecraft:stone{foo:}").is_none());
    }
}

#[cfg(test)]
mod tag_tests {
    use super::tag_members;
    use foton_utils::Identifier;

    fn tag(path: &'static str) -> Identifier {
        Identifier::vanilla_static(path)
    }

    /// `Registry#hasTag` reads absence from `None`, so an unknown tag must not
    /// look like an empty one.
    #[test]
    fn an_unknown_tag_is_absent_rather_than_empty() {
        foton_registry::init_vanilla_registry();
        assert!(tag_members("enchantment", &tag("not_a_tag")).is_none());
        assert!(tag_members("not_a_registry", &tag("in_enchanting_table")).is_none());
    }

    /// Paper's registry names and Bukkit's legacy ones reach the same tags.
    #[test]
    fn registries_are_named_either_way() {
        foton_registry::init_vanilla_registry();
        let table = tag_members("enchantment", &tag("in_enchanting_table")).unwrap_or_default();
        assert!(table.iter().any(|key| key == "minecraft:sharpness"));
        assert!(!table.iter().any(|key| key == "minecraft:mending"));
        assert_eq!(
            tag_members("items", &tag("trimmable_armor")),
            tag_members("minecraft:item", &tag("trimmable_armor"))
        );
    }
}

#[cfg(test)]
mod arrow_effect_bridge_tests {
    use foton_registry::{
        MobEffectInstance, MobEffectInstanceDetails, init_vanilla_registry, vanilla_mob_effects,
    };

    use super::{
        add_or_replace_arrow_custom_effect, encode_arrow_custom_effect, parse_arrow_custom_effect,
    };

    #[test]
    fn native_arrow_effect_encoding_preserves_hidden_but_bukkit_addition_drops_it() {
        init_vanilla_registry();
        let deepest = MobEffectInstanceDetails::new(4, 600, true, false, true, None);
        let hidden = MobEffectInstanceDetails::new(3, 400, false, true, false, Some(deepest));
        let original = MobEffectInstance::new(
            vanilla_mob_effects::LUCK,
            80,
            2,
            true,
            false,
            true,
            Some(hidden),
        );

        let encoded = encode_arrow_custom_effect(&original);
        assert_eq!(
            encoded, "luck|80|2|true|false|true|400|3|false|true|false|600|4|true|false|true",
            "native Arrow reads must retain the complete hidden-effect chain"
        );
        assert!(
            parse_arrow_custom_effect(&encoded).is_none(),
            "Bukkit addition must reject the native-only hidden payload"
        );
        let visible = "luck|80|2|true|false|true";
        let decoded = parse_arrow_custom_effect(visible)
            .unwrap_or_else(|| panic!("visible Arrow effect must decode: {visible}"));

        assert_eq!(decoded.effect(), vanilla_mob_effects::LUCK);
        assert_eq!(decoded.duration(), 80);
        assert_eq!(decoded.amplifier(), 2);
        assert!(decoded.ambient());
        assert!(!decoded.show_particles());
        assert!(decoded.show_icon());
        assert!(decoded.hidden_effect().is_none());
    }

    #[test]
    fn arrow_effect_override_removes_every_duplicate_before_appending() {
        init_vanilla_registry();
        let luck = || MobEffectInstance::simple(vanilla_mob_effects::LUCK, 20, 0);
        let mut effects = vec![
            luck(),
            MobEffectInstance::simple(vanilla_mob_effects::POISON, 40, 1),
            luck(),
        ];
        let replacement = MobEffectInstance::simple(vanilla_mob_effects::LUCK, 80, 2);

        assert!(add_or_replace_arrow_custom_effect(
            &mut effects,
            replacement,
            true,
        ));
        assert_eq!(effects.len(), 2);
        assert_eq!(effects[0].effect(), vanilla_mob_effects::POISON);
        assert_eq!(effects[1].effect(), vanilla_mob_effects::LUCK);
        assert_eq!((effects[1].duration(), effects[1].amplifier()), (80, 2));
    }
}

#[cfg(test)]
mod slot_codec_tests {
    use foton_registry::data_components::components::PotionContents;
    use foton_registry::data_components::vanilla_components::POTION_CONTENTS;
    use foton_registry::item_stack::ItemStack;
    use foton_registry::mob_effect::instance::MobEffectInstance;
    use foton_registry::{
        RegistryReference, init_vanilla_registry, vanilla_items, vanilla_mob_effects,
        vanilla_potions,
    };

    use super::{describe_slot, parse_slot};

    #[test]
    fn slot_codec_round_trip_preserves_damage_exactly() {
        init_vanilla_registry();
        let mut original = ItemStack::new(&vanilla_items::DIAMOND_SWORD);
        original.set_damage_value(37);

        let encoded = describe_slot(&original);
        let decoded = parse_slot(&encoded).expect("described damaged item should parse");

        assert!(encoded.split('\u{1d}').any(|field| field == "damage=37"));
        assert_eq!(decoded.get_damage_value(), 37);
        assert!(ItemStack::matches(&decoded, &original));
    }

    #[test]
    fn slot_codec_round_trip_preserves_base_potion_identity() {
        init_vanilla_registry();
        let mut original = ItemStack::new(&vanilla_items::POTION);
        original.set(
            POTION_CONTENTS,
            PotionContents::new(
                Some(RegistryReference::new(&vanilla_potions::WATER)),
                None,
                Vec::new(),
                None,
            ),
        );

        let encoded = describe_slot(&original);
        let decoded = parse_slot(&encoded).expect("described base potion should parse");

        assert!(
            encoded
                .split('\u{1d}')
                .any(|field| field.starts_with("basepotionhex="))
        );
        assert_eq!(decoded.get(POTION_CONTENTS), original.get(POTION_CONTENTS));
        assert!(ItemStack::matches(&decoded, &original));
    }

    #[test]
    fn slot_codec_round_trip_preserves_custom_potion_effects_exactly() {
        init_vanilla_registry();
        let mut original = ItemStack::new(&vanilla_items::POTION);
        original.set_opaque_nbt(Some("{foton_opaque:1b}".to_owned()));
        original.set(
            POTION_CONTENTS,
            PotionContents::new(
                None,
                None,
                vec![
                    MobEffectInstance::new(
                        vanilla_mob_effects::SPEED,
                        120,
                        2,
                        true,
                        false,
                        true,
                        None,
                    ),
                    MobEffectInstance::new(
                        vanilla_mob_effects::POISON,
                        45,
                        1,
                        false,
                        true,
                        false,
                        None,
                    ),
                ],
                None,
            ),
        );

        let encoded = describe_slot(&original);
        let decoded = parse_slot(&encoded).expect("described custom potion should parse");

        let effects = encoded
            .split('\u{1d}')
            .find_map(|field| field.strip_prefix("potioneffects="))
            .expect("potion effects should use their labeled metadata field");
        assert_eq!(
            effects,
            "speed,120,2,true,false,true;poison,45,1,false,true,false;"
        );
        assert!(
            effects
                .split(';')
                .filter(|effect| !effect.is_empty())
                .all(|effect| effect.split(',').count() == 6)
        );
        assert_eq!(decoded.opaque_nbt(), original.opaque_nbt());
        assert_eq!(decoded.get(POTION_CONTENTS), original.get(POTION_CONTENTS));
        assert!(ItemStack::matches(&decoded, &original));
    }

    #[test]
    fn slot_codec_round_trip_accepts_legacy_unlabeled_potion_effects() {
        init_vanilla_registry();

        let decoded =
            parse_slot("minecraft:potion 1\u{1d}speed,20,1;\u{1d}nbthex=7b666f6f3a31627d")
                .expect("legacy potion metadata should parse");
        let effects = decoded
            .get(POTION_CONTENTS)
            .expect("legacy potion effects should survive");

        assert_eq!(effects.custom_effects().len(), 1);
        assert!(!effects.custom_effects()[0].ambient());
        assert!(effects.custom_effects()[0].show_particles());
        assert!(effects.custom_effects()[0].show_icon());
        assert_eq!(decoded.opaque_nbt(), Some("{foo:1b}"));
    }

    #[test]
    fn legacy_slot_parser_rejects_more_than_256_lore_lines() {
        init_vanilla_registry();
        let mut encoded = "minecraft:stone 1".to_owned();
        for _ in 0..257 {
            encoded.push_str("\u{1d}lorehex=78");
        }

        assert!(parse_slot(&encoded).is_none());
    }
}

#[cfg(test)]
mod brewing_bridge_tests {
    mod brewing_reachable_gaps;
    mod brewing_review_regressions;
    mod brewing_union_jni;
    #[path = "brewing_recursive_bounds.rs"]
    mod recursive_bounds;
    use foton_core::behavior::init_behaviors;
    use foton_core::block_entity::entities::{BREWING_STAND_SLOTS, BrewingStandBlockEntity};
    use foton_core::block_entity::{BlockEntity as _, init_block_entities};
    use foton_core::entity::entities::ItemEntity;
    use foton_core::world::{LevelReader as _, World};
    use foton_registry::blocks::block_state_ext::BlockStateExt as _;
    use foton_registry::blocks::properties::BlockStateProperties;
    use foton_registry::data_component_predicate::DataComponentMatchers;
    use foton_registry::data_components::components::{
        Filterable, FireworkExplosion, FireworkExplosionShape, Fireworks, ItemContainerContents,
        ItemLore, PotionContents, WrittenBookContent,
    };
    use foton_registry::data_components::vanilla_components::{
        CONTAINER, CUSTOM_NAME, FIREWORKS, ITEM_NAME, LORE, POTION_CONTENTS, WRITTEN_BOOK_CONTENT,
    };
    use foton_registry::item_predicate::{IntBounds, ItemPredicate, LockCode};
    use foton_registry::item_stack::ItemStack;
    use foton_registry::potion_brewing::potion_item;
    use foton_registry::{
        ItemStackTemplate, REGISTRY, RegistryEntry as _, RegistryExt as _, RegistryHolderSet,
        RegistryReference, init_vanilla_registry, vanilla_blocks, vanilla_items, vanilla_potions,
    };
    use foton_utils::Identifier;
    use foton_utils::codec::VarInt;
    use foton_utils::serial::WriteTo as _;
    use foton_utils::types::UpdateFlags;
    use foton_utils::{BlockPos, Downcast as _};
    use std::cell::Cell;
    use std::ops::Deref;
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;
    use text_components::TextComponent;
    use text_components::format::Color;

    use super::{
        BrewingBridgeSnapshot, MAX_BREWING_ITEM_BYTES, MAX_BREWING_SNAPSHOT_BYTES,
        apply_brewing_snapshot, bindings, brewing_apply_matches,
        brewing_item_payload_length_is_valid, brewing_payload_length_is_valid,
        brewing_update_flags, decode_brewing_snapshot, encode_brewing_item,
        encode_brewing_snapshot, read_brewing_item,
    };

    fn snapshot(items: Vec<ItemStack>) -> BrewingBridgeSnapshot {
        BrewingBridgeSnapshot {
            identity: 7,
            items,
            brew_time: 37,
            recipe_brew_time: 240,
            fuel: 11,
            custom_name: Some(TextComponent::plain("Bridge brewer")),
            lock: LockCode::NO_LOCK,
        }
    }

    struct LoadedBridgeWorld(Arc<World>, brewing_union_jni::WorkerFailureObserver);

    impl LoadedBridgeWorld {
        fn new(world: Arc<World>) -> Self {
            let failures = brewing_union_jni::WorkerFailureObserver::new(&world);
            Self(world, failures)
        }
    }

    impl Deref for LoadedBridgeWorld {
        type Target = Arc<World>;

        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }

    impl Drop for LoadedBridgeWorld {
        fn drop(&mut self) {
            let world = &self.0;
            world.chunk_map.stop_generation_refill_loop();
            world.chunk_map.cancel_token.cancel();
            world.chunk_map.task_tracker.close();
            Arc::clone(&world.chunk_map.chunk_runtime)
                .block_on(world.chunk_map.task_tracker.wait());
            let failures = self.1.failures();
            if !thread::panicking() {
                assert_eq!(
                    failures, 0,
                    "brewing world workers failed before teardown completed"
                );
            }
        }
    }

    fn loaded_bridge_world() -> LoadedBridgeWorld {
        init_vanilla_registry();
        init_behaviors();
        init_block_entities();
        let mut failures = None;
        let world =
            super::entity_bridge_tests::rain_test_world_with_worker_observer(|runtime, pool| {
                failures = Some(brewing_union_jni::WorkerFailureObserver::for_workers(
                    runtime, pool,
                ));
            });
        LoadedBridgeWorld(
            world,
            failures.expect("worker observer installed before world creation"),
        )
    }

    #[test]
    fn loaded_bridge_world_teardown_joins_tracked_worldgen() {
        use std::{
            sync::mpsc::{self, RecvTimeoutError},
            thread,
        };
        use tokio::time::sleep;
        let world = loaded_bridge_world();
        let weak_world = Arc::downgrade(&world);
        let runtime = Arc::clone(&world.chunk_map.chunk_runtime);
        let task_tracker = world.chunk_map.task_tracker.clone();

        let cancel = world.chunk_map.cancel_token.clone();
        let alive = weak_world.clone();
        let task = task_tracker.spawn_on(
            async move {
                cancel.cancelled().await;
                sleep(Duration::from_millis(20)).await;
                assert!(
                    alive.upgrade().is_some(),
                    "world must outlive its tracked tasks"
                );
            },
            runtime.handle(),
        );
        let (finished, completion) = mpsc::channel();
        let teardown = thread::spawn(move || {
            drop(world);
            let _ = finished.send(());
        });
        // The deadline surrounds the destructor that actually blocks on the join.
        let completed = completion.recv_timeout(Duration::from_secs(5));
        assert!(
            !matches!(completed, Err(RecvTimeoutError::Timeout)),
            "brewing world teardown timed out"
        );
        teardown.join().expect("brewing world teardown panicked");
        completed.expect("teardown completion channel disconnected");
        runtime
            .block_on(task)
            .expect("tracked worldgen task panicked");
        assert!(weak_world.upgrade().is_none());
    }

    fn installed_brewing_identity(world: &Arc<World>, pos: BlockPos) -> u64 {
        world
            .get_block_entity(pos)
            .and_then(|entity| {
                entity
                    .downcast_ref::<BrewingStandBlockEntity>()
                    .map(|brewing| brewing.state_snapshot().identity)
            })
            .expect("test brewing stand must be installed")
    }

    fn replace_snapshot_blob(encoded: &mut Vec<u8>, length_offset: usize, replacement: &[u8]) {
        let start = length_offset + 4;
        let original_length = u32::from_be_bytes(
            encoded[length_offset..start]
                .try_into()
                .expect("test blob length is present"),
        ) as usize;
        encoded.splice(start..start + original_length, replacement.iter().copied());
        encoded[length_offset..start].copy_from_slice(&(replacement.len() as u32).to_be_bytes());
    }

    fn custom_name_length_offset(encoded: &[u8]) -> usize {
        let mut offset = 4 + 8 + 4 + 4 + 4;
        for _ in 0..BREWING_STAND_SLOTS {
            let start = offset + 4;
            let length = u32::from_be_bytes(
                encoded[offset..start]
                    .try_into()
                    .expect("test item length is present"),
            ) as usize;
            offset = start + length;
        }
        offset
    }

    #[test]
    fn bridge_rejected_nested_lock_predicate_stays_below_one_mib() {
        use foton_registry::data_component_predicate::{
            CollectionPredicate, DataComponentExactPredicate, DataComponentMatchers,
            DataComponentPredicateData, WrittenBookPagePredicate, WrittenBookPredicate,
            vanilla_data_component_predicate_types,
        };
        init_vanilla_registry();
        let partial = DataComponentPredicateData::new(
            &vanilla_data_component_predicate_types::WRITTEN_BOOK_CONTENT,
            WrittenBookPredicate::new(
                Some(CollectionPredicate::new(
                    Some(vec![WrittenBookPagePredicate::new(TextComponent::plain(
                        "x".repeat(2 << 20),
                    ))]),
                    None,
                    None,
                )),
                None,
                None,
                IntBounds::ANY,
                None,
            ),
        );
        let mut value = snapshot(vec![ItemStack::empty(); 5]);
        value.lock = LockCode::new(ItemPredicate::new(
            None,
            IntBounds::ANY,
            DataComponentMatchers::new(DataComponentExactPredicate::EMPTY, vec![partial])
                .expect("unique predicate"),
        ));
        let stats = allocation_counter::measure(|| {
            assert!(encode_brewing_snapshot(&value).is_none());
        });
        assert!(stats.bytes_max <= 1 << 20, "{stats:?}");
    }

    #[test]
    fn live_item_length_preflight_precedes_copy_and_uses_shared_decode() {
        init_vanilla_registry();
        let copied = Cell::new(false);
        assert!(
            super::read_brewing_item_after_length_check(MAX_BREWING_ITEM_BYTES + 1, || {
                copied.set(true);
                Some(Vec::new())
            })
            .is_none()
        );
        assert!(!copied.get());
        let mut item = ItemStack::with_count(&vanilla_items::STONE, -7);
        item.set(CUSTOM_NAME, TextComponent::plain("negative"));
        item.remove(ITEM_NAME);
        let bytes = encode_brewing_item(&item).expect("encode");
        let decoded = super::read_brewing_item_after_length_check(bytes.len(), || Some(bytes))
            .expect("shared decode");
        assert_eq!(decoded.item, item.item);
        assert_eq!(decoded.count, item.count);
        assert_eq!(decoded.components_patch(), item.components_patch());
    }

    #[test]
    fn bridge_zero_count_preserves_raw_item_count_and_patch() {
        init_vanilla_registry();
        let mut item = ItemStack::with_count(&vanilla_items::STONE, 0);
        item.set(CUSTOM_NAME, TextComponent::plain("zero"));
        item.remove(ITEM_NAME);
        let encoded = encode_brewing_item(&item).expect("item encodes");
        let decoded = read_brewing_item(&encoded).expect("item decodes");
        assert_eq!(decoded.item, item.item);
        assert_eq!(decoded.count, item.count);
        assert_eq!(decoded.components_patch(), item.components_patch());
        let live =
            super::read_brewing_item_after_length_check(encoded.len(), || Some(encoded.clone()))
                .expect("shared live decode");
        assert_eq!(live.item, item.item);
        assert_eq!(live.count, item.count);
        assert_eq!(live.components_patch(), item.components_patch());
        let mut items = vec![ItemStack::empty(); 5];
        items[0] = item;
        let original = snapshot(items);
        let encoded = encode_brewing_snapshot(&original).expect("snapshot encodes");
        let decoded = decode_brewing_snapshot(&encoded).expect("snapshot decodes");
        assert_eq!(decoded.items[0].item, original.items[0].item);
        assert_eq!(decoded.items[0].count, original.items[0].count);
        assert_eq!(
            decoded.items[0].components_patch(),
            original.items[0].components_patch()
        );
    }

    #[test]
    fn bridge_rejects_nonminimal_varints() {
        init_vanilla_registry();
        let mut bytes =
            encode_brewing_item(&ItemStack::new(&vanilla_items::STONE)).expect("encode");
        bytes.splice(0..1, [0x81, 0]);
        assert!(read_brewing_item(&bytes).is_none());
    }

    #[test]
    fn bridge_rejects_boolean_two() {
        init_vanilla_registry();
        let key = Identifier::vanilla_static("enchantment_glint_override");
        let id = REGISTRY
            .data_components
            .id_from_key(&key)
            .expect("registered");
        let mut bytes = Vec::new();
        for value in [1, vanilla_items::STONE.id() as i32, 1, 0, id as i32] {
            VarInt(value).write(&mut bytes).expect("write");
        }
        bytes.push(2);
        bytes.extend_from_slice(&u32::MAX.to_be_bytes());
        assert!(read_brewing_item(&bytes).is_none());
    }

    #[test]
    fn bridge_rejects_unsorted_and_duplicate_component_ids() {
        init_vanilla_registry();
        let mut ids = ["glider", "unbreakable"].map(|name| {
            REGISTRY
                .data_components
                .id_from_key(&Identifier::vanilla_static(name))
                .expect("registered")
        });
        ids.sort_unstable();
        for (added, removed) in [
            (vec![ids[1], ids[0]], vec![]),
            (vec![], vec![ids[1], ids[0]]),
            (vec![ids[0], ids[0]], vec![]),
            (vec![], vec![ids[0], ids[0]]),
            (vec![ids[0]], vec![ids[0]]),
        ] {
            let mut bytes = Vec::new();
            for value in [
                1,
                vanilla_items::STONE.id() as i32,
                added.len() as i32,
                removed.len() as i32,
            ] {
                VarInt(value).write(&mut bytes).expect("header");
            }
            for id in added.into_iter().chain(removed) {
                VarInt(id as i32).write(&mut bytes).expect("component id");
            }
            bytes.extend_from_slice(&u32::MAX.to_be_bytes());
            assert!(read_brewing_item(&bytes).is_none());
        }
    }

    #[test]
    fn bridge_rejected_text_conversion_stays_below_one_mib() {
        init_vanilla_registry();
        let mut value = snapshot(vec![ItemStack::empty(); 5]);
        value.custom_name = Some(TextComponent::plain("x".repeat(2 << 20)));
        let stats = allocation_counter::measure(|| {
            assert!(encode_brewing_snapshot(&value).is_none());
        });
        assert!(stats.bytes_max <= 1 << 20, "{stats:?}");
    }

    #[test]
    fn bridge_rejected_lock_conversion_stays_below_one_mib() {
        init_vanilla_registry();
        let mut value = snapshot(vec![ItemStack::empty(); 5]);
        value.lock = LockCode::new(ItemPredicate::new(
            Some(RegistryHolderSet::Direct(vec![
                &vanilla_items::STONE;
                100_000
            ])),
            IntBounds::ANY,
            DataComponentMatchers::ANY,
        ));
        let stats = allocation_counter::measure(|| {
            assert!(encode_brewing_snapshot(&value).is_none());
        });
        assert!(stats.bytes_max <= 1 << 20, "{stats:?}");
    }

    #[test]
    fn versioned_snapshot_round_trip_preserves_every_bridge_field() {
        init_vanilla_registry();
        let mut items = vec![ItemStack::empty(); BREWING_STAND_SLOTS];
        let mut potion = ItemStack::new(&vanilla_items::POTION);
        potion.set(
            POTION_CONTENTS,
            PotionContents::new(
                Some(RegistryReference::new(&vanilla_potions::WATER)),
                Some(0x12_34_56),
                Vec::new(),
                None,
            ),
        );
        items[0] = potion;
        let mut renamed = ItemStack::new(&vanilla_items::DIAMOND_SWORD);
        renamed.remove(ITEM_NAME);
        renamed.set_opaque_nbt(Some("{bridge:{unknown:1b}}".to_owned()));
        items[1] = renamed;
        items[3] = ItemStack::new(&vanilla_items::NETHER_WART);
        let snapshot = BrewingBridgeSnapshot {
            identity: 0x0102_0304_0506_0708,
            items,
            brew_time: 37,
            recipe_brew_time: 240,
            fuel: 11,
            custom_name: Some(TextComponent::plain("Bridge brewer")),
            lock: LockCode::NO_LOCK,
        };

        let encoded = encode_brewing_snapshot(&snapshot).expect("snapshot should encode");
        assert_eq!(&encoded[..4], b"FBS\x02");
        let decoded = decode_brewing_snapshot(&encoded).expect("snapshot should decode");

        assert_eq!(decoded.identity, snapshot.identity);
        assert_eq!(decoded.brew_time, snapshot.brew_time);
        assert_eq!(decoded.recipe_brew_time, snapshot.recipe_brew_time);
        assert_eq!(decoded.fuel, snapshot.fuel);
        assert_eq!(decoded.custom_name, snapshot.custom_name);
        assert_eq!(decoded.lock, snapshot.lock);
        assert_eq!(decoded.items, snapshot.items);
        let mut trailing = encoded;
        trailing.push(0);
        assert!(decode_brewing_snapshot(&trailing).is_none());
    }

    #[test]
    fn item_codec_exactly_preserves_diverse_components_and_removals() {
        init_vanilla_registry();
        let mut rich_name = TextComponent::plain("Root");
        rich_name.format.color = Some(Color::Aqua);
        let mut child = TextComponent::plain(" child");
        child.format.bold = Some(true);
        rich_name.children.push(child);

        let mut potion = ItemStack::new(&vanilla_items::POTION);
        potion.set(CUSTOM_NAME, rich_name.clone());
        potion.set(
            POTION_CONTENTS,
            PotionContents::new(None, Some(0x65_43_21), Vec::new(), Some("named".to_owned())),
        );

        let mut sword = ItemStack::new(&vanilla_items::DIAMOND_SWORD);
        sword.remove(ITEM_NAME);
        sword.set(
            LORE,
            ItemLore::new(vec![rich_name, TextComponent::plain("second")])
                .expect("two lore lines are valid"),
        );
        sword.set_opaque_nbt(Some("{bridge:{opaque:[I;1,2,3]}}".to_owned()));

        let mut rocket = ItemStack::new(&vanilla_items::FIREWORK_ROCKET);
        rocket.set(
            FIREWORKS,
            Fireworks::new(
                3,
                vec![FireworkExplosion::new(
                    FireworkExplosionShape::Creeper,
                    vec![0x11_22_33],
                    vec![0x44_55_66],
                    true,
                    true,
                )],
            )
            .expect("one firework explosion is valid"),
        );

        let mut book = ItemStack::new(&vanilla_items::WRITTEN_BOOK);
        book.set(
            WRITTEN_BOOK_CONTENT,
            WrittenBookContent::new(
                Filterable::pass_through("Bridge book".to_owned()),
                "Foton".to_owned(),
                2,
                vec![Filterable::pass_through(TextComponent::plain("Page"))],
                true,
            )
            .expect("written book fixture is valid"),
        );

        let mut container = ItemStack::new(&vanilla_items::CHEST);
        container.set(
            CONTAINER,
            ItemContainerContents::new(vec![
                None,
                Some(ItemStackTemplate::new(&vanilla_items::DIAMOND)),
            ])
            .expect("two container slots are valid"),
        );

        let original = vec![potion, sword, rocket, book, container];
        let encoded = encode_brewing_snapshot(&snapshot(original.clone()))
            .expect("diverse snapshot should encode");
        let decoded = decode_brewing_snapshot(&encoded).expect("diverse snapshot should decode");

        assert_eq!(decoded.items, original);
    }

    #[test]
    fn live_item_codec_rejects_duplicate_unknown_malformed_and_trailing_data() {
        init_vanilla_registry();
        let valid = encode_brewing_item(&ItemStack::new(&vanilla_items::STONE))
            .expect("stone should encode");

        let mut trailing = valid.clone();
        trailing.push(0);
        assert!(read_brewing_item(&trailing).is_none());
        assert!(read_brewing_item(&valid[..valid.len() - 1]).is_none());

        let item_name: Identifier = "minecraft:item_name"
            .parse()
            .expect("vanilla component identifier is valid");
        let item_name_id = REGISTRY
            .data_components
            .id_from_key(&item_name)
            .expect("item name component is registered");
        let mut duplicate = Vec::new();
        VarInt(1)
            .write(&mut duplicate)
            .expect("count should encode");
        VarInt(vanilla_items::STONE.id() as i32)
            .write(&mut duplicate)
            .expect("item id should encode");
        VarInt(0)
            .write(&mut duplicate)
            .expect("added count should encode");
        VarInt(2)
            .write(&mut duplicate)
            .expect("removed count should encode");
        for _ in 0..2 {
            VarInt(item_name_id as i32)
                .write(&mut duplicate)
                .expect("component id should encode");
        }
        duplicate.extend_from_slice(&u32::MAX.to_be_bytes());
        assert!(read_brewing_item(&duplicate).is_none());

        let mut unknown = Vec::new();
        VarInt(1).write(&mut unknown).expect("count should encode");
        VarInt(vanilla_items::STONE.id() as i32)
            .write(&mut unknown)
            .expect("item id should encode");
        VarInt(0)
            .write(&mut unknown)
            .expect("added count should encode");
        VarInt(1)
            .write(&mut unknown)
            .expect("removed count should encode");
        VarInt(i32::MAX)
            .write(&mut unknown)
            .expect("unknown component id should encode");
        unknown.extend_from_slice(&u32::MAX.to_be_bytes());
        assert!(read_brewing_item(&unknown).is_none());
    }

    #[test]
    fn live_item_codec_preflight_rejects_oversize_payloads() {
        assert!(brewing_item_payload_length_is_valid(MAX_BREWING_ITEM_BYTES));
        assert!(!brewing_item_payload_length_is_valid(
            MAX_BREWING_ITEM_BYTES + 1
        ));
        assert!(read_brewing_item(&vec![0; MAX_BREWING_ITEM_BYTES + 1]).is_none());
    }

    #[test]
    fn brewing_update_physics_controls_neighbor_notifications() {
        assert_eq!(brewing_update_flags(false), UpdateFlags::UPDATE_CLIENTS);
        assert_eq!(brewing_update_flags(true), UpdateFlags::UPDATE_ALL);
    }

    #[test]
    fn brewing_apply_requires_type_and_identity_unless_forced() {
        init_vanilla_registry();
        let brewing = vanilla_blocks::BREWING_STAND.default_state();
        let dirt = vanilla_blocks::DIRT.default_state();

        assert!(brewing_apply_matches(brewing, Some(7), brewing, 7, false));
        assert!(!brewing_apply_matches(brewing, Some(8), brewing, 7, false));
        assert!(!brewing_apply_matches(dirt, None, brewing, 7, false));
        assert!(brewing_apply_matches(brewing, Some(8), brewing, 7, true));
        assert!(brewing_apply_matches(dirt, None, brewing, 7, true));
        assert!(!brewing_apply_matches(brewing, Some(7), dirt, 7, true));
    }

    #[test]
    fn malformed_snapshot_item_binary_components_are_rejected() {
        init_vanilla_registry();
        let entry = REGISTRY
            .data_components
            .id_from_key(&Identifier::vanilla_static("custom_name"))
            .expect("registered");
        for body in [
            vec![8, 0, 4, b'x'],
            vec![10, 8, 0],
            vec![255],
            vec![8, 0, 1, b'x', 0],
        ] {
            let mut item = Vec::new();
            for value in [1, vanilla_items::STONE.id() as i32, 1, 0, entry as i32] {
                VarInt(value).write(&mut item).expect("header");
            }
            item.extend(body);
            item.extend_from_slice(&u32::MAX.to_be_bytes());
            let mut encoded =
                encode_brewing_snapshot(&snapshot(vec![ItemStack::empty(); 5])).expect("snapshot");
            replace_snapshot_blob(&mut encoded, 24, &item);
            assert!(decode_brewing_snapshot(&encoded).is_none());
        }
    }

    #[test]
    fn malformed_or_trailing_text_component_payload_is_rejected() {
        init_vanilla_registry();
        let encoded =
            encode_brewing_snapshot(&snapshot(vec![ItemStack::empty(); BREWING_STAND_SLOTS]))
                .expect("snapshot");
        let name_offset = custom_name_length_offset(&encoded);
        for replacement in [
            vec![8, 0, 2, b'x'],
            vec![8, 0, 1, b'x', 0],
            vec![10, 8, 0],
            vec![255],
        ] {
            let mut malformed = encoded.clone();
            replace_snapshot_blob(&mut malformed, name_offset, &replacement);
            assert!(decode_brewing_snapshot(&malformed).is_none());
        }
    }

    #[test]
    fn strict_snapshot_slot_decoder_preserves_valid_potion_metadata() {
        init_vanilla_registry();
        let mut potion = ItemStack::new(&vanilla_items::POTION);
        potion.set(
            POTION_CONTENTS,
            PotionContents::new(
                Some(RegistryReference::new(&vanilla_potions::WATER)),
                None,
                Vec::new(),
                None,
            ),
        );
        let mut items = vec![ItemStack::empty(); BREWING_STAND_SLOTS];
        items[0] = potion.clone();

        let encoded =
            encode_brewing_snapshot(&snapshot(items)).expect("test snapshot should encode");
        let decoded = decode_brewing_snapshot(&encoded).expect("valid snapshot should decode");

        assert_eq!(
            decoded.items[0].get(POTION_CONTENTS),
            potion.get(POTION_CONTENTS)
        );
    }

    #[test]
    fn brewing_payload_limit_includes_the_one_mebibyte_boundary() {
        assert!(brewing_payload_length_is_valid(MAX_BREWING_SNAPSHOT_BYTES));
        assert!(!brewing_payload_length_is_valid(
            MAX_BREWING_SNAPSHOT_BYTES + 1
        ));
    }

    #[test]
    fn rejected_force_apply_leaves_the_real_world_unchanged() {
        let world = loaded_bridge_world();
        let pos = BlockPos::new(3, 64, -2);
        let dirt = vanilla_blocks::DIRT.default_state();
        let brewing = vanilla_blocks::BREWING_STAND.default_state();
        assert!(world.set_block(pos, dirt, UpdateFlags::UPDATE_NONE));

        assert!(
            !apply_brewing_snapshot(&world, pos, brewing, snapshot(Vec::new()), true, false),
            "an invalid complete snapshot must reject"
        );
        assert_eq!(
            world.get_block_state(pos),
            dirt,
            "a rejected apply must not leave a replacement block behind"
        );
        assert!(
            world.get_block_entity(pos).is_none(),
            "a rejected apply must not create a brewing stand entity"
        );
    }

    #[test]
    fn real_apply_rejects_type_mismatch_without_force() {
        let world = loaded_bridge_world();
        let pos = BlockPos::new(4, 64, -2);
        let dirt = vanilla_blocks::DIRT.default_state();
        assert!(world.set_block(pos, dirt, UpdateFlags::UPDATE_NONE));

        assert!(!apply_brewing_snapshot(
            &world,
            pos,
            vanilla_blocks::BREWING_STAND.default_state(),
            snapshot(vec![ItemStack::empty(); BREWING_STAND_SLOTS]),
            false,
            false,
        ));
        assert_eq!(world.get_block_state(pos), dirt);
        assert!(world.get_block_entity(pos).is_none());
    }

    #[test]
    fn real_apply_rejects_a_stale_brewing_stand_identity() {
        let world = loaded_bridge_world();
        let pos = BlockPos::new(5, 64, -2);
        let state = vanilla_blocks::BREWING_STAND.default_state();
        assert!(world.set_block(pos, state, UpdateFlags::UPDATE_NONE));
        let identity = installed_brewing_identity(&world, pos);
        let mut stale_snapshot = snapshot(vec![ItemStack::empty(); BREWING_STAND_SLOTS]);
        stale_snapshot.identity = identity.wrapping_add(1);

        assert!(!apply_brewing_snapshot(
            &world,
            pos,
            state,
            stale_snapshot,
            false,
            false,
        ));
        assert_eq!(installed_brewing_identity(&world, pos), identity);
    }

    #[test]
    fn forced_real_apply_replaces_the_block_and_commits_the_snapshot() {
        let world = loaded_bridge_world();
        let pos = BlockPos::new(6, 64, -2);
        assert!(world.set_block(
            pos,
            vanilla_blocks::DIRT.default_state(),
            UpdateFlags::UPDATE_NONE,
        ));
        let mut items = vec![ItemStack::empty(); BREWING_STAND_SLOTS];
        items[0] = ItemStack::new(&vanilla_items::POTION);
        items[4] = ItemStack::new(&vanilla_items::BLAZE_POWDER);
        let expected = snapshot(items);

        assert!(apply_brewing_snapshot(
            &world,
            pos,
            vanilla_blocks::BREWING_STAND.default_state(),
            expected,
            true,
            false,
        ));
        assert_eq!(
            world.get_block_state(pos),
            vanilla_blocks::BREWING_STAND.default_state()
        );
        let entity = world
            .get_block_entity(pos)
            .expect("forced replacement must install a block entity");
        let brewing = entity
            .downcast_ref::<BrewingStandBlockEntity>()
            .expect("forced replacement must install a brewing stand entity");
        let actual = brewing.state_snapshot();
        assert!(actual.items[0].is(&vanilla_items::POTION));
        assert!(actual.items[4].is(&vanilla_items::BLAZE_POWDER));
        assert_eq!(actual.brew_time, 37);
        assert_eq!(actual.recipe_brew_time, 240);
        assert_eq!(actual.fuel, 11);
        assert_eq!(
            actual.custom_name,
            Some(TextComponent::plain("Bridge brewer"))
        );
    }

    #[test]
    fn forced_active_replacement_keeps_brewing_on_its_first_tick() {
        let world = loaded_bridge_world();
        let pos = BlockPos::new(6, 64, -3);
        assert!(world.set_block(
            pos,
            vanilla_blocks::DIRT.default_state(),
            UpdateFlags::UPDATE_NONE,
        ));
        let mut items = vec![ItemStack::empty(); BREWING_STAND_SLOTS];
        items[0] = potion_item(&vanilla_items::POTION, &vanilla_potions::WATER);
        items[3] = ItemStack::new(&vanilla_items::NETHER_WART);
        let mut active = snapshot(items);
        active.brew_time = 10;

        assert!(apply_brewing_snapshot(
            &world,
            pos,
            vanilla_blocks::BREWING_STAND.default_state(),
            active,
            true,
            false,
        ));
        let entity = world
            .get_block_entity(pos)
            .expect("forced replacement must install a block entity");
        let brewing = entity
            .downcast_ref::<BrewingStandBlockEntity>()
            .expect("forced replacement must install a brewing stand");

        brewing.tick(&world);

        assert_eq!(brewing.state_snapshot().brew_time, 9);
    }

    #[test]
    fn successful_real_apply_updates_the_placed_entity_state() {
        let world = loaded_bridge_world();
        let pos = BlockPos::new(7, 64, -2);
        let state = vanilla_blocks::BREWING_STAND.default_state();
        assert!(world.set_block(pos, state, UpdateFlags::UPDATE_NONE));
        let identity = installed_brewing_identity(&world, pos);
        let mut expected = snapshot(vec![ItemStack::empty(); BREWING_STAND_SLOTS]);
        expected.identity = identity;
        expected.fuel = 3;

        assert!(apply_brewing_snapshot(
            &world, pos, state, expected, false, false,
        ));
        assert_eq!(
            world.get_block_entity(pos).and_then(|entity| {
                entity
                    .downcast_ref::<BrewingStandBlockEntity>()
                    .map(|brewing| brewing.state_snapshot().fuel)
            }),
            Some(3),
            "the real placed entity must expose the committed state to its update path"
        );
    }

    #[test]
    fn forced_property_update_preserves_the_exact_entity_and_drops_no_inventory() {
        let world = loaded_bridge_world();
        let pos = BlockPos::new(8, 64, -2);
        let initial_state = vanilla_blocks::BREWING_STAND.default_state();
        let requested_state = initial_state.set_value(&BlockStateProperties::HAS_BOTTLE_0, true);
        assert!(world.set_block(pos, initial_state, UpdateFlags::UPDATE_NONE));
        let original = world
            .get_block_entity(pos)
            .expect("the brewing stand must own a block entity");
        let identity = original
            .downcast_ref::<BrewingStandBlockEntity>()
            .expect("the block entity must be a brewing stand")
            .identity();

        let mut old_items = vec![ItemStack::empty(); BREWING_STAND_SLOTS];
        old_items[0] = ItemStack::new(&vanilla_items::POTION);
        old_items[3] = ItemStack::new(&vanilla_items::NETHER_WART);
        let mut initial_snapshot = snapshot(old_items);
        initial_snapshot.identity = identity;
        assert!(apply_brewing_snapshot(
            &world,
            pos,
            initial_state,
            initial_snapshot,
            false,
            false,
        ));

        let mut requested_items = vec![ItemStack::empty(); BREWING_STAND_SLOTS];
        requested_items[1] = ItemStack::new(&vanilla_items::DIAMOND);
        requested_items[3] = ItemStack::new(&vanilla_items::REDSTONE);
        let mut requested = snapshot(requested_items.clone());
        requested.identity = identity.wrapping_add(1);

        assert!(apply_brewing_snapshot(
            &world,
            pos,
            requested_state,
            requested,
            true,
            true,
        ));

        let current = world
            .get_block_entity(pos)
            .expect("the property update must keep a block entity");
        assert!(
            Arc::ptr_eq(&current, &original),
            "same-block property updates must keep the exact current Arc"
        );
        assert_eq!(world.get_block_state(pos), requested_state);
        assert_eq!(
            current
                .downcast_ref::<BrewingStandBlockEntity>()
                .expect("the preserved entity must remain a brewing stand")
                .state_snapshot()
                .items,
            requested_items,
            "callbacks must observe and retain the complete requested inventory"
        );
        let dropped_items = world
            .accessible_entities()
            .into_iter()
            .filter(|entity| entity.downcast_ref::<ItemEntity>().is_some())
            .count();
        assert_eq!(
            dropped_items, 0,
            "a same-block property update must not run destructive pre-remove effects"
        );
    }

    #[test]
    fn genuine_brewing_stand_removal_still_drops_its_inventory() {
        let world = loaded_bridge_world();
        let pos = BlockPos::new(9, 64, -2);
        let brewing = vanilla_blocks::BREWING_STAND.default_state();
        assert!(world.set_block(pos, brewing, UpdateFlags::UPDATE_NONE));
        let identity = installed_brewing_identity(&world, pos);
        let mut items = vec![ItemStack::empty(); BREWING_STAND_SLOTS];
        items[0] = ItemStack::new(&vanilla_items::DIAMOND);
        let mut populated = snapshot(items);
        populated.identity = identity;
        assert!(apply_brewing_snapshot(
            &world, pos, brewing, populated, false, false,
        ));

        assert!(world.set_block(
            pos,
            vanilla_blocks::DIRT.default_state(),
            UpdateFlags::UPDATE_ALL,
        ));

        let dropped_diamonds = world
            .accessible_entities()
            .into_iter()
            .filter_map(|entity| {
                entity
                    .downcast_ref::<ItemEntity>()
                    .map(ItemEntity::get_item)
            })
            .filter(|item| item.is(&vanilla_items::DIAMOND))
            .count();
        assert_eq!(dropped_diamonds, 1);
    }

    fn lamp_receives_a_power_loss_callback(apply_physics: bool) -> bool {
        let world = loaded_bridge_world();
        let pos = BlockPos::new(8, 64, -2);
        let lamp_pos = pos.east();
        assert!(world.set_block(
            pos,
            vanilla_blocks::REDSTONE_BLOCK.default_state(),
            UpdateFlags::UPDATE_ALL,
        ));
        assert!(
            world.set_block(
                lamp_pos,
                vanilla_blocks::REDSTONE_LAMP
                    .default_state()
                    .set_value(&BlockStateProperties::LIT, true),
                UpdateFlags::UPDATE_NONE,
            )
        );

        assert!(apply_brewing_snapshot(
            &world,
            pos,
            vanilla_blocks::BREWING_STAND.default_state(),
            snapshot(vec![ItemStack::empty(); BREWING_STAND_SLOTS]),
            true,
            apply_physics,
        ));
        world.has_scheduled_block_tick(lamp_pos, &vanilla_blocks::REDSTONE_LAMP)
    }

    #[test]
    fn real_apply_physics_controls_neighbor_behavior() {
        assert!(!lamp_receives_a_power_loss_callback(false));
        assert!(lamp_receives_a_power_loss_callback(true));
    }

    #[test]
    fn brewing_bridge_bindings_match_the_java_contract() {
        let expected = [
            ("brewingStandSnapshot", "(Ljava/lang/String;III)[B"),
            ("brewingStandLiveItem", "(Ljava/lang/String;IIIJI)[B"),
            ("brewingStandSetLiveItem", "(Ljava/lang/String;IIIJI[B)Z"),
            (
                "brewingStandApply",
                "(Ljava/lang/String;IIILjava/lang/String;[BZZ)Z",
            ),
        ];
        let methods = bindings();

        for (name, signature) in expected {
            let method = methods
                .iter()
                .find(|method| method.name.to_str().ok() == Some(name));
            assert_eq!(
                method.and_then(|method| method.sig.to_str().ok()),
                Some(signature),
                "missing or mismatched native {name}"
            );
        }
    }
}

#[cfg(test)]
#[path = "task_one_tests.rs"]
mod task_one_tests;
