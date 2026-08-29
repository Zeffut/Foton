use std::io::Cursor;

use foton_registry::{
    RegistryExt, init_vanilla_registry, vanilla_attributes, vanilla_biomes, vanilla_damage_types,
    vanilla_entities, vanilla_items,
};
use foton_utils::types::InteractionHand;
use simdnbt::borrow::read_compound as read_borrowed_compound;

use crate::entity::damage::DamageSource;
use crate::entity::init_entities;
use crate::entity::{SharedEntity, next_entity_id};
use crate::test_support::{TestPlayerBuilder, fresh_test_world, insert_ready_full_chunk};
use foton_utils::ChunkPos;

use super::*;

mod core;
mod persistence;
