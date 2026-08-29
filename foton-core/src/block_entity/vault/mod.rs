//! The vault's machinery.
//!
//! Vanilla parity: the
//! `net.minecraft.world.level.block.entity.vault` package.

mod block_entity;
mod config;
mod server_data;
mod shared_data;
mod state;

pub use block_entity::VaultBlockEntity;
pub use config::{DEFAULT_ACTIVATION_RANGE, DEFAULT_DEACTIVATION_RANGE, VaultConfig};
pub use server_data::VaultServerData;
pub use shared_data::VaultSharedData;
pub use state::VaultStateExt;
