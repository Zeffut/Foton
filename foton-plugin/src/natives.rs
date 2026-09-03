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

use std::mem;
use std::ptr::null_mut;
use std::str::FromStr as _;
use std::sync::{Arc, OnceLock, Weak};
use std::thread::{self, ThreadId};

use foton_core::block_entity::entities::BannerBlockEntity;
use foton_core::block_entity::entities::JukeboxBlockEntity;
use foton_core::block_entity::entities::LecternBlockEntity;
use foton_core::block_entity::entities::SpawnerBlockEntity;
use foton_core::block_entity::entities::{SignBlockEntity, SignText};
use foton_core::boss_event::ServerBossEvent;
use foton_core::chunk::{
    chunk_request::{ChunkRequestHandle, ChunkTicketKind},
    status::ChunkStatus,
};
use foton_core::entity::conversion::{
    ConversionParams, ConversionReason, convert_to, replace_entity,
};
use foton_core::entity::damage::DamageSource;
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
use foton_core::entity::entities::objects::projectiles::ArrowEntity;
use foton_core::entity::entities::objects::projectiles::FireworkRocketEntity;
use foton_core::entity::entities::objects::projectiles::{
    DragonFireballEntity, LargeFireballEntity, SmallFireballEntity,
};
use foton_core::entity::entities::objects::vehicles::{BoatEntity, RaftEntity};
use foton_core::entity::neutral_mob::NeutralMob as _;
use foton_core::entity::neutral_mob::NeutralMob as _;
use foton_core::entity::projectile::Projectile as _;
use foton_core::entity::spellcaster_illager::{IllagerSpell, SpellcasterIllager};
use foton_core::entity::{
    Animal as _, Entity, EntitySpawnReason, ItemFrame as _, LivingEntity as _, LlamaVariant,
    Mob as _, MobEffectInstance, TamableAnimal as _, is_tamed, owner_uuid, set_owner_uuid,
    set_tamed, start_riding_entities,
};
use foton_core::inventory::container::Container;
use foton_core::inventory::equipment::EquipmentSlot;
use foton_core::inventory::lock::{ContainerLockGuard, ContainerRef};
use foton_core::inventory::menu::kinds::smithing;
use foton_core::permission::{PermissionExpr, PermissionKey, PermissionState};
use foton_core::player::Player;
use foton_core::player::connection::NetworkConnection;
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
use foton_registry::blocks::block_state_ext::BlockStateExt;
use foton_registry::data_components::components::{
    CustomModelData, ItemEnchantments, ItemLore, TooltipDisplay,
};
use foton_registry::data_components::vanilla_components::{
    CUSTOM_MODEL_DATA, CUSTOM_NAME, ENCHANTMENTS, FIREWORKS, FireworkExplosion,
    FireworkExplosionShape, Fireworks, ITEM_MODEL, ITEM_NAME, LORE, STORED_ENCHANTMENTS,
    TOOLTIP_DISPLAY, TOOLTIP_STYLE, UNBREAKABLE, WRITABLE_BOOK_CONTENT, WRITTEN_BOOK_CONTENT,
};
use foton_registry::entity_data::{Quaternionf, Vector3f};
use foton_registry::entity_variant::AxolotlVariant;
use foton_registry::item_stack::ItemStack;
use foton_registry::particle_type::ParticleData;
use foton_registry::recipe::{
    CraftingCategory, Ingredient, RecipeResult, ShapedRecipe, ShapelessRecipe,
};
use foton_registry::{
    REGISTRY, RegistryEntry as _, RegistryExt as _, TaggedRegistryExt as _, vanilla_entities,
    vanilla_items,
};
use foton_registry::{stat::Stat, vanilla_custom_stats};
use foton_utils::locks::{SyncMutex, SyncRwLock};
use foton_utils::nbt::{merge_nbt_compounds, parse_snbt_compound, to_canonical_snbt};
use foton_utils::serial::OptionalNbt;
use foton_utils::text::DisplayResolutor;
use foton_utils::types::UpdateFlags;
use foton_utils::types::{GameType, InteractionHand};
use foton_utils::{BlockPos, BlockStateId, WorldAabb};
use foton_utils::{Downcast as _, Identifier};
use glam::DVec3;
use jni::JNIEnv;
use jni::objects::{JByteArray, JClass, JDoubleArray, JObject, JObjectArray, JString};
use jni::sys::{
    jboolean, jbyte, jdouble, jdoubleArray, jfloat, jint, jlong, jobjectArray, jstring,
};
use rustc_hash::FxHashMap;
use simdnbt::owned::NbtCompound;
use text_components::{TextComponent, content::Content as TextContent};
use uuid::Uuid;

/// The server the natives answer about.
///
/// A `static` because a JNI native is a bare function pointer with nowhere to
/// put context. `Weak` because the plugin host must never be the reason a
/// server cannot shut down.
static SERVER: OnceLock<SyncRwLock<Weak<Server>>> = OnceLock::new();

/// Outstanding asynchronous Bukkit chunk requests, retained until Full status.
static CHUNK_REQUESTS: OnceLock<SyncMutex<FxHashMap<Uuid, ChunkRequestHandle>>> = OnceLock::new();

/// Non-persistent bars created through Bukkit, keyed by the opaque handle Java owns.
static BOSS_BARS: OnceLock<SyncRwLock<FxHashMap<Uuid, Arc<ServerBossEvent>>>> = OnceLock::new();

/// Bukkit updates header and footer independently, while the protocol packet carries both.
static PLAYER_TAB_LISTS: OnceLock<SyncRwLock<FxHashMap<Uuid, (String, String)>>> = OnceLock::new();

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
fn server() -> Option<Arc<Server>> {
    SERVER.get().and_then(|slot| slot.read().upgrade())
}

fn chunk_requests() -> &'static SyncMutex<FxHashMap<Uuid, ChunkRequestHandle>> {
    CHUNK_REQUESTS.get_or_init(|| SyncMutex::new(FxHashMap::default()))
}

fn boss_bars() -> &'static SyncRwLock<FxHashMap<Uuid, Arc<ServerBossEvent>>> {
    BOSS_BARS.get_or_init(|| SyncRwLock::new(FxHashMap::default()))
}

fn player_tab_lists() -> &'static SyncRwLock<FxHashMap<Uuid, (String, String)>> {
    PLAYER_TAB_LISTS.get_or_init(|| SyncRwLock::new(FxHashMap::default()))
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
fn player(env: &mut JNIEnv<'_>, uuid: &JString<'_>) -> Option<Arc<Player>> {
    let text: String = env.get_string(uuid).ok()?.into();
    let uuid = Uuid::parse_str(&text).ok()?;
    server()?.online_players().get_by_uuid(&uuid)
}

/// Returns a Java string, or Java's null when there is nothing to say.
fn to_java(env: &mut JNIEnv<'_>, value: Option<String>) -> jstring {
    value
        .and_then(|text| env.new_string(text).ok())
        .map_or_else(null_mut, JString::into_raw)
}

/// Returns a Java `String[]`, or null if the array could not be built.
fn string_array(env: &mut JNIEnv<'_>, values: &[String]) -> jobjectArray {
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

fn read_string_array(env: &mut JNIEnv<'_>, array: &JObjectArray<'_>) -> Option<Vec<String>> {
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
    let Some(enchantment) = REGISTRY.enchantments.by_key(&Identifier::vanilla(
        enchantment.to_str().unwrap_or_default().to_owned(),
    )) else {
        return 0;
    };
    let Some(item) = REGISTRY.items.by_key(&Identifier::vanilla(
        item.to_str().unwrap_or_default().to_owned(),
    )) else {
        return 0;
    };
    enchantment.can_enchant(item) as jboolean
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
        return std::ptr::null_mut();
    };
    let Ok(patch) = env.get_string(&patch) else {
        return std::ptr::null_mut();
    };
    let Ok(mut target) = parse_snbt_compound(existing.to_str().unwrap_or_default()) else {
        return std::ptr::null_mut();
    };
    let patch_text = patch.to_str().unwrap_or_default().trim();
    // Vanilla accepts a bare compound or an item id followed by legacy SNBT.
    let compound_text = if patch_text.starts_with('{') {
        patch_text
    } else {
        let Some(open) = patch_text.find('{') else {
            return std::ptr::null_mut();
        };
        if patch_text[..open].trim().parse::<Identifier>().is_err() {
            return std::ptr::null_mut();
        }
        &patch_text[open..]
    };
    let Ok(source) = parse_snbt_compound(compound_text) else {
        return std::ptr::null_mut();
    };
    merge_nbt_compounds(&mut target, &source);
    let value = to_canonical_snbt(&simdnbt::owned::NbtTag::Compound(target));
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
    let Ok(tag) = String::from(tag).parse::<Identifier>() else {
        return 0;
    };
    let Ok(value) = String::from(value).parse::<Identifier>() else {
        return 0;
    };
    let registry = String::from(registry);
    let answer = match registry.as_str() {
        "minecraft:items" | "items" => REGISTRY
            .items
            .by_key(&value)
            .is_some_and(|item| REGISTRY.items.is_in_tag(item, &tag)),
        "minecraft:blocks" | "blocks" => REGISTRY
            .blocks
            .by_key(&value)
            .is_some_and(|block| REGISTRY.blocks.is_in_tag(block, &tag)),
        _ => false,
    };
    answer as jboolean
}

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
    let Ok(tag) = String::from(tag).parse::<Identifier>() else {
        return null_mut();
    };
    let registry = String::from(registry);
    let values: Vec<String> = match registry.as_str() {
        "minecraft:items" | "items" => REGISTRY
            .items
            .iter_tag(&tag)
            .map(|entry| entry.key().to_string())
            .collect(),
        "minecraft:blocks" | "blocks" => REGISTRY
            .blocks
            .iter_tag(&tag)
            .map(|entry| entry.key().to_string())
            .collect(),
        _ => Vec::new(),
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
        .find(|entry| entry.last_known_name().eq_ignore_ascii_case(&needle))
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

extern "system" fn projectile_shooter(
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
    let owner = entity
        .as_ref()
        .downcast_ref::<LargeFireballEntity>()
        .and_then(|e| e.owner_uuid())
        .or_else(|| {
            entity
                .as_ref()
                .downcast_ref::<SmallFireballEntity>()
                .and_then(|e| e.owner_uuid())
        })
        .or_else(|| {
            entity
                .as_ref()
                .downcast_ref::<DragonFireballEntity>()
                .and_then(|e| e.owner_uuid())
        });
    to_java(&mut env, owner.map(|value| value.to_string()))
}

extern "system" fn set_projectile_shooter(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    owner: JString<'_>,
) {
    let Ok(text): Result<String, _> = env.get_string(&uuid).map(Into::into) else {
        return;
    };
    let Ok(id) = text.parse() else {
        return;
    };
    let Ok(owner_text): Result<String, _> = env.get_string(&owner).map(Into::into) else {
        return;
    };
    let owner_id = owner_text.parse().ok();
    let Some((_, entity)) = entity_by_uuid(&id) else {
        return;
    };
    if let Some(e) = entity.as_ref().downcast_ref::<LargeFireballEntity>() {
        e.set_owner_uuid(owner_id);
        return;
    }
    if let Some(e) = entity.as_ref().downcast_ref::<SmallFireballEntity>() {
        e.set_owner_uuid(owner_id);
        return;
    }
    if let Some(e) = entity.as_ref().downcast_ref::<DragonFireballEntity>() {
        e.set_owner_uuid(owner_id);
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
    let Some((world, entity)) = entity_by_uuid(&id) else {
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
        .map(|painting| painting.direction())
        .or_else(|| {
            entity
                .as_ref()
                .downcast_ref::<ItemFrameEntity>()
                .map(|frame| frame.direction())
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
        block_entity_type: foton_registry::vanilla_block_entity_types::SIGN.id() as i32,
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

fn entity_by_uuid(uuid: &Uuid) -> Option<(Arc<World>, foton_core::entity::SharedEntity)> {
    let server = server()?;
    for snapshot in server.worlds.snapshots() {
        let world = snapshot.world();
        if let Some(entity) = world.get_entity_by_uuid(uuid) {
            return Some((Arc::clone(world), entity));
        }
    }
    None
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
        .map_or(0, |orb| orb.value())
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
    entity
        .as_ref()
        .downcast_ref::<WolfEntity>()
        .is_some_and(|wolf| wolf.is_angry()) as jboolean
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
    let Some((world, entity)) = entity_by_uuid(&id) else {
        return;
    };
    let _ = world.remove_entity(entity.id());
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
        return false as jboolean;
    };
    let Ok(id) = Uuid::parse_str(text.to_str().unwrap_or_default()) else {
        return false as jboolean;
    };
    let Some((_world, entity)) = entity_by_uuid(&id) else {
        return false as jboolean;
    };
    let passengers = entity.passengers();
    if passengers.is_empty() {
        return false as jboolean;
    }
    for passenger in passengers {
        passenger.stop_riding();
    }
    true as jboolean
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
        return std::ptr::null_mut();
    };
    let Ok(id) = text
        .to_str()
        .ok()
        .and_then(|value| value.parse::<Uuid>().ok())
        .ok_or(())
    else {
        return std::ptr::null_mut();
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
    (was_riding && entity.vehicle().is_none()) as jboolean
}

extern "system" fn entity_passengers(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jstring {
    let Ok(text) = env.get_string(&uuid) else {
        return std::ptr::null_mut();
    };
    let Some(id) = text
        .to_str()
        .ok()
        .and_then(|value| value.parse::<Uuid>().ok())
    else {
        return std::ptr::null_mut();
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
        return false as jboolean;
    };
    let Ok(passenger) = env.get_string(&passenger) else {
        return false as jboolean;
    };
    let (Some(vehicle), Some(passenger)) = (
        vehicle.to_str().ok().and_then(|v| v.parse::<Uuid>().ok()),
        passenger.to_str().ok().and_then(|v| v.parse::<Uuid>().ok()),
    ) else {
        return false as jboolean;
    };
    let Some((_, vehicle)) = entity_by_uuid(&vehicle) else {
        return false as jboolean;
    };
    let Some((_, passenger)) = entity_by_uuid(&passenger) else {
        return false as jboolean;
    };
    start_riding_entities(&passenger, &vehicle) as jboolean
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
        return false as jboolean;
    };
    let Ok(id) = Uuid::parse_str(text.to_str().unwrap_or_default()) else {
        return false as jboolean;
    };
    entity_by_uuid(&id).is_some_and(|(_, entity)| entity.as_living_entity().is_some()) as jboolean
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
    entity_by_uuid(&id).is_some_and(|(_, entity)| {
        entity
            .as_living_entity()
            .is_some_and(|living| living.is_fall_flying())
    }) as jboolean
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
    entity_by_uuid(&id).is_some_and(|(_, entity)| is_tamed(entity.as_ref())) as jboolean
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
    villager.set_memory_global_pos(
        key_text.to_str().unwrap_or_default(),
        Some(foton_utils::GlobalPos::new(
            dimension,
            BlockPos::new(x, y, z),
        )),
    ) as jboolean
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
    if let Some((_, entity)) = entity_by_uuid(&id) {
        if let Some(villager) = entity.downcast_ref::<VillagerEntity>() {
            villager.set_memory_global_pos(key_text.to_str().unwrap_or_default(), None);
        }
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
            if let Ok(key) = key.parse() {
                if let Some(kind) = REGISTRY.villager_types.by_key(&key) {
                    villager.set_villager_type(kind);
                }
            }
        } else if let Some(villager) = entity.as_ref().downcast_ref::<ZombieVillagerEntity>() {
            if let Ok(key) = key.parse() {
                if let Some(kind) = REGISTRY.villager_types.by_key(&key) {
                    villager.set_villager_type(kind);
                }
            }
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
    if let Some((_, entity)) = entity_by_uuid(&id) {
        if let Some(villager) = entity.downcast_ref::<VillagerEntity>() {
            villager.set_level(level);
        }
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
                .map(|villager| villager.villager_level())
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
    if let Some((_, entity)) = entity_by_uuid(&id) {
        if let Some(villager) = entity.downcast_ref::<VillagerEntity>() {
            villager.set_villager_xp(experience);
        }
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
                .map(|villager| villager.villager_xp())
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
        let first = foton_registry::trading::ItemCost::new(cost_a.item(), cost_a.count());
        let second =
            cost_b.map(|stack| foton_registry::trading::ItemCost::new(stack.item(), stack.count()));
        parsed.push(foton_registry::trading::MerchantOffer::with_uses(
            first, second, result, uses, max_uses, 0, 0.05, demand,
        ));
    }
    if let Some((_, entity)) = entity_by_uuid(&id) {
        if let Some(villager) = entity.as_ref().downcast_ref::<VillagerEntity>() {
            villager.merchant().set_offers(parsed.into());
        }
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
    if let Some((_, entity)) = entity_by_uuid(&id) {
        if let Some(villager) = entity.as_ref().downcast_ref::<VillagerEntity>() {
            villager.merchant().clear_offers();
        }
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
    if let Some((_, entity)) = entity_by_uuid(&id) {
        if let Some(villager) = entity.downcast_ref::<ZombieVillagerEntity>() {
            villager.set_profession(profession);
        }
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
                .map(|cloud| cloud.radius())
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
                .map(|cloud| cloud.duration())
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
                .map(|cloud| cloud.wait_time())
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
                .map(|cloud| cloud.reapplication_delay())
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
                .map(|cloud| cloud.radius_per_tick())
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
                .map(|cloud| cloud.radius_on_use())
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
    let Ok(component) = Fireworks::new(power.clamp(0, u8::MAX as jint), explosions) else {
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
    entity_by_uuid(&id)
        .and_then(|(_, entity)| {
            entity
                .downcast_ref::<FoxEntity>()
                .map(|fox| fox.is_sitting())
        })
        .unwrap_or(false) as jboolean
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
    if let Some((_, entity)) = entity_by_uuid(&id) {
        if let Some(fox) = entity.downcast_ref::<FoxEntity>() {
            fox.set_sitting(value != 0);
        }
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
                .map(|slime| slime.cube_size())
        })
        .unwrap_or(0)
}

extern "system" fn set_slime_size(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    size: jint,
) {
    if size <= 0 {
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
    let Some(slime) = entity.as_ref().downcast_ref::<SlimeEntity>() else {
        return;
    };
    slime.set_cube_size(size, true);
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
        return false as jboolean;
    };
    let Some(id) = text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return false as jboolean;
    };
    entity_by_uuid(&id)
        .and_then(|(_, entity)| {
            entity
                .as_ref()
                .downcast_ref::<CreeperEntity>()
                .map(|creeper| creeper.is_powered())
        })
        .unwrap_or(false) as jboolean
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
    entity_by_uuid(&id)
        .and_then(|(_, entity)| {
            entity
                .downcast_ref::<GoatEntity>()
                .map(|goat| goat.has_left_horn())
        })
        .unwrap_or(false) as jboolean
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
    if let Some((_, entity)) = entity_by_uuid(&id) {
        if let Some(goat) = entity.downcast_ref::<GoatEntity>() {
            goat.set_left_horn(value != 0);
        }
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
    entity_by_uuid(&id)
        .and_then(|(_, entity)| {
            entity
                .downcast_ref::<GoatEntity>()
                .map(|goat| goat.has_right_horn())
        })
        .unwrap_or(false) as jboolean
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
    if let Some((_, entity)) = entity_by_uuid(&id) {
        if let Some(goat) = entity.downcast_ref::<GoatEntity>() {
            goat.set_right_horn(value != 0);
        }
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
    if let Some((_, entity)) = entity_by_uuid(&id) {
        if let Some(sheep) = entity.downcast_ref::<SheepEntity>() {
            sheep.set_color(value);
        }
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
    entity_by_uuid(&id)
        .and_then(|(_, entity)| {
            entity
                .downcast_ref::<SheepEntity>()
                .map(|sheep| sheep.is_sheared())
        })
        .unwrap_or(false) as jboolean
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
    if let Some((_, entity)) = entity_by_uuid(&id) {
        if let Some(sheep) = entity.downcast_ref::<SheepEntity>() {
            sheep.set_sheared(value != 0);
        }
    }
}

extern "system" fn goat_screaming(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jboolean {
    let Ok(text) = env.get_string(&uuid) else {
        return false as jboolean;
    };
    let Some(id) = text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return false as jboolean;
    };
    entity_by_uuid(&id)
        .and_then(|(_, entity)| {
            entity
                .as_ref()
                .downcast_ref::<GoatEntity>()
                .map(GoatEntity::is_screaming_goat)
        })
        .unwrap_or(false) as jboolean
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
    entity_by_uuid(&id).is_some_and(|(_, entity)| entity.entity_can_pick_up_loot()) as jboolean
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
        return false as jboolean;
    };
    let Some(id) = text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return false as jboolean;
    };
    entity_by_uuid(&id)
        .and_then(|(_, entity)| {
            entity
                .as_ref()
                .as_living_entity()
                .map(foton_core::entity::LivingEntity::is_baby)
        })
        .unwrap_or(false) as jboolean
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
    entity_by_uuid(&id).is_some_and(|(_, entity)| entity.is_ageable_age_locked()) as jboolean
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
        return false as jboolean;
    };
    let Some(id) = text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return false as jboolean;
    };
    entity_by_uuid(&id).is_some_and(|(_, entity)| {
        let Some(pig) = entity.as_ref().downcast_ref::<PigEntity>() else {
            return false;
        };
        let mut saddled = false;
        pig.with_equipment_slot(EquipmentSlot::Saddle, &mut |stack| {
            saddled = stack.is(&vanilla_items::SADDLE);
        });
        saddled
    }) as jboolean
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
            value = Some(describe_slot(item))
        });
    } else if let Some(mount) = entity.as_ref().downcast_ref::<NautilusEntity>() {
        mount.with_equipment_slot(equipment_slot, &mut |item| {
            value = Some(describe_slot(item))
        });
    } else if let Some(mount) = entity.as_ref().downcast_ref::<ZombieNautilusEntity>() {
        mount.with_equipment_slot(equipment_slot, &mut |item| {
            value = Some(describe_slot(item))
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
        value = Some(describe_slot(item))
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
        return false as jboolean;
    };
    let Some(id) = text.to_str().ok().and_then(|value| value.parse().ok()) else {
        return false as jboolean;
    };
    entity_by_uuid(&id)
        .and_then(|(_, entity)| entity.as_ref().has_carried_chest())
        .unwrap_or(false) as jboolean
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

fn panda_gene_name(gene: PandaGene) -> &'static str {
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
    foton_core::behavior::blocks::vegetation::tree_grower::generate_tree(
        &world,
        BlockPos::new(x, y, z),
        key,
    ) as jboolean
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
                .map(|phantom| phantom.phantom_size())
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
    entity_by_uuid(&id)
        .and_then(|(_, entity)| {
            entity
                .as_ref()
                .as_raider()
                .map(|raider| raider.is_patrol_leader())
        })
        .unwrap_or(false) as jboolean
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
    let Some(animal) = entity.as_ref().as_animal() else {
        return 0;
    };
    (animal.in_love_time() > 0).into()
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
    let Some(animal) = entity.as_ref().as_animal() else {
        return;
    };
    animal.set_in_love_time(if breed != 0 { 600 } else { 0 });
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
    remaining.max(0).min(i32::MAX as i64) as jint
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
    entity_by_uuid(&id)
        .and_then(|(_, entity)| {
            entity
                .downcast_ref::<WolfEntity>()
                .map(|wolf| wolf.is_in_sitting_pose())
        })
        .unwrap_or(false) as jboolean
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
    if let Some((_, entity)) = entity_by_uuid(&id) {
        if let Some(wolf) = entity.downcast_ref::<WolfEntity>() {
            wolf.set_in_sitting_pose(value != 0);
        }
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
    {
        if let Some((_, entity)) = entity_by_uuid(&id)
            && let Some(wolf) = entity.downcast_ref::<WolfEntity>()
        {
            wolf.set_collar_color(color);
        }
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
                .map(|horse| horse.markings())
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

extern "system" fn spawn_particle(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    world_name: JString<'_>,
    particle: JString<'_>,
    x: jdouble,
    y: jdouble,
    z: jdouble,
    count: jint,
    ox: jdouble,
    oy: jdouble,
    oz: jdouble,
    speed: jdouble,
) {
    let Ok(world_text): Result<String, _> = env.get_string(&world_name).map(Into::into) else {
        return;
    };
    let Ok(particle_text): Result<String, _> = env.get_string(&particle).map(Into::into) else {
        return;
    };
    let Ok(world_key) = world_text.parse::<Identifier>() else {
        return;
    };
    let Ok(particle_key) = particle_text.parse::<Identifier>() else {
        return;
    };
    let Some(world) = server().and_then(|server| server.worlds.get_owned(&world_key)) else {
        return;
    };
    let Some(particle_type) = REGISTRY.particle_types.by_key(&particle_key) else {
        return;
    };
    world.send_particles(
        ParticleData::simple(particle_type),
        DVec3::new(x, y, z),
        count,
        DVec3::new(ox, oy, oz),
        speed,
    );
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

fn boat_variant(name: &str) -> Option<(foton_registry::entity_type::EntityTypeRef, bool)> {
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

extern "system" fn set_block_display_brightness(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    block: jint,
    sky: jint,
) {
    let Ok(text): Result<String, _> = env.get_string(&uuid).map(Into::into) else {
        return;
    };
    let Ok(id) = text.parse() else {
        return;
    };
    if let Some((_, entity)) = entity_by_uuid(&id)
        && let Some(display) = entity.as_ref().downcast_ref::<BlockDisplayEntity>()
    {
        display.set_brightness(block, sky);
    }
}

extern "system" fn set_block_display_view_range(
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
    if let Some((_, entity)) = entity_by_uuid(&id)
        && let Some(display) = entity.as_ref().downcast_ref::<BlockDisplayEntity>()
    {
        display.set_view_range(value);
    }
}

extern "system" fn set_block_display_shadow_radius(
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
    if let Some((_, entity)) = entity_by_uuid(&id)
        && let Some(display) = entity.as_ref().downcast_ref::<BlockDisplayEntity>()
    {
        display.set_shadow_radius(value);
    }
}

#[allow(clippy::too_many_arguments)]
extern "system" fn set_block_display_transformation(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    tx: jfloat,
    ty: jfloat,
    tz: jfloat,
    sx: jfloat,
    sy: jfloat,
    sz: jfloat,
    lx: jfloat,
    ly: jfloat,
    lz: jfloat,
    lw: jfloat,
    rx: jfloat,
    ry: jfloat,
    rz: jfloat,
    rw: jfloat,
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
    let Some(display) = entity.as_ref().downcast_ref::<BlockDisplayEntity>() else {
        return;
    };
    let mut data = display.entity_data().lock();
    data.display_mut()
        .translation
        .set(Vector3f::new(tx, ty, tz));
    data.display_mut().scale.set(Vector3f::new(sx, sy, sz));
    data.display_mut()
        .left_rotation
        .set(Quaternionf::new(lx, ly, lz, lw));
    data.display_mut()
        .right_rotation
        .set(Quaternionf::new(rx, ry, rz, rw));
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
    let Some((_world, entity)) = entity_by_uuid(&id) else {
        return null_mut();
    };
    let reason = entity
        .base()
        .spawn_reason()
        .map_or("DEFAULT", |reason| match reason {
            EntitySpawnReason::Natural => "NATURAL",
            EntitySpawnReason::ChunkGeneration => "DEFAULT",
            EntitySpawnReason::Spawner | EntitySpawnReason::TrialSpawner => "SPAWNER",
            EntitySpawnReason::Breeding => "BREEDING",
            EntitySpawnReason::Command => "COMMAND",
            _ => "CUSTOM",
        });
    to_java(&mut env, Some(reason.to_owned()))
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
    to_position(
        &mut env,
        entity_by_uuid(&id).map(|(_, entity)| {
            let p = entity.position();
            [p.x, p.y, p.z, 0.0, 0.0]
        }),
    )
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
    entity_by_uuid(&id).is_some_and(|(_, entity)| entity.has_glowing_tag()) as jboolean
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
    entity_by_uuid(&id).is_some_and(|(_, entity)| entity.is_invulnerable()) as jboolean
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
    entity_by_uuid(&id).is_some_and(|(_, entity)| entity.on_ground()) as jboolean
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
    entity_by_uuid(&id).is_some_and(|(_, entity)| entity.is_in_water()) as jboolean
}

extern "system" fn entity_invisible(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jboolean {
    let Ok(text) = env.get_string(&uuid) else {
        return false as jboolean;
    };
    let Some(id) = text.to_str().ok().and_then(|v| Uuid::parse_str(v).ok()) else {
        return false as jboolean;
    };
    entity_by_uuid(&id).is_some_and(|(_, entity)| entity.is_invisible()) as jboolean
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
        .and_then(|(_, entity)| {
            entity
                .as_living_entity()
                .map(|living| living.ticks_frozen())
        })
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
    if let Some((_, entity)) = entity_by_uuid(&id) {
        if let Some(living) = entity.as_living_entity() {
            living.set_ticks_frozen(ticks.max(0));
        }
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
            .map_or(0, |living| living.no_damage_ticks())
    })
}

extern "system" fn entity_sprinting(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jboolean {
    let Ok(text) = env.get_string(&uuid) else {
        return false as jboolean;
    };
    let Some(id) = text.to_str().ok().and_then(|v| Uuid::parse_str(v).ok()) else {
        return false as jboolean;
    };
    entity_by_uuid(&id).map_or(false, |(_, entity)| {
        entity
            .as_ref()
            .as_living_entity()
            .is_some_and(|living| living.is_sprinting())
    }) as jboolean
}

extern "system" fn entity_swimming(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jboolean {
    let Ok(text) = env.get_string(&uuid) else {
        return false as jboolean;
    };
    let Some(id) = text.to_str().ok().and_then(|v| Uuid::parse_str(v).ok()) else {
        return false as jboolean;
    };
    entity_by_uuid(&id).map_or(false, |(_, entity)| entity.is_swimming()) as jboolean
}

extern "system" fn entity_is_using_item(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jboolean {
    let Ok(text) = env.get_string(&uuid) else {
        return false as jboolean;
    };
    let Some(id) = text.to_str().ok().and_then(|v| Uuid::parse_str(v).ok()) else {
        return false as jboolean;
    };
    entity_by_uuid(&id).map_or(false, |(_, entity)| {
        entity
            .as_ref()
            .as_living_entity()
            .is_some_and(|living| living.is_using_item())
    }) as jboolean
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
    if let Some((_, entity)) = entity_by_uuid(&id) {
        if let Some(living) = entity.as_ref().as_living_entity() {
            living.living_base().stop_using_item();
        }
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
    let p = entity.position();
    let bounds = WorldAabb::new(p.x - x, p.y - y, p.z - z, p.x + x, p.y + y, p.z + z);
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
    (!entity_world
        .entity_tracker()
        .is_hidden_for_player(entity.id(), player.id())) as jboolean
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
    let owner = entity_by_uuid(&id).and_then(|(_, entity)| entity.projectile_owner_uuid());
    to_java(&mut env, owner.map(|value| value.to_string()))
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
    entity_by_uuid(&id).is_some_and(|(_, entity)| entity.is_persistent()) as jboolean
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
    entity_by_uuid(&id).map_or(false, |(_, entity)| {
        entity
            .as_mob()
            .is_some_and(|mob| !mob.is_persistence_required())
    }) as jboolean
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
    if let Some((_, entity)) = entity_by_uuid(&id) {
        if let Some(mob) = entity.as_mob() {
            mob.set_persistence_required_value(remove == 0);
        }
    }
}

fn equipment_slot_from_index(slot: jint) -> Option<EquipmentSlot> {
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
    if chance.is_finite() && chance >= 0.0 {
        if let Some((_, entity)) = entity_by_uuid(&id) {
            if let Some(mob) = entity.as_mob() {
                mob.set_drop_chance(slot, chance);
            }
        }
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
            .and_then(|arrow| arrow.ammo_potion_contents())
            .and_then(|contents| {
                contents
                    .potion()
                    .map(|potion| potion.value().key.to_string())
            })
    });
    to_java(&mut env, value)
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
        .effects()
        .into_iter()
        .map(|effect| {
            format!(
                "{}|{}|{}|{}|{}|{}",
                effect.effect().key.path,
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

extern "system" fn set_entity_projectile_owner(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    owner: JString<'_>,
) -> jboolean {
    let Ok(uuid_text) = env.get_string(&uuid) else {
        return 0;
    };
    let Ok(entity_id) = Uuid::parse_str(uuid_text.to_str().unwrap_or_default()) else {
        return 0;
    };
    let Ok(owner_text) = env.get_string(&owner) else {
        return 0;
    };
    let owner = if owner_text.to_str().unwrap_or_default().is_empty() {
        None
    } else {
        Uuid::parse_str(owner_text.to_str().unwrap_or_default()).ok()
    };
    let Some((_, entity)) = entity_by_uuid(&entity_id) else {
        return 0;
    };
    let Some(projectile) = entity.as_projectile() else {
        return 0;
    };
    projectile.set_owner_uuid(owner);
    1
}

fn mutate_villager_offer(
    uuid: uuid::Uuid,
    index: usize,
    mutate: impl FnOnce(&mut foton_registry::trading::MerchantOffer),
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
    mutate_villager_offer(uuid, index, |offer| offer.set_uses(uses)) as jboolean
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
    mutate_villager_offer(uuid, index, |offer| offer.set_max_uses(max_uses)) as jboolean
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
    mutate_villager_offer(uuid, index, |offer| offer.set_demand(demand)) as jboolean
}

extern "system" fn entity_merchant_recipes(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jobjectArray {
    let Ok(text) = env.get_string(&uuid) else {
        return std::ptr::null_mut();
    };
    let Ok(uuid) = text
        .to_str()
        .ok()
        .and_then(|value| uuid::Uuid::parse_str(value).ok())
        .ok_or(())
    else {
        return std::ptr::null_mut();
    };
    let Some(server) = server() else {
        return std::ptr::null_mut();
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
    std::ptr::null_mut()
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
                .map(|golem| if golem.is_player_created() { 1 } else { 0 })
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
    if let Some((_, entity)) = entity_by_uuid(&id) {
        if let Some(golem) = entity.downcast_ref::<IronGolemEntity>() {
            golem.set_player_created(value != 0);
        }
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
    entity_by_uuid(&id)
        .map(|(_, entity)| {
            if entity.is_custom_name_visible() {
                1
            } else {
                0
            }
        })
        .unwrap_or(0)
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
    if let Some((_, entity)) = entity_by_uuid(&id) {
        if let Some(player) = entity.as_player() {
            player.send_message(&text_components::TextComponent::plain(String::from(
                message_text,
            )));
        }
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

extern "system" fn first_played(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jlong {
    global_player_timestamp(env, uuid, true)
}

extern "system" fn last_played(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jlong {
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
    world(&mut env, &world_name).is_some_and(|value| {
        value
            .chunk_map
            .world_gen_context
            .generator
            .structure_generator()
            .is_some()
    }) as jboolean
}

extern "system" fn world_allow_monsters(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    world_name: JString<'_>,
) -> jboolean {
    world(&mut env, &world_name).is_some_and(|value| value.allow_monsters()) as jboolean
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
    world(&mut env, &world_name).is_some_and(|value| value.allow_animals()) as jboolean
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
    world(&mut env, &world_name).is_some_and(|value| value.is_pvp()) as jboolean
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
    entity.set_fall_distance(f64::from(distance.max(0.0)));
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

extern "system" fn player_attribute(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    attribute: JString<'_>,
) -> jstring {
    let Ok(uuid_text): Result<String, _> = env.get_string(&uuid).map(Into::into) else {
        return null_mut();
    };
    let Ok(name): Result<String, _> = env.get_string(&attribute).map(Into::into) else {
        return null_mut();
    };
    let Some(id) = Uuid::parse_str(&uuid_text).ok() else {
        return null_mut();
    };
    let key_name = name
        .strip_prefix("GENERIC_")
        .or_else(|| name.strip_prefix("PLAYER_"))
        .unwrap_or(&name)
        .to_ascii_lowercase();
    let Ok(key) = Identifier::from_str(&key_name) else {
        return null_mut();
    };
    let Some((_, entity)) = entity_by_uuid(&id) else {
        return null_mut();
    };
    let Some(living) = entity.as_living_entity() else {
        return null_mut();
    };
    let Some(attribute_ref) = foton_registry::REGISTRY.attributes.by_key(&key) else {
        return null_mut();
    };
    let attrs = living.attributes().lock();
    let Some(base) = attrs.get_base_value(attribute_ref) else {
        return null_mut();
    };
    let Some(value) = attrs.get_value(attribute_ref) else {
        return null_mut();
    };
    to_java(&mut env, Some(format!("{base}|{value}")))
}

fn attribute_ref_from_name(name: &str) -> Option<foton_registry::attribute::AttributeRef> {
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

extern "system" fn add_attribute_modifier(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    attribute: JString<'_>,
    modifier_id: JString<'_>,
    amount: jdouble,
    operation: JString<'_>,
) -> jboolean {
    let Ok(uuid_text): Result<String, _> = env.get_string(&uuid).map(Into::into) else {
        return 0;
    };
    let Ok(attribute_name): Result<String, _> = env.get_string(&attribute).map(Into::into) else {
        return 0;
    };
    let Ok(id_text): Result<String, _> = env.get_string(&modifier_id).map(Into::into) else {
        return 0;
    };
    let Ok(operation): Result<String, _> = env.get_string(&operation).map(Into::into) else {
        return 0;
    };
    let Some(id) = Uuid::parse_str(&uuid_text).ok() else {
        return 0;
    };
    let Some(attribute) = attribute_ref_from_name(&attribute_name) else {
        return 0;
    };
    let Ok(modifier_uuid) = Uuid::parse_str(&id_text) else {
        return 0;
    };
    let Ok(modifier_id) = Identifier::from_str(&format!("plugin:{modifier_uuid}")) else {
        return 0;
    };
    let operation = match operation.as_str() {
        "ADD_NUMBER" => foton_core::entity::attribute::AttributeModifierOperation::AddValue,
        "ADD_SCALAR" => {
            foton_core::entity::attribute::AttributeModifierOperation::AddMultipliedBase
        }
        "MULTIPLY_SCALAR_1" => {
            foton_core::entity::attribute::AttributeModifierOperation::AddMultipliedTotal
        }
        _ => return 0,
    };
    let Some((_, entity)) = entity_by_uuid(&id) else {
        return 0;
    };
    let Some(living) = entity.as_living_entity() else {
        return 0;
    };
    living.attributes().lock().add_modifier(
        attribute,
        foton_core::entity::attribute::AttributeModifier {
            id: modifier_id,
            amount,
            operation,
        },
        true,
    ) as jboolean
}

extern "system" fn remove_attribute_modifier(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    attribute: JString<'_>,
    modifier_id: JString<'_>,
) -> jboolean {
    let Ok(uuid_text): Result<String, _> = env.get_string(&uuid).map(Into::into) else {
        return 0;
    };
    let Ok(attribute_name): Result<String, _> = env.get_string(&attribute).map(Into::into) else {
        return 0;
    };
    let Ok(id_text): Result<String, _> = env.get_string(&modifier_id).map(Into::into) else {
        return 0;
    };
    let Some(id) = Uuid::parse_str(&uuid_text).ok() else {
        return 0;
    };
    let Some(attribute) = attribute_ref_from_name(&attribute_name) else {
        return 0;
    };
    let Ok(modifier_uuid) = Uuid::parse_str(&id_text) else {
        return 0;
    };
    let Ok(modifier_id) = Identifier::from_str(&format!("plugin:{modifier_uuid}")) else {
        return 0;
    };
    let Some((_, entity)) = entity_by_uuid(&id) else {
        return 0;
    };
    let Some(living) = entity.as_living_entity() else {
        return 0;
    };
    living
        .attributes()
        .lock()
        .remove_modifier(attribute, &modifier_id) as jboolean
}

extern "system" fn attribute_modifiers(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    attribute: JString<'_>,
) -> jobjectArray {
    let Ok(uuid_text): Result<String, _> = env.get_string(&uuid).map(Into::into) else {
        return null_mut();
    };
    let Ok(attribute_name): Result<String, _> = env.get_string(&attribute).map(Into::into) else {
        return null_mut();
    };
    let Some(id) = Uuid::parse_str(&uuid_text).ok() else {
        return null_mut();
    };
    let Some(attribute) = attribute_ref_from_name(&attribute_name) else {
        return null_mut();
    };
    let Some((_, entity)) = entity_by_uuid(&id) else {
        return null_mut();
    };
    let Some(living) = entity.as_living_entity() else {
        return null_mut();
    };
    let attrs = living.attributes().lock();
    let Some(instance) = attrs.get_instance(attribute) else {
        return null_mut();
    };
    let values: Vec<String> = instance
        .modifiers()
        .iter()
        .map(|modifier| {
            let operation = match modifier.operation {
                foton_core::entity::attribute::AttributeModifierOperation::AddValue => "ADD_NUMBER",
                foton_core::entity::attribute::AttributeModifierOperation::AddMultipliedBase => {
                    "ADD_SCALAR"
                }
                foton_core::entity::attribute::AttributeModifierOperation::AddMultipliedTotal => {
                    "MULTIPLY_SCALAR_1"
                }
            };
            format!(
                "{}|{}|{}|{operation}",
                modifier.id.path, modifier.id.path, modifier.amount
            )
        })
        .collect();
    string_array(&mut env, &values)
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
        "PROTECTED_FROM_DEATH" => {
            Some(foton_utils::entity_events::EntityStatus::ProtectedFromDeath)
        }
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
        return std::ptr::null_mut();
    };
    let Some(id) = value
        .to_str()
        .ok()
        .and_then(|text| text.parse::<Identifier>().ok())
    else {
        return std::ptr::null_mut();
    };
    let Some(advancement) = REGISTRY.advancements.by_key(&id) else {
        return std::ptr::null_mut();
    };
    let Some(display) = advancement.display.as_ref() else {
        return std::ptr::null_mut();
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
        return std::ptr::null_mut();
    };
    let Ok(key) = key.to_str() else {
        return std::ptr::null_mut();
    };
    let Ok(key) = key.parse::<Identifier>() else {
        return std::ptr::null_mut();
    };
    let Some(advancement) = REGISTRY.advancements.by_key(&key) else {
        return std::ptr::null_mut();
    };
    let values = advancement
        .criteria
        .iter()
        .map(|criterion| criterion.name.to_owned())
        .collect::<Vec<_>>();
    string_array(&mut env, &values)
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
        .or_insert_with(|| (String::new(), String::new()));
    if let Some(header) = header {
        let Ok(value) = env.get_string(&header) else {
            return;
        };
        entry.0 = String::from(value);
    }
    if let Some(footer) = footer {
        let Ok(value) = env.get_string(&footer) else {
            return;
        };
        entry.1 = String::from(value);
    }
    let header: TextComponent = entry.0.clone().into();
    let footer: TextComponent = entry.1.clone().into();
    player.send_packet(CTabList::new(&header, &footer, player.as_ref()));
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
    if progress.is_finite() && (0.0..=1.0).contains(&progress) {
        if let Some(player) = player(&mut env, &uuid) {
            player.experience.lock().set_progress(progress);
        }
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
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
fn hex_decode(value: &str) -> Vec<u8> {
    (0..value.len())
        .step_by(2)
        .filter_map(|index| u8::from_str_radix(value.get(index..index + 2)?, 16).ok())
        .collect()
}

pub(crate) fn describe_slot(stack: &ItemStack) -> String {
    if stack.is_empty() {
        return String::new();
    }
    let mut value = format!("{} {}", stack.item().key, stack.count());
    if let Some(opaque) = stack.opaque_nbt() {
        value.push('\u{1d}');
        value.push_str("nbthex=");
        value.push_str(&hex_encode(opaque.as_bytes()));
    }
    if let Some(name) = stack.get(CUSTOM_NAME) {
        value.push('\u{1d}');
        value.push_str("namehex=");
        for byte in name.to_string().as_bytes() {
            value.push_str(&format!("{byte:02x}"));
        }
    }
    if let Some(lore) = stack.get(LORE) {
        for line in lore.lines() {
            value.push('\u{1d}');
            value.push_str("lorehex=");
            for byte in line.to_string().as_bytes() {
                value.push_str(&format!("{byte:02x}"));
            }
        }
    }
    if stack.has(UNBREAKABLE) {
        value.push('\u{1d}');
        value.push_str("unbreakable");
    }
    if let Some(model) = stack.get(ITEM_MODEL) {
        value.push('\u{1d}');
        value.push_str(&format!(
            "itemmodelhex={}",
            hex_encode(model.to_string().as_bytes())
        ));
    }
    if let Some(style) = stack.get(TOOLTIP_STYLE) {
        value.push('\u{1d}');
        value.push_str(&format!(
            "tooltipstylehex={}",
            hex_encode(style.to_string().as_bytes())
        ));
    }
    if let Some(display) = stack.get(TOOLTIP_DISPLAY) {
        if display.hide_tooltip {
            value.push('\u{1d}');
            value.push_str("hidetooltip");
        }
    }
    if let Some(model) = stack.get(CUSTOM_MODEL_DATA) {
        for float in model.floats() {
            value.push('\u{1d}');
            value.push_str(&format!("modelfloat={float}"));
        }
        for flag in model.flags() {
            value.push('\u{1d}');
            value.push_str(&format!("modelflag={flag}"));
        }
        for string in model.strings() {
            value.push('\u{1d}');
            value.push_str(&format!("modelstrhex={}", hex_encode(string.as_bytes())));
        }
        for color in model.colors() {
            value.push('\u{1d}');
            value.push_str(&format!("modelcolor={color}"));
        }
    }
    if let Some(enchantments) = stack.get(ENCHANTMENTS) {
        for (key, level) in enchantments.iter() {
            value.push('\u{1d}');
            value.push_str(&format!(
                "enchhex={}:{}",
                hex_encode(key.to_string().as_bytes()),
                level
            ));
        }
    }
    if let Some(enchantments) = stack.get(STORED_ENCHANTMENTS) {
        for (key, level) in enchantments.iter() {
            value.push('\u{1d}');
            value.push_str(&format!(
                "storedenchhex={}:{}",
                hex_encode(key.to_string().as_bytes()),
                level
            ));
        }
    }
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
pub(crate) fn parse_slot(text: &str) -> Option<ItemStack> {
    let mut encoded = text.trim().split('\u{1d}');
    let text = encoded.next().unwrap_or_default();
    let metadata = encoded.collect::<Vec<_>>();
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
    {
        if let Ok(raw) = String::from_utf8(hex_decode(encoded)) {
            stack.set_opaque_nbt(Some(raw));
        }
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
            use text_components::TextComponent;
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
    if !lore_lines.is_empty() {
        if let Ok(lore) = ItemLore::new(lore_lines) {
            stack.set(LORE, lore);
        }
    }
    if metadata.iter().any(|value| *value == "unbreakable") {
        stack.set(UNBREAKABLE, ());
    }
    if metadata.iter().any(|value| *value == "hidetooltip") {
        stack.set(TOOLTIP_DISPLAY, TooltipDisplay::new(true));
    }
    if let Some(encoded) = metadata
        .iter()
        .find_map(|value| value.strip_prefix("tooltipstylehex="))
    {
        if let Ok(style) = String::from_utf8(hex_decode(encoded)) {
            if let Ok(style) = style.parse() {
                stack.set(TOOLTIP_STYLE, style);
            }
        }
    }
    if let Some(encoded) = metadata
        .iter()
        .find_map(|value| value.strip_prefix("itemmodelhex="))
    {
        if let Ok(model) = String::from_utf8(hex_decode(encoded)) {
            if let Ok(model) = model.parse() {
                stack.set(ITEM_MODEL, model);
            }
        }
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
    {
        if model.is_finite() {
            stack.set(
                CUSTOM_MODEL_DATA,
                CustomModelData::new(vec![model], Vec::new(), Vec::new(), Vec::new()),
            );
        }
    }
    let mut enchantments = ItemEnchantments::empty();
    for encoded in metadata
        .iter()
        .filter_map(|value| value.strip_prefix("enchhex="))
    {
        if let Some((key, level)) = encoded.rsplit_once(':') {
            let bytes = hex_decode(key);
            if let (Ok(name), Ok(level)) = (String::from_utf8(bytes), level.parse::<u32>()) {
                if let Ok(key) = name.parse() {
                    enchantments.set(key, level);
                }
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
            if let (Ok(name), Ok(level)) = (String::from_utf8(bytes), level.parse::<u32>()) {
                if let Ok(key) = name.parse() {
                    stored.set(key, level);
                }
            }
        }
    }
    if !stored.is_empty() {
        stack.set(STORED_ENCHANTMENTS, stored);
    }
    if let Some(effects) = metadata.iter().find(|value| {
        !value.starts_with("damage=")
            && !value.starts_with("namehex=")
            && !value.starts_with("lorehex=")
            && !value.starts_with("enchhex=")
            && !value.starts_with("storedenchhex=")
            && !value.starts_with("model=")
            && !value.starts_with("modelfloat=")
            && !value.starts_with("modelflag=")
            && !value.starts_with("modelstrhex=")
            && !value.starts_with("modelcolor=")
            && !value.starts_with("itemmodelhex=")
            && !value.starts_with("tooltipstylehex=")
            && **value != "hidetooltip"
            && **value != "unbreakable"
    }) {
        use foton_registry::data_components::components::PotionContents;
        use foton_registry::data_components::vanilla_components::POTION_CONTENTS;
        use foton_registry::mob_effect::instance::MobEffectInstance;
        let mut custom = Vec::new();
        for field in effects.split(';') {
            let mut parts = field.split(',');
            let (Some(name), Some(duration), Some(amplifier)) =
                (parts.next(), parts.next(), parts.next())
            else {
                continue;
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
            custom.push(MobEffectInstance::simple(effect, duration, amplifier));
        }
        if !custom.is_empty() {
            stack.set(
                POTION_CONTENTS,
                PotionContents::new(None, None, custom, None),
            );
        }
    }
    Some(stack)
}

/// A block state as `minecraft:name[facing=north]`, the way `/setblock` writes it.
fn describe_state(state: BlockStateId) -> Option<String> {
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
    let stat = match statistic.to_str().ok() {
        Some("TIME_SINCE_REST") => Stat::custom(&vanilla_custom_stats::TIME_SINCE_REST),
        Some("JUMP") => Stat::custom(&vanilla_custom_stats::JUMP),
        _ => return 0,
    };
    player.stat_value(stat)
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
    server().map_or(0, |server| {
        server.offline_statistic(uuid, statistic_text.to_str().unwrap_or_default())
    })
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

/// `foton.Native.blockState`
extern "system" fn biome_key(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    world_name: JString<'_>,
    x: jint,
    y: jint,
    z: jint,
) -> jstring {
    let Ok(name) = env.get_string(&world_name) else {
        return null_mut();
    };
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
        return std::ptr::null_mut();
    };
    let state = world.get_block_state(BlockPos::new(x, y, z));
    let value = match state.get_block().config.push_reaction {
        foton_registry::blocks::behavior::PushReaction::Normal => "NORMAL",
        foton_registry::blocks::behavior::PushReaction::Destroy => "BREAK",
        foton_registry::blocks::behavior::PushReaction::Block => "BLOCK",
        foton_registry::blocks::behavior::PushReaction::Ignore => "IGNORE",
        foton_registry::blocks::behavior::PushReaction::PushOnly => "PUSH_ONLY",
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
    removed as jboolean
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
        return false as jboolean;
    };
    let Some(result_key) = env
        .get_string(&result)
        .ok()
        .and_then(|v| v.to_str().ok()?.parse().ok())
    else {
        return false as jboolean;
    };
    let Some(result_item) = REGISTRY.items.by_key(&result_key) else {
        return false as jboolean;
    };
    if count <= 0 {
        return false as jboolean;
    }
    let Ok(length) = env.get_array_length(&ingredients) else {
        return false as jboolean;
    };
    let mut parsed = Vec::with_capacity(length as usize);
    for index in 0..length {
        let Ok(value) = env.get_object_array_element(&ingredients, index) else {
            return false as jboolean;
        };
        let value = JString::from(value);
        let Ok(text) = env.get_string(&value) else {
            return false as jboolean;
        };
        let Ok(item_key) = text.to_str().unwrap_or_default().parse::<Identifier>() else {
            return false as jboolean;
        };
        let Some(item) = REGISTRY.items.by_key(&item_key) else {
            return false as jboolean;
        };
        parsed.push(Ingredient::Item(item));
    }
    let ingredients: &'static [Ingredient] = Box::leak(parsed.into_boxed_slice());
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
        }) as jboolean
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
        return false as jboolean;
    };
    let Some(result_key) = env
        .get_string(&result)
        .ok()
        .and_then(|v| v.to_str().ok()?.parse().ok())
    else {
        return false as jboolean;
    };
    let Some(result_item) = REGISTRY.items.by_key(&result_key) else {
        return false as jboolean;
    };
    if count <= 0 {
        return false as jboolean;
    }
    let Some(rows) = read_string_array(&mut env, &shape) else {
        return false as jboolean;
    };
    if rows.is_empty() || rows.len() > 3 || rows.iter().any(|row| row.is_empty() || row.len() > 3) {
        return false as jboolean;
    }
    let width = rows[0].len();
    if rows.iter().any(|row| row.len() != width) {
        return false as jboolean;
    }
    let Some(definitions) = read_string_array(&mut env, &ingredients) else {
        return false as jboolean;
    };
    let mut parsed = FxHashMap::default();
    for definition in definitions {
        let Some((character, item_name)) = definition.split_once('=') else {
            return false as jboolean;
        };
        let mut chars = character.chars();
        let Some(character) = chars.next() else {
            return false as jboolean;
        };
        if chars.next().is_some() || character == ' ' {
            return false as jboolean;
        }
        let Ok(item_key) = item_name.parse::<Identifier>() else {
            return false as jboolean;
        };
        let Some(item) = REGISTRY.items.by_key(&item_key) else {
            return false as jboolean;
        };
        if parsed.insert(character, Ingredient::Item(item)).is_some() {
            return false as jboolean;
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
                return false as jboolean;
            }
        }
    }
    let pattern: &'static [Ingredient] = Box::leak(pattern.into_boxed_slice());
    REGISTRY.recipes.register_runtime_shaped(ShapedRecipe::new(
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
    )) as jboolean
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
    world(&mut env, &name).is_some_and(|world| world.has_neighbor_signal(BlockPos::new(x, y, z)))
        as jboolean
}

extern "system" fn block_light(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    x: jint,
    y: jint,
    z: jint,
) -> jbyte {
    world(&mut env, &name)
        .map(|world| {
            world.light_value_at(
                foton_core::chunk::light::LightLayer::Block,
                BlockPos::new(x, y, z),
            ) as jbyte
        })
        .unwrap_or(0)
}

extern "system" fn sky_light(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    x: jint,
    y: jint,
    z: jint,
) -> jbyte {
    world(&mut env, &name)
        .map(|world| {
            world.light_value_at(
                foton_core::chunk::light::LightLayer::Sky,
                BlockPos::new(x, y, z),
            ) as jbyte
        })
        .unwrap_or(0)
}

extern "system" fn block_passable(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    x: jint,
    y: jint,
    z: jint,
) -> jboolean {
    world(&mut env, &name)
        .map(|world| {
            let pos = BlockPos::new(x, y, z);
            world
                .get_block_state(pos)
                .get_collision_shape_at(pos)
                .is_empty() as jboolean
        })
        .unwrap_or(false as jboolean)
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
    player(&mut env, &uuid).is_some_and(|player| player.set_open_container_item(slot, stack))
        as jboolean
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
    player(&mut env, &uuid).is_some_and(|player| player.set_game_mode(game_mode)) as jboolean
}

extern "system" fn allow_flight(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jboolean {
    player(&mut env, &uuid).is_some_and(|player| player.get_abilities().may_fly) as jboolean
}

extern "system" fn is_flying(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jboolean {
    player(&mut env, &uuid).is_some_and(|player| player.is_flying()) as jboolean
}

extern "system" fn is_sleeping_ignored(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
) -> jboolean {
    player(&mut env, &uuid).is_some_and(|player| player.is_sleeping_ignored()) as jboolean
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
    player.open_generic_inventory(title, rows, items);
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
        TextComponent::translated(foton_utils::translations::CONTAINER_UPGRADE.msg()),
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
        TextComponent::translated(foton_utils::translations::CONTAINER_LOOM.msg()),
        move |context| {
            foton_core::inventory::menu::kinds::loom(
                inventory,
                context.container_id,
                BlockPos::new(x, y, z),
            )
        },
    );
    1
}

extern "system" fn damage_player(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    uuid: JString<'_>,
    amount: jdouble,
    source_uuid: JString<'_>,
) {
    let Some(player) = player(&mut env, &uuid) else {
        return;
    };
    let source_id = env
        .get_string(&source_uuid)
        .ok()
        .and_then(|value| value.to_str().ok().and_then(|text| text.parse().ok()));
    let world = player.get_world();
    let mut source =
        DamageSource::environment(&foton_registry::vanilla_damage_types::PLAYER_ATTACK);
    if let Some(id) =
        source_id.and_then(|id| world.get_entity_by_uuid(&id).map(|entity| entity.id()))
    {
        source = source.with_causing_entity(id).with_direct_entity(id);
    }
    let _ = player.hurt(&world, &source, amount as f32);
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
        TextComponent::translated(foton_utils::translations::CONTAINER_CARTOGRAPHY_TABLE.msg()),
        move |context| {
            foton_core::inventory::menu::kinds::cartography(
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
        TextComponent::translated(foton_utils::translations::CONTAINER_REPAIR.msg()),
        move |context| {
            foton_core::inventory::menu::kinds::anvil(
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
        TextComponent::translated(foton_utils::translations::CONTAINER_STONECUTTER.msg()),
        move |context| {
            foton_core::inventory::menu::kinds::stonecutter(
                inventory,
                context.container_id,
                BlockPos::new(x, y, z),
            )
        },
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
        TextComponent::translated(foton_utils::translations::CONTAINER_GRINDSTONE_TITLE.msg()),
        move |context| {
            foton_core::inventory::menu::kinds::grindstone(
                inventory,
                context.container_id,
                BlockPos::new(x, y, z),
                &context.world,
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
        TextComponent::translated(foton_utils::translations::CONTAINER_CRAFTING.msg()),
        move |context| {
            foton_core::inventory::menu::kinds::crafting(
                inventory,
                context.container_id,
                BlockPos::new(x, y, z),
            )
        },
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
    server().is_some_and(|value| value.config.allow_flight) as jboolean
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

extern "system" fn server_tps(mut env: JNIEnv<'_>, _class: JClass<'_>) -> jdoubleArray {
    let Some(server) = server() else {
        return null_mut();
    };
    let manager = server.tick_rate_manager.read();
    let tps = f64::from(manager.get_tps());
    let values = [tps, tps, tps];
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
    server().is_some_and(|server| server.request_world_removal_with_save(key, save != 0))
        as jboolean
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
    chunk_requests().lock().insert(id, handle);
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
    let Some(handle) = requests.get(&id) else {
        return 0;
    };
    match handle.poll() {
        foton_core::chunk::chunk_request::ChunkRequestState::Ready => {
            requests.remove(&id);
            1
        }
        foton_core::chunk::chunk_request::ChunkRequestState::Cancelled => {
            requests.remove(&id);
            0
        }
        foton_core::chunk::chunk_request::ChunkRequestState::Pending { .. } => 0,
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
    world(&mut env, &name).is_some_and(|world| world.is_chunk_loaded(x, z)) as jboolean
}

extern "system" fn world_chunk_generated(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    x: jint,
    z: jint,
) -> jboolean {
    world(&mut env, &name).is_some_and(|world| world.is_chunk_generated(x, z)) as jboolean
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
    let Some(category) = category.to_str().ok().map(|v| v.to_ascii_uppercase()) else {
        return;
    };
    let category = match category.as_str() {
        "MONSTER" => foton_registry::entity_type::MobCategory::Monster,
        "CREATURE" | "ANIMAL" => foton_registry::entity_type::MobCategory::Creature,
        "AMBIENT" => foton_registry::entity_type::MobCategory::Ambient,
        "AXOLOTL" | "AXOLOTLS" => foton_registry::entity_type::MobCategory::Axolotls,
        "WATER_CREATURE" | "WATER_ANIMAL" => {
            foton_registry::entity_type::MobCategory::WaterCreature
        }
        "WATER_AMBIENT" => foton_registry::entity_type::MobCategory::WaterAmbient,
        "UNDERGROUND_WATER_CREATURE" => {
            foton_registry::entity_type::MobCategory::UndergroundWaterCreature
        }
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
    let Some(category) = category.to_str().ok().map(|v| v.to_ascii_uppercase()) else {
        return 0;
    };
    let category = match category.as_str() {
        "MONSTER" => foton_registry::entity_type::MobCategory::Monster,
        "CREATURE" | "ANIMAL" => foton_registry::entity_type::MobCategory::Creature,
        "AMBIENT" => foton_registry::entity_type::MobCategory::Ambient,
        "AXOLOTL" | "AXOLOTLS" => foton_registry::entity_type::MobCategory::Axolotls,
        "WATER_CREATURE" | "WATER_ANIMAL" => {
            foton_registry::entity_type::MobCategory::WaterCreature
        }
        "WATER_AMBIENT" => foton_registry::entity_type::MobCategory::WaterAmbient,
        "UNDERGROUND_WATER_CREATURE" => {
            foton_registry::entity_type::MobCategory::UndergroundWaterCreature
        }
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
        .map(|v| v.to_ascii_uppercase())
        .as_deref()
    {
        Some("MONSTER") => foton_registry::entity_type::MobCategory::Monster,
        Some("CREATURE") => foton_registry::entity_type::MobCategory::Creature,
        Some("AMBIENT") => foton_registry::entity_type::MobCategory::Ambient,
        Some("AXOLOTL") | Some("AXOLOTLS") => foton_registry::entity_type::MobCategory::Axolotls,
        Some("UNDERGROUND_WATER_CREATURE") => {
            foton_registry::entity_type::MobCategory::UndergroundWaterCreature
        }
        Some("WATER_CREATURE") => foton_registry::entity_type::MobCategory::WaterCreature,
        Some("WATER_AMBIENT") => foton_registry::entity_type::MobCategory::WaterAmbient,
        _ => return,
    };
    world.set_spawn_limit(category, limit);
}

extern "system" fn world_keep_spawn_in_memory(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
) -> jboolean {
    world(&mut env, &name).is_some_and(|w| w.keep_spawn_in_memory()) as jboolean
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
    world(&mut env, &name).map_or(0, |world| world.is_raining() as jboolean)
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
    world(&mut env, &name).is_some_and(|world| world.has_bonus_chest()) as jboolean
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
        "PEACEFUL" => foton_utils::types::Difficulty::Peaceful,
        "EASY" => foton_utils::types::Difficulty::Easy,
        "NORMAL" => foton_utils::types::Difficulty::Normal,
        "HARD" => foton_utils::types::Difficulty::Hard,
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
    world(&mut env, &name).map_or(0, |world| world.is_thundering() as jboolean)
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
) -> jstring {
    let Ok(world_text): Result<String, _> = env.get_string(&world_name).map(Into::into) else {
        return null_mut();
    };
    let Ok(type_text): Result<String, _> = env.get_string(&type_name).map(Into::into) else {
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
    let Some(entity) =
        foton_core::entity::spawn_util::spawn_entity_at(&world, &type_key, DVec3::new(x, y, z))
    else {
        return null_mut();
    };
    to_java(&mut env, Some(entity.uuid().to_string()))
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
        foton_registry::game_rules::GameRuleType::Bool => value_text
            .parse::<bool>()
            .ok()
            .map(foton_registry::game_rules::GameRuleValue::new),
        foton_registry::game_rules::GameRuleType::Int => value_text
            .parse::<i32>()
            .ok()
            .map(foton_registry::game_rules::GameRuleValue::new),
    };
    parsed.is_some_and(|value| world.set_erased_game_rule(rule, value)) as jboolean
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
    let Some(world) = server().and_then(|server| server.worlds.get_owned(&world_key)) else {
        return null_mut();
    };
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
        .and_then(|entity| {
            entity
                .downcast_ref::<foton_core::block_entity::entities::HopperBlockEntity>()?
                .custom_name()
        })
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
    let Some(hopper) =
        entity.downcast_ref::<foton_core::block_entity::entities::HopperBlockEntity>()
    else {
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
                .downcast_ref::<foton_core::block_entity::entities::JukeboxBlockEntity>()
                .is_some_and(foton_core::block_entity::entities::JukeboxBlockEntity::is_playing)
        });
    let playing = playing.unwrap_or(false);
    if playing { 1 } else { 0 }
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
    if waxed { 1 } else { 0 }
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
    value as jboolean
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
    banner.set_pattern_descriptions(descriptions) as jboolean
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
    world(&mut env, &name).is_some_and(|world| world.is_auto_save()) as jboolean
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
            .and_then(|scoreboard| {
                scoreboard.holder_team_name(&foton_core::scoreboard::ScoreHolder::new(entry))
            })
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
        return std::ptr::null_mut();
    };
    let (x, z, size) = world.world_border_center_size();
    let Ok(array) = env.new_double_array(3) else {
        return std::ptr::null_mut();
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
        world.explosion_destroy_type(&foton_registry::vanilla_game_rules::TNT_EXPLOSION_DROP_DECAY)
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
    let interaction =
        world.explosion_destroy_type(&foton_registry::vanilla_game_rules::TNT_EXPLOSION_DROP_DECAY);
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
    if player.get_world().key.to_string() != String::from(world_name) {
        return 0;
    }
    u8::from(player.teleport(DVec3::new(x, y, z), yaw, pitch).is_ok())
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

    vec![
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
            "(Ljava/lang/String;DDDLjava/lang/String;)Ljava/lang/String;",
            spawn_entity as *mut c_void,
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
            "spawnParticle",
            "(Ljava/lang/String;Ljava/lang/String;DDDIDDDD)V",
            spawn_particle as *mut c_void,
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
            "setBlockDisplayBrightness",
            "(Ljava/lang/String;II)V",
            set_block_display_brightness as *mut c_void,
        ),
        method(
            "setBlockDisplayViewRange",
            "(Ljava/lang/String;F)V",
            set_block_display_view_range as *mut c_void,
        ),
        method(
            "setBlockDisplayShadowRadius",
            "(Ljava/lang/String;F)V",
            set_block_display_shadow_radius as *mut c_void,
        ),
        method(
            "setBlockDisplayTransformation",
            "(Ljava/lang/String;FFFFFFFFFFFFFF)V",
            set_block_display_transformation as *mut c_void,
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
            "setEntityProjectileOwner",
            "(Ljava/lang/String;Ljava/lang/String;)Z",
            set_entity_projectile_owner as *mut c_void,
        ),
        method(
            "entityPotionEffects",
            "(Ljava/lang/String;)[Ljava/lang/String;",
            entity_potion_effects as *mut c_void,
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
            "arrowPotion",
            "(Ljava/lang/String;)Ljava/lang/String;",
            arrow_potion as *mut c_void,
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
            "damagePlayer",
            "(Ljava/lang/String;DLjava/lang/String;)V",
            damage_player as *mut c_void,
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
            "playerAttribute",
            "(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
            player_attribute as *mut c_void,
        ),
        method(
            "setAttributeBase",
            "(Ljava/lang/String;Ljava/lang/String;D)V",
            set_attribute_base as *mut c_void,
        ),
        method(
            "addAttributeModifier",
            "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;DLjava/lang/String;)Z",
            add_attribute_modifier as *mut c_void,
        ),
        method(
            "removeAttributeModifier",
            "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)Z",
            remove_attribute_modifier as *mut c_void,
        ),
        method(
            "attributeModifiers",
            "(Ljava/lang/String;Ljava/lang/String;)[Ljava/lang/String;",
            attribute_modifiers as *mut c_void,
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
    ]
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
