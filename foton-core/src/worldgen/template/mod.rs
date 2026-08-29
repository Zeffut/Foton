use std::collections::{BTreeMap, BTreeSet};
use std::io::{Cursor, Read};
use std::str::FromStr;

use flate2::read::GzDecoder;
use foton_registry::block_entity_type::BlockEntityTypeRef;
use foton_registry::blocks::properties::Direction as BlockPropertyDirection;
use foton_registry::blocks::properties::{BlockStateProperties, Half};
use foton_registry::blocks::{self};
use foton_registry::blocks::{BlockRef, block_state_ext::BlockStateExt as _};
use foton_registry::entity_type::EntityTypeRef;
use foton_registry::fluid::FluidState;
use foton_registry::shared_structs::BlockStateData;
use foton_registry::structure::LiquidSettingsData;
use foton_registry::structure_processor::{
    PosRuleTestData, ProcessorRuleData, RuleBlockEntityModifierData, StructureProcessorAxis,
    StructureProcessorKind, StructureRuleTestData,
};
use foton_registry::template_pool::Projection;
use foton_registry::vanilla_block_tags::BlockTag;
use foton_registry::{
    Registry, RegistryExt, TaggedRegistryExt, vanilla_block_entity_types, vanilla_blocks,
    vanilla_template_pools,
};
use foton_utils::random::legacy_random::LegacyRandom;
use foton_utils::random::{PositionalRandom, Random, RandomSource};
use foton_utils::value_providers::IntProvider;
use foton_utils::{
    BlockPos, BlockStateId, BoundingBox, Direction, Identifier, Rotation, types::UpdateFlags,
};
use glam::{DVec3, IVec3};
use simdnbt::borrow::{
    Nbt as BorrowedNbt, NbtCompound as BorrowedNbtCompound,
    NbtCompoundList as BorrowedNbtCompoundList, NbtList as BorrowedNbtList, read as read_nbt,
    read_compound as read_borrowed_compound,
};
use simdnbt::owned::{NbtCompound, NbtTag};
use uuid::Uuid;

use crate::behavior::BLOCK_BEHAVIORS;
use crate::chunk::heightmap::HeightmapType;
use crate::entity::{
    ENTITIES, EntityBaseSaveData, EntityFireFreezeState, EntityLoadRequest,
    nbt_load::read_entity_nbt,
};
use crate::world::WorldGenLevel;
use foton_worldgen::state_resolver::WorldgenStateResolver;
use foton_worldgen::structure::{StructureBlockIgnore, StructureMirror};

/// Loaded vanilla structure template payload.
///
/// Foton keeps template data separate from template-pool metadata. Pools only need jigsaw
/// summaries during structure-start planning; feature and piece placement need the full NBT
/// block payload and processors, so this type mirrors vanilla's loaded `StructureTemplate`.
#[derive(Debug, Clone)]
pub(crate) struct StructureTemplate {
    author: String,
    size: IVec3,
    palettes: Vec<StructureTemplatePalette>,
    entities: Vec<StructureEntityInfo>,
}

#[derive(Debug, Clone)]
struct StructureTemplatePalette {
    blocks: Vec<StructureBlockInfo>,
}

#[derive(Debug, Clone)]
struct StructureBlockInfo {
    pos: BlockPos,
    state: BlockStateId,
    nbt: Option<NbtCompound>,
}

#[derive(Debug, Clone)]
struct StructureEntityInfo {
    pos: DVec3,
    block_pos: BlockPos,
    entity_type: EntityTypeRef,
    rotation: (f32, f32),
    velocity: DVec3,
    fall_distance: f64,
    fire_freeze: EntityFireFreezeState,
    on_ground: bool,
    save_data: EntityBaseSaveData,
    nbt: NbtCompound,
}

#[derive(Debug, Clone, PartialEq)]
struct ProcessedBlockInfo {
    template_pos: BlockPos,
    world_pos: BlockPos,
    state: BlockStateId,
    nbt: Option<NbtCompound>,
}

pub(crate) struct StructureDataMarker {
    pub(crate) metadata: String,
    pub(crate) pos: BlockPos,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StructureProcessorRandom {
    /// Vanilla `StructurePlaceSettings.setRandom(random)`, where the settings hold
    /// the same source the caller passes to `placeInWorld`.
    Placement,
    /// Vanilla `StructurePlaceSettings.getRandom(pos)` fallback.
    Positional,
    /// Vanilla `setRandom(createRandom(seed))`: the settings hold a stream of their
    /// own, seeded here, which the placement random does not share. A structure
    /// block seeds both from the same number and still gets two streams.
    Seeded(i64),
}

pub(crate) struct StructurePlaceSettings<'a> {
    pub(crate) mirror: StructureMirror,
    pub(crate) rotation: Rotation,
    pub(crate) rotation_pivot: BlockPos,
    pub(crate) bounding_box: BoundingBox,
    pub(crate) processors: &'a [StructureProcessorKind],
    pub(crate) block_ignore: StructureBlockIgnore,
    pub(crate) late_block_ignore: StructureBlockIgnore,
    pub(crate) replace_jigsaws: bool,
    pub(crate) projection: Option<Projection>,
    pub(crate) processor_random: StructureProcessorRandom,
    pub(crate) liquid_settings: LiquidSettingsData,
    /// Vanilla `StructurePlaceSettings.isIgnoreEntities`.
    pub(crate) ignore_entities: bool,
}

mod loading;
mod placement;
mod processors;
mod state_transforms;

#[cfg(test)]
mod tests;
