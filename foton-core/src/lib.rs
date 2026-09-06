//! # Foton Core
//!
//! The core library for the Foton Minecraft server. Handles everything related to the PLAY state.

#![feature(try_as_dyn)]
// The release profile sets `panic = "abort"`, so an `expect` a running server
// can reach is not a style question: it kills the process without unwinding,
// `shutdown_worlds()` never runs, and every dirty chunk goes with it. The lint
// is scoped to `not(test)` on purpose -- a panicking test is how a test reports
// a failure, while a panicking server is how a world is lost.
#![cfg_attr(not(test), warn(clippy::expect_used))]
// Rustdoc infers `Send`/`Sync` for private types too, and the command tree
// behind `FunctionLibrary` nests deep enough to blow the default limit of 128
// once the workspace unifies every feature. The compiler itself is fine; only
// `cargo doc --all --document-private-items --all-features` needs the headroom.
#![recursion_limit = "512"]

use crate::chunk::chunk_map::ChunkMap;

pub mod advancement;
pub mod behavior;
pub mod block_entity;
pub mod bootstrap;
pub mod boss_event;
pub mod bug_dialog;
pub mod bug_report;
pub mod chunk;
pub mod chunk_saver;
pub mod command;
pub mod config;
pub mod dimension;
pub(crate) mod enchantment_helper;
pub(crate) mod enchantment_selection;
pub mod entity;
pub mod event;
pub mod fluid;
pub mod inventory;
pub mod level_data;
pub mod map;
pub mod permission;
pub mod physics;
pub mod player;
pub mod poi;
pub(crate) mod portal;
pub mod raid;
pub mod scoreboard;
pub mod server;
pub mod stat;
#[cfg(test)]
#[path = "../tests/support/mod.rs"]
pub(crate) mod test_support;
pub mod trading;
pub mod world;
pub mod worldgen;

pub use portal::TeleportTransitionCause;
