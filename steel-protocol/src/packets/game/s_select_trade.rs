//! Serverbound packet sent when a player clicks a trade in the trading screen.

use std::io::{Cursor, Result};

use steel_macros::ServerPacket;
use steel_utils::codec::VarInt;
use steel_utils::serial::ReadFrom;

/// Sent when the player picks one of the merchant's trades.
///
/// Vanilla parity: `ServerboundSelectTradePacket`. Vanilla names the field
/// `item`, which it has not been since trades stopped being recipes; it is the
/// index of the chosen trade in the merchant's offer list.
#[derive(ServerPacket, Clone, Copy, Debug)]
pub struct SSelectTrade {
    /// Index into the merchant's offers.
    ///
    /// Not validated here: vanilla range-checks it against the live offer list
    /// in `MerchantMenu`, which is the only place that knows how long the list
    /// currently is.
    pub selected_trade: i32,
}

impl ReadFrom for SSelectTrade {
    fn read(data: &mut Cursor<&[u8]>) -> Result<Self> {
        Ok(Self {
            selected_trade: VarInt::read(data)?.0,
        })
    }
}
