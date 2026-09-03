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

use crate::portal::TeleportTransitionCause;
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
// SAFETY: This Foton-owned key uniquely identifies this concrete event type.
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
    /// Called by Foton when it fires the event. A plugin receives one of these; it never builds one.
    #[must_use]
    pub const fn new(player_id: Uuid, advancement: String, criterion: String) -> Self {
        Self {
            player_id,
            advancement,
            criterion,
            cancelled: false,
        }
    }
    /// Who did it.
    #[must_use]
    pub const fn player_id(&self) -> Uuid {
        self.player_id
    }
    /// Which advancement the criterion belongs to.
    #[must_use]
    pub fn advancement(&self) -> &str {
        &self.advancement
    }
    /// Which of its criteria was just met.
    #[must_use]
    pub fn criterion(&self) -> &str {
        &self.criterion
    }
    /// Whether a listener has stopped this from happening.
    #[must_use]
    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }
    /// Stops this from happening, or lets it happen again.
    pub const fn set_cancelled(&mut self, value: bool) {
        self.cancelled = value;
    }
}

/// Fired when a player completes an advancement.
pub struct PlayerAdvancementDoneEvent {
    player_id: Uuid,
    advancement: String,
}
// SAFETY: This Foton-owned key uniquely identifies this concrete event type.
unsafe impl DowncastType for PlayerAdvancementDoneEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/player_advancement_done");
}
impl Event for PlayerAdvancementDoneEvent {
    fn is_cancelled(&self) -> bool {
        false
    }
}
impl PlayerAdvancementDoneEvent {
    /// Called by Foton when it fires the event. A plugin receives one of these; it never builds one.
    #[must_use]
    pub const fn new(player_id: Uuid, advancement: String) -> Self {
        Self {
            player_id,
            advancement,
        }
    }
    /// Who did it.
    #[must_use]
    pub const fn player_id(&self) -> Uuid {
        self.player_id
    }
    /// Which advancement they finished.
    #[must_use]
    pub fn advancement(&self) -> &str {
        &self.advancement
    }
}

/// Asynchronous admission check before a player object is created.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AsyncPlayerPreLoginResult {
    /// Let them in.
    Allowed,
    /// Turn them away: the server is full.
    KickFull,
    /// Turn them away: they are banned.
    KickBanned,
    /// Turn them away: they are not on the whitelist.
    KickWhitelist,
    /// Turn them away for a reason of the plugin's own.
    KickOther,
}

/// A player is about to travel through a portal.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlayerOpenSignCause {
    /// They clicked a sign that was already standing.
    Interact,
    /// They placed it, and the editor opens by itself.
    Place,
}

/// Fired before a sign's text editor opens for a player.
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
    /// Called by Foton when it fires the event. A plugin receives one of these; it never builds one.
    #[must_use]
    pub const fn new(
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
    /// Who did it.
    #[must_use]
    pub const fn player_id(&self) -> Uuid {
        self.player_id
    }
    /// Which world this happened in.
    #[must_use]
    pub fn world(&self) -> &str {
        &self.world
    }
    /// Where it happened.
    #[must_use]
    pub const fn position(&self) -> BlockPos {
        self.position
    }
    /// True for the front of the sign, false for the back.
    #[must_use]
    pub const fn front_side(&self) -> bool {
        self.front_side
    }
    /// What brought this about.
    #[must_use]
    pub const fn cause(&self) -> PlayerOpenSignCause {
        self.cause
    }
    /// Whether a listener has stopped this from happening.
    #[must_use]
    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }
    /// Stops this from happening, or lets it happen again.
    pub const fn set_cancelled(&mut self, value: bool) {
        self.cancelled = value;
    }
}

/// Fired before a player is carried through a portal.
pub struct PlayerPortalEvent {
    player_id: Uuid,
    from_world: String,
    from_position: DVec3,
    from_rotation: (f32, f32),
    to_world: String,
    to_position: DVec3,
    to_rotation: (f32, f32),
    cause: TeleportTransitionCause,
    cancelled: bool,
}
// SAFETY: This Foton-owned key uniquely identifies this concrete event type.
unsafe impl DowncastType for PlayerPortalEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/player_portal");
}
impl Event for PlayerPortalEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}
impl PlayerPortalEvent {
    /// Called by Foton when it fires the event. A plugin receives one of these; it never builds one.
    #[must_use]
    #[expect(
        clippy::too_many_arguments,
        reason = "a portal crossing is a where-from and a where-to, each of them a world,                   a position and a rotation, plus who and why -- grouping them into a                   struct would name the halves and hide nothing"
    )]
    pub const fn new(
        player_id: Uuid,
        from_world: String,
        from_position: DVec3,
        from_rotation: (f32, f32),
        to_world: String,
        to_position: DVec3,
        to_rotation: (f32, f32),
        cause: TeleportTransitionCause,
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
    /// Who did it.
    #[must_use]
    pub const fn player_id(&self) -> Uuid {
        self.player_id
    }
    /// Which world they came from.
    #[must_use]
    #[expect(
        clippy::wrong_self_convention,
        reason = "`from_world` is where they came from, not a constructor -- it pairs with `to_world` and renaming either would make the pair unreadable"
    )]
    pub fn from_world(&self) -> &str {
        &self.from_world
    }
    /// Where they came from.
    #[must_use]
    #[expect(
        clippy::wrong_self_convention,
        reason = "`from_world` is where they came from, not a constructor -- it pairs with `to_world` and renaming either would make the pair unreadable"
    )]
    pub const fn from_position(&self) -> DVec3 {
        self.from_position
    }
    /// Which way they were facing when they entered.
    #[must_use]
    #[expect(
        clippy::wrong_self_convention,
        reason = "`from_world` is where they came from, not a constructor -- it pairs with `to_world` and renaming either would make the pair unreadable"
    )]
    pub const fn from_rotation(&self) -> (f32, f32) {
        self.from_rotation
    }
    /// Which world they are going to.
    #[must_use]
    pub fn to_world(&self) -> &str {
        &self.to_world
    }
    /// Where they are going.
    #[must_use]
    pub const fn to_position(&self) -> DVec3 {
        self.to_position
    }
    /// Which way they will face when they arrive.
    #[must_use]
    pub const fn to_rotation(&self) -> (f32, f32) {
        self.to_rotation
    }
    /// What brought this about.
    #[must_use]
    pub const fn cause(&self) -> TeleportTransitionCause {
        self.cause
    }
    /// Sends them somewhere other than where the portal leads.
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
    /// Stops this from happening, or lets it happen again.
    pub const fn set_cancelled(&mut self, value: bool) {
        self.cancelled = value;
    }
}
/// Fired while a player is still connecting, off the tick thread, before they are let in.
///
/// Off the tick means a listener here must not touch the world. It runs before the player exists on the server at all, which is what makes it the right place to turn somebody away and the wrong place for anything else.
pub struct AsyncPlayerPreLoginEvent {
    uuid: Uuid,
    name: String,
    address: SocketAddr,
    result: AsyncPlayerPreLoginResult,
    kick_message: Option<String>,
}
// SAFETY: This Foton-owned key uniquely identifies this concrete event type.
unsafe impl DowncastType for AsyncPlayerPreLoginEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/async_pre_login");
}
impl Event for AsyncPlayerPreLoginEvent {
    fn is_cancelled(&self) -> bool {
        self.kick_message.is_some()
    }
}
impl AsyncPlayerPreLoginEvent {
    /// Called by Foton when it fires the event. A plugin receives one of these; it never builds one.
    #[must_use]
    pub const fn new(uuid: Uuid, name: String, address: SocketAddr) -> Self {
        Self {
            uuid,
            name,
            address,
            result: AsyncPlayerPreLoginResult::Allowed,
            kick_message: None,
        }
    }
    /// Who is trying to connect.
    #[must_use]
    pub const fn uuid(&self) -> Uuid {
        self.uuid
    }
    /// The name they are connecting under.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
    /// Where they are connecting from.
    #[must_use]
    pub const fn address(&self) -> SocketAddr {
        self.address
    }
    /// Why they are being turned away, if they are.
    #[must_use]
    pub fn kick_message(&self) -> Option<&str> {
        self.kick_message.as_deref()
    }
    /// What will happen unless a listener changes it.
    #[must_use]
    pub const fn result(&self) -> AsyncPlayerPreLoginResult {
        self.result
    }
    /// Turns them away, with a reason they will be shown.
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
    /// Called by Foton when it fires the event. A plugin receives one of these; it never builds one.
    #[must_use]
    pub const fn new(player_id: Uuid, food_level: i32) -> Self {
        Self {
            player_id,
            food_level,
            cancelled: false,
        }
    }
    /// Who did it.
    #[must_use]
    pub const fn player_id(&self) -> Uuid {
        self.player_id
    }
    /// The hunger they will be left with, out of twenty.
    #[must_use]
    pub const fn food_level(&self) -> i32 {
        self.food_level
    }
    /// Changes the hunger they are left with.
    pub const fn set_food_level(&mut self, food_level: i32) {
        self.food_level = food_level;
    }
    /// Whether a listener has stopped this from happening.
    #[must_use]
    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }
    /// Stops this from happening, or lets it happen again.
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
// SAFETY: This Foton-owned key uniquely identifies this concrete event type.
unsafe impl DowncastType for PlayerBucketFillEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/player_bucket_fill");
}
impl Event for PlayerBucketFillEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}
impl PlayerBucketFillEvent {
    /// Called by Foton when it fires the event. A plugin receives one of these; it never builds one.
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
    /// Who did it.
    #[must_use]
    pub const fn player_id(&self) -> Uuid {
        self.player_id
    }
    /// Which world this happened in.
    #[must_use]
    pub fn world(&self) -> &str {
        &self.world
    }
    /// Where it happened.
    #[must_use]
    pub const fn position(&self) -> BlockPos {
        self.position
    }
    /// Which bucket they filled.
    #[must_use]
    pub fn bucket(&self) -> &str {
        &self.bucket
    }
    /// Stops this from happening, or lets it happen again.
    pub const fn set_cancelled(&mut self, value: bool) {
        self.cancelled = value;
    }
}

/// A player's equipped item reached zero durability.
pub struct PlayerItemBreakEvent {
    player_id: Uuid,
    item: ItemStack,
}
// SAFETY: This Foton-owned key uniquely identifies this concrete event type.
unsafe impl DowncastType for PlayerItemBreakEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/player_item_break");
}
impl Event for PlayerItemBreakEvent {}
impl PlayerItemBreakEvent {
    /// Called by Foton when it fires the event. A plugin receives one of these; it never builds one.
    #[must_use]
    pub const fn new(player_id: Uuid, item: ItemStack) -> Self {
        Self { player_id, item }
    }
    /// Who did it.
    #[must_use]
    pub const fn player_id(&self) -> Uuid {
        self.player_id
    }
    /// The item involved.
    #[must_use]
    pub const fn item(&self) -> &ItemStack {
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
    /// Called by Foton when it fires the event. A plugin receives one of these; it never builds one.
    pub fn new(player_id: Uuid, hook_id: Uuid, state: impl Into<String>) -> Self {
        Self {
            player_id,
            hook_id,
            state: state.into(),
            cancelled: false,
        }
    }
    /// Who did it.
    #[must_use]
    pub const fn player_id(&self) -> Uuid {
        self.player_id
    }
    /// The bobber in the water.
    #[must_use]
    pub const fn hook_id(&self) -> Uuid {
        self.hook_id
    }
    /// What just happened to the line, as Bukkit names it -- `CAUGHT_FISH` and the rest of its State enum.
    #[must_use]
    pub fn state(&self) -> &str {
        &self.state
    }
    /// Whether a listener has stopped this from happening.
    #[must_use]
    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }
    /// Stops this from happening, or lets it happen again.
    pub const fn set_cancelled(&mut self, cancelled: bool) {
        self.cancelled = cancelled;
    }
}
// SAFETY: This Foton-owned key uniquely identifies this concrete event type.
unsafe impl DowncastType for PlayerBucketEmptyEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/player_bucket_empty");
}
impl Event for PlayerBucketEmptyEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}
impl PlayerBucketEmptyEvent {
    /// Called by Foton when it fires the event. A plugin receives one of these; it never builds one.
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
    /// Who did it.
    #[must_use]
    pub const fn player_id(&self) -> Uuid {
        self.player_id
    }
    /// Which world this happened in.
    #[must_use]
    pub fn world(&self) -> &str {
        &self.world
    }
    /// Where it happened.
    #[must_use]
    pub const fn position(&self) -> BlockPos {
        self.position
    }
    /// Which bucket they emptied.
    #[must_use]
    pub fn bucket(&self) -> &str {
        &self.bucket
    }
    /// Stops this from happening, or lets it happen again.
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
    /// Called by Foton when it fires the event. A plugin receives one of these; it never builds one.
    #[must_use]
    pub const fn new(player_id: Uuid, item_id: Uuid) -> Self {
        Self {
            player_id,
            item_id,
            cancelled: false,
        }
    }
    /// Who did it.
    #[must_use]
    pub const fn player_id(&self) -> Uuid {
        self.player_id
    }
    /// The dropped item, which is now an entity lying in the world.
    #[must_use]
    pub const fn item_id(&self) -> Uuid {
        self.item_id
    }
    /// Whether a listener has stopped this from happening.
    #[must_use]
    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }
    /// Stops this from happening, or lets it happen again.
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
// SAFETY: This Foton-owned key uniquely identifies this concrete event type.
unsafe impl DowncastType for PlayerSpawnLocationEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/player_spawn_location");
}
impl Event for PlayerSpawnLocationEvent {}
impl PlayerSpawnLocationEvent {
    /// Called by Foton when it fires the event. A plugin receives one of these; it never builds one.
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
    /// Who did it.
    #[must_use]
    pub const fn player_id(&self) -> Uuid {
        self.player_id
    }
    /// Which world this happened in.
    #[must_use]
    pub fn world(&self) -> &str {
        &self.world
    }
    /// Where it happened.
    #[must_use]
    pub const fn position(&self) -> [f64; 3] {
        self.position
    }
    /// Which way they were facing, as yaw and pitch.
    #[must_use]
    pub const fn rotation(&self) -> (f32, f32) {
        self.rotation
    }
    /// Chooses where they appear instead.
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
// SAFETY: This Foton-owned key uniquely identifies this concrete event type.
unsafe impl DowncastType for PlayerRespawnEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/player_respawn");
}
impl Event for PlayerRespawnEvent {}
impl PlayerRespawnEvent {
    /// Called by Foton when it fires the event. A plugin receives one of these; it never builds one.
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
    /// Who did it.
    #[must_use]
    pub const fn player_id(&self) -> Uuid {
        self.player_id
    }
    /// Which world this happened in.
    #[must_use]
    pub fn world(&self) -> &str {
        &self.world
    }
    /// Where it happened.
    #[must_use]
    pub const fn position(&self) -> [f64; 3] {
        self.position
    }
    /// Which way they were facing, as yaw and pitch.
    #[must_use]
    pub const fn rotation(&self) -> (f32, f32) {
        self.rotation
    }
    /// Whether they came back at a respawn anchor rather than a bed or the world spawn.
    #[must_use]
    pub const fn is_anchor_spawn(&self) -> bool {
        self.anchor_spawn
    }
    /// Chooses where they come back instead.
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
    /// Called by Foton when it fires the event. A plugin receives one of these; it never builds one.
    pub fn new(player_id: Uuid, death_message: impl Into<String>) -> Self {
        Self {
            player_id,
            death_message: Some(death_message.into()),
            drops: Vec::new(),
            keep_inventory: false,
        }
    }
    /// Called by Foton when the death leaves items on the ground.
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
    /// Who did it.
    #[must_use]
    pub const fn player_id(&self) -> Uuid {
        self.player_id
    }
    /// What everyone will be told, if anything.
    #[must_use]
    pub fn death_message(&self) -> Option<&str> {
        self.death_message.as_deref()
    }
    /// Changes what everyone is told, or silences it.
    pub fn set_death_message(&mut self, message: Option<String>) {
        self.death_message = message;
    }
    /// What will be left on the ground.
    #[must_use]
    pub fn drops(&self) -> &[ItemStack] {
        &self.drops
    }
    /// What will be left on the ground, so a listener can add to it or take from it.
    pub const fn drops_mut(&mut self) -> &mut Vec<ItemStack> {
        &mut self.drops
    }
    /// Whether they keep what they were carrying.
    #[must_use]
    pub const fn keep_inventory(&self) -> bool {
        self.keep_inventory
    }
    /// Decides whether they keep what they were carrying.
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
    /// Called by Foton when it fires the event. A plugin receives one of these; it never builds one.
    #[must_use]
    pub const fn new(player_id: Uuid, entity_id: Uuid) -> Self {
        Self {
            player_id,
            entity_id,
            cancelled: false,
        }
    }
    /// Who did it.
    #[must_use]
    pub const fn player_id(&self) -> Uuid {
        self.player_id
    }
    /// Which entity this is about.
    #[must_use]
    pub const fn entity_id(&self) -> Uuid {
        self.entity_id
    }
    /// Whether a listener has stopped this from happening.
    #[must_use]
    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }
    /// Stops this from happening, or lets it happen again.
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
    #[must_use]
    pub fn recipients(&self) -> &[Uuid] {
        &self.recipients
    }
    /// Who will see the message, so a listener can narrow it.
    pub const fn recipients_mut(&mut self) -> &mut Vec<Uuid> {
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
// SAFETY: This Foton-owned key uniquely identifies this concrete event type.
unsafe impl DowncastType for PlayerLocaleChangeEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/player_locale_change");
}
impl Event for PlayerLocaleChangeEvent {}
impl PlayerLocaleChangeEvent {
    /// Called by Foton when it fires the event. A plugin receives one of these; it never builds one.
    pub const fn new(player: Arc<Player>, old_locale: String, new_locale: String) -> Self {
        Self {
            player,
            old_locale,
            new_locale,
        }
    }
    /// Who did it.
    #[must_use]
    pub const fn player(&self) -> &Arc<Player> {
        &self.player
    }
    /// The language they had.
    #[must_use]
    pub fn old_locale(&self) -> &str {
        &self.old_locale
    }
    /// The language they switched to.
    #[must_use]
    pub fn new_locale(&self) -> &str {
        &self.new_locale
    }
}
