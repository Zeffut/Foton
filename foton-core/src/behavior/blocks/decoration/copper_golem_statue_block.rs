//! Copper golem statue blocks.
//!
//! Vanilla parity: `CopperGolemStatueBlock` and `WeatheringCopperGolemStatueBlock`.
//! A statue is what a fully oxidized copper golem becomes, and an unoxidized one
//! is what an axe turns back into a golem. Clicking a statue cycles it through
//! its four poses, which is also what its comparator reading is.

use std::sync::{Arc, Weak};

use foton_macros::block_behavior;
use foton_registry::blocks::BlockRef;
use foton_registry::blocks::block_state_ext::BlockStateExt as _;
use foton_registry::blocks::properties::{
    BlockStateProperties, BoolProperty, Direction, EnumProperty, Pose,
};
use foton_registry::items::item::BlockHitResult;
use foton_registry::vanilla_block_tags::BlockTag;
use foton_registry::vanilla_item_tags::ItemTag;
use foton_registry::{
    REGISTRY, TaggedRegistryExt as _, sound_events, vanilla_block_entity_types, vanilla_entities,
    vanilla_game_events, vanilla_items,
};
use foton_utils::types::{InteractionHand, UpdateFlags};
use foton_utils::{BlockPos, BlockStateId, Downcast as _};
use glam::DVec3;

use crate::behavior::block::{
    BlockBehavior, BlockEntityCreation, schedule_water_tick_if_waterlogged,
};
use crate::behavior::blocks::{WeatherState, WeatheringCopper};
use crate::behavior::context::BlockPlaceContext;
use crate::behavior::{InteractionResult, InventoryAccess};
use crate::block_entity::BLOCK_ENTITIES;
use crate::block_entity::entities::CopperGolemStatueBlockEntity;
use crate::entity::ai::path::PathComputationType;
use crate::entity::entities::CopperGolemEntity;
use crate::entity::{ENTITIES, Entity, SharedEntity, next_entity_id};
use crate::player::Player;
use crate::world::game_event::GameEventContext;
use crate::world::{LevelReader, ScheduledTickAccess, World};

const COPPER_GOLEM_POSE: &EnumProperty<Pose> = &BlockStateProperties::COPPER_GOLEM_POSE;
const HORIZONTAL_FACING: &EnumProperty<Direction> = &BlockStateProperties::HORIZONTAL_FACING;
const WATERLOGGED: &BoolProperty = &BlockStateProperties::WATERLOGGED;

/// Returns the pose a click moves a statue on to.
///
/// Vanilla parity: `CopperGolemStatueBlock.Pose.getNextPose`, whose
/// out-of-bounds strategy is `ZERO`, so the last pose wraps back to standing.
const fn next_pose(pose: &Pose) -> Pose {
    match pose {
        Pose::Standing => Pose::Sitting,
        Pose::Sitting => Pose::Running,
        Pose::Running => Pose::Star,
        Pose::Star => Pose::Standing,
    }
}

/// Returns the comparator reading a pose gives.
///
/// Vanilla parity: `CopperGolemStatueBlock.getAnalogOutputSignal`, which is the
/// pose's ordinal plus one.
const fn pose_signal(pose: &Pose) -> i32 {
    match pose {
        Pose::Standing => 1,
        Pose::Sitting => 2,
        Pose::Running => 3,
        Pose::Star => 4,
    }
}

/// A waxed copper golem statue.
///
/// Vanilla parity: `CopperGolemStatueBlock`, which backs the four waxed blocks.
#[block_behavior]
pub struct CopperGolemStatueBlock {
    block: BlockRef,
    #[json_arg(r#enum = "WeatherState", json = "weathering_state")]
    weathering_state: WeatherState,
}

impl CopperGolemStatueBlock {
    /// Creates the statue behavior.
    #[must_use]
    pub const fn new(block: BlockRef, weathering_state: WeatherState) -> Self {
        Self {
            block,
            weathering_state,
        }
    }

    /// Returns how oxidized this statue is.
    ///
    /// Vanilla parity: `CopperGolemStatueBlock.getWeatheringState`.
    #[must_use]
    pub const fn weathering_state(&self) -> WeatherState {
        self.weathering_state
    }

    /// Moves the statue on to its next pose.
    ///
    /// Vanilla parity: `CopperGolemStatueBlock.updatePose`.
    pub fn update_pose(world: &Arc<World>, state: BlockStateId, pos: BlockPos, player: &Player) {
        world.play_block_sound(
            &sound_events::ENTITY_COPPER_GOLEM_BECOME_STATUE,
            pos,
            1.0,
            1.0,
            None,
        );
        let posed = state.set_value(
            COPPER_GOLEM_POSE,
            next_pose(&state.get_value(COPPER_GOLEM_POSE)),
        );
        world.set_block(pos, posed, UpdateFlags::UPDATE_ALL);
        world.game_event(
            &vanilla_game_events::BLOCK_CHANGE,
            pos,
            &GameEventContext::new(Some(player as &dyn Entity), Some(posed)),
        );
    }

    fn statue_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> BlockStateId {
        self.block
            .default_state()
            .set_value(HORIZONTAL_FACING, context.horizontal_direction().opposite())
            .set_value(COPPER_GOLEM_POSE, Pose::Standing)
            .set_value(WATERLOGGED, context.is_water_source())
    }
}

impl BlockBehavior for CopperGolemStatueBlock {
    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        Some(self.statue_state_for_placement(context))
    }

    /// Vanilla parity: `CopperGolemStatueBlock.useItemOn`. An axe passes so the
    /// axe item can scrape or unwax the block instead of posing it.
    fn use_item_on(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        player: &Player,
        _hand: InteractionHand,
        _hit_result: &BlockHitResult,
        inv: &mut InventoryAccess,
    ) -> InteractionResult {
        let holds_axe = inv.with_item(|item| REGISTRY.items.is_in_tag(item.item(), &ItemTag::AXES));
        if holds_axe {
            return InteractionResult::Pass;
        }

        Self::update_pose(world, state, pos, player);
        InteractionResult::Success
    }

    fn update_shape(
        &self,
        state: BlockStateId,
        world: &dyn ScheduledTickAccess,
        pos: BlockPos,
        _direction: Direction,
        _neighbor_pos: BlockPos,
        _neighbor_state: BlockStateId,
    ) -> BlockStateId {
        schedule_water_tick_if_waterlogged(state, world, pos);
        state
    }

    /// Vanilla parity: `CopperGolemStatueBlock.isPathfindable`.
    fn is_pathfindable(&self, state: BlockStateId, computation_type: PathComputationType) -> bool {
        computation_type == PathComputationType::Water && state.get_value(WATERLOGGED)
    }

    fn new_block_entity(
        &self,
        level: Weak<World>,
        pos: BlockPos,
        state: BlockStateId,
    ) -> BlockEntityCreation {
        BlockEntityCreation::from_registered_factory(BLOCK_ENTITIES.create(
            &vanilla_block_entity_types::COPPER_GOLEM_STATUE,
            level,
            pos,
            state,
        ))
    }

    /// Vanilla parity: `CopperGolemStatueBlock.shouldChangedStateKeepBlockEntity`,
    /// which is what lets a statue weather and wax without losing its name.
    fn should_keep_block_entity(&self, old_state: BlockStateId, _new_state: BlockStateId) -> bool {
        old_state
            .get_block()
            .has_tag(&BlockTag::COPPER_GOLEM_STATUES)
    }

    fn has_analog_output_signal(&self, _state: BlockStateId) -> bool {
        true
    }

    fn get_analog_output_signal(
        &self,
        state: BlockStateId,
        _world: &dyn LevelReader,
        _pos: BlockPos,
        _direction: Direction,
    ) -> i32 {
        pose_signal(&state.get_value(COPPER_GOLEM_POSE))
    }

    fn affect_neighbors_after_removal(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        _moved_by_piston: bool,
    ) {
        world.update_neighbor_for_output_signal(pos, state.get_block());
    }
}

/// An unwaxed copper golem statue.
///
/// Vanilla parity: `WeatheringCopperGolemStatueBlock`, which backs the four
/// unwaxed blocks. Only these oxidize, and only an unoxidized one wakes up.
#[block_behavior]
pub struct WeatheringCopperGolemStatueBlock {
    statue: CopperGolemStatueBlock,
    #[json_arg(r#enum = "WeatherState", json = "weathering_state")]
    weathering: WeatheringCopper,
}

impl WeatheringCopperGolemStatueBlock {
    /// Creates the weathering statue behavior.
    #[must_use]
    pub const fn new(block: BlockRef, weathering_state: WeatherState) -> Self {
        Self {
            statue: CopperGolemStatueBlock::new(block, weathering_state),
            weathering: WeatheringCopper::new(weathering_state),
        }
    }

    /// Wakes a statue back up into the golem it came from.
    ///
    /// Vanilla parity: `CopperGolemStatueBlockEntity.removeStatue` together
    /// with its `initCopperGolem`. Foton builds the golem here rather than in
    /// the block entity so the block-entity layer keeps out of the entity
    /// registry.
    fn wake_statue(world: &Arc<World>, state: BlockStateId, pos: BlockPos) -> Option<SharedEntity> {
        let statue_entity = world.get_block_entity(pos)?;
        let statue_entity = statue_entity.downcast_ref::<CopperGolemStatueBlockEntity>()?;

        let facing = state.get_value(HORIZONTAL_FACING);
        let position = DVec3::new(
            f64::from(pos.x()) + 0.5,
            f64::from(pos.y()),
            f64::from(pos.z()) + 0.5,
        );
        let golem = ENTITIES.create(
            &vanilla_entities::COPPER_GOLEM,
            next_entity_id(),
            position,
            Arc::downgrade(world),
        )?;

        golem.set_custom_name(statue_entity.custom_name());
        golem.set_rotation((facing.to_yaw(), 0.0));
        if let Some(living) = golem.as_living_entity() {
            living.set_y_head_rot(facing.to_yaw());
            living.set_y_body_rot(facing.to_yaw());
        }
        if let Some(copper_golem) = golem.downcast_ref::<CopperGolemEntity>() {
            copper_golem.play_spawn_sound();
        }

        Some(golem)
    }
}

impl BlockBehavior for WeatheringCopperGolemStatueBlock {
    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        self.statue.get_state_for_placement(context)
    }

    /// Vanilla parity: `WeatheringCopperGolemStatueBlock.useItemOn`.
    fn use_item_on(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        player: &Player,
        _hand: InteractionHand,
        _hit_result: &BlockHitResult,
        inv: &mut InventoryAccess,
    ) -> InteractionResult {
        if world.get_block_entity(pos).is_none() {
            return InteractionResult::Pass;
        }

        let (holds_axe, holds_honeycomb) = inv.with_item(|item| {
            (
                REGISTRY.items.is_in_tag(item.item(), &ItemTag::AXES),
                item.is(&vanilla_items::HONEYCOMB),
            )
        });

        if !holds_axe {
            if holds_honeycomb {
                return InteractionResult::Pass;
            }
            CopperGolemStatueBlock::update_pose(world, state, pos, player);
            return InteractionResult::Success;
        }

        if self.statue.weathering_state() != WeatherState::Unaffected {
            return InteractionResult::Pass;
        }

        let Some(golem) = Self::wake_statue(world, state, pos) else {
            return InteractionResult::Pass;
        };

        let has_infinite_materials = player.has_infinite_materials();
        inv.with_item(|item| item.hurt_and_break(1, has_infinite_materials));

        if let Err(error) = world.try_add_entity(golem) {
            log::debug!("copper golem statue could not wake at {pos:?}: {error}");
            return InteractionResult::Pass;
        }
        world.remove_block(pos, false);

        InteractionResult::Success
    }

    fn random_tick(&self, state: BlockStateId, world: &Arc<World>, pos: BlockPos) {
        self.weathering.change_over_time(state, world, pos);
    }

    fn update_shape(
        &self,
        state: BlockStateId,
        world: &dyn ScheduledTickAccess,
        pos: BlockPos,
        direction: Direction,
        neighbor_pos: BlockPos,
        neighbor_state: BlockStateId,
    ) -> BlockStateId {
        self.statue
            .update_shape(state, world, pos, direction, neighbor_pos, neighbor_state)
    }

    fn is_pathfindable(&self, state: BlockStateId, computation_type: PathComputationType) -> bool {
        self.statue.is_pathfindable(state, computation_type)
    }

    fn new_block_entity(
        &self,
        level: Weak<World>,
        pos: BlockPos,
        state: BlockStateId,
    ) -> BlockEntityCreation {
        self.statue.new_block_entity(level, pos, state)
    }

    fn should_keep_block_entity(&self, old_state: BlockStateId, new_state: BlockStateId) -> bool {
        self.statue.should_keep_block_entity(old_state, new_state)
    }

    fn has_analog_output_signal(&self, state: BlockStateId) -> bool {
        self.statue.has_analog_output_signal(state)
    }

    fn get_analog_output_signal(
        &self,
        state: BlockStateId,
        world: &dyn LevelReader,
        pos: BlockPos,
        direction: Direction,
    ) -> i32 {
        self.statue
            .get_analog_output_signal(state, world, pos, direction)
    }

    fn affect_neighbors_after_removal(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        moved_by_piston: bool,
    ) {
        self.statue
            .affect_neighbors_after_removal(state, world, pos, moved_by_piston);
    }
}
