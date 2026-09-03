//! Events about a player arriving and leaving.
//!
//! These two first because they are what the ecosystem asks for most: of the
//! fifty-nine most-downloaded server plugins surveyed in
//! `dev/plugin-api-usage.json`, forty need the join and thirty-six the quit.
//! Two events reach two thirds of that corpus, which is why the measurement
//! came before the design.

use foton_utils::BlockPos;
use glam::DVec3;
use std::net::SocketAddr;
use std::sync::Arc;
use uuid::Uuid;

/// Fired immediately before one advancement criterion is granted.
///
/// Cancellation prevents the grant, matching Paper's event contract.
pub struct PlayerAdvancementCriterionGrantEvent {
    player_id: Uuid,
    advancement: String,
    criterion: String,
    cancelled: bool,
}
unsafe impl DowncastType for PlayerAdvancementCriterionGrantEvent {
    const TYPE_KEY: DowncastTypeKey =
        DowncastTypeKey::new("foton:event/player_advancement_criterion_grant");
}
impl Event for PlayerAdvancementCriterionGrantEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}
impl PlayerAdvancementCriterionGrantEvent {
    pub fn new(player_id: Uuid, advancement: String, criterion: String) -> Self {
        Self {
            player_id,
            advancement,
            criterion,
            cancelled: false,
        }
    }
    pub const fn player_id(&self) -> Uuid {
        self.player_id
    }
    pub fn advancement(&self) -> &str {
        &self.advancement
    }
    pub fn criterion(&self) -> &str {
        &self.criterion
    }
    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }
    pub const fn set_cancelled(&mut self, value: bool) {
        self.cancelled = value;
    }
}

/// Fired when a player completes an advancement.
pub struct PlayerAdvancementDoneEvent {
    player_id: Uuid,
    advancement: String,
}
unsafe impl DowncastType for PlayerAdvancementDoneEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/player_advancement_done");
}
impl Event for PlayerAdvancementDoneEvent {
    fn is_cancelled(&self) -> bool {
        false
    }
}
impl PlayerAdvancementDoneEvent {
    pub fn new(player_id: Uuid, advancement: String) -> Self {
        Self {
            player_id,
            advancement,
        }
    }
    pub const fn player_id(&self) -> Uuid {
        self.player_id
    }
    pub fn advancement(&self) -> &str {
        &self.advancement
    }
}

/// Asynchronous admission check before a player object is created.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AsyncPlayerPreLoginResult {
    Allowed,
    KickFull,
    KickBanned,
    KickWhitelist,
    KickOther,
}

/// A player is about to travel through a portal.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlayerOpenSignCause {
    Interact,
    Place,
}

pub struct PlayerOpenSignEvent {
    player_id: Uuid,
    world: String,
    position: BlockPos,
    front_side: bool,
    cause: PlayerOpenSignCause,
    cancelled: bool,
}

// SAFETY: This Foton-owned key uniquely identifies the concrete Rust type.
unsafe impl DowncastType for PlayerOpenSignEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/player_open_sign");
}
impl Event for PlayerOpenSignEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}
impl PlayerOpenSignEvent {
    pub fn new(
        player_id: Uuid,
        world: String,
        position: BlockPos,
        front_side: bool,
        cause: PlayerOpenSignCause,
    ) -> Self {
        Self {
            player_id,
            world,
            position,
            front_side,
            cause,
            cancelled: false,
        }
    }
    pub const fn player_id(&self) -> Uuid {
        self.player_id
    }
    pub fn world(&self) -> &str {
        &self.world
    }
    pub const fn position(&self) -> BlockPos {
        self.position
    }
    pub const fn front_side(&self) -> bool {
        self.front_side
    }
    pub const fn cause(&self) -> PlayerOpenSignCause {
        self.cause
    }
    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }
    pub const fn set_cancelled(&mut self, value: bool) {
        self.cancelled = value;
    }
}

pub struct PlayerPortalEvent {
    player_id: Uuid,
    from_world: String,
    from_position: DVec3,
    from_rotation: (f32, f32),
    to_world: String,
    to_position: DVec3,
    to_rotation: (f32, f32),
    cause: crate::portal::TeleportTransitionCause,
    cancelled: bool,
}
unsafe impl DowncastType for PlayerPortalEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/player_portal");
}
impl Event for PlayerPortalEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}
impl PlayerPortalEvent {
    pub fn new(
        player_id: Uuid,
        from_world: String,
        from_position: DVec3,
        from_rotation: (f32, f32),
        to_world: String,
        to_position: DVec3,
        to_rotation: (f32, f32),
        cause: crate::portal::TeleportTransitionCause,
    ) -> Self {
        Self {
            player_id,
            from_world,
            from_position,
            from_rotation,
            to_world,
            to_position,
            to_rotation,
            cause,
            cancelled: false,
        }
    }
    pub const fn player_id(&self) -> Uuid {
        self.player_id
    }
    pub fn from_world(&self) -> &str {
        &self.from_world
    }
    pub const fn from_position(&self) -> DVec3 {
        self.from_position
    }
    pub const fn from_rotation(&self) -> (f32, f32) {
        self.from_rotation
    }
    pub fn to_world(&self) -> &str {
        &self.to_world
    }
    pub const fn to_position(&self) -> DVec3 {
        self.to_position
    }
    pub const fn to_rotation(&self) -> (f32, f32) {
        self.to_rotation
    }
    pub const fn cause(&self) -> crate::portal::TeleportTransitionCause {
        self.cause
    }
    pub fn set_destination(
        &mut self,
        world: impl Into<String>,
        position: DVec3,
        rotation: (f32, f32),
    ) {
        self.to_world = world.into();
        self.to_position = position;
        self.to_rotation = rotation;
    }
    pub const fn set_cancelled(&mut self, value: bool) {
        self.cancelled = value;
    }
}
pub struct AsyncPlayerPreLoginEvent {
    uuid: Uuid,
    name: String,
    address: SocketAddr,
    result: AsyncPlayerPreLoginResult,
    kick_message: Option<String>,
}
unsafe impl DowncastType for AsyncPlayerPreLoginEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/async_pre_login");
}
impl Event for AsyncPlayerPreLoginEvent {
    fn is_cancelled(&self) -> bool {
        self.kick_message.is_some()
    }
}
impl AsyncPlayerPreLoginEvent {
    pub fn new(uuid: Uuid, name: String, address: SocketAddr) -> Self {
        Self {
            uuid,
            name,
            address,
            result: AsyncPlayerPreLoginResult::Allowed,
            kick_message: None,
        }
    }
    pub const fn uuid(&self) -> Uuid {
        self.uuid
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub const fn address(&self) -> SocketAddr {
        self.address
    }
    pub fn kick_message(&self) -> Option<&str> {
        self.kick_message.as_deref()
    }
    pub const fn result(&self) -> AsyncPlayerPreLoginResult {
        self.result
    }
    pub fn disallow(&mut self, result: AsyncPlayerPreLoginResult, message: impl Into<String>) {
        self.result = result;
        self.kick_message = Some(message.into());
    }
}

use foton_utils::downcast::{DowncastType, DowncastTypeKey};
use text_components::TextComponent;

use super::Event;
use crate::player::Player;
use foton_registry::item_stack::ItemStack;
use foton_utils::Identifier;

/// A player's food level is about to change.
pub struct FoodLevelChangeEvent {
    player_id: Uuid,
    food_level: i32,
    cancelled: bool,
}
// SAFETY: This Foton-owned key uniquely identifies the concrete Rust type.
unsafe impl DowncastType for FoodLevelChangeEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/food_level_change");
}
impl Event for FoodLevelChangeEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}
impl FoodLevelChangeEvent {
    pub const fn new(player_id: Uuid, food_level: i32) -> Self {
        Self {
            player_id,
            food_level,
            cancelled: false,
        }
    }
    pub const fn player_id(&self) -> Uuid {
        self.player_id
    }
    pub const fn food_level(&self) -> i32 {
        self.food_level
    }
    pub const fn set_food_level(&mut self, food_level: i32) {
        self.food_level = food_level;
    }
    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }
    pub const fn set_cancelled(&mut self, cancelled: bool) {
        self.cancelled = cancelled;
    }
}

/// A player is about to empty a filled bucket.
pub struct PlayerBucketEmptyEvent {
    player_id: Uuid,
    world: String,
    position: BlockPos,
    bucket: String,
    cancelled: bool,
}

/// A player is about to fill an empty bucket from a block or waterlogged block.
pub struct PlayerBucketFillEvent {
    player_id: Uuid,
    world: String,
    position: BlockPos,
    bucket: String,
    cancelled: bool,
}
unsafe impl DowncastType for PlayerBucketFillEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/player_bucket_fill");
}
impl Event for PlayerBucketFillEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}
impl PlayerBucketFillEvent {
    pub fn new(
        player_id: Uuid,
        world: impl Into<String>,
        position: BlockPos,
        bucket: impl Into<String>,
    ) -> Self {
        Self {
            player_id,
            world: world.into(),
            position,
            bucket: bucket.into(),
            cancelled: false,
        }
    }
    pub const fn player_id(&self) -> Uuid {
        self.player_id
    }
    pub fn world(&self) -> &str {
        &self.world
    }
    pub const fn position(&self) -> BlockPos {
        self.position
    }
    pub fn bucket(&self) -> &str {
        &self.bucket
    }
    pub const fn set_cancelled(&mut self, value: bool) {
        self.cancelled = value;
    }
}

/// A player's equipped item reached zero durability.
pub struct PlayerItemBreakEvent {
    player_id: Uuid,
    item: ItemStack,
}
unsafe impl DowncastType for PlayerItemBreakEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/player_item_break");
}
impl Event for PlayerItemBreakEvent {}
impl PlayerItemBreakEvent {
    pub fn new(player_id: Uuid, item: ItemStack) -> Self {
        Self { player_id, item }
    }
    pub const fn player_id(&self) -> Uuid {
        self.player_id
    }
    pub fn item(&self) -> &ItemStack {
        &self.item
    }
}

/// Fired when a fishing hook retrieves a catch.
pub struct PlayerFishEvent {
    player_id: Uuid,
    hook_id: Uuid,
    state: String,
    cancelled: bool,
}

// SAFETY: This Foton-owned key uniquely identifies the concrete Rust type.
unsafe impl DowncastType for PlayerFishEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/player_fish");
}
impl Event for PlayerFishEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}
impl PlayerFishEvent {
    pub fn new(player_id: Uuid, hook_id: Uuid, state: impl Into<String>) -> Self {
        Self {
            player_id,
            hook_id,
            state: state.into(),
            cancelled: false,
        }
    }
    pub const fn player_id(&self) -> Uuid {
        self.player_id
    }
    pub const fn hook_id(&self) -> Uuid {
        self.hook_id
    }
    pub fn state(&self) -> &str {
        &self.state
    }
    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }
    pub const fn set_cancelled(&mut self, cancelled: bool) {
        self.cancelled = cancelled;
    }
}
unsafe impl DowncastType for PlayerBucketEmptyEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/player_bucket_empty");
}
impl Event for PlayerBucketEmptyEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}
impl PlayerBucketEmptyEvent {
    pub fn new(
        player_id: Uuid,
        world: impl Into<String>,
        position: BlockPos,
        bucket: impl Into<String>,
    ) -> Self {
        Self {
            player_id,
            world: world.into(),
            position,
            bucket: bucket.into(),
            cancelled: false,
        }
    }
    pub const fn player_id(&self) -> Uuid {
        self.player_id
    }
    pub fn world(&self) -> &str {
        &self.world
    }
    pub const fn position(&self) -> BlockPos {
        self.position
    }
    pub fn bucket(&self) -> &str {
        &self.bucket
    }
    pub const fn set_cancelled(&mut self, value: bool) {
        self.cancelled = value;
    }
}

/// A player dropped an item entity into the world.
pub struct PlayerDropItemEvent {
    player_id: Uuid,
    item_id: Uuid,
    cancelled: bool,
}
// SAFETY: This Foton-owned key uniquely identifies the concrete Rust type.
unsafe impl DowncastType for PlayerDropItemEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/player_drop_item");
}
impl Event for PlayerDropItemEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}
impl PlayerDropItemEvent {
    pub const fn new(player_id: Uuid, item_id: Uuid) -> Self {
        Self {
            player_id,
            item_id,
            cancelled: false,
        }
    }
    pub const fn player_id(&self) -> Uuid {
        self.player_id
    }
    pub const fn item_id(&self) -> Uuid {
        self.item_id
    }
    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }
    pub const fn set_cancelled(&mut self, cancelled: bool) {
        self.cancelled = cancelled;
    }
}

/// A player has completed a death respawn.
pub struct PlayerRespawnEvent {
    player_id: Uuid,
    world: String,
    position: [f64; 3],
    rotation: (f32, f32),
    anchor_spawn: bool,
}

/// A player's initial spawn location may be redirected by a plugin.
pub struct PlayerSpawnLocationEvent {
    player_id: Uuid,
    world: String,
    position: [f64; 3],
    rotation: (f32, f32),
}
unsafe impl DowncastType for PlayerSpawnLocationEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/player_spawn_location");
}
impl Event for PlayerSpawnLocationEvent {}
impl PlayerSpawnLocationEvent {
    pub fn new(
        player_id: Uuid,
        world: impl Into<String>,
        position: [f64; 3],
        rotation: (f32, f32),
    ) -> Self {
        Self {
            player_id,
            world: world.into(),
            position,
            rotation,
        }
    }
    pub const fn player_id(&self) -> Uuid {
        self.player_id
    }
    pub fn world(&self) -> &str {
        &self.world
    }
    pub const fn position(&self) -> [f64; 3] {
        self.position
    }
    pub const fn rotation(&self) -> (f32, f32) {
        self.rotation
    }
    pub fn set_spawn(
        &mut self,
        world: impl Into<String>,
        position: [f64; 3],
        rotation: (f32, f32),
    ) {
        self.world = world.into();
        self.position = position;
        self.rotation = rotation;
    }
}
unsafe impl DowncastType for PlayerRespawnEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/player_respawn");
}
impl Event for PlayerRespawnEvent {}
impl PlayerRespawnEvent {
    pub fn new(
        player_id: Uuid,
        world: impl Into<String>,
        position: [f64; 3],
        rotation: (f32, f32),
        anchor_spawn: bool,
    ) -> Self {
        Self {
            player_id,
            world: world.into(),
            position,
            rotation,
            anchor_spawn,
        }
    }
    pub const fn player_id(&self) -> Uuid {
        self.player_id
    }
    pub fn world(&self) -> &str {
        &self.world
    }
    pub const fn position(&self) -> [f64; 3] {
        self.position
    }
    pub const fn rotation(&self) -> (f32, f32) {
        self.rotation
    }
    pub const fn is_anchor_spawn(&self) -> bool {
        self.anchor_spawn
    }
    pub fn set_spawn(
        &mut self,
        world: impl Into<String>,
        position: [f64; 3],
        rotation: (f32, f32),
    ) {
        self.world = world.into();
        self.position = position;
        self.rotation = rotation;
    }
}

/// A player has died, before the death drops are processed.
pub struct PlayerDeathEvent {
    player_id: Uuid,
    death_message: Option<String>,
    drops: Vec<ItemStack>,
    keep_inventory: bool,
}
// SAFETY: This Foton-owned key uniquely identifies the concrete Rust type.
unsafe impl DowncastType for PlayerDeathEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/player_death");
}
impl Event for PlayerDeathEvent {}
impl PlayerDeathEvent {
    pub fn new(player_id: Uuid, death_message: impl Into<String>) -> Self {
        Self {
            player_id,
            death_message: Some(death_message.into()),
            drops: Vec::new(),
            keep_inventory: false,
        }
    }
    pub fn with_drops(
        player_id: Uuid,
        death_message: impl Into<String>,
        drops: Vec<ItemStack>,
        keep_inventory: bool,
    ) -> Self {
        Self {
            player_id,
            death_message: Some(death_message.into()),
            drops,
            keep_inventory,
        }
    }
    pub const fn player_id(&self) -> Uuid {
        self.player_id
    }
    pub fn death_message(&self) -> Option<&str> {
        self.death_message.as_deref()
    }
    pub fn set_death_message(&mut self, message: Option<String>) {
        self.death_message = message;
    }
    pub fn drops(&self) -> &[ItemStack] {
        &self.drops
    }
    pub fn drops_mut(&mut self) -> &mut Vec<ItemStack> {
        &mut self.drops
    }
    pub const fn keep_inventory(&self) -> bool {
        self.keep_inventory
    }
    pub const fn set_keep_inventory(&mut self, keep_inventory: bool) {
        self.keep_inventory = keep_inventory;
    }
}

/// A player has completed protocol login and may enter the world.
pub struct PlayerLoginEvent {
    player: Arc<Player>,
    kick_message: Option<String>,
}

/// A player attempted to interact with an entity.
#[derive(Debug)]
pub struct PlayerInteractEntityEvent {
    player_id: Uuid,
    entity_id: Uuid,
    cancelled: bool,
}
// SAFETY: This Foton-owned key uniquely identifies the concrete Rust type.
unsafe impl DowncastType for PlayerInteractEntityEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/player_interact_entity");
}
impl Event for PlayerInteractEntityEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}
impl PlayerInteractEntityEvent {
    pub const fn new(player_id: Uuid, entity_id: Uuid) -> Self {
        Self {
            player_id,
            entity_id,
            cancelled: false,
        }
    }
    pub const fn player_id(&self) -> Uuid {
        self.player_id
    }
    pub const fn entity_id(&self) -> Uuid {
        self.entity_id
    }
    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }
    pub const fn set_cancelled(&mut self, cancelled: bool) {
        self.cancelled = cancelled;
    }
}

/// A player attempted to use an item or interact with the air/block.
pub struct PlayerInteractEvent {
    player_id: Uuid,
    cancelled: bool,
}

// SAFETY: This Foton-owned key uniquely identifies the concrete Rust type.
unsafe impl DowncastType for PlayerInteractEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/player_interact");
}

impl Event for PlayerInteractEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}

impl PlayerInteractEvent {
    #[must_use]
    /// Creates an uncancelled interaction event.
    pub const fn new(player_id: Uuid) -> Self {
        Self {
            player_id,
            cancelled: false,
        }
    }
    #[must_use]
    /// Returns the player's UUID.
    pub const fn player_id(&self) -> Uuid {
        self.player_id
    }
    #[must_use]
    /// Returns whether the interaction was cancelled.
    pub const fn cancelled(&self) -> bool {
        self.cancelled
    }
    /// Cancels or uncancels the interaction.
    pub const fn set_cancelled(&mut self, cancelled: bool) {
        self.cancelled = cancelled;
    }
}

// SAFETY: This Foton-owned key uniquely identifies the concrete Rust type.
unsafe impl DowncastType for PlayerLoginEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/player_login");
}

impl Event for PlayerLoginEvent {
    fn is_cancelled(&self) -> bool {
        self.kick_message.is_some()
    }
}

impl PlayerLoginEvent {
    #[must_use]
    /// Creates an allowed login event.
    pub const fn new(player: Arc<Player>) -> Self {
        Self {
            player,
            kick_message: None,
        }
    }
    #[must_use]
    /// Returns the logging-in player.
    pub const fn player(&self) -> &Arc<Player> {
        &self.player
    }
    #[must_use]
    /// Returns the denial message, if admission was denied.
    pub fn kick_message(&self) -> Option<&str> {
        self.kick_message.as_deref()
    }
    /// Denies admission with a kick message.
    pub fn deny(&mut self, message: String) {
        self.kick_message = Some(message);
    }
}

/// A player finished joining, and the server is about to announce it.
///
/// Not cancellable, matching `org.bukkit.event.player.PlayerJoinEvent`: by the
/// time this fires the player is in the world, and a listener that wanted to
/// stop them should have done it before they got there. What it can change is
/// the announcement.
pub struct PlayerJoinEvent {
    player: Arc<Player>,
    message: Option<TextComponent>,
}

// SAFETY: This Foton-owned key uniquely identifies the concrete Rust type
// within the process.
unsafe impl DowncastType for PlayerJoinEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/player_join");
}

impl Event for PlayerJoinEvent {}

impl PlayerJoinEvent {
    /// Creates the event with the announcement the server would make on its own.
    #[must_use]
    pub const fn new(player: Arc<Player>, message: Option<TextComponent>) -> Self {
        Self { player, message }
    }

    /// The player who joined.
    #[must_use]
    pub const fn player(&self) -> &Arc<Player> {
        &self.player
    }

    /// What will be announced, or `None` when nothing will be.
    #[must_use]
    pub const fn message(&self) -> Option<&TextComponent> {
        self.message.as_ref()
    }

    /// Changes the announcement. `None` suppresses it entirely.
    pub fn set_message(&mut self, message: Option<TextComponent>) {
        self.message = message;
    }

    /// Takes the announcement out, for the server to send.
    #[must_use]
    pub fn into_message(self) -> Option<TextComponent> {
        self.message
    }
}

/// A player is leaving, and the server is about to announce it.
///
/// Not cancellable, matching `org.bukkit.event.player.PlayerQuitEvent`. A
/// connection that has gone will not come back because a listener objected.
pub struct PlayerQuitEvent {
    player: Arc<Player>,
    message: Option<TextComponent>,
}

/// A player movement that passed vanilla movement validation.
pub struct PlayerMoveEvent {
    player: Arc<Player>,
    from: DVec3,
    to: DVec3,
    cancelled: bool,
}

// SAFETY: This Foton-owned key uniquely identifies the concrete Rust type.
unsafe impl DowncastType for PlayerMoveEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/player_move");
}

impl Event for PlayerMoveEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}

impl PlayerMoveEvent {
    /// Creates an event for an accepted movement.
    #[must_use]
    pub const fn new(player: Arc<Player>, from: DVec3, to: DVec3) -> Self {
        Self {
            player,
            from,
            to,
            cancelled: false,
        }
    }
    /// Returns the moving player.
    #[must_use]
    pub const fn player(&self) -> &Arc<Player> {
        &self.player
    }
    /// Returns the starting position.
    #[must_use]
    pub const fn from(&self) -> DVec3 {
        self.from
    }
    /// Returns the destination selected by listeners.
    #[must_use]
    pub const fn to(&self) -> DVec3 {
        self.to
    }
    /// Changes the destination selected by listeners.
    pub const fn set_to(&mut self, to: DVec3) {
        self.to = to;
    }
    /// Cancels or uncancels the movement.
    pub const fn set_cancelled(&mut self, cancelled: bool) {
        self.cancelled = cancelled;
    }
}

// SAFETY: This Foton-owned key uniquely identifies the concrete Rust type
// within the process.
unsafe impl DowncastType for PlayerQuitEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/player_quit");
}

impl Event for PlayerQuitEvent {}

impl PlayerQuitEvent {
    /// Creates the event with the announcement the server would make on its own.
    #[must_use]
    pub const fn new(player: Arc<Player>, message: Option<TextComponent>) -> Self {
        Self { player, message }
    }

    /// The player who left.
    #[must_use]
    pub const fn player(&self) -> &Arc<Player> {
        &self.player
    }

    /// What will be announced, or `None` when nothing will be.
    #[must_use]
    pub const fn message(&self) -> Option<&TextComponent> {
        self.message.as_ref()
    }

    /// Changes the announcement. `None` suppresses it entirely.
    pub fn set_message(&mut self, message: Option<TextComponent>) {
        self.message = message;
    }

    /// Takes the announcement out, for the server to send.
    #[must_use]
    pub fn into_message(self) -> Option<TextComponent> {
        self.message
    }
}

/// A player said something, before anyone else hears it.
///
/// Corresponds to `org.bukkit.event.player.AsyncPlayerChatEvent`, which the
/// corpus still prefers ten to four over Paper's newer `AsyncChatEvent`. The
/// name drops the `Async` because Foton's chat is not: it is handled on the
/// packet path and dispatched from there, and calling it async would be
/// describing Bukkit's threading rather than Foton's.
///
/// Cancellable, and five of the ten plugins that touch it do cancel.
pub struct PlayerChatEvent {
    player: Arc<Player>,
    message: String,
    recipients: Vec<Uuid>,
    changed: bool,
    cancelled: bool,
}

// SAFETY: This Foton-owned key uniquely identifies the concrete Rust type
// within the process.
unsafe impl DowncastType for PlayerChatEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/player_chat");
}

impl Event for PlayerChatEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}

impl PlayerChatEvent {
    /// Creates the event carrying what the player actually typed.
    #[must_use]
    pub const fn new(player: Arc<Player>, message: String) -> Self {
        Self {
            player,
            message,
            recipients: Vec::new(),
            changed: false,
            cancelled: false,
        }
    }

    /// UUIDs of players who should receive this message.
    pub fn recipients(&self) -> &[Uuid] {
        &self.recipients
    }
    pub fn recipients_mut(&mut self) -> &mut Vec<Uuid> {
        &mut self.recipients
    }

    /// The player who spoke.
    #[must_use]
    pub const fn player(&self) -> &Arc<Player> {
        &self.player
    }

    /// What will be said.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Rewrites what will be said.
    ///
    /// This has a consequence a listener cannot see: the client signed the
    /// text it sent, and that signature does not cover a rewritten one. A
    /// changed message therefore goes out unsigned, which is the only honest
    /// option -- forwarding someone's signature over words they did not write
    /// is exactly what signed chat exists to prevent.
    pub fn set_message(&mut self, message: String) {
        self.message = message;
        self.changed = true;
    }

    /// Whether a listener rewrote the message.
    #[must_use]
    pub const fn was_changed(&self) -> bool {
        self.changed
    }

    /// Stops the message from being said at all.
    pub const fn set_cancelled(&mut self, cancelled: bool) {
        self.cancelled = cancelled;
    }

    /// Takes the message out, for the server to send.
    #[must_use]
    pub fn into_message(self) -> String {
        self.message
    }
}

/// An opaque custom payload sent by a player.
///
/// Vanilla discards payload types it does not understand. Exposing the bytes
/// here preserves that default while allowing an optional protocol extension,
/// such as the plugin host, to subscribe without coupling it to the player.
pub struct PlayerCustomPayloadEvent {
    player: Arc<Player>,
    channel: Identifier,
    payload: Vec<u8>,
}

// SAFETY: This Foton-owned key uniquely identifies the concrete Rust type
// within the process.
unsafe impl DowncastType for PlayerCustomPayloadEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/player_custom_payload");
}

impl Event for PlayerCustomPayloadEvent {}

impl PlayerCustomPayloadEvent {
    /// Creates an event carrying the packet's untouched channel and bytes.
    #[must_use]
    pub const fn new(player: Arc<Player>, channel: Identifier, payload: Vec<u8>) -> Self {
        Self {
            player,
            channel,
            payload,
        }
    }

    /// The player who sent the payload.
    #[must_use]
    pub const fn player(&self) -> &Arc<Player> {
        &self.player
    }

    /// The custom payload type identifier.
    #[must_use]
    pub const fn channel(&self) -> &Identifier {
        &self.channel
    }

    /// The packet bytes after its identifier.
    #[must_use]
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }
}

/// Emitted when a client changes its language preference.
pub struct PlayerLocaleChangeEvent {
    player: Arc<Player>,
    old_locale: String,
    new_locale: String,
}
unsafe impl DowncastType for PlayerLocaleChangeEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/player_locale_change");
}
impl Event for PlayerLocaleChangeEvent {}
impl PlayerLocaleChangeEvent {
    pub fn new(player: Arc<Player>, old_locale: String, new_locale: String) -> Self {
        Self {
            player,
            old_locale,
            new_locale,
        }
    }
    pub const fn player(&self) -> &Arc<Player> {
        &self.player
    }
    pub fn old_locale(&self) -> &str {
        &self.old_locale
    }
    pub fn new_locale(&self) -> &str {
        &self.new_locale
    }
}
