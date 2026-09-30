//! Events about how a player moves, and about the server correcting it.

use foton_utils::downcast::{DowncastType, DowncastTypeKey};
use glam::DVec3;
use uuid::Uuid;

use super::Event;

/// Which movement check refused a move.
///
/// The names are Paper's `PlayerFailMoveEvent.FailReason`, because that is the
/// only reason this exists: a plugin sees one of these when the server snaps a
/// player back without a teleport of its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailMoveReason {
    /// The move covered more ground than the player could have.
    MovedTooQuickly,
    /// The server's replay of the move ended somewhere else.
    MovedWrongly,
    /// Accepting the move would put the player inside a block.
    ClippedIntoBlock,
}

impl FailMoveReason {
    /// The Bukkit constant name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::MovedTooQuickly => "MOVED_TOO_QUICKLY",
            Self::MovedWrongly => "MOVED_WRONGLY",
            Self::ClippedIntoBlock => "CLIPPED_INTO_BLOCK",
        }
    }
}

/// The server is about to refuse a player's move and put them back.
///
/// Not cancellable: a listener *allows* the move instead, which skips the
/// check that failed, and may silence the warning vanilla logs for it.
pub struct PlayerFailMoveEvent {
    player: Uuid,
    world: String,
    reason: FailMoveReason,
    from: DVec3,
    from_rotation: (f32, f32),
    to: DVec3,
    to_rotation: (f32, f32),
    allowed: bool,
    log_warning: bool,
}

// SAFETY: This Foton-owned key uniquely identifies the concrete Rust type.
unsafe impl DowncastType for PlayerFailMoveEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/player_fail_move");
}

impl Event for PlayerFailMoveEvent {}

impl PlayerFailMoveEvent {
    /// Creates the event for one refused move.
    #[must_use]
    pub const fn new(
        player: Uuid,
        world: String,
        reason: FailMoveReason,
        from: (DVec3, (f32, f32)),
        to: (DVec3, (f32, f32)),
        log_warning: bool,
    ) -> Self {
        Self {
            player,
            world,
            reason,
            from: from.0,
            from_rotation: from.1,
            to: to.0,
            to_rotation: to.1,
            allowed: false,
            log_warning,
        }
    }

    /// Who tried to move.
    #[must_use]
    pub const fn player(&self) -> Uuid {
        self.player
    }

    /// The world they are in.
    #[must_use]
    pub fn world(&self) -> &str {
        &self.world
    }

    /// Which check failed.
    #[must_use]
    pub const fn reason(&self) -> FailMoveReason {
        self.reason
    }

    /// Where they were, with their yaw and pitch.
    #[must_use]
    pub const fn from(&self) -> (DVec3, (f32, f32)) {
        (self.from, self.from_rotation)
    }

    /// Where they asked to be, with the yaw and pitch they asked for.
    #[must_use]
    pub const fn to(&self) -> (DVec3, (f32, f32)) {
        (self.to, self.to_rotation)
    }

    /// Whether a listener let the move through.
    #[must_use]
    pub const fn allowed(&self) -> bool {
        self.allowed
    }

    /// Lets the move through, or refuses it again.
    pub const fn set_allowed(&mut self, allowed: bool) {
        self.allowed = allowed;
    }

    /// Whether the refusal is logged the way vanilla logs it.
    #[must_use]
    pub const fn log_warning(&self) -> bool {
        self.log_warning
    }

    /// Turns the warning on or off.
    pub const fn set_log_warning(&mut self, log_warning: bool) {
        self.log_warning = log_warning;
    }
}

/// A player who may fly is starting or stopping.
///
/// Cancelling leaves them as they were and tells their client so.
pub struct PlayerToggleFlightEvent {
    player: Uuid,
    flying: bool,
    cancelled: bool,
}

// SAFETY: This Foton-owned key uniquely identifies the concrete Rust type.
unsafe impl DowncastType for PlayerToggleFlightEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/player_toggle_flight");
}

impl Event for PlayerToggleFlightEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}

impl PlayerToggleFlightEvent {
    /// Creates the event for a player asking to fly (`true`) or land.
    #[must_use]
    pub const fn new(player: Uuid, flying: bool) -> Self {
        Self {
            player,
            flying,
            cancelled: false,
        }
    }

    /// Who is toggling.
    #[must_use]
    pub const fn player(&self) -> Uuid {
        self.player
    }

    /// Whether they are starting to fly.
    #[must_use]
    pub const fn flying(&self) -> bool {
        self.flying
    }

    /// Stops the toggle, or lets it happen again.
    pub const fn set_cancelled(&mut self, cancelled: bool) {
        self.cancelled = cancelled;
    }
}

/// The server is about to send a player a velocity of its own making --
/// knockback, an explosion, a plugin's push.
///
/// A listener may change the velocity sent, or cancel it so the player's
/// client is not pushed at all.
pub struct PlayerVelocityEvent {
    player: Uuid,
    velocity: DVec3,
    cancelled: bool,
}

// SAFETY: This Foton-owned key uniquely identifies the concrete Rust type.
unsafe impl DowncastType for PlayerVelocityEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/player_velocity");
}

impl Event for PlayerVelocityEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}

impl PlayerVelocityEvent {
    /// Creates the event for the velocity the player currently has.
    #[must_use]
    pub const fn new(player: Uuid, velocity: DVec3) -> Self {
        Self {
            player,
            velocity,
            cancelled: false,
        }
    }

    /// Who is being pushed.
    #[must_use]
    pub const fn player(&self) -> Uuid {
        self.player
    }

    /// The velocity that will be sent.
    #[must_use]
    pub const fn velocity(&self) -> DVec3 {
        self.velocity
    }

    /// Changes the velocity that will be sent.
    pub const fn set_velocity(&mut self, velocity: DVec3) {
        self.velocity = velocity;
    }

    /// Stops the velocity being sent, or lets it be sent again.
    pub const fn set_cancelled(&mut self, cancelled: bool) {
        self.cancelled = cancelled;
    }
}

/// Why a player is being moved somewhere else.
///
/// Bukkit parity: the `PlayerTeleportEvent.TeleportCause` values Foton
/// raises. Portals are not here: they raise `PlayerPortalEvent` instead.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TeleportCause {
    /// Something consumed teleported them: a chorus fruit, or any item with a
    /// `teleport_randomly` consume effect.
    ConsumableEffect,
    /// Their thrown ender pearl landed.
    EnderPearl,
    /// `/tp` or `/teleport`.
    Command,
    /// A move nothing classified.
    Unknown,
}

impl TeleportCause {
    /// Bukkit's name for it.
    #[must_use]
    pub const fn bukkit_name(self) -> &'static str {
        match self {
            Self::ConsumableEffect => "CONSUMABLE_EFFECT",
            Self::EnderPearl => "ENDER_PEARL",
            Self::Command => "COMMAND",
            Self::Unknown => "UNKNOWN",
        }
    }
}

/// One end of a teleport: what a Bukkit `Location` holds.
#[derive(Clone, Debug, PartialEq)]
pub struct TeleportPoint {
    /// The world's key.
    pub world: String,
    /// Where in it.
    pub position: DVec3,
    /// Yaw and pitch.
    pub rotation: (f32, f32),
}

/// A player is about to be teleported.
///
/// Bukkit parity: `PlayerTeleportEvent`. Cancelling keeps them where they
/// were; a changed destination is where they go.
pub struct PlayerTeleportEvent {
    player: Uuid,
    from: TeleportPoint,
    to: TeleportPoint,
    cause: TeleportCause,
    cancelled: bool,
}

// SAFETY: This Foton-owned key uniquely identifies the concrete Rust type.
unsafe impl DowncastType for PlayerTeleportEvent {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:event/player_teleport");
}

impl Event for PlayerTeleportEvent {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}

impl PlayerTeleportEvent {
    /// Creates the event for a move from `from` to `to`.
    #[must_use]
    pub const fn new(
        player: Uuid,
        from: TeleportPoint,
        to: TeleportPoint,
        cause: TeleportCause,
    ) -> Self {
        Self {
            player,
            from,
            to,
            cause,
            cancelled: false,
        }
    }

    /// Who is moving.
    #[must_use]
    pub const fn player(&self) -> Uuid {
        self.player
    }

    /// Where they are.
    #[must_use]
    pub const fn from(&self) -> &TeleportPoint {
        &self.from
    }

    /// Where they are going.
    #[must_use]
    pub const fn to(&self) -> &TeleportPoint {
        &self.to
    }

    /// Sends them somewhere else.
    pub fn set_to(&mut self, to: TeleportPoint) {
        self.to = to;
    }

    /// Why.
    #[must_use]
    pub const fn cause(&self) -> TeleportCause {
        self.cause
    }

    /// Keeps them where they are, or lets them go again.
    pub const fn set_cancelled(&mut self, cancelled: bool) {
        self.cancelled = cancelled;
    }
}
