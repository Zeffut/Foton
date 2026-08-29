use std::slice;

use super::*;
use std::sync::Arc;

use crate::bootstrap::init_globals_once;
use crate::entity::{
    DEFAULT_MAX_AIR_SUPPLY, ENTITIES, Entity, MobEffectInstance, SharedEntity,
    attribute::{AttributeModifier, AttributeModifierOperation},
    entities::{EndCrystalEntity, RawEntity},
    next_entity_id,
};
use crate::test_support::{fresh_test_world, insert_ready_full_chunk};
use foton_registry::init_vanilla_registry;
use foton_registry::vanilla_attributes;
use foton_registry::vanilla_block_entity_types;
use foton_registry::vanilla_blocks;
use foton_registry::vanilla_entities;
use foton_registry::vanilla_fluids;
use foton_registry::vanilla_mob_effects;
use foton_utils::BoundingBox;
use foton_utils::types::UpdateFlags;
use foton_worldgen::structure::StructureReferenceSet;
use glam::DVec3;
use rustc_hash::FxHashMap;
use text_components::TextComponent;

fn test_structure_piece() -> StructurePiece {
    StructurePiece {
        piece_type: Identifier::new_static("minecraft", "mscorridor"),
        bounding_box: BoundingBox::new(IVec3::new(0, 64, 0), IVec3::new(1, 65, 1)),
        gen_depth: 0,
        orientation: None,
        payload: StructurePiecePayload::Procedural(ProceduralPieceData::Unimplemented),
        ground_level_delta: 0,
        junctions: Vec::new(),
        projection: None,
    }
}

fn single_empty_section() -> Sections {
    Sections::from_owned(vec![ChunkSection::new_empty()].into_boxed_slice())
}

fn visible_homogeneous_value(section: Option<&LightSection>) -> Option<u8> {
    let Some(LightSection::Visible(LightSectionData::Homogeneous(value))) = section else {
        return None;
    };
    Some(*value)
}

mod chunks_sections;
mod entities;
mod light;
mod structures;
mod ticks;
