//! The jigsaw block block entity.
//!
//! Vanilla parity: `JigsawBlockEntity`. Seven fields and nothing else: the name
//! other jigsaws aim at, the name this one aims at, the template pool it draws
//! from, the state it leaves behind, whether a connected piece may roll, and the
//! two priorities that order a pool's candidates.
//!
//! Steel's worldgen already reads all of this out of the *template* copies of
//! jigsaw blocks when it builds a village. What was missing is the placed block:
//! a jigsaw a map-maker puts down by hand and configures through its editor.
//!
//! Not implemented: `generate`. Writing the pieces is no longer what stops it --
//! `StructurePiecePlacer::place_piece` takes any `WorldGenLevel`, so a live world
//! is a fine target. What is missing is the assembly:
//!
//! * Vanilla's `JigsawPlacement.generateJigsaw` runs the same `addPieces` a jigsaw
//!   structure runs, against a `Structure.GenerationContext` built from the live
//!   level's chunk source. Steel's `steel_worldgen::structure::jigsaw::assemble`
//!   needs the terrain-height query that context provides, and the only things
//!   implementing `StructureGenerationContext` are the per-chunk contexts a
//!   generator owns while it is generating. A live world has no way to ask for one.
//! * `assemble` starts from a chunk corner and a sampled start height; a jigsaw
//!   block starts from the block in front of itself, at its own Y.
//! * `place_pool_element` always replaces jigsaw blocks with their final state;
//!   `generateJigsaw` has a `keepJigsaws` flag that leaves them standing.
//!
//! Until then the `ServerboundJigsawGeneratePacket` -- which Steel does not model
//! either -- has nothing to call, so it stays unhandled.

use std::sync::Weak;

use simdnbt::borrow::{BaseNbtCompound as BorrowedNbtCompound, NbtCompound as NbtCompoundView};
use simdnbt::owned::NbtCompound;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::blocks::properties::BlockStateProperties;
use steel_registry::vanilla_block_entity_types;
use steel_utils::axis::Axis;
use steel_utils::locks::SyncMutex;
use steel_utils::{BlockPos, BlockStateId, DowncastType, DowncastTypeKey, Identifier};

use crate::block_entity::{BlockEntity, BlockEntityBase};
use crate::world::World;

/// The identifier an unconfigured jigsaw uses for its name, target and pool.
///
/// Vanilla parity: `JigsawBlockEntity.EMPTY_ID` and `Pools.EMPTY`.
const EMPTY_ID: &str = "empty";

/// The state an unconfigured jigsaw leaves behind.
///
/// Vanilla parity: `JigsawBlockEntity.DEFAULT_FINAL_STATE`.
const DEFAULT_FINAL_STATE: &str = "minecraft:air";

/// Whether a piece connected here may roll around the joint axis.
///
/// Vanilla parity: `JigsawBlockEntity.JointType`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JigsawJointType {
    /// The connected piece may be rotated freely about the joint.
    Rollable,
    /// The connected piece keeps its up direction.
    Aligned,
}

impl JigsawJointType {
    /// Returns the joint a serialized name describes.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "rollable" => Some(Self::Rollable),
            "aligned" => Some(Self::Aligned),
            _ => None,
        }
    }

    /// Returns the serialized name of this joint.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Rollable => "rollable",
            Self::Aligned => "aligned",
        }
    }
}

/// The default joint for a jigsaw in `state`.
///
/// Vanilla parity: `StructureTemplate.getDefaultJointType`, which a jigsaw
/// falls back to when its NBT names no joint: a jigsaw pointing up or down is
/// rollable, because there is no meaningful up for the piece it connects.
#[must_use]
pub fn default_joint_type(state: BlockStateId) -> JigsawJointType {
    let Some(orientation) = state.try_get_value(&BlockStateProperties::ORIENTATION) else {
        return JigsawJointType::Aligned;
    };
    if orientation.front().axis() == Axis::Y {
        JigsawJointType::Rollable
    } else {
        JigsawJointType::Aligned
    }
}

/// The seven fields a jigsaw block remembers.
#[derive(Debug, Clone)]
struct JigsawState {
    name: Identifier,
    target: Identifier,
    pool: Identifier,
    final_state: String,
    joint: JigsawJointType,
    placement_priority: i32,
    selection_priority: i32,
}

/// Everything a jigsaw editor sends in one go.
///
/// Vanilla passes these as seven separate setter calls; one struct keeps the
/// two same-typed priorities from being swapped by position.
pub struct JigsawSettings {
    /// The name other jigsaws aim at to connect here.
    pub name: Identifier,
    /// The name this jigsaw aims at.
    pub target: Identifier,
    /// The template pool this jigsaw draws its next piece from.
    pub pool: Identifier,
    /// The block state left behind once the jigsaw has been used.
    pub final_state: String,
    /// Whether a connected piece may roll around the joint axis.
    pub joint: JigsawJointType,
    /// How early this jigsaw is chosen among its pool's candidates.
    pub selection_priority: i32,
    /// How early the piece it places is expanded.
    pub placement_priority: i32,
}

/// A jigsaw block.
pub struct JigsawBlockEntity {
    base: BlockEntityBase,
    state: SyncMutex<JigsawState>,
}

// SAFETY: This key is owned by Steel and uniquely identifies the block entity.
unsafe impl DowncastType for JigsawBlockEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:block_entity/jigsaw");
}

impl JigsawBlockEntity {
    /// Creates a jigsaw block entity.
    #[must_use]
    pub fn new(level: Weak<World>, pos: BlockPos, block_state: BlockStateId) -> Self {
        Self {
            base: BlockEntityBase::new(
                &vanilla_block_entity_types::JIGSAW,
                level,
                pos,
                block_state,
            ),
            state: SyncMutex::new(JigsawState {
                name: Identifier::vanilla_static(EMPTY_ID),
                target: Identifier::vanilla_static(EMPTY_ID),
                pool: Identifier::vanilla_static(EMPTY_ID),
                final_state: DEFAULT_FINAL_STATE.to_owned(),
                joint: JigsawJointType::Rollable,
                placement_priority: 0,
                selection_priority: 0,
            }),
        }
    }

    /// Returns the name other jigsaws aim at to connect here.
    #[must_use]
    pub fn name(&self) -> Identifier {
        self.state.lock().name.clone()
    }

    /// Returns the name this jigsaw aims at.
    #[must_use]
    pub fn target(&self) -> Identifier {
        self.state.lock().target.clone()
    }

    /// Returns the template pool this jigsaw draws its next piece from.
    #[must_use]
    pub fn pool(&self) -> Identifier {
        self.state.lock().pool.clone()
    }

    /// Returns the block state left behind once the jigsaw has been used.
    #[must_use]
    pub fn final_state(&self) -> String {
        self.state.lock().final_state.clone()
    }

    /// Returns whether a connected piece may roll around the joint axis.
    #[must_use]
    pub fn joint(&self) -> JigsawJointType {
        self.state.lock().joint
    }

    /// Returns how early the piece this jigsaw places is expanded.
    #[must_use]
    pub fn placement_priority(&self) -> i32 {
        self.state.lock().placement_priority
    }

    /// Returns how early this jigsaw is chosen among its pool's candidates.
    #[must_use]
    pub fn selection_priority(&self) -> i32 {
        self.state.lock().selection_priority
    }

    /// Stores everything a jigsaw editor sends at once.
    ///
    /// Vanilla parity: the seven setters `handleSetJigsawBlock` calls in a row.
    pub fn configure(&self, settings: JigsawSettings) {
        {
            let mut state = self.state.lock();
            state.name = settings.name;
            state.target = settings.target;
            state.pool = settings.pool;
            state.final_state = settings.final_state;
            state.joint = settings.joint;
            state.selection_priority = settings.selection_priority;
            state.placement_priority = settings.placement_priority;
        }
        self.base.set_changed();
    }
}

impl BlockEntity for JigsawBlockEntity {
    fn base(&self) -> &BlockEntityBase {
        &self.base
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        let state = self.state.lock();
        nbt.insert("name", state.name.to_string());
        nbt.insert("target", state.target.to_string());
        nbt.insert("pool", state.pool.to_string());
        nbt.insert("final_state", state.final_state.clone());
        nbt.insert("joint", state.joint.name());
        nbt.insert("placement_priority", state.placement_priority);
        nbt.insert("selection_priority", state.selection_priority);
    }

    fn load_additional(&self, nbt: &BorrowedNbtCompound<'_>) {
        let view: NbtCompoundView<'_, '_> = nbt.into();
        let mut state = self.state.lock();
        state.name = identifier_or_empty(&view, "name");
        state.target = identifier_or_empty(&view, "target");
        state.pool = identifier_or_empty(&view, "pool");
        state.final_state = view
            .string("final_state")
            .map_or_else(|| DEFAULT_FINAL_STATE.to_owned(), ToString::to_string);
        // Vanilla parity: a jigsaw with no stored joint takes the one its
        // orientation implies, not a flat default.
        state.joint = view
            .string("joint")
            .and_then(|value| JigsawJointType::from_name(&value.to_str()))
            .unwrap_or_else(|| default_joint_type(self.base.block_state()));
        state.placement_priority = view.int("placement_priority").unwrap_or(0);
        state.selection_priority = view.int("selection_priority").unwrap_or(0);
    }

    /// Vanilla parity: `JigsawBlockEntity.getUpdateTag`, which is
    /// `saveCustomOnly` -- the editor is filled from the same shape the chunk
    /// packet carries.
    fn get_update_tag(&self) -> Option<NbtCompound> {
        let mut nbt = NbtCompound::new();
        self.save_additional(&mut nbt);
        Some(nbt)
    }
}

/// Reads an identifier field, falling back to `minecraft:empty`.
fn identifier_or_empty(nbt: &NbtCompoundView<'_, '_>, key: &str) -> Identifier {
    nbt.string(key)
        .and_then(|value| value.to_str().parse::<Identifier>().ok())
        .unwrap_or_else(|| Identifier::vanilla_static(EMPTY_ID))
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use simdnbt::borrow::read_compound as read_borrowed_compound;
    use steel_registry::blocks::properties::FrontAndTop;
    use steel_registry::{init_vanilla_registry, vanilla_blocks};

    use super::*;

    fn jigsaw(state: BlockStateId) -> JigsawBlockEntity {
        JigsawBlockEntity::new(Weak::new(), BlockPos::new(8, 64, 8), state)
    }

    fn reload(state: BlockStateId, nbt: &NbtCompound) -> JigsawBlockEntity {
        let mut bytes = Vec::new();
        nbt.write(&mut bytes);
        let borrowed = read_borrowed_compound(&mut Cursor::new(&bytes))
            .unwrap_or_else(|error| panic!("test nbt should reborrow: {error}"));
        let entity = jigsaw(state);
        entity.load_additional(&borrowed);
        entity
    }

    fn oriented(front_and_top: FrontAndTop) -> BlockStateId {
        vanilla_blocks::JIGSAW
            .default_state()
            .set_value(&BlockStateProperties::ORIENTATION, front_and_top)
    }

    /// Everything a map-maker types into the editor has to survive a save, and
    /// the two priorities in particular are easy to swap.
    #[test]
    fn every_configured_field_round_trips() {
        init_vanilla_registry();
        let entity = jigsaw(oriented(FrontAndTop::NorthUp));
        entity.configure(JigsawSettings {
            name: Identifier::vanilla_static("street"),
            target: Identifier::vanilla_static("house"),
            pool: Identifier::vanilla_static("village/plains/houses"),
            final_state: "minecraft:stone".to_owned(),
            joint: JigsawJointType::Aligned,
            selection_priority: 7,
            placement_priority: 3,
        });

        let mut nbt = NbtCompound::new();
        entity.save_additional(&mut nbt);
        assert_eq!(nbt.int("selection_priority"), Some(7));
        assert_eq!(nbt.int("placement_priority"), Some(3));

        let reloaded = reload(oriented(FrontAndTop::NorthUp), &nbt);
        assert_eq!(reloaded.name(), Identifier::vanilla_static("street"));
        assert_eq!(reloaded.target(), Identifier::vanilla_static("house"));
        assert_eq!(
            reloaded.pool(),
            Identifier::vanilla_static("village/plains/houses")
        );
        assert_eq!(reloaded.final_state(), "minecraft:stone");
        assert_eq!(reloaded.joint(), JigsawJointType::Aligned);
        assert_eq!(reloaded.selection_priority(), 7);
        assert_eq!(reloaded.placement_priority(), 3);
    }

    /// A jigsaw with no stored joint takes the one its orientation implies: a
    /// vertical jigsaw is rollable because a piece hanging off it has no
    /// meaningful up, and a horizontal one is aligned.
    #[test]
    fn a_missing_joint_comes_from_the_orientation() {
        init_vanilla_registry();
        assert_eq!(
            reload(oriented(FrontAndTop::UpNorth), &NbtCompound::new()).joint(),
            JigsawJointType::Rollable
        );
        assert_eq!(
            reload(oriented(FrontAndTop::DownNorth), &NbtCompound::new()).joint(),
            JigsawJointType::Rollable
        );
        assert_eq!(
            reload(oriented(FrontAndTop::NorthUp), &NbtCompound::new()).joint(),
            JigsawJointType::Aligned
        );
    }

    /// An unconfigured jigsaw reads as `minecraft:empty` everywhere rather than
    /// as a blank string, which is what a pool lookup compares against.
    #[test]
    fn an_unconfigured_jigsaw_is_empty_not_blank() {
        init_vanilla_registry();
        let entity = reload(oriented(FrontAndTop::NorthUp), &NbtCompound::new());
        assert_eq!(entity.name(), Identifier::vanilla_static("empty"));
        assert_eq!(entity.target(), Identifier::vanilla_static("empty"));
        assert_eq!(entity.pool(), Identifier::vanilla_static("empty"));
        assert_eq!(entity.final_state(), "minecraft:air");
    }
}
