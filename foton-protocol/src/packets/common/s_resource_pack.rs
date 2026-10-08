use foton_macros::{ReadFrom, ServerPacket};
use uuid::Uuid;

/// What the client did with a pack the server pushed.
///
/// Vanilla parity: `ServerboundResourcePackPacket.Action`, in its order.
#[derive(ReadFrom, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResourcePackAction {
    SuccessfullyLoaded = 0,
    Declined = 1,
    FailedDownload = 2,
    Accepted = 3,
    Downloaded = 4,
    InvalidUrl = 5,
    FailedReload = 6,
    Discarded = 7,
}

impl ResourcePackAction {
    /// Whether the client is done with the pack.
    ///
    /// Vanilla parity: `Action.isTerminal`; accepting and downloading are
    /// progress reports, everything else ends the exchange.
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        !matches!(self, Self::Accepted | Self::Downloaded)
    }
}

/// The client's answer to a resource pack push.
///
/// Vanilla parity: `ServerboundResourcePackPacket`. Valid in configuration and
/// in play alike.
#[derive(ReadFrom, ServerPacket, Clone, Debug)]
pub struct SResourcePack {
    /// The pack this answers.
    pub id: Uuid,
    /// What the client did with it.
    pub action: ResourcePackAction,
}
