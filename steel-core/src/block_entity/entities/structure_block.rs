//! The structure block block entity.
//!
//! Vanilla parity: `StructureBlockEntity`. It remembers a name, a box relative
//! to itself, and the placement settings a load would use.
//!
//! Three of its four modes are here in full:
//!
//! * **corner** marks a bound for a save block of the same name,
//! * **data** carries a metadata marker string for a structure to read back,
//! * **save** can work its own box out from the matching corner blocks
//!   ([`Self::detect_size`]).
//!
//! Load mode places for real: `StructureTemplate::place_in_world` accepts any
//! `WorldGenLevel`, and a live world is one.
//!
//! What is not here is the capture. `saveStructure` needs
//! `StructureTemplate.fillFromWorld` plus a `StructureTemplateManager` to write
//! `generated/<namespace>/structures/<path>.nbt`, and neither exists, so a save
//! reports failure the way vanilla does rather than pretending. That is also why a
//! load only finds the structures bundled with the game: nothing has been saved.

use std::sync::{Arc, Weak};

use glam::IVec3;
use simdnbt::borrow::{BaseNbtCompound as BorrowedNbtCompound, NbtCompound as NbtCompoundView};
use simdnbt::owned::NbtCompound;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::blocks::properties::{BlockStateProperties, StructureMode};
use steel_registry::structure::LiquidSettingsData;
use steel_registry::structure_processor::StructureProcessorKind;
use steel_registry::{REGISTRY, vanilla_block_entity_types, vanilla_blocks};
use steel_utils::locks::SyncMutex;
use steel_utils::random::legacy_random::LegacyRandom;
use steel_utils::types::UpdateFlags;
use steel_utils::{
    BlockPos, BlockStateId, BoundingBox, Downcast as _, DowncastType, DowncastTypeKey, Identifier,
    Rotation,
};
use steel_worldgen::structure::{StructureBlockIgnore, StructureMirror as PlacementMirror};

use crate::block_entity::{BlockEntity, BlockEntityBase};
use crate::world::{LevelReader as _, World};
use crate::worldgen::template::{
    StructurePlaceSettings, StructureProcessorRandom, StructureTemplate,
};

/// Furthest a structure block's corner may sit from the block itself.
///
/// Vanilla parity: `StructureBlockEntity.MAX_OFFSET_PER_AXIS`.
pub const MAX_OFFSET_PER_AXIS: i32 = 48;

/// Largest structure a structure block may capture on one axis.
///
/// Vanilla parity: `StructureBlockEntity.MAX_SIZE_PER_AXIS`.
pub const MAX_SIZE_PER_AXIS: i32 = 48;

/// How far a save block looks for its corner blocks, horizontally.
///
/// Vanilla parity: the `int radius = 80` of `detectSize`, which is wider than
/// the 48-block structure limit so a misplaced corner is still found and can
/// still be rejected for being too far.
const CORNER_SEARCH_RADIUS: i32 = 80;

/// Where a fresh structure block's box starts.
///
/// Vanilla parity: `StructureBlockEntity.DEFAULT_POS`, one block up so the
/// default box sits on top of the block rather than inside it.
const DEFAULT_OFFSET: (i32, i32, i32) = (0, 1, 0);

/// How a structure block was rotated when it was saved or will be placed.
///
/// Vanilla parity: `net.minecraft.world.level.block.Rotation`, whose
/// `LEGACY_CODEC` is ordinal order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StructureRotation {
    /// Placed the way it was saved.
    #[default]
    None,
    /// Turned a quarter turn clockwise.
    Clockwise90,
    /// Turned a half turn.
    Clockwise180,
    /// Turned a quarter turn anticlockwise.
    Counterclockwise90,
}

impl StructureRotation {
    /// Returns the rotation an ordinal names.
    #[must_use]
    pub const fn from_ordinal(value: i32) -> Option<Self> {
        match value {
            0 => Some(Self::None),
            1 => Some(Self::Clockwise90),
            2 => Some(Self::Clockwise180),
            3 => Some(Self::Counterclockwise90),
            _ => None,
        }
    }

    /// Returns this rotation's ordinal.
    #[must_use]
    pub const fn ordinal(self) -> i32 {
        match self {
            Self::None => 0,
            Self::Clockwise90 => 1,
            Self::Clockwise180 => 2,
            Self::Counterclockwise90 => 3,
        }
    }
}

impl StructureRotation {
    /// Returns the rotation template placement applies.
    ///
    /// The two enums are the same vanilla `Rotation`; this one carries the ordinal
    /// the editor packet uses, the other the transform placement applies.
    const fn placement_rotation(self) -> Rotation {
        match self {
            Self::None => Rotation::None,
            Self::Clockwise90 => Rotation::Clockwise90,
            Self::Clockwise180 => Rotation::Clockwise180,
            Self::Counterclockwise90 => Rotation::CounterClockwise90,
        }
    }
}

/// How a structure block mirrors what it places.
///
/// Vanilla parity: `net.minecraft.world.level.block.Mirror`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StructureMirror {
    /// Placed the way it was saved.
    #[default]
    None,
    /// Flipped across the X axis.
    LeftRight,
    /// Flipped across the Z axis.
    FrontBack,
}

impl StructureMirror {
    /// Returns the mirror an ordinal names.
    #[must_use]
    pub const fn from_ordinal(value: i32) -> Option<Self> {
        match value {
            0 => Some(Self::None),
            1 => Some(Self::LeftRight),
            2 => Some(Self::FrontBack),
            _ => None,
        }
    }

    /// Returns this mirror's ordinal.
    #[must_use]
    pub const fn ordinal(self) -> i32 {
        match self {
            Self::None => 0,
            Self::LeftRight => 1,
            Self::FrontBack => 2,
        }
    }
}

impl StructureMirror {
    /// Returns the mirror template placement applies.
    ///
    /// See [`StructureRotation::placement_rotation`] for why there are two enums.
    const fn placement_mirror(self) -> PlacementMirror {
        match self {
            Self::None => PlacementMirror::None,
            Self::LeftRight => PlacementMirror::LeftRight,
            Self::FrontBack => PlacementMirror::FrontBack,
        }
    }
}

/// Everything a structure block remembers.
#[derive(Debug, Clone)]
struct StructureBlockState {
    structure_name: Option<Identifier>,
    author: String,
    metadata: String,
    offset: (i32, i32, i32),
    size: (i32, i32, i32),
    mirror: StructureMirror,
    rotation: StructureRotation,
    mode: StructureMode,
    ignore_entities: bool,
    strict: bool,
    powered: bool,
    show_air: bool,
    show_bounding_box: bool,
    integrity: f32,
    seed: i64,
}

/// A structure block.
pub struct StructureBlockEntity {
    base: BlockEntityBase,
    state: SyncMutex<StructureBlockState>,
}

// SAFETY: This key is owned by Steel and uniquely identifies the block entity.
unsafe impl DowncastType for StructureBlockEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:block_entity/structure_block");
}

impl StructureBlockEntity {
    /// Creates a structure block entity in the mode its block state names.
    #[must_use]
    pub fn new(level: Weak<World>, pos: BlockPos, block_state: BlockStateId) -> Self {
        let mode = block_state
            .try_get_value(&BlockStateProperties::STRUCTUREBLOCK_MODE)
            .unwrap_or(StructureMode::Data);
        Self {
            base: BlockEntityBase::new(
                &vanilla_block_entity_types::STRUCTURE_BLOCK,
                level,
                pos,
                block_state,
            ),
            state: SyncMutex::new(StructureBlockState {
                structure_name: None,
                author: String::new(),
                metadata: String::new(),
                offset: DEFAULT_OFFSET,
                size: (0, 0, 0),
                mirror: StructureMirror::None,
                rotation: StructureRotation::None,
                mode,
                ignore_entities: true,
                strict: false,
                powered: false,
                show_air: false,
                show_bounding_box: true,
                integrity: 1.0,
                seed: 0,
            }),
        }
    }

    /// Returns the structure's name, or the empty string when it has none.
    ///
    /// Vanilla parity: `StructureBlockEntity.getStructureName`.
    #[must_use]
    pub fn structure_name(&self) -> String {
        self.state
            .lock()
            .structure_name
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_default()
    }

    /// Returns whether the block has a parsable structure name.
    ///
    /// Vanilla parity: `StructureBlockEntity.hasStructureName`. A name that
    /// will not parse as an identifier counts as no name at all, which is what
    /// the editor's "invalid structure name" message reports.
    #[must_use]
    pub fn has_structure_name(&self) -> bool {
        self.state.lock().structure_name.is_some()
    }

    /// Sets the structure's name from what the editor sent.
    pub fn set_structure_name(&self, name: &str) {
        let parsed = if name.is_empty() {
            None
        } else {
            name.parse::<Identifier>().ok()
        };
        self.state.lock().structure_name = parsed;
    }

    /// Returns who saved this structure.
    #[must_use]
    pub fn author(&self) -> String {
        self.state.lock().author.clone()
    }

    /// Records who placed this block.
    ///
    /// Vanilla parity: `StructureBlockEntity.createdBy`.
    pub fn set_author(&self, author: String) {
        self.state.lock().author = author;
    }

    /// Returns the data-mode marker string.
    #[must_use]
    pub fn metadata(&self) -> String {
        self.state.lock().metadata.clone()
    }

    /// Sets the data-mode marker string.
    pub fn set_metadata(&self, metadata: String) {
        self.state.lock().metadata = metadata;
    }

    /// Returns the offset from this block to the low corner of the box.
    #[must_use]
    pub fn offset(&self) -> (i32, i32, i32) {
        self.state.lock().offset
    }

    /// Sets the offset from this block to the low corner of the box.
    pub fn set_offset(&self, offset: (i32, i32, i32)) {
        self.state.lock().offset = clamp_offset(offset);
    }

    /// Returns the size of the box.
    #[must_use]
    pub fn size(&self) -> (i32, i32, i32) {
        self.state.lock().size
    }

    /// Sets the size of the box.
    pub fn set_size(&self, size: (i32, i32, i32)) {
        self.state.lock().size = clamp_size(size);
    }

    /// Returns the mirror a load would place with.
    #[must_use]
    pub fn mirror(&self) -> StructureMirror {
        self.state.lock().mirror
    }

    /// Sets the mirror a load would place with.
    pub fn set_mirror(&self, mirror: StructureMirror) {
        self.state.lock().mirror = mirror;
    }

    /// Returns the rotation a load would place with.
    #[must_use]
    pub fn rotation(&self) -> StructureRotation {
        self.state.lock().rotation
    }

    /// Sets the rotation a load would place with.
    pub fn set_rotation(&self, rotation: StructureRotation) {
        self.state.lock().rotation = rotation;
    }

    /// Returns which of the four things this block does.
    #[must_use]
    pub fn mode(&self) -> StructureMode {
        self.state.lock().mode.clone()
    }

    /// Sets the mode, and swaps the block state to match.
    ///
    /// Vanilla parity: `StructureBlockEntity.setMode`. The mode lives in the
    /// block state as well as here, because that is what the client renders.
    pub fn set_mode(&self, mode: StructureMode) {
        self.state.lock().mode = mode;
        self.publish_mode_to_block_state();
    }

    fn publish_mode_to_block_state(&self) {
        let Some(world) = self.get_level() else {
            return;
        };
        let pos = self.get_block_pos();
        let state = world.get_block_state(pos);
        if state.get_block() != &vanilla_blocks::STRUCTURE_BLOCK {
            return;
        }
        let mode = self.state.lock().mode.clone();
        world.set_block(
            pos,
            state.set_value(&BlockStateProperties::STRUCTUREBLOCK_MODE, mode),
            UpdateFlags::UPDATE_CLIENTS,
        );
    }

    /// Returns whether entities inside the box are left out of a capture.
    #[must_use]
    pub fn ignores_entities(&self) -> bool {
        self.state.lock().ignore_entities
    }

    /// Sets whether entities inside the box are left out of a capture.
    pub fn set_ignore_entities(&self, ignore_entities: bool) {
        self.state.lock().ignore_entities = ignore_entities;
    }

    /// Returns whether placement must not fall back to a looser match.
    #[must_use]
    pub fn is_strict(&self) -> bool {
        self.state.lock().strict
    }

    /// Sets whether placement must not fall back to a looser match.
    pub fn set_strict(&self, strict: bool) {
        self.state.lock().strict = strict;
    }

    /// Returns whether redstone is holding this block triggered.
    #[must_use]
    pub fn is_powered(&self) -> bool {
        self.state.lock().powered
    }

    /// Sets whether redstone is holding this block triggered.
    pub fn set_powered(&self, powered: bool) {
        self.state.lock().powered = powered;
    }

    /// Sets whether the client draws air blocks in the preview.
    pub fn set_show_air(&self, show_air: bool) {
        self.state.lock().show_air = show_air;
    }

    /// Sets whether the client draws the bounding box.
    pub fn set_show_bounding_box(&self, show_bounding_box: bool) {
        self.state.lock().show_bounding_box = show_bounding_box;
    }

    /// Returns the fraction of blocks a load would keep.
    #[must_use]
    pub fn integrity(&self) -> f32 {
        self.state.lock().integrity
    }

    /// Sets the fraction of blocks a load would keep.
    pub fn set_integrity(&self, integrity: f32) {
        self.state.lock().integrity = integrity;
    }

    /// Returns the seed a load's integrity roll would use.
    #[must_use]
    pub fn seed(&self) -> i64 {
        self.state.lock().seed
    }

    /// Sets the seed a load's integrity roll would use.
    pub fn set_seed(&self, seed: i64) {
        self.state.lock().seed = seed;
    }

    /// Works the box out from the matching corner blocks around this one.
    ///
    /// Vanilla parity: `StructureBlockEntity.detectSize`. Only a save block
    /// scans, and only corner blocks carrying the *same name* count -- that is
    /// what lets two structures being authored side by side not confuse each
    /// other.
    ///
    /// A single corner block encloses the save block itself as well, so one
    /// corner is enough to define a box; two or more use only the corners.
    pub fn detect_size(&self) -> bool {
        if self.mode() != StructureMode::Save {
            return false;
        }
        let Some(world) = self.get_level() else {
            return false;
        };

        let pos = self.get_block_pos();
        let corners = self.related_corners(&world, pos);
        let Some(bounds) = enclosing_bounds(pos, &corners) else {
            return false;
        };

        let (min, max) = bounds;
        let delta = (max.0 - min.0, max.1 - min.1, max.2 - min.2);
        if delta.0 <= 1 || delta.1 <= 1 || delta.2 <= 1 {
            return false;
        }

        {
            let mut state = self.state.lock();
            state.offset = (
                min.0 - pos.x() + 1,
                min.1 - pos.y() + 1,
                min.2 - pos.z() + 1,
            );
            state.size = (delta.0 - 1, delta.1 - 1, delta.2 - 1);
        }
        self.base.set_changed();
        true
    }

    /// Loads the template this block names.
    ///
    /// Vanilla parity: `StructureBlockEntity.getStructureTemplate`, which asks the
    /// level's `StructureTemplateManager`. Steel's manager side is the bundled vanilla
    /// datapack only -- there is nowhere to save a structure to yet -- so a name that is
    /// not a vanilla structure takes the same branch vanilla takes for a missing file.
    fn structure_template(&self) -> Option<StructureTemplate> {
        let key = self.state.lock().structure_name.clone()?;
        match StructureTemplate::load_vanilla(&REGISTRY, &key) {
            Ok(template) => Some(template),
            Err(err) => {
                log::debug!(
                    "structure block at {:?} cannot load {key}: {err}",
                    self.get_block_pos()
                );
                None
            }
        }
    }

    /// Returns whether a load block could find the structure it names.
    ///
    /// Vanilla parity: `StructureBlockEntity.isStructureLoadable`.
    #[must_use]
    pub fn is_structure_loadable(&self) -> bool {
        self.mode() == StructureMode::Load && self.structure_template().is_some()
    }

    /// Copies the author and the size out of a loaded template.
    ///
    /// Vanilla parity: `StructureBlockEntity.loadStructureInfo`, which is what fills
    /// the editor's size fields in on the first press of the load button.
    fn load_structure_info(&self, template: &StructureTemplate) {
        {
            let mut state = self.state.lock();
            template.author().clone_into(&mut state.author);
            let size = template.size(Rotation::None);
            state.size = (size.x, size.y, size.z);
        }
        self.base.set_changed();
    }

    /// Places the structure, but only once the editor already shows its size.
    ///
    /// Vanilla parity: `StructureBlockEntity.placeStructureIfSameSize`. The load
    /// button takes two presses: the first fills the size in and reports "prepare",
    /// the second places.
    pub fn place_structure_if_same_size(&self, world: &Arc<World>) -> bool {
        if self.mode() != StructureMode::Load {
            return false;
        }
        let Some(template) = self.structure_template() else {
            return false;
        };

        let size = template.size(Rotation::None);
        if (size.x, size.y, size.z) != self.size() {
            self.load_structure_info(&template);
            return false;
        }

        self.place_loaded_structure(world, &template);
        true
    }

    /// Places the structure this block names.
    ///
    /// Vanilla parity: `StructureBlockEntity.placeStructure`, what a redstone pulse on
    /// a load block does.
    pub fn place_structure(&self, world: &Arc<World>) -> bool {
        let Some(template) = self.structure_template() else {
            return false;
        };
        self.place_loaded_structure(world, &template);
        true
    }

    /// Vanilla parity: the private `StructureBlockEntity.placeStructure(level, template)`.
    fn place_loaded_structure(&self, world: &Arc<World>, template: &StructureTemplate) {
        self.load_structure_info(template);
        let state = self.state.lock().clone();

        // Vanilla parity: `StructureBlockEntity.createRandom`. A zero seed means
        // "pick one", which vanilla does from the wall clock and Steel does from the
        // runtime source; either way the placement is not meant to be repeatable.
        let seed = if state.seed == 0 {
            rand::random()
        } else {
            state.seed
        };

        let rot_processor;
        let processors: &[StructureProcessorKind] = if state.integrity < 1.0 {
            rot_processor = [StructureProcessorKind::BlockRot {
                rottable_blocks: None,
                integrity: state.integrity.clamp(0.0, 1.0),
            }];
            &rot_processor
        } else {
            &[]
        };

        let settings = StructurePlaceSettings {
            mirror: state.mirror.placement_mirror(),
            rotation: state.rotation.placement_rotation(),
            rotation_pivot: BlockPos::ZERO,
            // Vanilla leaves the box unset here, which means "no limit"; Steel's
            // placement always has one, so it gets the whole buildable column.
            bounding_box: BoundingBox::new(
                IVec3::new(i32::MIN, world.get_min_y(), i32::MIN),
                IVec3::new(i32::MAX, world.get_max_y(), i32::MAX),
            ),
            processors,
            block_ignore: StructureBlockIgnore::None,
            late_block_ignore: StructureBlockIgnore::None,
            replace_jigsaws: false,
            projection: None,
            processor_random: if state.integrity < 1.0 {
                StructureProcessorRandom::Seeded(seed)
            } else {
                StructureProcessorRandom::Positional
            },
            liquid_settings: LiquidSettingsData::ApplyWaterlogging,
            ignore_entities: state.ignore_entities,
        };

        let pos = self
            .get_block_pos()
            .offset(state.offset.0, state.offset.1, state.offset.2);
        // Vanilla parity: `2 | (strict ? 816 : 0)`.
        let mut flags = UpdateFlags::UPDATE_CLIENTS;
        if state.strict {
            flags |= UpdateFlags::UPDATE_KNOWN_SHAPE
                | UpdateFlags::UPDATE_SUPPRESS_DROPS
                | UpdateFlags::UPDATE_SKIP_BLOCK_ENTITY_SIDEEFFECTS
                | UpdateFlags::UPDATE_SKIP_ON_PLACE;
        }

        template.place_in_world(
            world,
            &REGISTRY,
            pos,
            pos,
            &settings,
            &mut LegacyRandom::from_seed(seed as u64),
            flags,
        );
    }

    /// Returns the positions of every corner block that belongs to this one.
    ///
    /// Vanilla parity: `StructureBlockEntity.getRelatedCorners`.
    fn related_corners(&self, world: &World, pos: BlockPos) -> Vec<Corner> {
        let wanted = self.state.lock().structure_name.clone();
        let mut corners = Vec::new();

        for x in (pos.x() - CORNER_SEARCH_RADIUS)..=(pos.x() + CORNER_SEARCH_RADIUS) {
            for z in (pos.z() - CORNER_SEARCH_RADIUS)..=(pos.z() + CORNER_SEARCH_RADIUS) {
                for y in world.get_min_y()..=world.get_max_y() {
                    let candidate = BlockPos::new(x, y, z);
                    if world.get_block_state(candidate).get_block()
                        != &vanilla_blocks::STRUCTURE_BLOCK
                    {
                        continue;
                    }
                    let Some(shared) = world.get_block_entity(candidate) else {
                        continue;
                    };
                    let Some(other) = shared.downcast_ref::<Self>() else {
                        continue;
                    };
                    let other_state = other.state.lock();
                    if other_state.mode == StructureMode::Corner
                        && other_state.structure_name == wanted
                    {
                        corners.push((x, y, z));
                    }
                }
            }
        }

        corners
    }
}

/// One corner of a scanned box.
type Corner = (i32, i32, i32);

/// The low and high corners of a scanned box.
type ScannedBounds = (Corner, Corner);

/// Returns the box every corner encloses, plus `pos` when there is only one.
///
/// Vanilla parity: `StructureBlockEntity.calculateEnclosingBoundingBox`.
fn enclosing_bounds(pos: BlockPos, corners: &[Corner]) -> Option<ScannedBounds> {
    let (first, rest) = corners.split_first()?;
    let mut min = *first;
    let mut max = *first;

    if rest.is_empty() {
        // Vanilla parity: one corner encloses the save block too, so a single
        // corner still defines a box.
        encapsulate(&mut min, &mut max, (pos.x(), pos.y(), pos.z()));
    } else {
        for corner in rest {
            encapsulate(&mut min, &mut max, *corner);
        }
    }

    Some((min, max))
}

fn encapsulate(min: &mut Corner, max: &mut Corner, point: Corner) {
    min.0 = min.0.min(point.0);
    min.1 = min.1.min(point.1);
    min.2 = min.2.min(point.2);
    max.0 = max.0.max(point.0);
    max.1 = max.1.max(point.1);
    max.2 = max.2.max(point.2);
}

/// Vanilla parity: the `Mth.clamp(..., -48, 48)` of `loadAdditional`.
fn clamp_offset(offset: (i32, i32, i32)) -> (i32, i32, i32) {
    (
        offset.0.clamp(-MAX_OFFSET_PER_AXIS, MAX_OFFSET_PER_AXIS),
        offset.1.clamp(-MAX_OFFSET_PER_AXIS, MAX_OFFSET_PER_AXIS),
        offset.2.clamp(-MAX_OFFSET_PER_AXIS, MAX_OFFSET_PER_AXIS),
    )
}

/// Vanilla parity: the `Mth.clamp(..., 0, 48)` of `loadAdditional`.
fn clamp_size(size: (i32, i32, i32)) -> (i32, i32, i32) {
    (
        size.0.clamp(0, MAX_SIZE_PER_AXIS),
        size.1.clamp(0, MAX_SIZE_PER_AXIS),
        size.2.clamp(0, MAX_SIZE_PER_AXIS),
    )
}

/// Vanilla parity: `StructureMode.LEGACY_CODEC`, which is ordinal order.
const fn mode_ordinal(mode: &StructureMode) -> i32 {
    match *mode {
        StructureMode::Save => 0,
        StructureMode::Load => 1,
        StructureMode::Corner => 2,
        StructureMode::Data => 3,
    }
}

/// Returns the mode an ordinal names.
#[must_use]
pub const fn mode_from_ordinal(value: i32) -> Option<StructureMode> {
    match value {
        0 => Some(StructureMode::Save),
        1 => Some(StructureMode::Load),
        2 => Some(StructureMode::Corner),
        3 => Some(StructureMode::Data),
        _ => None,
    }
}

impl BlockEntity for StructureBlockEntity {
    fn base(&self) -> &BlockEntityBase {
        &self.base
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        let state = self.state.lock();
        nbt.insert(
            "name",
            state
                .structure_name
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_default(),
        );
        nbt.insert("author", state.author.clone());
        nbt.insert("metadata", state.metadata.clone());
        nbt.insert("posX", state.offset.0);
        nbt.insert("posY", state.offset.1);
        nbt.insert("posZ", state.offset.2);
        nbt.insert("sizeX", state.size.0);
        nbt.insert("sizeY", state.size.1);
        nbt.insert("sizeZ", state.size.2);
        nbt.insert("rotation", state.rotation.ordinal());
        nbt.insert("mirror", state.mirror.ordinal());
        nbt.insert("mode", mode_ordinal(&state.mode));
        nbt.insert("ignoreEntities", state.ignore_entities);
        nbt.insert("strict", state.strict);
        nbt.insert("powered", state.powered);
        nbt.insert("showair", state.show_air);
        nbt.insert("showboundingbox", state.show_bounding_box);
        nbt.insert("integrity", state.integrity);
        nbt.insert("seed", state.seed);
    }

    fn load_additional(&self, nbt: &BorrowedNbtCompound<'_>) {
        let view: NbtCompoundView<'_, '_> = nbt.into();
        let name = view.string("name").map(ToString::to_string);
        self.set_structure_name(name.as_deref().unwrap_or_default());

        {
            let mut state = self.state.lock();
            state.author = view
                .string("author")
                .map(ToString::to_string)
                .unwrap_or_default();
            state.metadata = view
                .string("metadata")
                .map(ToString::to_string)
                .unwrap_or_default();
            state.offset = clamp_offset((
                view.int("posX").unwrap_or(DEFAULT_OFFSET.0),
                view.int("posY").unwrap_or(DEFAULT_OFFSET.1),
                view.int("posZ").unwrap_or(DEFAULT_OFFSET.2),
            ));
            state.size = clamp_size((
                view.int("sizeX").unwrap_or(0),
                view.int("sizeY").unwrap_or(0),
                view.int("sizeZ").unwrap_or(0),
            ));
            state.rotation = view
                .int("rotation")
                .and_then(StructureRotation::from_ordinal)
                .unwrap_or_default();
            state.mirror = view
                .int("mirror")
                .and_then(StructureMirror::from_ordinal)
                .unwrap_or_default();
            // Vanilla parity: a block written without a mode loads as DATA,
            // which is the harmless one.
            state.mode = view
                .int("mode")
                .and_then(mode_from_ordinal)
                .unwrap_or(StructureMode::Data);
            state.ignore_entities = view.byte("ignoreEntities").is_none_or(|value| value != 0);
            state.strict = view.byte("strict").is_some_and(|value| value != 0);
            state.powered = view.byte("powered").is_some_and(|value| value != 0);
            state.show_air = view.byte("showair").is_some_and(|value| value != 0);
            state.show_bounding_box = view.byte("showboundingbox").is_none_or(|value| value != 0);
            state.integrity = view.float("integrity").unwrap_or(1.0);
            state.seed = view.long("seed").unwrap_or(0);
        }

        // Vanilla parity: `loadAdditional` ends with `updateBlockState`, so the
        // rendered mode follows the saved one rather than the placed block.
        self.publish_mode_to_block_state();
    }

    /// Vanilla parity: `StructureBlockEntity.getUpdateTag`, which is
    /// `saveCustomOnly`.
    fn get_update_tag(&self) -> Option<NbtCompound> {
        let mut nbt = NbtCompound::new();
        self.save_additional(&mut nbt);
        Some(nbt)
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use simdnbt::borrow::read_compound as read_borrowed_compound;
    use steel_registry::init_vanilla_registry;
    use steel_utils::ChunkPos;

    use super::*;
    use crate::behavior::init_behaviors;
    use crate::block_entity::init_block_entities;
    use crate::test_support::{fresh_test_world, insert_ready_full_chunk};

    fn state_for(mode: StructureMode) -> BlockStateId {
        vanilla_blocks::STRUCTURE_BLOCK
            .default_state()
            .set_value(&BlockStateProperties::STRUCTUREBLOCK_MODE, mode)
    }

    fn structure_block(mode: StructureMode) -> StructureBlockEntity {
        init_vanilla_registry();
        StructureBlockEntity::new(Weak::new(), BlockPos::new(8, 64, 8), state_for(mode))
    }

    fn reload(mode: StructureMode, nbt: &NbtCompound) -> StructureBlockEntity {
        let mut bytes = Vec::new();
        nbt.write(&mut bytes);
        let borrowed = read_borrowed_compound(&mut Cursor::new(&bytes))
            .unwrap_or_else(|error| panic!("test nbt should reborrow: {error}"));
        let entity = structure_block(mode);
        entity.load_additional(&borrowed);
        entity
    }

    /// A load block places the bundled template it names, at its offset.
    ///
    /// `nether_fossils/fossil_5` is two by five by one of bone blocks and air, with
    /// no processors, so exactly which positions it fills is fixed.
    #[test]
    fn a_load_block_places_the_structure_it_names() {
        init_vanilla_registry();
        init_behaviors();
        init_block_entities();

        let world = fresh_test_world("structure_block_load");
        let pos = BlockPos::new(4, 64, 4);
        insert_ready_full_chunk(&world, ChunkPos::from_block_pos(pos));

        let entity =
            StructureBlockEntity::new(Arc::downgrade(&world), pos, state_for(StructureMode::Load));
        entity.set_structure_name("minecraft:nether_fossils/fossil_5");
        assert!(entity.place_structure(&world));

        // The default offset is one block up, so the template's own origin lands there.
        let origin = pos.above();
        let bone_blocks = (0..2)
            .flat_map(|x| (0..5).flat_map(move |y| (0..1).map(move |z| (x, y, z))))
            .filter(|&(x, y, z)| {
                world.get_block_state(origin.offset(x, y, z)).get_block()
                    == &vanilla_blocks::BONE_BLOCK
            })
            .count();
        assert!(
            bone_blocks > 0,
            "the load block should have placed the fossil's bone blocks"
        );

        // Loading also reports the template's size back to the editor.
        assert_eq!(entity.size(), (2, 5, 1));
    }

    /// The load button fills the size in on the first press and places on the second.
    ///
    /// Vanilla parity: `placeStructureIfSameSize`, which is what makes the editor
    /// report `structure_block.load_prepare` once and `structure_block.load_success`
    /// after.
    #[test]
    fn the_load_button_prepares_before_it_places() {
        init_vanilla_registry();
        init_behaviors();
        init_block_entities();

        let world = fresh_test_world("structure_block_load_button");
        let pos = BlockPos::new(4, 64, 4);
        insert_ready_full_chunk(&world, ChunkPos::from_block_pos(pos));

        let entity =
            StructureBlockEntity::new(Arc::downgrade(&world), pos, state_for(StructureMode::Load));
        entity.set_structure_name("minecraft:nether_fossils/fossil_5");

        // The editor starts with a zero size, so the first press only reports one.
        assert!(!entity.place_structure_if_same_size(&world));
        assert_eq!(entity.size(), (2, 5, 1));
        assert!(
            world.get_block_state(pos.above()).is_air(),
            "nothing should be placed while the size is still being reported"
        );

        assert!(entity.place_structure_if_same_size(&world));
        assert_eq!(
            world.get_block_state(pos.above()).get_block(),
            &vanilla_blocks::BONE_BLOCK
        );
    }

    /// Integrity below one drops blocks from a stream seeded by the block's own seed,
    /// not from the one the placement draws loot seeds out of.
    ///
    /// `igloo/middle` is three by three by three, so a repeat matching by chance is
    /// out of the question.
    #[test]
    fn integrity_rots_the_structure_from_the_blocks_own_seed() {
        init_vanilla_registry();
        init_behaviors();
        init_block_entities();

        let world = fresh_test_world("structure_block_integrity");
        insert_ready_full_chunk(&world, ChunkPos::new(0, 0));

        let whole = place_igloo_middle(&world, BlockPos::new(1, 64, 1), 1.0, 4242);
        let rotted = place_igloo_middle(&world, BlockPos::new(6, 64, 1), 0.5, 4242);
        let again = place_igloo_middle(&world, BlockPos::new(11, 64, 1), 0.5, 4242);

        assert_eq!(whole.len(), 15, "the whole template is fifteen blocks");
        assert!(
            rotted.len() < whole.len(),
            "half integrity should drop some of the fifteen"
        );
        assert_eq!(rotted, again, "the same seed should rot the same blocks");
    }

    /// Places `igloo/middle` from a load block and returns the offsets it filled.
    fn place_igloo_middle(
        world: &Arc<World>,
        pos: BlockPos,
        integrity: f32,
        seed: i64,
    ) -> Vec<(i32, i32, i32)> {
        let entity =
            StructureBlockEntity::new(Arc::downgrade(world), pos, state_for(StructureMode::Load));
        entity.set_structure_name("minecraft:igloo/middle");
        entity.set_integrity(integrity);
        entity.set_seed(seed);
        assert!(entity.place_structure(world));

        let origin = pos.above();
        (0..3)
            .flat_map(|x| (0..3).flat_map(move |y| (0..3).map(move |z| (x, y, z))))
            .filter(|&(x, y, z)| !world.get_block_state(origin.offset(x, y, z)).is_air())
            .collect()
    }

    /// A name nothing has saved is not loadable, which is what makes the editor say
    /// "not found" instead of placing nothing and claiming success.
    #[test]
    fn a_load_block_naming_no_bundled_structure_is_not_loadable() {
        init_vanilla_registry();
        init_block_entities();

        let entity = structure_block(StructureMode::Load);
        entity.set_structure_name("minecraft:nether_fossils/fossil_5");
        assert!(entity.is_structure_loadable());

        entity.set_structure_name("mypack:a_house_nobody_saved");
        assert!(!entity.is_structure_loadable());
    }

    /// Only a load block loads: vanilla's save and corner modes go down other
    /// branches of the same button.
    #[test]
    fn only_a_load_block_is_loadable() {
        init_vanilla_registry();
        init_block_entities();

        for mode in [
            StructureMode::Save,
            StructureMode::Corner,
            StructureMode::Data,
        ] {
            let entity = structure_block(mode.clone());
            entity.set_structure_name("minecraft:nether_fossils/fossil_5");
            assert!(!entity.is_structure_loadable(), "{mode:?} should not load");
        }
    }

    /// A name that will not parse as an identifier is no name at all, which is
    /// what the editor's "invalid structure name" message reports on.
    #[test]
    fn an_unparsable_name_counts_as_no_name() {
        let entity = structure_block(StructureMode::Save);
        assert!(!entity.has_structure_name());

        entity.set_structure_name("my structure");
        assert!(!entity.has_structure_name());

        entity.set_structure_name("mypack:house");
        assert!(entity.has_structure_name());
        assert_eq!(entity.structure_name(), "mypack:house");
    }

    /// Offsets and sizes are clamped on the way in as well as on the wire, so
    /// a value that reached the block entity another way is still bounded.
    #[test]
    fn the_offset_and_size_are_clamped() {
        let entity = structure_block(StructureMode::Save);
        entity.set_offset((-500, 500, 3));
        assert_eq!(entity.offset(), (-48, 48, 3));

        entity.set_size((999, -4, 12));
        assert_eq!(entity.size(), (48, 0, 12));
    }

    /// Vanilla's defaults are not all false: `ignoreEntities` and
    /// `showboundingbox` default to true, so a block written before those keys
    /// existed must not come back with them off.
    #[test]
    fn a_block_saved_without_flags_keeps_vanillas_true_defaults() {
        let entity = reload(StructureMode::Save, &NbtCompound::new());
        assert!(entity.ignores_entities());
        assert!(!entity.is_strict());
        assert!(!entity.is_powered());
        assert!((entity.integrity() - 1.0).abs() < f32::EPSILON);
        // A block with no stored mode loads as DATA, the harmless one.
        assert_eq!(entity.mode(), StructureMode::Data);
    }

    /// Everything the editor sets has to survive a save.
    #[test]
    fn the_configured_box_and_settings_round_trip() {
        let entity = structure_block(StructureMode::Save);
        entity.set_structure_name("mypack:house");
        entity.set_offset((-3, 2, 5));
        entity.set_size((11, 7, 9));
        entity.set_rotation(StructureRotation::Clockwise180);
        entity.set_mirror(StructureMirror::LeftRight);
        entity.set_metadata("chest_loot".to_owned());
        entity.set_integrity(0.5);
        entity.set_seed(1234);
        entity.set_ignore_entities(false);
        entity.set_strict(true);

        let mut nbt = NbtCompound::new();
        entity.save_additional(&mut nbt);
        assert_eq!(nbt.int("rotation"), Some(2));
        assert_eq!(nbt.int("mirror"), Some(1));
        assert_eq!(nbt.int("mode"), Some(0));

        let reloaded = reload(StructureMode::Save, &nbt);
        assert_eq!(reloaded.structure_name(), "mypack:house");
        assert_eq!(reloaded.offset(), (-3, 2, 5));
        assert_eq!(reloaded.size(), (11, 7, 9));
        assert_eq!(reloaded.rotation(), StructureRotation::Clockwise180);
        assert_eq!(reloaded.mirror(), StructureMirror::LeftRight);
        assert_eq!(reloaded.metadata(), "chest_loot");
        assert!((reloaded.integrity() - 0.5).abs() < f32::EPSILON);
        assert_eq!(reloaded.seed(), 1234);
        assert!(!reloaded.ignores_entities());
        assert!(reloaded.is_strict());
    }

    /// One corner block is enough: vanilla encloses the save block itself when
    /// there is only one, so a two-block diagonal defines the box.
    #[test]
    fn one_matching_corner_defines_the_box_with_the_save_block() {
        init_vanilla_registry();
        init_behaviors();
        init_block_entities();

        let world = fresh_test_world("structure_block_detect_size");
        insert_ready_full_chunk(&world, ChunkPos::new(0, 0));

        let save_pos = BlockPos::new(2, 64, 2);
        let corner_pos = BlockPos::new(7, 69, 8);
        world.set_block(
            save_pos,
            state_for(StructureMode::Save),
            UpdateFlags::UPDATE_CLIENTS,
        );
        world.set_block(
            corner_pos,
            state_for(StructureMode::Corner),
            UpdateFlags::UPDATE_CLIENTS,
        );

        let Some(save_shared) = world.get_block_entity(save_pos) else {
            panic!("placing a structure block must create its block entity");
        };
        let Some(corner_shared) = world.get_block_entity(corner_pos) else {
            panic!("placing a structure block must create its block entity");
        };
        let save = save_shared
            .downcast_ref::<StructureBlockEntity>()
            .expect("a structure block's entity is a StructureBlockEntity");
        let corner = corner_shared
            .downcast_ref::<StructureBlockEntity>()
            .expect("a structure block's entity is a StructureBlockEntity");

        save.set_structure_name("mypack:house");
        save.set_mode(StructureMode::Save);
        corner.set_structure_name("mypack:house");
        corner.set_mode(StructureMode::Corner);

        assert!(save.detect_size(), "one matching corner must define a box");
        // The box is the open interior between the two markers.
        assert_eq!(save.offset(), (1, 1, 1));
        assert_eq!(save.size(), (4, 4, 5));
    }

    /// A corner level with the save block leaves no interior on that axis, and
    /// vanilla refuses the whole scan rather than saving a flat structure.
    /// This is the common mistake -- putting the corner at the same height --
    /// so it has to report failure rather than silently capturing nothing.
    #[test]
    fn a_corner_that_leaves_a_flat_box_is_refused() {
        init_vanilla_registry();
        init_behaviors();
        init_block_entities();

        let world = fresh_test_world("structure_block_flat_box");
        insert_ready_full_chunk(&world, ChunkPos::new(0, 0));

        let save_pos = BlockPos::new(2, 64, 2);
        // Level with the save block: the box would be zero blocks tall.
        let corner_pos = BlockPos::new(7, 64, 8);
        world.set_block(
            save_pos,
            state_for(StructureMode::Save),
            UpdateFlags::UPDATE_CLIENTS,
        );
        world.set_block(
            corner_pos,
            state_for(StructureMode::Corner),
            UpdateFlags::UPDATE_CLIENTS,
        );

        let save_shared = world
            .get_block_entity(save_pos)
            .expect("placing a structure block must create its block entity");
        let corner_shared = world
            .get_block_entity(corner_pos)
            .expect("placing a structure block must create its block entity");
        let save = save_shared
            .downcast_ref::<StructureBlockEntity>()
            .expect("a structure block's entity is a StructureBlockEntity");
        let corner = corner_shared
            .downcast_ref::<StructureBlockEntity>()
            .expect("a structure block's entity is a StructureBlockEntity");

        save.set_structure_name("mypack:house");
        save.set_mode(StructureMode::Save);
        corner.set_structure_name("mypack:house");
        corner.set_mode(StructureMode::Corner);

        let before = save.size();
        assert!(
            !save.detect_size(),
            "a box with no interior on one axis must be refused"
        );
        assert_eq!(
            save.size(),
            before,
            "a refused scan must leave the stored box alone"
        );
    }

    /// A corner saved under a different name belongs to somebody else's
    /// structure and must not be picked up -- that is what lets two authors
    /// work side by side.
    #[test]
    fn a_corner_with_another_name_is_ignored() {
        init_vanilla_registry();
        init_behaviors();
        init_block_entities();

        let world = fresh_test_world("structure_block_detect_size_other_name");
        insert_ready_full_chunk(&world, ChunkPos::new(0, 0));

        let save_pos = BlockPos::new(2, 64, 2);
        let corner_pos = BlockPos::new(7, 69, 8);
        world.set_block(
            save_pos,
            state_for(StructureMode::Save),
            UpdateFlags::UPDATE_CLIENTS,
        );
        world.set_block(
            corner_pos,
            state_for(StructureMode::Corner),
            UpdateFlags::UPDATE_CLIENTS,
        );

        let save_shared = world
            .get_block_entity(save_pos)
            .expect("placing a structure block must create its block entity");
        let corner_shared = world
            .get_block_entity(corner_pos)
            .expect("placing a structure block must create its block entity");
        let save = save_shared
            .downcast_ref::<StructureBlockEntity>()
            .expect("a structure block's entity is a StructureBlockEntity");
        let corner = corner_shared
            .downcast_ref::<StructureBlockEntity>()
            .expect("a structure block's entity is a StructureBlockEntity");

        save.set_structure_name("mypack:house");
        save.set_mode(StructureMode::Save);
        corner.set_structure_name("otherpack:barn");
        corner.set_mode(StructureMode::Corner);

        assert!(
            !save.detect_size(),
            "a corner under another name is not this structure's corner"
        );
    }

    /// Only a save block scans. A corner or data block asked to scan reports
    /// failure rather than silently rewriting its own box.
    ///
    /// This runs in a world with a matching corner already placed, so the mode
    /// guard is the only thing that can refuse. Off-level the scan bails on the
    /// missing world first, and the test would stay green with the guard gone.
    #[test]
    fn only_a_save_block_detects_its_size() {
        init_vanilla_registry();
        init_behaviors();
        init_block_entities();

        let world = fresh_test_world("structure_block_only_save_scans");
        insert_ready_full_chunk(&world, ChunkPos::new(0, 0));

        let scanner_pos = BlockPos::new(2, 64, 2);
        let corner_pos = BlockPos::new(7, 69, 8);
        world.set_block(
            corner_pos,
            state_for(StructureMode::Corner),
            UpdateFlags::UPDATE_CLIENTS,
        );
        let corner_shared = world
            .get_block_entity(corner_pos)
            .expect("placing a structure block must create its block entity");
        let corner = corner_shared
            .downcast_ref::<StructureBlockEntity>()
            .expect("a structure block's entity is a StructureBlockEntity");
        corner.set_structure_name("mypack:house");
        corner.set_mode(StructureMode::Corner);

        for (mode, expected) in [
            (StructureMode::Save, true),
            (StructureMode::Corner, false),
            (StructureMode::Data, false),
            (StructureMode::Load, false),
        ] {
            world.set_block(
                scanner_pos,
                state_for(mode.clone()),
                UpdateFlags::UPDATE_CLIENTS,
            );
            let shared = world
                .get_block_entity(scanner_pos)
                .expect("placing a structure block must create its block entity");
            let scanner = shared
                .downcast_ref::<StructureBlockEntity>()
                .expect("a structure block's entity is a StructureBlockEntity");
            scanner.set_structure_name("mypack:house");
            scanner.set_mode(mode.clone());

            assert_eq!(
                scanner.detect_size(),
                expected,
                "a {mode:?} block scanning its own size"
            );
        }
    }
}
