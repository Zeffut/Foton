//! Block shapes and ray traces a plugin asks the world for: the boxes behind
//! `Block#getBoundingBox` and `getCollisionShape`, the flags behind
//! `Block#isLiquid` and `BlockData#isOccluding`, and `World#rayTraceBlocks`.

use std::ffi::c_void;

use foton_core::world::{ClipBlockShape, ClipFluid, LevelReader as _};
use foton_registry::blocks::block_state_ext::BlockStateExt as _;
use foton_utils::{BlockPos, Direction};
use glam::DVec3;
use jni::JNIEnv;
use jni::objects::{JClass, JString};
use jni::sys::{jboolean, jdouble, jdoubleArray, jint};

use super::parse_state;
use super::support::{doubles, method, text, world};

const OUTLINE: jint = 0;
const COLLISION: jint = 1;

const LIQUID: jint = 1;
const OCCLUDING: jint = 2;

/// The boxes of a block's outline (0) or collision (1) shape, six values
/// each, in block-local coordinates with the block's random offset applied.
///
/// Vanilla parity: `BlockState.getShape` and `getCollisionShape` with an
/// empty collision context, which is what `CraftBlock` asks for.
extern "system" fn block_shape_boxes(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    x: jint,
    y: jint,
    z: jint,
    kind: jint,
) -> jdoubleArray {
    let Some(world) = world(&mut env, &name) else {
        return doubles(&mut env, None);
    };
    let pos = BlockPos::new(x, y, z);
    let state = world.get_block_state(pos);
    let shape = match kind {
        OUTLINE => state.get_outline_shape_at(pos),
        COLLISION => state.get_collision_shape_at(pos),
        _ => return doubles(&mut env, None),
    };
    let values: Vec<f64> = shape
        .iter()
        .flat_map(|aabb| {
            [
                aabb.min_x(),
                aabb.min_y(),
                aabb.min_z(),
                aabb.max_x(),
                aabb.max_y(),
                aabb.max_z(),
            ]
        })
        .collect();
    doubles(&mut env, Some(&values))
}

/// Flags of a block state written as block data: 1 when it is a liquid
/// (vanilla's `BlockState.liquid()`), 2 when it occludes (`canOcclude()`).
extern "system" fn block_state_flags(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    data: JString<'_>,
) -> jint {
    let Some(state) = text(&mut env, &data).and_then(|value| parse_state(&value)) else {
        return 0;
    };
    let config = &state.get_block().config;
    let mut flags = 0;
    if config.liquid {
        flags |= LIQUID;
    }
    if config.can_occlude {
        flags |= OCCLUDING;
    }
    flags
}

/// Bukkit's `BlockFace` ordinal for a hit face.
const fn face_ordinal(direction: Direction) -> f64 {
    match direction {
        Direction::North => 0.0,
        Direction::East => 1.0,
        Direction::South => 2.0,
        Direction::West => 3.0,
        Direction::Up => 4.0,
        Direction::Down => 5.0,
    }
}

/// Clips from `start` to `end` against block and fluid shapes, as
/// `CraftWorld`'s `rayTraceBlocks`: the collision shape when passable blocks
/// are ignored, the outline shape otherwise; fluids per the mode (0 never,
/// 1 sources only, 2 any). Answers `{hit x, y, z, block x, y, z, face}`, or
/// null on a miss.
extern "system" fn ray_trace_blocks(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    name: JString<'_>,
    start_x: jdouble,
    start_y: jdouble,
    start_z: jdouble,
    end_x: jdouble,
    end_y: jdouble,
    end_z: jdouble,
    fluid_mode: jint,
    ignore_passable: jboolean,
) -> jdoubleArray {
    let Some(world) = world(&mut env, &name) else {
        return doubles(&mut env, None);
    };
    let start = DVec3::new(start_x, start_y, start_z);
    let end = DVec3::new(end_x, end_y, end_z);
    if !start.is_finite() || !end.is_finite() {
        return doubles(&mut env, None);
    }
    let fluid = match fluid_mode {
        1 => ClipFluid::SourceOnly,
        2 => ClipFluid::Any,
        _ => ClipFluid::None,
    };
    let shape = if ignore_passable == 0 {
        ClipBlockShape::Outline
    } else {
        ClipBlockShape::Collider
    };
    let hit = world.clip(start, end, shape, fluid);
    if hit.is_miss() {
        return doubles(&mut env, None);
    }
    let values = [
        hit.location.x,
        hit.location.y,
        hit.location.z,
        f64::from(hit.block_pos.x()),
        f64::from(hit.block_pos.y()),
        f64::from(hit.block_pos.z()),
        face_ordinal(hit.direction),
    ];
    doubles(&mut env, Some(&values))
}

pub(super) fn bindings() -> Vec<jni::NativeMethod> {
    vec![
        method(
            "blockShapeBoxes",
            "(Ljava/lang/String;IIII)[D",
            block_shape_boxes as *mut c_void,
        ),
        method(
            "blockStateFlags",
            "(Ljava/lang/String;)I",
            block_state_flags as *mut c_void,
        ),
        method(
            "rayTraceBlocks",
            "(Ljava/lang/String;DDDDDDIZ)[D",
            ray_trace_blocks as *mut c_void,
        ),
    ]
}
