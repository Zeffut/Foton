//! Trading: the seam a mob implements so a player can trade with it.
//!
//! The offer types themselves live in `foton_registry::trading`, because
//! the protocol needs them; what lives here is the part that needs a
//! `Player` and a menu.

mod merchant;

pub use merchant::{Merchant, open_trading_screen};
