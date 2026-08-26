//! The three behaviors an amphibian uses to get between land and water.
//!
//! Vanilla parity: `TryFindLand`, `TryFindLandNearWater` and
//! `TryLaySpawnOnFluidNearLand`. All three are the same shape -- a cooldown, a
//! Manhattan walk outward from the mob, and the first block that passes a test
//! becomes the walk target -- so they are read more easily together than apart.

use steel_registry::blocks::BlockRef;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::vanilla_block_tags::BlockTag;
use steel_registry::vanilla_fluid_tags::FluidTag;
use steel_registry::{sound_events, vanilla_blocks, vanilla_game_events};
use steel_utils::types::UpdateFlags;
use steel_utils::{BlockPos, Direction};

use super::{BrainContext, MemoryModuleId, Trigger};

use crate::entity::ai::brain::memory::{MemoryStatus, WalkTarget, memory_module_types};
use crate::entity::ai::brain::position_tracker::PositionTracker;
use crate::world::game_event::GameEventContext;
use crate::world::{LevelReader as _, World};

/// Vanilla parity: `TryFindLand.COOLDOWN_TICKS`.
const FIND_LAND_COOLDOWN_TICKS: i64 = 60;
/// Vanilla parity: the `40L` cooldown of `TryFindLandNearWater`.
const FIND_LAND_NEAR_WATER_COOLDOWN_TICKS: i64 = 40;

/// Walks a mob out of the water onto the nearest dry, solid-topped block.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.behavior.TryFindLand`.
pub struct TryFindLand {
    range: i32,
    speed_modifier: f64,
    next_ok_start_time: i64,
}

impl TryFindLand {
    /// Vanilla parity: `TryFindLand.create(int, float)`.
    #[must_use]
    pub const fn new(range: i32, speed_modifier: f64) -> Self {
        Self {
            range,
            speed_modifier,
            next_ok_start_time: 0,
        }
    }
}

impl Trigger for TryFindLand {
    fn required_memories(&self) -> Vec<MemoryModuleId> {
        vec![
            memory_module_types::ATTACK_TARGET.id(),
            memory_module_types::WALK_TARGET.id(),
            memory_module_types::LOOK_TARGET.id(),
        ]
    }

    fn trigger(&mut self, ctx: &BrainContext<'_>) -> bool {
        if !memories_free_for_land_search(ctx) {
            return false;
        }

        let body = ctx.mob();
        let world = ctx.world();
        let body_pos = body.block_position();
        if !world
            .get_block_state(body_pos)
            .get_fluid_state()
            .fluid_id
            .has_tag(&FluidTag::WATER)
        {
            return false;
        }

        let timestamp = ctx.game_time();
        if timestamp < self.next_ok_start_time {
            self.next_ok_start_time = timestamp + FIND_LAND_COOLDOWN_TICKS;
            return true;
        }

        for pos in within_manhattan(body_pos, self.range) {
            if pos.x() == body_pos.x() && pos.z() == body_pos.z() {
                continue;
            }
            let state = world.get_block_state(pos);
            if state.get_block() == &vanilla_blocks::WATER
                || !state.get_fluid_state().is_empty()
                || !has_empty_collision_shape(state, pos)
            {
                continue;
            }
            let below_pos = pos.below();
            if !world.is_face_sturdy(world.get_block_state(below_pos), below_pos, Direction::Up) {
                continue;
            }

            set_land_target(ctx, pos, self.speed_modifier, 1);
            break;
        }

        self.next_ok_start_time = timestamp + FIND_LAND_COOLDOWN_TICKS;
        true
    }

    fn debug_name(&self) -> &'static str {
        "TryFindLand"
    }
}

/// Walks a mob on land to a block that has water beside it.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.behavior.TryFindLandNearWater`.
/// This is what puts a pregnant frog on the bank rather than in the pond.
pub struct TryFindLandNearWater {
    range: i32,
    speed_modifier: f64,
    next_ok_start_time: i64,
}

impl TryFindLandNearWater {
    /// Vanilla parity: `TryFindLandNearWater.create(int, float)`.
    #[must_use]
    pub const fn new(range: i32, speed_modifier: f64) -> Self {
        Self {
            range,
            speed_modifier,
            next_ok_start_time: 0,
        }
    }
}

impl Trigger for TryFindLandNearWater {
    fn required_memories(&self) -> Vec<MemoryModuleId> {
        vec![
            memory_module_types::ATTACK_TARGET.id(),
            memory_module_types::WALK_TARGET.id(),
            memory_module_types::LOOK_TARGET.id(),
        ]
    }

    fn trigger(&mut self, ctx: &BrainContext<'_>) -> bool {
        if !memories_free_for_land_search(ctx) {
            return false;
        }

        let body = ctx.mob();
        let world = ctx.world();
        let body_pos = body.block_position();
        if world
            .get_block_state(body_pos)
            .get_fluid_state()
            .fluid_id
            .has_tag(&FluidTag::WATER)
        {
            return false;
        }

        let timestamp = ctx.game_time();
        if timestamp < self.next_ok_start_time {
            self.next_ok_start_time = timestamp + FIND_LAND_NEAR_WATER_COOLDOWN_TICKS;
            return true;
        }

        'search: for pos in within_manhattan(body_pos, self.range) {
            if pos.x() == body_pos.x() && pos.z() == body_pos.z() {
                continue;
            }
            if !has_empty_collision_shape(world.get_block_state(pos), pos) {
                continue;
            }
            let below_pos = pos.below();
            if has_empty_collision_shape(world.get_block_state(below_pos), below_pos) {
                continue;
            }

            for direction in Direction::HORIZONTAL {
                let beside = pos.relative(direction);
                if world.get_block_state(beside).is_air()
                    && world.get_block_state(beside.below()).get_block() == &vanilla_blocks::WATER
                {
                    set_land_target(ctx, pos, self.speed_modifier, 0);
                    break 'search;
                }
            }
        }

        self.next_ok_start_time = timestamp + FIND_LAND_NEAR_WATER_COOLDOWN_TICKS;
        true
    }

    fn debug_name(&self) -> &'static str {
        "TryFindLandNearWater"
    }
}

/// Lays a spawn block on the water beside a mob standing on the bank.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.behavior.TryLaySpawnOnFluidNearLand`.
/// This is the near end of the frogspawn loop: a bred frog carrying
/// `IS_PREGNANT` puts the block down here, and the block hatches tadpoles.
pub struct TryLaySpawnOnFluidNearLand {
    spawn_block: BlockRef,
}

impl TryLaySpawnOnFluidNearLand {
    /// Vanilla parity: `TryLaySpawnOnFluidNearLand.create(Block)`.
    #[must_use]
    pub const fn new(spawn_block: BlockRef) -> Self {
        Self { spawn_block }
    }
}

impl Trigger for TryLaySpawnOnFluidNearLand {
    fn required_memories(&self) -> Vec<MemoryModuleId> {
        vec![
            memory_module_types::ATTACK_TARGET.id(),
            memory_module_types::WALK_TARGET.id(),
            memory_module_types::IS_PREGNANT.id(),
        ]
    }

    fn trigger(&mut self, ctx: &BrainContext<'_>) -> bool {
        let brain = ctx.brain();
        if !brain.check_memory(
            memory_module_types::ATTACK_TARGET.id(),
            MemoryStatus::ValueAbsent,
        ) || !brain.check_memory(
            memory_module_types::WALK_TARGET.id(),
            MemoryStatus::ValuePresent,
        ) || !brain.check_memory(
            memory_module_types::IS_PREGNANT.id(),
            MemoryStatus::ValuePresent,
        ) {
            return false;
        }

        let body = ctx.mob();
        if body.is_in_water() || !body.on_ground() {
            return false;
        }

        let world = ctx.world();
        let below_pos = body.block_position().below();
        for direction in Direction::HORIZONTAL {
            let relative_pos = below_pos.relative(direction);
            if !supports_frogspawn(world, relative_pos) {
                continue;
            }

            let spawn_pos = relative_pos.above();
            if !world.get_block_state(spawn_pos).is_air() {
                continue;
            }

            let new_state = self.spawn_block.default_state();
            world.set_block(spawn_pos, new_state, UpdateFlags::UPDATE_ALL);
            world.game_event(
                &vanilla_game_events::BLOCK_PLACE,
                spawn_pos,
                &GameEventContext::new(Some(body.as_entity_event_source()), Some(new_state)),
            );
            body.play_sound(&sound_events::ENTITY_FROG_LAY_SPAWN, 1.0, 1.0);
            brain.erase_memory(memory_module_types::IS_PREGNANT.id());
            return true;
        }

        true
    }

    fn debug_name(&self) -> &'static str {
        "TryLaySpawnOnFluidNearLand"
    }
}

/// Vanilla parity: the shared `i.absent(ATTACK_TARGET), i.absent(WALK_TARGET),
/// i.registered(LOOK_TARGET)` group of both land searches.
fn memories_free_for_land_search(ctx: &BrainContext<'_>) -> bool {
    let brain = ctx.brain();
    brain.check_memory(
        memory_module_types::ATTACK_TARGET.id(),
        MemoryStatus::ValueAbsent,
    ) && brain.check_memory(
        memory_module_types::WALK_TARGET.id(),
        MemoryStatus::ValueAbsent,
    ) && brain.check_memory(
        memory_module_types::LOOK_TARGET.id(),
        MemoryStatus::Registered,
    )
}

/// Sets the look and walk targets both land searches end with.
fn set_land_target(ctx: &BrainContext<'_>, pos: BlockPos, speed_modifier: f64, close_enough: i32) {
    let brain = ctx.brain();
    brain.set_memory(
        memory_module_types::LOOK_TARGET,
        PositionTracker::of_block(pos),
    );
    brain.set_memory(
        memory_module_types::WALK_TARGET,
        WalkTarget::of_block(pos, speed_modifier, close_enough),
    );
}

/// Vanilla parity: the `SUPPORTS_FROGSPAWN` half of
/// `TryLaySpawnOnFluidNearLand`, which accepts both the fluid tag and the block
/// tag so a frog can lay on a waterlogged surface too.
fn supports_frogspawn(world: &World, pos: BlockPos) -> bool {
    let state = world.get_block_state(pos);
    // Vanilla asks for an empty *up* face rather than an empty shape, so a
    // waterlogged slab counts and a full block does not.
    if !has_empty_collision_shape(state, pos) && world.is_collision_shape_full_block_at(pos, state)
    {
        return false;
    }
    state
        .get_fluid_state()
        .fluid_id
        .has_tag(&FluidTag::SUPPORTS_FROGSPAWN)
        || state.get_block().has_tag(&BlockTag::SUPPORTS_FROGSPAWN)
}

/// Returns whether this block has nothing to bump into.
///
/// Vanilla parity: `state.getCollisionShape(level, pos, context).isEmpty()`.
fn has_empty_collision_shape(state: steel_utils::BlockStateId, pos: BlockPos) -> bool {
    state.get_collision_shape_at(pos).is_empty()
}

/// Walks the blocks within a Manhattan radius, nearest first.
///
/// Vanilla parity: `BlockPos.withinManhattan(pos, range, range, range)`, whose
/// order decides which land a mob picks when several qualify.
fn within_manhattan(origin: BlockPos, range: i32) -> impl Iterator<Item = BlockPos> {
    (0..=(range * 3)).flat_map(move |radius| {
        (-range..=range).flat_map(move |dx| {
            (-range..=range).flat_map(move |dy| {
                (-range..=range)
                    .filter(move |dz| dx.abs() + dy.abs() + dz.abs() == radius)
                    .map(move |dz| BlockPos::new(origin.x() + dx, origin.y() + dy, origin.z() + dz))
            })
        })
    })
}
