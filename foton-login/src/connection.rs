//! Connection utilities for the login crate.
//!
//! The actual `JavaConnection` struct stays in `foton-core` since it's used
//! for play-state packet handling. This module provides re-exports and
//! helper utilities for the connection upgrade process.

pub use foton_core::player::connection::JavaConnection;
