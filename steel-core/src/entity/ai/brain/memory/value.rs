//! The values a brain memory can hold.

use std::sync::{Arc, Weak};

use glam::DVec3;
use rustc_hash::FxHashSet;
use simdnbt::borrow::{NbtCompound as BorrowedNbtCompound, NbtTag as BorrowedNbtTag};
use simdnbt::owned::{NbtCompound, NbtList, NbtTag};
use steel_utils::uuid_ext::UuidExt as _;
use steel_utils::{BlockPos, GlobalPos, Identifier};
use uuid::Uuid;

use super::super::position_tracker::PositionTracker;
use super::nearest_visible::NearestVisibleLivingEntities;
use super::walk_target::WalkTarget;
use crate::entity::ai::path::Path;
use crate::entity::damage::DamageSource;
use crate::entity::entities::ItemEntity;
use crate::entity::{SharedEntity, WeakEntity};

/// The presence-only payload of memories such as `IS_IN_WATER`.
///
/// Vanilla parity: `net.minecraft.util.Unit`, whose codec writes the string
/// `"unit"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Unit;

/// A memory that points at another entity.
///
/// Vanilla stores the entity itself and relies on the garbage collector; Steel
/// keeps a [`Weak`] so a remembered entity that leaves the world is dropped
/// rather than kept alive by the brain that noticed it. The id is cached so
/// [`PartialEq`] -- which backs vanilla's `Brain.isMemoryValue` -- still works
/// once the entity is gone.
#[derive(Debug, Clone)]
pub struct EntityMemory {
    entity: WeakEntity,
    id: i32,
}

impl EntityMemory {
    /// Remembers `entity`.
    #[must_use]
    pub fn new(entity: &SharedEntity) -> Self {
        Self {
            entity: Arc::downgrade(entity),
            id: entity.id(),
        }
    }

    /// Returns the entity, unless it has been dropped from the world.
    #[must_use]
    pub fn get(&self) -> Option<SharedEntity> {
        self.entity.upgrade()
    }

    /// Returns the remembered entity id, even after the entity is gone.
    #[must_use]
    pub const fn id(&self) -> i32 {
        self.id
    }
}

impl PartialEq for EntityMemory {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Default for EntityMemory {
    fn default() -> Self {
        Self {
            entity: Weak::<ItemEntity>::new(),
            id: -1,
        }
    }
}

/// Every shape a brain memory can take.
///
/// Vanilla makes `MemoryModuleType<U>` generic over an arbitrary `U`; Rust has
/// no heterogeneous map, so the value side is this closed enum and
/// [`super::MemoryModuleType`] carries the phantom type that keeps
/// [`super::super::Brain::get_memory`] and `set_memory` type-checked. Vanilla's
/// value types collapse onto these variants one-for-one: every entity-shaped
/// memory becomes [`Self::Entity`], and every list-of-entities memory becomes
/// [`Self::Entities`].
#[derive(Debug, Clone)]
pub enum MemoryValue {
    /// Presence only.
    Unit,
    Bool(bool),
    Int(i32),
    Long(i64),
    BlockPos(BlockPos),
    GlobalPos(GlobalPos),
    GlobalPosSet(FxHashSet<GlobalPos>),
    Uuid(Uuid),
    Uuids(Vec<Uuid>),
    Vec3(DVec3),
    Entity(EntityMemory),
    Entities(Vec<EntityMemory>),
    NearestVisibleLivingEntities(NearestVisibleLivingEntities),
    WalkTarget(WalkTarget),
    PositionTracker(PositionTracker),
    DamageSource(DamageSource),
    Path(Path),
}

impl MemoryValue {
    /// Returns whether this value is an empty collection.
    ///
    /// Vanilla parity: `Brain.isEmptyCollection`, which turns setting an empty
    /// list into erasing the memory so `VALUE_PRESENT` never matches a memory
    /// that holds nothing.
    #[must_use]
    pub(super) fn is_empty_collection(&self) -> bool {
        match self {
            Self::Entities(entities) => entities.is_empty(),
            Self::GlobalPosSet(positions) => positions.is_empty(),
            Self::Uuids(uuids) => uuids.is_empty(),
            _ => false,
        }
    }

    /// Writes this value the way vanilla's memory codec would.
    ///
    /// Returns `None` for the memories vanilla registers without a codec; those
    /// are never written to disk.
    #[must_use]
    pub(super) fn to_nbt(&self) -> Option<NbtTag> {
        match self {
            Self::Unit => Some(NbtTag::String("unit".into())),
            Self::Bool(value) => Some(NbtTag::Byte(i8::from(*value))),
            Self::Int(value) => Some(NbtTag::Int(*value)),
            Self::Long(value) => Some(NbtTag::Long(*value)),
            Self::BlockPos(pos) => Some(block_pos_to_nbt(*pos)),
            Self::GlobalPos(pos) => Some(NbtTag::Compound(global_pos_to_nbt(pos))),
            Self::GlobalPosSet(positions) => {
                let entries: Vec<NbtCompound> = positions.iter().map(global_pos_to_nbt).collect();
                Some(NbtTag::List(NbtList::Compound(entries)))
            }
            Self::Uuid(uuid) => Some(NbtTag::IntArray(uuid.to_int_array().to_vec())),
            Self::Uuids(_)
            | Self::Vec3(_)
            | Self::Entity(_)
            | Self::Entities(_)
            | Self::NearestVisibleLivingEntities(_)
            | Self::WalkTarget(_)
            | Self::PositionTracker(_)
            | Self::DamageSource(_)
            | Self::Path(_) => None,
        }
    }
}

/// Vanilla parity: `BlockPos.CODEC`, which is an int array of `[x, y, z]`.
fn block_pos_to_nbt(pos: BlockPos) -> NbtTag {
    NbtTag::IntArray(vec![pos.x(), pos.y(), pos.z()])
}

const fn block_pos_from_nbt(tag: &[i32]) -> Option<BlockPos> {
    let [x, y, z] = tag else {
        return None;
    };
    Some(BlockPos::new(*x, *y, *z))
}

/// Vanilla parity: `GlobalPos.MAP_CODEC`.
fn global_pos_to_nbt(pos: &GlobalPos) -> NbtCompound {
    let mut compound = NbtCompound::new();
    compound.insert("dimension", pos.dimension.to_string());
    compound.insert("pos", block_pos_to_nbt(pos.pos));
    compound
}

fn global_pos_from_nbt(compound: &BorrowedNbtCompound<'_, '_>) -> Option<GlobalPos> {
    let dimension = compound
        .string("dimension")?
        .to_str()
        .parse::<Identifier>()
        .ok()?;
    let pos = block_pos_from_nbt(compound.int_array("pos")?.as_ref())?;
    Some(GlobalPos::new(dimension, pos))
}

/// How one Rust type maps onto a [`MemoryValue`].
///
/// Implemented once per value shape; [`super::MemoryModuleType`] is generic
/// over this trait, which is what makes a mistyped `set_memory` a compile
/// error rather than a silently ignored write.
pub trait MemoryValueType: Clone + Sized + 'static {
    /// Wraps this value for storage in a brain.
    fn into_memory_value(self) -> MemoryValue;

    /// Reads this value back out, or `None` when the stored shape differs.
    fn from_memory_value(value: &MemoryValue) -> Option<Self>;

    /// Reads a value of this shape back from NBT.
    ///
    /// Returning `None` means vanilla registers the memory without a codec, so
    /// nothing was written for it in the first place.
    fn from_nbt(_value: &BorrowedNbtTag<'_, '_>) -> Option<MemoryValue> {
        None
    }
}

/// Implements [`MemoryValueType`] for a shape that maps to exactly one variant.
///
/// `from_nbt` names the reader for the shapes vanilla gives a codec; leaving it
/// off is how a memory says it is never written to disk.
macro_rules! impl_memory_value_type {
    ($ty:ty, $variant:ident $(, from_nbt = $from_nbt:path)?) => {
        impl MemoryValueType for $ty {
            fn into_memory_value(self) -> MemoryValue {
                MemoryValue::$variant(self)
            }

            fn from_memory_value(value: &MemoryValue) -> Option<Self> {
                match value {
                    MemoryValue::$variant(inner) => Some(inner.clone()),
                    _ => None,
                }
            }

            $(
                fn from_nbt(value: &BorrowedNbtTag<'_, '_>) -> Option<MemoryValue> {
                    $from_nbt(value).map(MemoryValue::$variant)
                }
            )?
        }
    };
}

fn read_bool(value: &BorrowedNbtTag<'_, '_>) -> Option<bool> {
    value.byte().map(|byte| byte != 0)
}

fn read_int(value: &BorrowedNbtTag<'_, '_>) -> Option<i32> {
    value.int()
}

fn read_long(value: &BorrowedNbtTag<'_, '_>) -> Option<i64> {
    value.long()
}

fn read_block_pos(value: &BorrowedNbtTag<'_, '_>) -> Option<BlockPos> {
    block_pos_from_nbt(value.int_array()?.as_ref())
}

fn read_global_pos(value: &BorrowedNbtTag<'_, '_>) -> Option<GlobalPos> {
    global_pos_from_nbt(&value.compound()?)
}

fn read_uuid(value: &BorrowedNbtTag<'_, '_>) -> Option<Uuid> {
    Uuid::from_int_array(value.int_array()?.as_ref())
}

fn read_global_pos_set(value: &BorrowedNbtTag<'_, '_>) -> Option<FxHashSet<GlobalPos>> {
    Some(
        value
            .list()?
            .compounds()
            .into_iter()
            .flatten()
            .filter_map(|compound| global_pos_from_nbt(&compound))
            .collect(),
    )
}

impl MemoryValueType for Unit {
    fn into_memory_value(self) -> MemoryValue {
        MemoryValue::Unit
    }

    fn from_memory_value(value: &MemoryValue) -> Option<Self> {
        matches!(value, MemoryValue::Unit).then_some(Self)
    }

    fn from_nbt(_value: &BorrowedNbtTag<'_, '_>) -> Option<MemoryValue> {
        Some(MemoryValue::Unit)
    }
}

impl_memory_value_type!(bool, Bool, from_nbt = read_bool);
impl_memory_value_type!(i32, Int, from_nbt = read_int);
impl_memory_value_type!(i64, Long, from_nbt = read_long);
impl_memory_value_type!(BlockPos, BlockPos, from_nbt = read_block_pos);
impl_memory_value_type!(GlobalPos, GlobalPos, from_nbt = read_global_pos);
impl_memory_value_type!(
    FxHashSet<GlobalPos>,
    GlobalPosSet,
    from_nbt = read_global_pos_set
);
impl_memory_value_type!(Uuid, Uuid, from_nbt = read_uuid);
impl_memory_value_type!(Vec<Uuid>, Uuids);
impl_memory_value_type!(DVec3, Vec3);
impl_memory_value_type!(EntityMemory, Entity);
impl_memory_value_type!(Vec<EntityMemory>, Entities);
impl_memory_value_type!(NearestVisibleLivingEntities, NearestVisibleLivingEntities);
impl_memory_value_type!(WalkTarget, WalkTarget);
impl_memory_value_type!(PositionTracker, PositionTracker);
impl_memory_value_type!(DamageSource, DamageSource);
impl_memory_value_type!(Path, Path);
