//! # Foton Core
//!
//! The core library for the Foton Minecraft server. Handles everything related to the PLAY state.

#![feature(try_as_dyn)]

use crate::chunk::chunk_map::ChunkMap;

pub mod advancement;
pub mod behavior;
pub mod block_entity;
pub mod bootstrap;
pub mod boss_event;
pub mod bug_report;
pub mod chunk;
pub mod chunk_saver;
pub mod command;
pub mod config;
pub mod dimension;
pub(crate) mod enchantment_helper;
pub(crate) mod enchantment_selection;
pub mod entity;
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
