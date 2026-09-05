//! This module contains all things player-related.
mod abilities;
mod advancements;
pub mod chat;
pub mod chunk_sender;
/// This module contains the `PlayerConnection` trait that abstracts network connections.
pub mod connection;
mod container_counter;
mod entity_state;
/// Experience System
pub mod experience;
pub mod food_data;
/// Game mode specific logic for player interactions.
pub mod game_mode;
mod health_sync;
mod item_cooldowns;
mod lifecycle;
mod map_sync;
pub mod movement;
mod permissions;
pub mod player_data;
pub mod player_data_storage;
pub mod player_inventory;
mod profile;
mod shoulder;
mod sleep;
mod sleep_state;
mod statistics;
mod tick_state;

pub use abilities::{Abilities, DEFAULT_FLYING_SPEED};
use chat::ChatState;
pub use chat::{LastSeen, LastSeenMessagesValidator, MessageCache};
use connection::NetworkConnection as _;
pub use connection::{ClientInformation, PlayerConnection};
use container_counter::ContainerCounter;
use food_data::FoodData;
use foton_protocol::packets::game::{
    CEntityEvent, CHurtAnimation, CPlayerCombatKill, CPlayerInfoUpdate, CPlayerLookAt, CRespawn,
    CSetDefaultSpawnPosition, CSetHealth, CSetHeldSlot, CSetPassengers, ClientCommandAction,
    LookAtAnchor, RelativeMovement, SoundSource,
};
use foton_protocol::packets::game::{CLevelEvent, CSetEntityData, CSetExperience};
use foton_registry::blocks::block_state_ext::BlockStateExt as _;
use foton_registry::entity_data::{EntityPose, HumanoidArm, ParticleList};
use foton_registry::entity_type::{EntityDimensions, EntityTypeRef};
use foton_registry::game_rules::GameRuleRef;
use foton_registry::sound_event::SoundEventRef;
use foton_registry::stat::Stat;
use foton_registry::vanilla_block_tags::BlockTag;
use foton_registry::vanilla_entity_data::PlayerEntityData;
use foton_registry::vanilla_game_rules::{
    DROWNING_DAMAGE, FALL_DAMAGE, FIRE_DAMAGE, FREEZE_DAMAGE, IMMEDIATE_RESPAWN, KEEP_INVENTORY,
    SHOW_DEATH_MESSAGES,
};
use foton_registry::{
    level_events, sound_events, vanilla_attributes, vanilla_damage_type_tags, vanilla_entities,
    vanilla_game_events,
};
use foton_registry::{vanilla_custom_stats, vanilla_stat_types};
use foton_utils::locks::IntoShared;
use foton_utils::{entity_events::EntityStatus, locks::Shared};
use game_mode::{BlockBreakingManager, PlayerGameModeState};
use glam::DVec3;
use health_sync::HealthSyncState;
use item_cooldowns::ItemCooldowns;
use lifecycle::PlayerLifecycleState;
pub use lifecycle::PlayerRespawnConfig;
pub(crate) use lifecycle::ResetReason;
pub use movement::PlayerInput;
use movement::{MovementState, TeleportState};
use permissions::PlayerPermissionState;
pub(crate) use profile::{GAME_PROFILE_CACHE_LIMIT, KnownPlayerNameLookup, lookup_online_profile};
pub use profile::{
    GameProfile, GameProfileAction, KnownPlayer, KnownPlayers, ProfileLookupError,
    is_valid_player_name, offline_uuid,
};
use shoulder::ShoulderEntities;
use simdnbt::owned::{NbtCompound, NbtList, NbtTag};
use sleep_state::PlayerSleepState;
use std::sync::{Arc, Weak};

use crate::inventory::container::SimpleContainer;

/// Slots in a player's ender chest.
///
/// Vanilla parity: `PlayerEnderChestContainer`, three rows like a small
/// chest.
pub const ENDER_CHEST_SLOTS: usize = 27;

use tick_state::PlayerTickState;
use uuid::Uuid;

use arc_swap::ArcSwap;
use foton_utils::locks::SyncMutex;
use foton_utils::types::{Difficulty, GameType, InteractionHand};
use text_components::resolving::TextResolutor;
use text_components::{
    Modifier as _, TextComponent,
    interactivity::{ClickEvent, HoverEvent},
};
use text_components::{content::Resolvable, custom::CustomData};

use crate::advancement::{PlayerAdvancements, triggers};
use crate::behavior::{BlockStateBehaviorExt as _, InteractionResult};
use crate::chunk::chunk_request::{ChunkRequestHandle, ChunkRequestState};
use crate::config::RuntimeConfig;
use crate::enchantment_helper;
use crate::entity::damage::DamageSource;
use crate::entity::entities::{ExperienceOrbEntity, WardenSpawnTracker};
use crate::entity::kill_score;
use crate::entity::{
    DEATH_DURATION, Entity, EntityAnchor, EntityBase, EntityEventSource, EntityMovementEmission,
    EntitySyncedData, LivingEntity, LivingEntityBase, LivingEntitySyncedData, MobEffectSyncChange,
    MobEffectSyncPacket, RemovalReason, SharedEntity, apply_entity_look_at, start_riding_entities,
};
use crate::fluid::get_fluid_state;
use crate::inventory::equipment::{EntityEquipment, EquipmentSlot, EquipmentSlotType};
use crate::inventory::lock::{ContainerLockGuard, ContainerRef};
use crate::inventory::menu::Menu;
use crate::inventory::menu::kinds::chest;
use crate::inventory::menu::kinds::inventory_menu;
use crate::inventory::slot_ranges::{
    CURSOR_AND_MOUNT_CHEST_SLOT, ENDER_CHEST_SLOT_OFFSET, PLAYER_CRAFTING_SIZE,
    PLAYER_CRAFTING_SLOT_OFFSET, container_slot_item,
};
use crate::level_data::RespawnData;
use crate::permission::{
    PermissionContext, PermissionExpr, PermissionMetadataSet, PermissionMetadataValue,
    PermissionSet, PermissionState,
};
use crate::physics::MoveResult;
use crate::player::experience::Experience;
use crate::player::player_data::{PersistentEnderPearl, PersistentRootVehicle};
use crate::player::player_inventory::{
    MenuItemDisposition, MenuRemovalStatus, PlayerInventory, PlayerInventorySyncState,
};
use crate::server::{
    Server,
    jobs::{JobPoll, ServerJob, ServerJobContext},
};
use crate::stat::StatsCounter;
use crate::world::player_spawn_finder::{PlayerSpawnSearch, PlayerSpawnSearchPoll};
use foton_registry::vanilla_damage_types;

use foton_protocol::packets::{
    common::SCustomPayload,
    game::{CContainerClose, CGameEvent, CSystemChat, GameEventType},
};
use foton_registry::RegistryEntry;
use foton_registry::item_stack::ItemStack;

use foton_utils::{
    BlockPos, BlockStateId, ChunkPos, DowncastType, DowncastTypeKey, Identifier, UuidExt as _,
};

use crate::inventory::container::Container;

const RESPAWN_SEARCH_READY_CANDIDATE_BUDGET: usize = 8;

use crate::bug_dialog;
use crate::bug_report::{BugCategory, BugReport, MAX_DESCRIPTION, forward};
use crate::chunk::player_chunk_view::PlayerChunkView;
use crate::event::{Event, PlayerCustomPayloadEvent};
use crate::event::{PlayerDeathEvent, PlayerItemBreakEvent};
use crate::player::chunk_sender::ChunkSender;
use crate::portal::{
    PortalTicketTarget, TeleportPostAction, TeleportPostTransition, TeleportTransition,
};
use crate::world::World;
use foton_protocol::packets::common::SCustomClickAction;
use foton_utils::text::DisplayResolutor;
use std::env::current_dir;
use std::path::PathBuf;

/// A struct representing a player.
pub struct Player {
    /// The player's game profile.
    pub gameprofile: GameProfile,
    /// Optional tab-list display name supplied by Bukkit plugins.
    tab_list_name: SyncMutex<Option<TextComponent>>,
    /// The player's connection (abstracted for testing).
    pub connection: Arc<PlayerConnection>,

    /// The world the player is in.
    pub world: ArcSwap<World>,

    /// Reference to the server (for entity ID generation, etc.).
    pub(crate) server: Weak<Server>,
    /// Runtime configuration shared with the server.
    pub(crate) config: Arc<RuntimeConfig>,
    /// This player, as the world holds them.
    ///
    /// `Arc::new_cyclic` hands the weak reference out at construction and it
    /// was already being used for the inventory menu; keeping it lets a player
    /// produce their own shared handle. Events need that: an event is
    /// `'static`, so it cannot borrow the player it is about, and most of the
    /// places one fires from hold only a `&Player`.
    shared: Weak<Self>,

    /// Common entity fields (id, uuid, position, rotation, removal, callback).
    base: EntityBase,

    /// Client lifecycle flags.
    lifecycle: SyncMutex<PlayerLifecycleState>,

    /// Movement tracking state
    pub(crate) movement: SyncMutex<MovementState>,

    /// Synchronized entity data (health, pose, flags, etc.) for network sync.
    entity_data: SyncMutex<PlayerEntityData>,

    /// The last chunk position of the player.
    pub last_chunk_pos: SyncMutex<ChunkPos>,
    /// The last chunk tracking view of the player.
    pub last_tracking_view: SyncMutex<Option<PlayerChunkView>>,
    /// The chunk sender for the player.
    pub chunk_sender: SyncMutex<ChunkSender>,

    /// The client's settings/information (language, view distance, chat visibility, etc.).
    /// Updated when the client sends `SClientInformation` during config or play phase.
    client_information: SyncMutex<ClientInformation>,

    /// Chat state: message counters, signature cache, validator, session, chain.
    pub chat: SyncMutex<ChatState>,

    /// Current and previous game mode.
    game_modes: SyncMutex<PlayerGameModeState>,

    /// The player's inventory container (shared with `inventory_menu`).
    pub inventory: Shared<PlayerInventory>,

    /// The twenty-seven slots every ender chest in the world opens.
    ///
    /// Vanilla parity: `Player.enderChestInventory`. It belongs to the player,
    /// not to any block, which is why it lives here and travels with them
    /// between worlds and through death.
    pub ender_chest: Shared<SimpleContainer>,

    /// Logical inventory slots that must be resent directly to this player's client.
    inventory_sync: SyncMutex<PlayerInventorySyncState>,

    /// Last main-hand stack used for vanilla attack-strength reset checks.
    last_item_in_main_hand: SyncMutex<ItemStack>,

    /// The player's inventory menu (always open, even when `container_id` is 0).
    inventory_menu: SyncMutex<Menu>,

    /// The currently open menu (None if player inventory is open).
    /// This is separate from `inventory_menu` which is always present.
    open_menu: SyncMutex<player_inventory::OpenMenuState>,

    /// Counter for generating container IDs (1-100, wraps around).
    container_counter: SyncMutex<ContainerCounter>,

    /// Pending server-initiated teleport state (ID, position, timeout).
    teleport_state: SyncMutex<TeleportState>,
    /// Vanilla item use cooldown groups.
    item_cooldowns: SyncMutex<ItemCooldowns>,

    /// Local tick and once-per-tick packet state.
    tick_state: SyncMutex<PlayerTickState>,
    /// Vanilla sleep/wake animation counter.
    sleep_state: SyncMutex<PlayerSleepState>,

    /// Entities riding on this player's shoulders.
    ///
    /// Vanilla parity: the `shoulderEntityLeft`/`shoulderEntityRight` pair
    /// of `ServerPlayer`.
    shoulder_entities: ShoulderEntities,
    /// Persisted personal bed or respawn-anchor target.
    respawn_config: SyncMutex<Option<PlayerRespawnConfig>>,

    /// Player abilities (flight, invulnerability, build permissions, speeds, etc.)
    pub abilities: SyncMutex<Abilities>,

    /// Block breaking state machine.
    pub block_breaking: SyncMutex<BlockBreakingManager>,

    /// Shared living-entity runtime fields (attributes, speed, damage/death state).
    /// Vanilla: `LivingEntity` (L230-232) + `Entity.invulnerableTime` (L256).
    living_base: LivingEntityBase,

    /// Player food/hunger state (food level, saturation, exhaustion).
    pub food_data: SyncMutex<FoodData>,

    /// Delta-tracking state for `CSetHealth` deduplication.
    health_sync: SyncMutex<HealthSyncState>,

    /// The Player's Experience
    pub experience: SyncMutex<Experience>,

    /// Vanilla `Player.enchantmentSeed`.
    ///
    /// The three offers an enchanting table shows are drawn from this, so they
    /// stay put while the player opens and closes the table and only change
    /// once something is actually enchanted. That is what makes a bad set of
    /// offers worth burning a cheap item on.
    ///
    /// Saved as `XpSeed`, so the offers a player walked away from are the ones
    /// waiting when they come back.
    enchantment_seed: SyncMutex<i32>,

    /// Assigned groups, direct overrides, and the effective permission set.
    permissions: SyncMutex<PlayerPermissionState>,

    /// Whether the player has completed the vanilla End credits flow.
    seen_credits: SyncMutex<bool>,
    /// Vanilla `ServerPlayer.wardenSpawnTracker`, how close this player is to a warden.
    warden_spawn_tracker: SyncMutex<WardenSpawnTracker>,

    /// Vanilla `ServerPlayer.wonGame`; transient while the End credits screen is open.
    won_game: SyncMutex<bool>,

    /// Monotonic counter bumped on world teleport/reset. The chunk sending tick
    /// snapshots this before encoding and compares after to detect stale batches.
    pub chunk_send_epoch: SyncMutex<u32>,

    /// Domain-residence identity and persisted entities awaiting restoration.
    residence: SyncMutex<PlayerResidenceState>,
    /// In-flight ender pearls thrown by this player, kept weakly so they persist
    /// with the player and re-spawn on login (vanilla `ServerPlayer.enderPearls`).
    ender_pearls: SyncMutex<Vec<Weak<dyn Entity>>>,
    /// The fishing hook this player has cast (vanilla `Player.fishing`).
    ///
    /// Held weakly: the hook lives in the world's entity table, and this field
    /// only answers whether a bobber is already out when the rod is used again.
    fishing: SyncMutex<Option<Weak<dyn Entity>>>,
    /// Where this player was standing when Bad Omen turned into Raid Omen.
    ///
    /// Vanilla parity: `ServerPlayer.raidOmenPosition`, which is what the Raid
    /// Omen effect raids when it runs out -- the village the omen was absorbed
    /// in, not wherever the player has wandered to thirty seconds later.
    ///
    /// Vanilla persists this in the player file; Foton does not, so a player
    /// who logs out inside the thirty-second window and back in loses the
    /// pending raid. The effect itself is not persisted across that boundary
    /// either, so the two gaps line up.
    raid_omen_position: SyncMutex<Option<BlockPos>>,

    /// Which advancements this player has earned and what the screen has
    /// already been told.
    ///
    /// Vanilla parity: `ServerPlayer.advancements`. Vanilla caches these on the
    /// player list by UUID so they survive the new `ServerPlayer` a respawn
    /// builds; Foton keeps them on the player and restores them from the save,
    /// which is the same thing through a different door.
    advancements: SyncMutex<PlayerAdvancements>,

    /// The player inventory as the advancement triggers last saw it.
    ///
    /// Vanilla parity: `AbstractContainerMenu.lastSlots`, which is what decides
    /// a slot changed and hands `INVENTORY_CHANGED` the stack that landed in
    /// it. Foton keeps it on the player instead of the open menu, so opening a
    /// container does not replay every slot; replaying only ever re-awards a
    /// criterion that is already met, and starting empty reproduces the login
    /// case vanilla gets from a freshly built menu.
    last_seen_inventory: SyncMutex<Box<[ItemStack]>>,

    /// What this player has counted.
    ///
    /// Vanilla parity: `ServerPlayer.stats`, which vanilla keeps on the player
    /// list by uuid; Foton keeps it on the player and restores it from the
    /// save, the same way the advancements above are handled.
    stats: SyncMutex<StatsCounter>,
}

// SAFETY: This key is owned by Foton and uniquely identifies `Player`.
unsafe impl DowncastType for Player {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:entity/player");
}

#[derive(Clone)]
struct PendingRootVehicleRestore {
    world: Identifier,
    root_vehicle: PersistentRootVehicle,
}

/// Runtime identity for one continuous stay in a Foton domain.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DomainResidenceToken(u64);

struct PlayerResidenceState {
    token: DomainResidenceToken,
    pending_root_vehicle: Option<PendingRootVehicleRestore>,
    pending_ender_pearls: Vec<PersistentEnderPearl>,
}

impl PlayerResidenceState {
    const fn new() -> Self {
        Self {
            token: DomainResidenceToken(1),
            pending_root_vehicle: None,
            pending_ender_pearls: Vec::new(),
        }
    }

    fn advance(&mut self) -> DomainResidenceToken {
        let Some(next_token) = self.token.0.checked_add(1) else {
            panic!("domain residence token space exhausted");
        };
        self.token = DomainResidenceToken(next_token);
        self.pending_root_vehicle = None;
        self.pending_ender_pearls.clear();
        self.token
    }
}

impl Player {
    /// Returns the optional custom tab-list display name.
    pub fn tab_list_name(&self) -> Option<TextComponent> {
        self.tab_list_name.lock().clone()
    }

    /// Updates the tab-list display name for every online viewer.
    pub fn set_tab_list_name(&self, name: Option<TextComponent>) {
        (*self.tab_list_name.lock()).clone_from(&name);
        self.server()
            .broadcast_to_online(CPlayerInfoUpdate::update_display_name(self.uuid(), name));
    }

    /// Returns the player's cumulative experience points.
    #[must_use]
    pub fn total_experience(&self) -> i32 {
        self.experience.lock().total_points()
    }

    /// Replaces cumulative experience and schedules the vanilla sync packet.
    pub fn set_total_experience(&self, total: i32) {
        *self.experience.lock() = experience::Experience::new(total);
    }

    /// Opens a generic chest-backed inventory for plugin-created inventories.
    ///
    /// The backing container is captured by the menu so it remains alive for
    /// exactly as long as the open view; no global inventory table is needed.
    pub fn open_generic_inventory(
        &self,
        title: impl Into<TextComponent>,
        rows: usize,
        items: Vec<ItemStack>,
    ) {
        let rows = rows.clamp(1, 6);
        let mut initial = items;
        initial.resize(rows * 9, ItemStack::empty());
        initial.truncate(rows * 9);
        let backing = SimpleContainer::from_items(initial).into_shared();
        let player_inventory = Arc::clone(&self.inventory);
        self.open_menu(title, move |context| {
            chest(
                player_inventory,
                context.container_id,
                Arc::clone(&backing),
                rows,
            )
        });
    }
    /// Returns the player's configured main arm.
    #[must_use]
    pub fn main_arm(&self) -> HumanoidArm {
        self.client_information.lock().main_hand
    }

    /// Damages the item in `hand`, and announces it if the item breaks.
    ///
    /// Vanilla parity: `ItemStack.hurtAndBreak(int, LivingEntity,
    /// EquipmentSlot)`, whose break callback is
    /// `entity.onEquippedItemBroken(item, slot)` -- the snap, the splinters and
    /// the attribute modifiers coming back off. Foton's `ItemStack` cannot
    /// reach the holder, so this is where the two halves meet; going through
    /// `player.inventory` directly damages the item silently.
    pub fn hurt_item_in_hand(&self, hand: InteractionHand, amount: i32) {
        let has_infinite_materials = self.has_infinite_materials();
        let broken_item = self.inventory.lock().get_item_in_hand(hand).clone();
        let broke = self
            .inventory
            .lock()
            .hurt_item_in_hand(hand, amount, has_infinite_materials);
        if broke {
            let mut event = PlayerItemBreakEvent::new(self.gameprofile.id, broken_item);
            self.fire_event(&mut event);
            LivingEntity::on_equipped_item_broken(self, EquipmentSlot::for_hand(hand));
        }
    }

    /// Computes the start (eye position) and end positions for a raytrace.
    pub fn get_ray_endpoints(&self) -> (DVec3, DVec3) {
        let pos = self.position();
        let start_pos = DVec3::new(pos.x, self.get_eye_y(), pos.z);
        let block_interaction_range = self
            .attributes()
            .lock()
            .get_value(vanilla_attributes::BLOCK_INTERACTION_RANGE)
            .unwrap_or(4.5);
        let direction = self.look_angle() * block_interaction_range;

        let end_pos = start_pos + direction;
        (start_pos, end_pos)
    }

    /// Returns the player's current game mode.
    #[must_use]
    pub fn game_mode(&self) -> GameType {
        self.game_modes.lock().current()
    }

    /// Returns the player's previous game mode.
    #[must_use]
    pub fn previous_game_mode(&self) -> Option<GameType> {
        self.game_modes.lock().previous()
    }

    /// Restores current and previous game mode from persistent player data.
    pub(crate) fn restore_game_modes(&self, current: GameType, previous: Option<GameType>) {
        self.game_modes.lock().set_pair(current, previous);
    }

    /// Changes the current game mode and records the old current mode as previous.
    fn change_game_mode_state(&self, game_mode: GameType) -> bool {
        self.game_modes.lock().change_current(game_mode)
    }

    /// Creates a new player.
    #[expect(
        clippy::too_many_arguments,
        reason = "a player is assembled from its whole join context at once;                   `self_weak` is the `Arc::new_cyclic` handle its inventory                   menu needs so an armor slot knows who wears it"
    )]
    pub fn new(
        gameprofile: GameProfile,
        connection: Arc<PlayerConnection>,
        world: Arc<World>,
        server: Weak<Server>,
        config: Arc<RuntimeConfig>,
        entity_id: i32,
        client_information: ClientInformation,
        self_weak: &Weak<Self>,
    ) -> Self {
        // Create a single shared inventory container used by both the player and inventory menu
        let inventory = Arc::new(SyncMutex::new(PlayerInventory::new()));
        // Vanilla parity: `Player.enderChestInventory`, twenty-seven slots
        // that belong to the player rather than to any block.
        let ender_chest = Arc::new(SyncMutex::new(SimpleContainer::new(ENDER_CHEST_SLOTS)));

        let pos = DVec3::new(0.0, 0.0, 0.0);

        let equipment = inventory.clone();
        let living_base = LivingEntityBase::with_equipment(&vanilla_entities::PLAYER, equipment);
        let player_uuid = gameprofile.id;
        let world_ref = Arc::downgrade(&world);
        let chat_spam_threshold_seconds = config.chat_spam_threshold_seconds;
        let command_spam_threshold_seconds = config.command_spam_threshold_seconds;

        Self {
            gameprofile,
            connection,

            tab_list_name: SyncMutex::new(None),
            world: ArcSwap::new(world),
            server,
            config,
            base: EntityBase::with_uuid(
                entity_id,
                player_uuid,
                pos,
                Self::dimensions_for_pose(EntityPose::Standing),
                world_ref,
            ),
            lifecycle: SyncMutex::new(PlayerLifecycleState::default()),
            movement: SyncMutex::new(MovementState::new()),
            entity_data: SyncMutex::new({
                let mut data = PlayerEntityData::new();
                living_base.initialize_synced_data(&mut data);
                data
            }),
            last_chunk_pos: SyncMutex::new(ChunkPos::new(0, 0)),
            last_tracking_view: SyncMutex::new(None),
            chunk_sender: SyncMutex::new(ChunkSender::default()),
            client_information: SyncMutex::new(client_information),
            chat: SyncMutex::new(ChatState::new(
                chat_spam_threshold_seconds,
                command_spam_threshold_seconds,
            )),
            game_modes: SyncMutex::new(PlayerGameModeState::new(GameType::Survival)),
            inventory: inventory.clone(),
            ender_chest,
            inventory_sync: SyncMutex::new(PlayerInventorySyncState::new()),
            last_item_in_main_hand: SyncMutex::new(ItemStack::empty()),
            shared: self_weak.clone(),
            inventory_menu: SyncMutex::new(inventory_menu(inventory, self_weak.clone())),
            open_menu: SyncMutex::new(player_inventory::OpenMenuState::new()),
            container_counter: SyncMutex::new(ContainerCounter::new()),
            teleport_state: SyncMutex::new(TeleportState::new()),
            item_cooldowns: SyncMutex::new(ItemCooldowns::default()),
            tick_state: SyncMutex::new(PlayerTickState::new()),
            sleep_state: SyncMutex::new(PlayerSleepState::new()),
            shoulder_entities: ShoulderEntities::new(),
            respawn_config: SyncMutex::new(None),
            abilities: SyncMutex::new(Abilities::default()),
            block_breaking: SyncMutex::new(BlockBreakingManager::new()),
            living_base,
            food_data: SyncMutex::new(FoodData::new()),
            health_sync: SyncMutex::new(HealthSyncState::new()),
            experience: SyncMutex::new(Experience::default()),
            enchantment_seed: SyncMutex::new(rand::random()),
            permissions: SyncMutex::new(PlayerPermissionState::default()),
            seen_credits: SyncMutex::new(false),
            warden_spawn_tracker: SyncMutex::new(WardenSpawnTracker::default()),
            won_game: SyncMutex::new(false),
            chunk_send_epoch: SyncMutex::new(0),
            residence: SyncMutex::new(PlayerResidenceState::new()),
            ender_pearls: SyncMutex::new(Vec::new()),
            fishing: SyncMutex::new(None),
            raid_omen_position: SyncMutex::new(None),
            advancements: SyncMutex::new(PlayerAdvancements::new()),
            last_seen_inventory: SyncMutex::new(
                vec![ItemStack::empty(); PlayerInventory::CONTAINER_SIZE].into_boxed_slice(),
            ),
            stats: SyncMutex::new(StatsCounter::new()),
        }
    }

    /// Ticks the player.
    ///
    /// # Panics
    ///
    /// Panics if the player position cannot be restored after `ai_step`. Vanilla treats the
    /// pre-tick position as authoritative here, so a rejection indicates corrupted entity state.
    #[expect(
        clippy::too_many_lines,
        reason = "one vanilla tick, whose order between steps is the behavior"
    )]
    pub fn tick(&self) {
        self.advance_tick();
        // Vanilla parity: the `this.wardenSpawnTracker.tick()` of `ServerPlayer.tick`.
        self.warden_spawn_tracker.lock().tick();
        self.tick_item_cooldowns();
        self.tick_attack_strength();
        self.tick_spam_throttlers();
        self.tick_client_load_timeout();
        self.tick_sleep_counter();
        if self.is_sleeping() {
            let world = self.get_world();
            if !self.bed_rule_value_allows(world.dimension_type.bed_rule.can_sleep) {
                self.stop_sleep_in_bed(false, true);
            } else if !self.can_interact_with_level()
                || self
                    .sleeping_pos()
                    .is_none_or(|pos| !world.get_block_state(pos).is_bed())
            {
                self.stop_sleep_in_bed(true, true);
            }
        }

        self.set_no_physics(self.is_spectator());
        if self.is_spectator() || self.is_passenger() {
            self.set_on_ground(false);
        }

        let tick_position = self.position();

        // Vanilla: ServerGamePacketListenerImpl.resetPosition().
        self.movement.lock().reset_for_tick(tick_position);
        self.set_old_position_to_current();
        self.reset_vehicle_movement_for_tick();

        self.default_tick();
        self.detect_equipment_updates();
        // Vanilla parity: `Inventory.tick`, which `Player.aiStep` runs first.
        self.tick_inventory_items();
        self.ai_step();

        // Vanilla snaps the player back to firstGood after ServerPlayer.doTick().
        if let Err(error) = self.try_set_position(tick_position) {
            // Under `panic = "abort"` this kills the server and every dirty
            // chunk with it. The identical failure is already a `log::warn!`
            // twenty-four lines up in `movement`, and the entity manager
            // rejects a move whenever the player has left `live_by_id` --
            // a disconnect racing a tick, not an invariant the compiler
            // holds. A desynchronized player is recoverable; a dead world
            // is not.
            log::error!(
                "failed to restore player {} tick position after ai_step: {error}",
                self.id()
            );
        }
        self.refresh_fluid_contact();

        self.tick_ack_block_changes();

        if !self.has_client_loaded() {
            //return;
        }

        self.living_base.decrement_invulnerable_time();
        self.tick_mob_effects();
        self.updating_using_item();

        if self.get_health() <= 0.0 {
            self.tick_death();
        } else {
            let world = self.get_world();
            self.touch_nearby_items();
            self.block_breaking.lock().tick(self, &world);

            // TODO: Implement remaining player ticking logic here
            // - Managing game mode specific logic
            // - Handling falling

            self.update_player_attributes();
            self.living_base.refresh_speed_from_attributes();
            self.tick_regeneration();

            if self.is_sprinting() && !self.food_data.lock().has_enough_food() {
                self.set_sprinting(false);
            }
        }

        if self.disconnect_if_floating_too_long() {
            return;
        }
        if self.disconnect_if_vehicle_floating_too_long() {
            return;
        }

        self.tick_living_state();

        self.sync_map_item_updates();
        self.tick_open_menu();
        self.flush_inventory_resync();
        self.broadcast_inventory_changes();
        // Vanilla fires this from the container listener that
        // `AbstractContainerMenu.broadcastChanges` drives, which is the line
        // above.
        triggers::inventory::inventory_changed(self);
        self.update_pose();

        {
            let health = self.get_health();
            let (food, saturation) = {
                let food_data = self.food_data.lock();
                (food_data.food_level, food_data.saturation_level)
            };

            let mut sync = self.health_sync.lock();
            if sync.needs_update(health, food, saturation) {
                self.send_packet(CSetHealth {
                    health,
                    food,
                    food_saturation: saturation,
                });
                sync.record_sent(health, food, saturation);
            }
        }

        let experience_packet = {
            let mut experience = self.experience.lock();
            if experience.dirty {
                experience.dirty = false;
                Some(CSetExperience {
                    progress: experience.progress(),
                    level: experience.level(),
                    total_experience: experience.total_points(),
                })
            } else {
                None
            }
        };
        if let Some(packet) = experience_packet {
            self.send_packet(packet);
        }

        // Vanilla parity: the `CriteriaTriggers.TICK.trigger(this)` of
        // `ServerPlayer.tick` and the twenty-tick `CriteriaTriggers.LOCATION`
        // of `ServerPlayer.doTick`, both of which run before the flush below.
        triggers::world::tick(self);
        if self.tick_count() % 20 == 0 {
            triggers::world::location(self);
        }
        self.flush_dirty_advancements();
        self.tick_time_statistics();

        self.connection.tick();
    }

    /// Ticks the death animation timer.
    /// Vanilla: `LivingEntity.tickDeath()` (not overridden by `ServerPlayer`).
    fn tick_death(&self) {
        let death_time = self.living_base.increment_death_time();

        if death_time >= DEATH_DURATION && !self.is_removed() {
            let world = self.get_world();
            let chunk_pos = *self.last_chunk_pos.lock();
            world.broadcast_to_nearby(
                chunk_pos,
                CEntityEvent {
                    entity_id: self.id(),
                    event: EntityStatus::Poof,
                },
                None,
            );

            world.unregister_player_entity(self);
            world.chunk_map.remove_player(self);
            world.entity_tracker().on_player_leave(self);
            world.player_area_map.remove_by_entity_id(self.id());
            self.set_removed(RemovalReason::Killed);
            assert_eq!(
                self.remove_all_menus_with_disposition(MenuItemDisposition::Drop),
                MenuRemovalStatus::Complete,
                "death removal menu cleanup must run outside a menu callback"
            );
        }
    }

    /// Immediately flushes dirty player entity data to tracking players and self.
    fn sync_entity_data(&self) {
        if let Some(dirty_values) = self.entity_data.lock().pack_dirty() {
            let packet = CSetEntityData::new(self.id(), dirty_values);
            self.get_world()
                .broadcast_to_entity_trackers(self.id(), packet.clone(), None);
            self.send_packet(packet);
        }
    }

    fn update_dirty_mob_effect_entity_data(&self) {
        if !self.living_base.take_effects_dirty() {
            return;
        }

        let mut display = self.living_base.mob_effect_display_state();
        if self.game_mode() == GameType::Spectator {
            display.particles = ParticleList::default();
            display.invisible = true;
        }

        {
            let mut entity_data = self.entity_data.lock();
            let living = entity_data.living_entity_mut();
            living.effect_particles.set(display.particles);
            living.effect_ambience.set(display.ambient);
        }

        self.entity_data.set_base_invisible_flag(display.invisible);
        self.entity_data
            .set_base_glowing_flag(self.has_glowing_tag() || display.glowing);
    }

    /// This player as a shared handle, or `None` once nothing holds them.
    ///
    /// `None` means the player has already been dropped, which a caller
    /// reached from a live `&Player` will not normally see.
    #[must_use]
    pub fn shared(&self) -> Option<Arc<Self>> {
        self.shared.upgrade()
    }

    /// Offers an otherwise unknown custom payload to protocol extensions.
    pub fn handle_custom_payload(&self, packet: SCustomPayload) {
        let Some(player) = self.shared() else {
            return;
        };
        let mut event = PlayerCustomPayloadEvent::new(player, packet.identifier, packet.payload.0);
        self.fire_event(&mut event);
    }

    /// Files the report a player submitted through the `/bug` form.
    ///
    /// Vanilla parity: `ServerCommonPacketListenerImpl.handleCustomClickAction`,
    /// which hands the action to whatever the server registered for it. Foton
    /// registers one.
    ///
    /// Everything here comes from the client, so nothing here is trusted: an
    /// unknown action is ignored, an unknown category falls back rather than
    /// failing, and the description is bounded before it reaches the disk.
    pub fn handle_custom_click_action(&self, packet: &SCustomClickAction) {
        if packet.id != Identifier::from_foton(bug_dialog::BUG_REPORT_ACTION) {
            return;
        }

        let description = packet
            .payload
            .string(bug_dialog::DESCRIPTION_KEY)
            .unwrap_or_default();
        let description = description.trim();
        if description.is_empty() {
            self.send_bug_feedback("Nothing was written, so nothing was filed.");
            return;
        }
        let description: String = description.chars().take(MAX_DESCRIPTION).collect();

        // A category the server does not know is a client that sent something
        // of its own. Filing under `Other` keeps the report rather than the
        // argument about it.
        let category = packet
            .payload
            .string(bug_dialog::CATEGORY_KEY)
            .and_then(|name| BugCategory::parse(&name))
            .unwrap_or(BugCategory::Other);

        let position = self.position();
        let world = self.get_world();
        let report = BugReport::now(
            self.gameprofile.name.clone(),
            self.gameprofile.id.to_string(),
            world.key.to_string(),
            [position.x, position.y, position.z],
            category,
            description,
        );

        let run_dir = current_dir().unwrap_or_else(|_| PathBuf::from("."));
        match report.append_in(&run_dir) {
            Ok(number) => {
                log::info!(
                    "bug report #{number} from {} [{}]: {}",
                    report.player,
                    report.category.name(),
                    report.description.lines().next().unwrap_or_default()
                );
                if let Some(webhook) = self.config.bug_report_webhook.as_ref() {
                    forward(webhook, &report, number);
                }
                self.send_bug_feedback(&format!("Filed report #{number}. Thanks."));
            }
            Err(error) => {
                log::error!("failed to write a bug report: {error}");
                self.send_bug_feedback("The report could not be saved. Tell an operator.");
            }
        }
    }

    /// Answers the reporter, and only them.
    fn send_bug_feedback(&self, message: &str) {
        self.send_packet(CSystemChat {
            content: TextComponent::from(message.to_owned()),
            overlay: false,
        });
    }

    /// Handles the end of a client tick.
    pub fn handle_client_tick_end(&self) {
        self.movement.lock().finish_client_tick();
    }

    /// Main entry point for dealing damage. Returns `true` if damage was applied.
    ///
    /// `world` is vanilla's explicit `ServerLevel` argument and controls
    /// difficulty scaling and damage gamerules.
    pub fn hurt(&self, world: &World, source: &DamageSource, amount: f32) -> bool {
        if LivingEntity::is_invulnerable_to(self, world, source) {
            return false;
        }

        {
            let abilities = self.abilities.lock();
            if abilities.invulnerable && !source.bypasses_invulnerability() {
                return false;
            }
        }

        // TODO: reset player noActionTime.
        self.remove_entities_on_shoulder();
        if self.get_health() <= 0.0 {
            return false;
        }

        // Difficulty scaling (vanilla: Player.hurtServer)
        let mut amount = amount;
        let causing_entity = source
            .causing_entity_id
            .and_then(|entity_id| world.get_entity_by_id(entity_id));
        if source.scales_with_difficulty(causing_entity.as_deref()) {
            let difficulty = world.level_data.read().data().difficulty;
            match difficulty {
                Difficulty::Peaceful => {
                    amount = 0.0;
                }
                Difficulty::Easy => {
                    amount = (amount / 2.0 + 1.0).min(amount);
                }
                Difficulty::Hard => {
                    amount = amount * 3.0 / 2.0;
                }
                Difficulty::Normal => {}
            }
        }

        if amount == 0.0 {
            return false;
        }

        LivingEntity::hurt_server(self, world, source, amount)
    }

    fn disabled_damage_game_rule(source: &DamageSource) -> Option<GameRuleRef<bool>> {
        if source.is(&vanilla_damage_type_tags::DamageTypeTag::IS_DROWNING) {
            Some(&DROWNING_DAMAGE)
        } else if source.is(&vanilla_damage_type_tags::DamageTypeTag::IS_FALL) {
            Some(&FALL_DAMAGE)
        } else if source.is(&vanilla_damage_type_tags::DamageTypeTag::IS_FIRE) {
            Some(&FIRE_DAMAGE)
        } else if source.is(&vanilla_damage_type_tags::DamageTypeTag::IS_FREEZING) {
            Some(&FREEZE_DAMAGE)
        } else {
            None
        }
    }

    /// Applies vanilla player damage reductions and health loss.
    fn actually_hurt(&self, world: &World, source: &DamageSource, amount: f32) {
        if LivingEntity::is_invulnerable_to(self, world, source) {
            return;
        }

        let damage = LivingEntity::get_damage_after_armor_absorb(self, source, amount);
        let damage = LivingEntity::get_damage_after_magic_absorb(self, source, damage);
        let original_damage = damage;
        let damage = (damage - self.get_absorption_amount()).max(0.0);
        self.set_absorption_amount(self.get_absorption_amount() - (original_damage - damage));

        // TODO: combat tracker (getCombatTracker().recordDamage)
        if damage != 0.0 {
            self.cause_food_exhaustion(source.damage_type.exhaustion);
            self.set_health(self.get_health() - damage);
            self.game_event(&vanilla_game_events::ENTITY_DAMAGE);
        }
    }

    /// Vanilla: `ServerPlayer.die()` (does NOT call `super.die()`).
    fn die(&self, source: &DamageSource) {
        if self.is_removed() {
            return;
        }
        if !self.living_base.mark_death_processed() {
            return;
        }

        self.game_event(&vanilla_game_events::ENTITY_DIE);

        self.sync_entity_data();

        // NOTE: Vanilla `ServerPlayer.die()` does NOT set Pose::Dying — only
        // `LivingEntity.die()` does (which ServerPlayer never calls via super).
        // The death screen covers the player model, so the pose is irrelevant.

        let world = self.get_world();

        // Broadcast entity event 3 (death sound) to all nearby players.
        let chunk_pos = *self.last_chunk_pos.lock();
        world.broadcast_to_nearby(
            chunk_pos,
            CEntityEvent {
                entity_id: self.id(),
                event: EntityStatus::Death,
            },
            None,
        );

        let show_death_messages = world.get_game_rule(&SHOW_DEATH_MESSAGES);

        // TODO: `CombatTracker` proper, for the fall-damage variants and the
        // "intentional game design" message. Everything else a death line needs
        // is in the damage source itself.
        // Vanilla parity: `LivingEntity.getKillCredit`. The last player to hurt
        // them wins over the last mob, which is what makes a shove into lava
        // read as a murder rather than an accident.
        let kill_credit = self
            .living_base
            .last_hurt_by_player_uuid()
            .and_then(|uuid| world.get_entity_by_uuid(&uuid))
            .or_else(|| self.living_base.last_hurt_by_mob());
        // Vanilla parity: the `killCredit.awardKillScore(this, source)` of
        // `LivingEntity.die`. `ServerPlayer.die` does not call super, so vanilla
        // reaches it through `ServerPlayer.dieFromDamage`; either way the credit
        // is the same value the death message is written from.
        // Vanilla parity: the statistics `ServerPlayer.die` awards around the
        // kill credit, and the two counters it resets. `TIME_SINCE_REST` is not
        // decoration -- vanilla's phantom spawner reads it.
        if let Some(credit) = kill_credit.as_deref() {
            self.award_stat(Stat::new(
                &vanilla_stat_types::KILLED_BY,
                credit.entity_type(),
            ));
            kill_score::award_kill_score(credit, self, source);
        }
        self.award_custom_stat(&vanilla_custom_stats::DEATHS);
        self.reset_stat(Stat::custom(&vanilla_custom_stats::TIME_SINCE_DEATH));
        self.reset_stat(Stat::custom(&vanilla_custom_stats::TIME_SINCE_REST));
        let death_message = source.localized_death_message(&world, self, kill_credit.as_deref());
        let drops =
            if !world.get_game_rule(&KEEP_INVENTORY) && self.game_mode() != GameType::Spectator {
                self.inventory.lock().take_death_drops()
            } else {
                Vec::new()
            };
        let mut death_event = PlayerDeathEvent::with_drops(
            self.gameprofile.id,
            death_message.to_plain(&DisplayResolutor),
            drops,
            world.get_game_rule(&KEEP_INVENTORY) || self.game_mode() == GameType::Spectator,
        );
        self.fire_event(&mut death_event);
        let death_message = match death_event.death_message() {
            Some(message) => TextComponent::plain(message.to_owned()),
            None => TextComponent::const_plain(""),
        };

        self.send_packet(CPlayerCombatKill {
            player_id: self.id(),
            message: if show_death_messages {
                death_message.clone()
            } else {
                TextComponent::const_plain("")
            },
        });

        // TODO: team death message visibility (ALWAYS / HIDE_FOR_OTHER_TEAMS / HIDE_FOR_OWN_TEAM)
        if show_death_messages {
            world.broadcast_system_chat(CSystemChat {
                content: death_message,
                overlay: false,
            });
        }

        if !world.get_game_rule(&KEEP_INVENTORY) && self.game_mode() != GameType::Spectator {
            for item in death_event.drops().iter().cloned() {
                let _ = self.drop_item(item, true, false);
            }

            let reward = self.experience.lock().death_xp_reward();
            if reward > 0 {
                ExperienceOrbEntity::award(&world, self.position(), reward);
            }
        }

        self.clear_fire();
        self.set_ticks_frozen(0);

        if world.get_game_rule(&IMMEDIATE_RESPAWN) {
            self.respawn();
        }
    }

    /// Returns whether the Player can eat
    pub fn can_eat(&self, can_always_eat: bool) -> bool {
        let invulnerable = { self.abilities.lock().invulnerable };
        let needs_foods = { self.food_data.lock().needs_food() };
        invulnerable || can_always_eat || needs_foods
    }

    /// Cleans up player resources.
    #[expect(clippy::unused_self, reason = "this is an api function")]
    pub const fn cleanup(&self) {}

    /// Returns the world the player is currently in.
    pub fn get_world(&self) -> Arc<World> {
        self.world.load_full()
    }

    /// Returns the server this player belongs to.
    pub(crate) fn server(&self) -> Arc<Server> {
        self.server
            .upgrade()
            .expect("player must not outlive server")
    }

    /// Fires an event on this player's server, if they have one.
    ///
    /// [`Self::server`] panics without one, and a player built without a
    /// server is normal in tests. A dispatch point on a gameplay path must not
    /// turn that into a failure of whatever the test was actually about.
    pub(crate) fn fire_event<E: Event>(&self, event: &mut E) {
        if let Some(server) = self.server.upgrade() {
            server.events.fire(event);
        }
    }

    /// Returns the identity of the player's current continuous domain stay.
    pub(crate) fn domain_residence_token(&self) -> DomainResidenceToken {
        self.residence.lock().token
    }

    /// Starts a new continuous domain stay and invalidates old restore work.
    pub(crate) fn advance_domain_residence(&self) -> DomainResidenceToken {
        self.residence.lock().advance()
    }

    /// Returns whether delayed work still belongs to the current domain stay.
    pub(crate) fn is_domain_residence_current(&self, token: DomainResidenceToken) -> bool {
        self.residence.lock().token == token
    }

    /// Installs both persisted restore payloads for a token-owned domain stay.
    pub(crate) fn install_pending_domain_restores(
        &self,
        token: DomainResidenceToken,
        world: &World,
        root_vehicle: Option<PersistentRootVehicle>,
        ender_pearls: Vec<PersistentEnderPearl>,
    ) -> bool {
        let mut residence = self.residence.lock();
        if residence.token != token {
            return false;
        }

        residence.pending_root_vehicle =
            root_vehicle.map(|root_vehicle| PendingRootVehicleRestore {
                world: world.key.clone(),
                root_vehicle,
            });
        residence.pending_ender_pearls = ender_pearls;
        true
    }

    pub(crate) fn clear_pending_root_vehicle(&self) {
        self.residence.lock().pending_root_vehicle = None;
    }

    pub(crate) fn pending_root_vehicle_for_current_world(&self) -> Option<PersistentRootVehicle> {
        let world_key = self.get_world().key.clone();
        self.residence
            .lock()
            .pending_root_vehicle
            .as_ref()
            .filter(|pending| pending.world == world_key)
            .map(|pending| pending.root_vehicle.clone())
    }

    pub(crate) fn take_matching_pending_root_vehicle(
        &self,
        token: DomainResidenceToken,
        world: &World,
        attach: [u8; 16],
        root_uuid: [u8; 16],
    ) -> Option<PersistentRootVehicle> {
        let mut residence = self.residence.lock();
        if residence.token != token {
            return None;
        }
        let matches = residence
            .pending_root_vehicle
            .as_ref()
            .is_some_and(|pending| {
                pending.world == world.key
                    && pending.root_vehicle.attach == attach
                    && pending.root_vehicle.entity.uuid == root_uuid
            });
        if matches {
            residence
                .pending_root_vehicle
                .take()
                .map(|pending| pending.root_vehicle)
        } else {
            None
        }
    }

    pub(crate) fn pending_ender_pearls(&self) -> Vec<PersistentEnderPearl> {
        self.residence.lock().pending_ender_pearls.clone()
    }

    pub(crate) fn remove_pending_ender_pearl(&self, uuid: Uuid) {
        self.residence
            .lock()
            .pending_ender_pearls
            .retain(|pearl| Uuid::from_bytes(pearl.entity.uuid) != uuid);
    }

    pub(crate) fn discard_pending_ender_pearl(
        &self,
        token: DomainResidenceToken,
        uuid: Uuid,
    ) -> bool {
        let mut residence = self.residence.lock();
        if residence.token != token {
            return false;
        }
        let old_len = residence.pending_ender_pearls.len();
        residence
            .pending_ender_pearls
            .retain(|pearl| Uuid::from_bytes(pearl.entity.uuid) != uuid);
        residence.pending_ender_pearls.len() != old_len
    }

    pub(crate) fn take_matching_pending_ender_pearl(
        &self,
        token: DomainResidenceToken,
        world: &World,
        uuid: Uuid,
    ) -> Option<PersistentEnderPearl> {
        let mut residence = self.residence.lock();
        if residence.token != token {
            return None;
        }
        let world_key = world.key.to_string();
        let index = residence.pending_ender_pearls.iter().position(|pearl| {
            pearl.world == world_key && Uuid::from_bytes(pearl.entity.uuid) == uuid
        })?;
        Some(residence.pending_ender_pearls.remove(index))
    }

    /// Returns the fishing hook this player currently has out, if any.
    ///
    /// Vanilla `Player.fishing`. A hook already removed from the world reads as
    /// absent, so a rod whose bobber was killed casts again instead of trying to
    /// reel in a corpse.
    #[must_use]
    pub fn fishing_hook(&self) -> Option<SharedEntity> {
        let mut fishing = self.fishing.lock();
        match fishing.as_ref().and_then(Weak::upgrade) {
            Some(hook) if !hook.is_removed() => Some(hook),
            _ => {
                *fishing = None;
                None
            }
        }
    }

    /// Sets or clears this player's fishing hook (vanilla `Player.fishing`).
    pub fn set_fishing_hook(&self, hook: Option<&SharedEntity>) {
        *self.fishing.lock() = hook.map(Arc::downgrade);
    }

    /// Returns where this player absorbed a Bad Omen, if one is pending.
    ///
    /// Vanilla parity: `ServerPlayer.getRaidOmenPosition`.
    #[must_use]
    pub fn raid_omen_position(&self) -> Option<BlockPos> {
        *self.raid_omen_position.lock()
    }

    /// Remembers where this player absorbed a Bad Omen.
    ///
    /// Vanilla parity: `ServerPlayer.setRaidOmenPosition`.
    pub fn set_raid_omen_position(&self, pos: BlockPos) {
        *self.raid_omen_position.lock() = Some(pos);
    }

    /// Forgets the pending raid position.
    ///
    /// Vanilla parity: `ServerPlayer.clearRaidOmenPosition`.
    pub fn clear_raid_omen_position(&self) {
        *self.raid_omen_position.lock() = None;
    }

    /// Registers a thrown ender pearl so it persists with this player and
    /// re-spawns on login (vanilla `ServerPlayer.registerEnderPearl`).
    pub fn register_ender_pearl(&self, pearl: &SharedEntity) {
        let uuid = pearl.uuid();
        let mut pearls = self.ender_pearls.lock();
        pearls.retain(|weak| {
            weak.upgrade()
                .is_some_and(|p| !p.is_removed() && p.uuid() != uuid)
        });
        pearls.push(Arc::downgrade(pearl));
        drop(pearls);
        self.remove_pending_ender_pearl(uuid);
    }

    /// Deregisters a thrown ender pearl once it hits, teleports, or is discarded
    /// (vanilla `ServerPlayer.deregisterEnderPearl`).
    pub fn deregister_ender_pearl(&self, uuid: Uuid) {
        self.ender_pearls
            .lock()
            .retain(|weak| weak.upgrade().is_some_and(|p| p.uuid() != uuid));
    }

    /// Returns this player's live, in-flight ender pearls, pruning dead entries.
    #[must_use]
    pub fn ender_pearls(&self) -> Vec<SharedEntity> {
        let mut pearls = self.ender_pearls.lock();
        pearls.retain(|weak| weak.upgrade().is_some_and(|p| !p.is_removed()));
        pearls.iter().filter_map(Weak::upgrade).collect()
    }

    /// Appends vanilla-shaped player state used by command NBT predicates.
    pub(crate) fn save_command_nbt(&self, nbt: &mut NbtCompound) {
        {
            let inventory = self.inventory.lock();
            nbt.insert("Inventory", inventory.to_vanilla_inventory_nbt());
            nbt.insert("SelectedItemSlot", i32::from(inventory.get_selected_slot()));
        }

        {
            let experience = self.experience.lock();
            nbt.insert("XpP", experience.progress());
            nbt.insert("XpLevel", experience.level());
            nbt.insert("XpTotal", experience.total_points());
        }
        nbt.insert("XpSeed", self.enchantment_seed());
        nbt.insert("Score", self.score());

        {
            let food = self.food_data.lock();
            nbt.insert("foodLevel", food.food_level);
            nbt.insert("foodTickTimer", food.tick_timer);
            nbt.insert("foodSaturationLevel", food.saturation_level);
            nbt.insert("foodExhaustionLevel", food.exhaustion_level);
        }

        {
            let abilities = self.abilities.lock();
            let mut abilities_nbt = NbtCompound::new();
            abilities_nbt.insert(
                "invulnerable",
                NbtTag::Byte(i8::from(abilities.invulnerable)),
            );
            abilities_nbt.insert("flying", NbtTag::Byte(i8::from(abilities.flying)));
            abilities_nbt.insert("mayfly", NbtTag::Byte(i8::from(abilities.may_fly)));
            abilities_nbt.insert("instabuild", NbtTag::Byte(i8::from(abilities.instabuild)));
            abilities_nbt.insert("mayBuild", NbtTag::Byte(i8::from(abilities.may_build)));
            abilities_nbt.insert("flySpeed", abilities.flying_speed);
            abilities_nbt.insert("walkSpeed", abilities.walking_speed);
            nbt.insert("abilities", NbtTag::Compound(abilities_nbt));
        }

        nbt.insert("playerGameType", self.game_mode() as i32);
        if let Some(previous_game_mode) = self.previous_game_mode() {
            nbt.insert("previousPlayerGameType", previous_game_mode as i32);
        }
        nbt.insert(
            "seenCredits",
            NbtTag::Byte(i8::from(self.has_seen_credits())),
        );
        nbt.insert("Dimension", self.get_world().key.to_string());

        if let Some(vehicle) = self.vehicle()
            && let Some(root_vehicle) = self.root_vehicle()
            && root_vehicle.id() != self.id()
            && root_vehicle.has_exactly_one_player_passenger()
            && let Some(entity_nbt) = root_vehicle.nbt_for_passenger_save()
        {
            let mut root_vehicle_nbt = NbtCompound::new();
            root_vehicle_nbt.insert(
                "Attach",
                NbtTag::IntArray(vehicle.uuid().to_int_array().to_vec()),
            );
            root_vehicle_nbt.insert("Entity", NbtTag::Compound(entity_nbt));
            nbt.insert("RootVehicle", NbtTag::Compound(root_vehicle_nbt));
        }

        let ender_pearls = self
            .ender_pearls()
            .into_iter()
            .filter_map(|pearl| {
                let world = pearl.level()?;
                let mut pearl_nbt = pearl.nbt_for_passenger_save()?;
                pearl_nbt.insert("ender_pearl_dimension", world.key.to_string());
                Some(pearl_nbt)
            })
            .collect::<Vec<_>>();
        if !ender_pearls.is_empty() {
            nbt.insert("ender_pearls", NbtList::Compound(ender_pearls));
        }
    }

    /// Marks live ender pearls as stored with this player so chunk saves remove
    /// them from world storage and player data remains the sole owner.
    pub fn store_ender_pearls_with_player(&self) {
        for pearl in self.ender_pearls() {
            let world = pearl.level();
            let chunk = ChunkPos::from_entity_pos(pearl.position());
            pearl.set_removed(RemovalReason::StoredWithPlayer);
            if let Some(world) = world {
                world.mark_chunk_dirty(chunk);
            }
        }
    }

    /// Returns this player's local server tick count.
    #[must_use]
    pub fn tick_count(&self) -> i32 {
        self.tick_state.lock().tick_count()
    }

    /// Returns vanilla `Player.takeXpDelay`.
    #[must_use]
    pub(crate) fn take_xp_delay(&self) -> i32 {
        self.tick_state.lock().take_xp_delay()
    }

    /// Sets vanilla `Player.takeXpDelay`.
    pub(crate) fn set_take_xp_delay(&self, delay: i32) {
        self.tick_state.lock().set_take_xp_delay(delay);
    }

    fn primary_step_sound_block_pos(&self, affecting_pos: BlockPos) -> BlockPos {
        let above_pos = affecting_pos.above();
        let above_state = self.get_world().get_block_state(above_pos);
        let above_block = above_state.get_block();

        if above_block.has_tag(&BlockTag::INSIDE_STEP_SOUND_BLOCKS)
            || above_block.has_tag(&BlockTag::COMBINATION_STEP_SOUND_BLOCKS)
        {
            above_pos
        } else {
            affecting_pos
        }
    }
}

impl Entity for Player {
    fn base(&self) -> &EntityBase {
        &self.base
    }

    fn entity_type(&self) -> EntityTypeRef {
        &vanilla_entities::PLAYER
    }

    /// Vanilla parity: `Player.getSlot`, in its order.
    ///
    /// The player is the only entity with four numeric ranges of its own: the
    /// cursor, its own crafting grid, the storage slots and the ender chest.
    /// Everything else falls through to the equipment `LivingEntity` owns.
    fn slot_item(&self, slot: i32) -> Option<ItemStack> {
        if slot == CURSOR_AND_MOUNT_CHEST_SLOT {
            return Some(self.carried_item());
        }

        let crafting_slot = slot - PLAYER_CRAFTING_SLOT_OFFSET;
        if (0..PLAYER_CRAFTING_SIZE).contains(&crafting_slot) {
            return self.inventory_menu.lock().crafting_slot_item(crafting_slot);
        }

        // Vanilla bounds this on `getNonEquipmentItems().size()`. Foton's
        // player container runs longer than that -- it keeps the worn
        // equipment inline behind the storage slots -- and those seven belong
        // to the `armor.*`, `weapon.*` and `saddle` names instead.
        if (0..PlayerInventory::INVENTORY_SIZE as i32).contains(&slot) {
            return container_slot_item(&*self.inventory.lock(), slot);
        }

        let ender_slot = slot - ENDER_CHEST_SLOT_OFFSET;
        if ender_slot >= 0
            && let Some(item) = container_slot_item(&*self.ender_chest.lock(), ender_slot)
        {
            return Some(item);
        }

        self.entity_slot_item(slot)
    }

    fn base_tick(&self) {
        LivingEntity::base_tick_living_entity(self);
    }

    /// Vanilla parity: `Player.playSound`, which is not `Entity.playSound`.
    ///
    /// The difference is the excluded listener, and it is not an
    /// optimization. `ClientLevel.playSeededSound` plays a sound only when
    /// its excluded listener *is* the local player, so the exclusion is how
    /// vanilla says "this player's own client already made this noise". Every
    /// damage event makes the receiving client run
    /// `LivingEntity.handleDamageEvent`, which plays the hurt sound exactly
    /// that way; sending the broadcast to the victim as well gives them a
    /// second copy of a sound they have already heard. Against a rhythm like
    /// burning that reads as two damage ticks landing on top of each other.
    ///
    /// Vanilla's override also drops the `isSilent` guard, and so does this.
    fn play_sound(&self, sound: SoundEventRef, volume: f32, pitch: f32) {
        if let Some(world) = self.level() {
            world.play_sound_at(
                sound,
                self.sound_source(),
                self.position(),
                volume,
                pitch,
                Some(self.id()),
            );
        }
    }

    fn scoreboard_name(&self) -> String {
        self.gameprofile.name.clone()
    }

    fn name(&self) -> TextComponent {
        TextComponent::plain(self.gameprofile.name.clone())
    }

    fn display_name(&self) -> TextComponent {
        self.name()
            .click_event(ClickEvent::suggest_command(format!(
                "/tell {} ",
                self.gameprofile.name
            )))
            .hover_event(HoverEvent::show_entity(
                "minecraft:player",
                self.uuid(),
                Some(self.name()),
            ))
            .insertion(self.gameprofile.name.clone())
    }

    fn plain_text_name(&self) -> String {
        self.gameprofile.name.clone()
    }

    fn look_at(&self, from_anchor: EntityAnchor, target: DVec3) {
        apply_entity_look_at(self, from_anchor, target);
        self.send_packet(CPlayerLookAt::position(
            protocol_look_at_anchor(from_anchor),
            target,
        ));
    }

    fn look_at_entity(
        &self,
        from_anchor: EntityAnchor,
        target: &dyn Entity,
        target_anchor: EntityAnchor,
    ) {
        let target_position = target_anchor.position(target);
        apply_entity_look_at(self, from_anchor, target_position);
        self.send_packet(CPlayerLookAt::entity(
            protocol_look_at_anchor(from_anchor),
            target_position,
            target.id(),
            protocol_look_at_anchor(target_anchor),
        ));
    }

    fn is_always_ticking(&self) -> bool {
        true
    }

    fn update_swimming(&self) {
        if self.is_flying() {
            self.set_shared_swimming(false);
        } else {
            self.default_update_swimming();
        }
    }

    fn stop_riding(&self) {
        let old_vehicle = self.vehicle();
        self.base().stop_riding();
        let Some(old_vehicle) = old_vehicle else {
            return;
        };

        self.remove_active_effects_for_vehicle(old_vehicle.as_ref());
        self.send_packet(CSetPassengers::new(
            old_vehicle.id(),
            Self::passenger_ids_for_packet(old_vehicle.as_ref()),
        ));
    }

    fn start_riding(&self, entity_to_ride: &SharedEntity) -> bool {
        let Some(world) = self.level() else {
            return false;
        };
        let Some(passenger) = world.get_entity_by_id(self.id()) else {
            return false;
        };
        if !start_riding_entities(&passenger, entity_to_ride) {
            return false;
        }

        entity_to_ride.position_rider(self.as_entity_event_source());
        let position = self.position();
        let (yaw, pitch) = self.rotation();
        if let Err(error) = self.teleport(position, yaw, pitch) {
            // Under `panic = "abort"` this kills the server and every dirty
            // chunk with it. The identical failure is already a `log::warn!`
            // twenty-four lines up in `movement`, and the entity manager
            // rejects a move whenever the player has left `live_by_id` --
            // a disconnect racing a tick, not an invariant the compiler
            // holds. A desynchronized player is recoverable; a dead world
            // is not.
            log::error!(
                "failed to synchronize player {} mounted position: {error}",
                self.id()
            );
        }
        self.send_active_effects_for_vehicle(entity_to_ride.as_ref());
        self.send_packet(CSetPassengers::new(
            entity_to_ride.id(),
            Self::passenger_ids_for_packet(entity_to_ride.as_ref()),
        ));
        true
    }

    fn broadcast_to_player(&self, player: &Player) -> bool {
        if player.is_spectator() {
            true
        } else {
            !self.is_spectator()
        }
    }

    fn fall_sounds(&self) -> (SoundEventRef, SoundEventRef) {
        (
            &sound_events::ENTITY_PLAYER_SMALL_FALL,
            &sound_events::ENTITY_PLAYER_BIG_FALL,
        )
    }

    fn is_alive(&self) -> bool {
        !self.is_removed() && self.get_health() > 0.0
    }

    fn forces_fall_flying_velocity_sync(&self) -> bool {
        self.is_fall_flying()
    }

    fn blocks_building(&self) -> bool {
        true
    }

    fn is_pickable(&self) -> bool {
        !self.is_spectator() && !self.is_removed()
    }

    fn is_pushable(&self) -> bool {
        self.get_health() > 0.0 && !self.is_spectator() && !self.on_climbable()
    }

    fn on_climbable(&self) -> bool {
        Player::on_climbable(self)
    }

    fn is_spectator(&self) -> bool {
        self.game_mode() == GameType::Spectator
    }

    fn is_flying_player(&self) -> bool {
        self.is_flying()
    }

    fn fire_immune_ticks(&self) -> i32 {
        20
    }

    fn remaining_fire_ticks_cap(&self) -> Option<i32> {
        self.abilities.lock().invulnerable.then_some(1)
    }

    fn get_default_gravity(&self) -> f64 {
        LivingEntity::get_attribute_gravity(self)
    }

    fn fire_ignite_extra_ticks(&self) -> i32 {
        rand::random_range(1..=2)
    }

    fn can_freeze(&self) -> bool {
        if self.is_spectator() {
            return false;
        }

        self.default_living_can_freeze()
    }

    fn make_stuck_in_block(&self, state: BlockStateId, speed_multiplier: DVec3) {
        if !self.is_flying() {
            self.default_make_stuck_in_block(state, speed_multiplier);
        }

        // TODO: Reset current impulse context once vehicle/player impulse contexts exist.
    }

    fn can_be_hit_by_projectile(&self) -> bool {
        self.get_health() > 0.0 && self.is_pickable()
    }

    fn uses_client_movement_packets(&self) -> bool {
        true
    }

    fn can_simulate_movement(&self) -> bool {
        true
    }

    fn is_effective_ai(&self) -> bool {
        true
    }

    fn known_movement(&self) -> DVec3 {
        if let Some(vehicle) = self.vehicle()
            && vehicle
                .controlling_passenger()
                .is_none_or(|controller| controller.id() != self.id())
        {
            return vehicle.known_movement();
        }

        self.movement.lock().last_known_client_movement()
    }

    fn known_speed(&self) -> DVec3 {
        if let Some(vehicle) = self.vehicle()
            && vehicle
                .controlling_passenger()
                .is_none_or(|controller| controller.id() != self.id())
        {
            return vehicle.known_speed();
        }

        self.movement.lock().last_known_client_movement()
    }

    fn is_suppressing_bounce(&self) -> bool {
        self.is_crouching()
    }

    /// Vanilla parity: `ServerPlayer.onExplosionHit`.
    ///
    /// This is what makes wind-charge jumping survivable. The impulse
    /// machinery it arms was already here in full -- `current_impulse_impact_pos`,
    /// the grace time, and the `causeFallDamage` clamp that reads them -- but
    /// the mace was its only caller, so a blast never armed it and a player
    /// launched by a wind charge paid the whole fall on the way down.
    ///
    /// Vanilla names `EntityTypes.WIND_CHARGE` alone: a breeze's charge is
    /// deliberately not forgiving, and neither is any other explosion.
    ///
    /// MISSING FOUNDATION: vanilla also records `currentExplosionImpactPos`
    /// and `currentExplosionCause` here, which `MaceItem` reads to decide a
    /// smash attack's bonus. Foton has no equivalent pair yet, so only the
    /// fall-damage half of the method is ported.
    fn on_explosion_hit(&self, explosion_caused_by: Option<&SharedEntity>) {
        let from_wind_charge = explosion_caused_by
            .is_some_and(|source| source.entity_type() == &vanilla_entities::WIND_CHARGE);
        self.set_ignore_fall_damage_from_current_impulse(from_wind_charge, self.position());
    }

    fn cause_fall_damage(
        &self,
        fall_distance: f64,
        damage_modifier: f32,
        source: &DamageSource,
    ) -> bool {
        if self.abilities.lock().may_fly {
            return false;
        }

        // Vanilla parity: `Player.causeFallDamage`, which only counts a fall
        // of two blocks or more and counts it in centimeters.
        if fall_distance >= 2.0 {
            let centimeters = (fall_distance * 100.0).round();
            self.award_custom_stat_amount(
                &vanilla_custom_stats::FALL_ONE_CM,
                i32::try_from(centimeters as i64).unwrap_or(i32::MAX),
            );
        }
        LivingEntity::cause_living_fall_damage(self, fall_distance, damage_modifier, source)
    }

    fn synced_data(&self) -> Option<&dyn EntitySyncedData> {
        Some(&self.entity_data)
    }

    fn update_data_before_sync(&self) {
        self.update_dirty_mob_effect_entity_data();
    }

    fn max_up_step(&self) -> f32 {
        self.attributes()
            .lock()
            .get_value(vanilla_attributes::STEP_HEIGHT)
            .unwrap_or(0.6) as f32
    }

    fn backs_off_from_edge(&self) -> bool {
        self.is_crouching() && !self.is_flying()
    }

    fn is_pushed_by_fluid(&self) -> bool {
        !self.is_flying()
    }

    fn is_crouching(&self) -> bool {
        Player::is_crouching(self)
    }

    fn may_interact(&self, world: &World, pos: BlockPos) -> bool {
        world.may_interact(self, pos)
    }

    fn is_swimming(&self) -> bool {
        Player::is_swimming(self)
    }

    fn sound_source(&self) -> SoundSource {
        SoundSource::Players
    }

    fn swim_sound(&self) -> SoundEventRef {
        &sound_events::ENTITY_PLAYER_SWIM
    }

    fn play_step_sound(&self, on_pos: BlockPos, on_state: BlockStateId) {
        if self.is_in_water() {
            self.water_swim_sound();
            self.play_muffled_step_sound(on_state);
            return;
        }

        let primary_step_sound_pos = self.primary_step_sound_block_pos(on_pos);
        if primary_step_sound_pos == on_pos {
            self.play_block_step_sound(on_state);
        } else {
            let primary_state = self.get_world().get_block_state(primary_step_sound_pos);
            if primary_state
                .get_block()
                .has_tag(&BlockTag::COMBINATION_STEP_SOUND_BLOCKS)
            {
                self.play_combination_step_sounds(primary_state, on_state);
            } else {
                self.play_block_step_sound(primary_state);
            }
        }
    }

    fn movement_emission(&self) -> EntityMovementEmission {
        if self.is_flying() || self.on_ground() && self.is_discrete() {
            EntityMovementEmission::None
        } else {
            EntityMovementEmission::All
        }
    }

    fn on_below_world(&self) {
        let world = self.get_world();
        self.hurt(
            &world,
            &DamageSource::environment(&vanilla_damage_types::OUT_OF_WORLD),
            4.0,
        );
    }

    fn dimensions_for_pose(&self, pose: EntityPose) -> EntityDimensions {
        let dimensions = Player::dimensions_for_pose(pose);
        if pose == EntityPose::Sleeping || self.entity_type().fixed {
            dimensions
        } else {
            dimensions.scale(LivingEntity::get_scale(self))
        }
    }

    fn hurt(&self, world: &World, source: &DamageSource, amount: f32) -> bool {
        // Delegates to Player's inherent hurt method which handles
        // player-specific prechecks before the shared living hurt path.
        Player::hurt(self, world, source, amount)
    }
}

const fn protocol_look_at_anchor(anchor: EntityAnchor) -> LookAtAnchor {
    match anchor {
        EntityAnchor::Feet => LookAtAnchor::Feet,
        EntityAnchor::Eyes => LookAtAnchor::Eyes,
    }
}

impl LivingEntity for Player {
    fn living_synced_data(&self) -> Option<&dyn LivingEntitySyncedData> {
        Some(&self.entity_data)
    }

    fn tick_living_entity(&self) {
        Player::tick(self);
    }

    fn get_health(&self) -> f32 {
        *self.entity_data.lock().living_entity().health.get()
    }

    fn set_health(&self, health: f32) {
        let max_health = self.get_max_health();
        let clamped = health.clamp(0.0, max_health);
        self.entity_data
            .lock()
            .living_entity_mut()
            .health
            .set(clamped);
    }

    fn living_base(&self) -> &LivingEntityBase {
        &self.living_base
    }

    /// Vanilla parity: `Player.doesEmitEquipEvent`. Only worn armor rings a
    /// sculk sensor; a player swapping what they hold does not.
    fn does_emit_equip_event(&self, slot: EquipmentSlot) -> bool {
        slot.slot_type() == EquipmentSlotType::HumanoidArmor
    }

    fn is_using_item(&self) -> bool {
        self.living_base.is_using_item()
    }

    fn get_luck(&self) -> f32 {
        self.attributes()
            .lock()
            .required_value(vanilla_attributes::LUCK) as f32
    }

    fn can_be_seen_as_enemy(&self) -> bool {
        !self.abilities.lock().invulnerable
            && !self.is_invulnerable()
            && self.can_be_seen_by_anyone()
    }

    fn is_invulnerable_to(&self, world: &World, source: &DamageSource) -> bool {
        if self.is_invulnerable_to_base(source)
            || enchantment_helper::is_immune_to_damage(world, self, source)
        {
            return true;
        }

        if let Some(rule) = Self::disabled_damage_game_rule(source) {
            return !world.get_game_rule(rule);
        }

        !self.has_client_loaded()
    }

    fn hurt_armor(&self, source: &DamageSource, damage: f32) {
        self.do_hurt_equipment(
            source,
            damage,
            &[
                EquipmentSlot::Feet,
                EquipmentSlot::Legs,
                EquipmentSlot::Chest,
                EquipmentSlot::Head,
            ],
        );
    }

    fn actually_hurt(&self, world: &World, source: &DamageSource, amount: f32) {
        Player::actually_hurt(self, world, source, amount);
    }

    fn hurt_broadcast_chunk(&self) -> ChunkPos {
        *self.last_chunk_pos.lock()
    }

    fn die(&self, source: &DamageSource) {
        Player::die(self, source);
    }

    /// Vanilla parity: `Player.hurtHelmet`, the only override of it.
    fn hurt_helmet(&self, source: &DamageSource, damage: f32) {
        self.do_hurt_equipment(source, damage, &[EquipmentSlot::Head]);
    }

    fn with_equipment_slot(&self, slot: EquipmentSlot, visitor: &mut dyn FnMut(&ItemStack)) {
        let inventory = self.inventory.lock();
        visitor(inventory.get_ref(slot));
    }

    fn with_equipment_slot_mut(
        &self,
        slot: EquipmentSlot,
        visitor: &mut dyn FnMut(&mut ItemStack),
    ) {
        let mut inventory = self.inventory.lock();
        inventory.with_equipment_item_mut(slot, visitor);
    }

    fn interact_living_entity_with_equippable(
        &self,
        player: &Player,
        hand: InteractionHand,
    ) -> InteractionResult {
        let item_stack = {
            let inventory = player.inventory.lock();
            let item_stack = inventory.get_item_in_hand(hand);
            item_stack.copy_with_count(item_stack.count())
        };
        let Some(equippable) = item_stack.get_equippable() else {
            return InteractionResult::Pass;
        };
        if !equippable.equip_on_interact {
            return InteractionResult::Pass;
        }

        let slot = equippable.slot;
        let can_equip = |stack: &ItemStack| {
            stack.get_equippable().is_some_and(|equippable| {
                equippable.equip_on_interact
                    && equippable.slot == slot
                    && self.is_equippable_in_slot(stack, slot)
            })
        };
        if !can_equip(&item_stack) || !Entity::is_alive(self) {
            return InteractionResult::Pass;
        }

        let source_ref = ContainerRef::from(player.inventory.clone());
        let target_ref = ContainerRef::from(self.inventory.clone());
        let source_id = source_ref.container_id();
        let target_id = target_ref.container_id();
        let mut guard = ContainerLockGuard::lock_all(&[source_ref, target_ref]);
        let source_slot = match hand {
            InteractionHand::MainHand => EquipmentSlot::MainHand,
            InteractionHand::OffHand => EquipmentSlot::OffHand,
        };

        let equipped = if source_id == target_id {
            let Some(inventory) = guard.get_typed_mut::<PlayerInventory>(source_id) else {
                unreachable!("player inventory container retains its concrete type");
            };
            if !can_equip(inventory.get_item_in_hand(hand)) || !inventory.get_ref(slot).is_empty() {
                return InteractionResult::Pass;
            }

            let equipped = inventory.get_mut(source_slot).split(1);
            if equipped.is_empty() {
                return InteractionResult::Pass;
            }
            let equipped_for_effects = equipped.copy_with_count(1);
            *inventory.get_mut(slot) = equipped;
            equipped_for_effects
        } else {
            let Some((source_inventory, target_inventory)) =
                guard.get_two_typed_mut::<PlayerInventory, PlayerInventory>(source_id, target_id)
            else {
                unreachable!("player inventory containers retain their concrete type");
            };
            if !can_equip(source_inventory.get_item_in_hand(hand))
                || !target_inventory.get_ref(slot).is_empty()
            {
                return InteractionResult::Pass;
            }

            let equipped = source_inventory.get_mut(source_slot).split(1);
            if equipped.is_empty() {
                return InteractionResult::Pass;
            }
            let equipped_for_effects = equipped.copy_with_count(1);
            *target_inventory.get_mut(slot) = equipped;
            equipped_for_effects
        };
        drop(guard);

        player.inventory.lock().set_changed();
        if source_id != target_id {
            self.inventory.lock().set_changed();
        }

        // Vanilla routes this through `setItemSlot` and so through
        // `onEquipItem`, whose sound goes out to everyone.
        if let Some(sound) = self.equip_sound(slot, &equipped) {
            self.play_server_side_sound(sound, 1.0, 1.0);
        }
        // TODO: Emit EQUIP game event once game-event dispatch is implemented.
        InteractionResult::Success
    }

    fn has_infinite_materials(&self) -> bool {
        Player::has_infinite_materials(self)
    }

    /// Tilts the player's own screen toward whatever hit them.
    ///
    /// Vanilla parity: `ServerPlayer.indicateDamage`, the only override of
    /// `LivingEntity.indicateDamage` and the only sender of
    /// `ClientboundHurtAnimationPacket` in the game. Two things about it are
    /// easy to get wrong and were: the angle is the direction of the blow
    /// *relative to where the player is looking*, not the player's own yaw, and
    /// it goes to that one player rather than to everyone nearby -- a camera
    /// tilt means nothing to anyone else's camera. The red flash every observer
    /// does see is `broadcast_damage_event`, a different packet.
    ///
    /// Vanilla keeps the angle in `Player.hurtDir` because its own client
    /// reads the field back while rendering; a server has nothing to read it
    /// with, so it is computed here and sent.
    fn indicate_damage(&self, xd: f64, zd: f64) {
        let (yaw, _) = self.rotation();
        let hurt_dir = zd.atan2(xd).to_degrees() as f32 - yaw;
        self.send_packet(CHurtAnimation {
            entity_id: self.id(),
            yaw: hurt_dir,
        });
    }

    fn get_absorption_amount(&self) -> f32 {
        *self.entity_data.lock().player_absorption.get()
    }

    fn set_absorption_amount(&self, amount: f32) {
        let max_absorption = self
            .living_base
            .attributes()
            .lock()
            .required_value(vanilla_attributes::MAX_ABSORPTION) as f32;
        self.internal_set_absorption_amount(amount.clamp(0.0, max_absorption));
    }

    fn internal_set_absorption_amount(&self, amount: f32) {
        self.entity_data.lock().player_absorption.set(amount);
    }

    fn is_affected_by_fluids(&self) -> bool {
        !self.is_flying()
    }

    fn can_glide(&self) -> bool {
        !self.is_flying() && self.default_can_glide()
    }

    fn is_immobile(&self) -> bool {
        self.default_is_immobile() || self.is_sleeping()
    }

    fn stop_sleeping(&self) {
        self.stop_sleep_in_bed(true, true);
    }

    fn jump_from_ground(&self) {
        self.default_jump_from_ground();
        self.award_custom_stat(&vanilla_custom_stats::JUMP);
        if self.is_sprinting() {
            self.cause_food_exhaustion(0.2);
        } else {
            self.cause_food_exhaustion(0.05);
        }
    }

    fn ai_step(&self) -> Option<MoveResult> {
        if self.is_flying() && !self.is_passenger() {
            self.reset_fall_distance();
        }

        let result = self.default_ai_step();
        self.set_y_head_rot(self.rotation().0);
        // Vanilla parity: the `setSpeed(getAttributeValue(MOVEMENT_SPEED))` of
        // `Player.aiStep`. `Player.isSweepAttack` compares the player's known
        // movement against this, so leaving it at zero silently kills every
        // sweep attack.
        let movement_speed = self
            .attributes()
            .lock()
            .get_value(vanilla_attributes::MOVEMENT_SPEED)
            .unwrap_or(0.0) as f32;
        self.set_speed(movement_speed);
        // Vanilla parity: the `handleShoulderEntities` at the end of
        // `Player.aiStep`.
        self.handle_shoulder_entities();
        result
    }

    fn travel(&self, input: DVec3) -> Option<MoveResult> {
        if self.is_passenger() {
            return self.default_travel(input);
        }

        if self.is_swimming() {
            let look_angle_y = self.look_angle().y;
            let multiplier = if look_angle_y < -0.2 { 0.085 } else { 0.06 };
            let has_fluid_above = self.level().is_some_and(|world| {
                let position = self.position();
                let pos = BlockPos::containing(position.x, position.y + 0.9, position.z);
                !get_fluid_state(&world, pos).is_empty()
            });
            if look_angle_y <= 0.0 || self.is_jumping() || has_fluid_above {
                let velocity = self.velocity();
                self.set_velocity(
                    velocity + DVec3::new(0.0, (look_angle_y - velocity.y) * multiplier, 0.0),
                );
            }
        }

        if self.is_flying() {
            let original_movement_y = self.velocity().y;
            let result = self.default_travel(input);
            let velocity = self.velocity();
            self.set_velocity(DVec3::new(
                velocity.x,
                original_movement_y * 0.6,
                velocity.z,
            ));
            result
        } else {
            self.default_travel(input)
        }
    }

    fn get_flying_speed(&self) -> f32 {
        if self.is_flying() && !self.is_passenger() {
            let flying_speed = self.abilities.lock().flying_speed;
            if self.is_sprinting() {
                flying_speed * 2.0
            } else {
                flying_speed
            }
        } else if self.is_sprinting() {
            0.025_999_999
        } else {
            0.02
        }
    }
}

impl TextResolutor for Player {
    fn resolve_content(&self, _resolvable: &Resolvable) -> TextComponent {
        TextComponent::new()
    }

    fn resolve_custom(&self, _data: &CustomData) -> Option<TextComponent> {
        None
    }

    fn translate(&self, _key: &str) -> Option<String> {
        None
    }
}

#[cfg(test)]
mod tests;
