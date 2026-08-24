//! Vanilla `TransportItemsBetweenContainers`.

use std::sync::Arc;

use glam::DVec3;
use rustc_hash::{FxHashMap, FxHashSet};
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::blocks::properties::{BlockStateProperties, ChestType};
use steel_registry::item_stack::ItemStack;
use steel_utils::{BlockPos, BlockStateId, ChunkPos, Direction, GlobalPos, WorldAabb};

use super::{BrainContext, TimedBehavior};
use crate::behavior::blocks::ChestBlock;
use crate::behavior::{BLOCK_BEHAVIORS, BlockCollisionContext};
use crate::block_entity::SharedBlockEntity;
use crate::block_entity::entities::attached_containers_at;
use crate::entity::PathfinderMob;
use crate::entity::ai::brain::behavior::utils::set_walk_and_look_target_memories;
use crate::entity::ai::brain::memory::{MemoryModuleId, MemoryStatus, memory_module_types};
use crate::entity::ai::brain::position_tracker::PositionTracker;
use crate::entity::ai::path::Path;
use crate::entity::living_base::LivingTravelInput;
use crate::inventory::container::Container;
use crate::inventory::equipment::EquipmentSlot;
use crate::inventory::lock::{AttachedContainers, ContainerLockGuard};
use crate::world::{ClipBlockShape, ClipFluid, World};

/// How long the golem stands at a chest before the transfer resolves.
///
/// Vanilla parity: `TransportItemsBetweenContainers.TARGET_INTERACTION_TIME`.
pub const TARGET_INTERACTION_TIME: i32 = 60;
/// Vanilla parity: `VISITED_POSITIONS_MEMORY_TIME`.
const VISITED_POSITIONS_MEMORY_TIME: i64 = 6000;
/// Vanilla parity: `TRANSPORTED_ITEM_MAX_STACK_SIZE`.
const TRANSPORTED_ITEM_MAX_STACK_SIZE: i32 = 16;
/// Vanilla parity: `MAX_VISITED_POSITIONS`.
const MAX_VISITED_POSITIONS: usize = 10;
/// Vanilla parity: `MAX_UNREACHABLE_POSITIONS`.
const MAX_UNREACHABLE_POSITIONS: usize = 50;
/// Vanilla parity: `PASSENGER_MOB_TARGET_SEARCH_DISTANCE`.
const PASSENGER_MOB_TARGET_SEARCH_DISTANCE: i32 = 1;
/// Vanilla parity: `IDLE_COOLDOWN`.
const IDLE_COOLDOWN: i32 = 140;
/// Vanilla parity: `CLOSE_ENOUGH_TO_START_QUEUING_DISTANCE`.
const CLOSE_ENOUGH_TO_START_QUEUING_DISTANCE: f64 = 3.0;
/// Vanilla parity: `CLOSE_ENOUGH_TO_START_INTERACTING_WITH_TARGET_DISTANCE`.
const CLOSE_ENOUGH_TO_START_INTERACTING_WITH_TARGET_DISTANCE: f64 = 0.5;
/// Vanilla parity: `CLOSE_ENOUGH_TO_START_INTERACTING_WITH_TARGET_PATH_END_DISTANCE`.
const CLOSE_ENOUGH_TO_START_INTERACTING_WITH_TARGET_PATH_END_DISTANCE: f64 = 1.0;
/// Vanilla parity: `CLOSE_ENOUGH_TO_CONTINUE_INTERACTING_WITH_TARGET`.
const CLOSE_ENOUGH_TO_CONTINUE_INTERACTING_WITH_TARGET: f64 = 2.0;

/// What the mob is doing about its current container.
///
/// Vanilla parity: `TransportItemsBetweenContainers.TransportItemState`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TransportItemState {
    Travelling,
    Queuing,
    Interacting,
}

/// Which of the four outcomes the mob reached its container for.
///
/// Vanilla parity: `TransportItemsBetweenContainers.ContainerInteractionState`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[expect(
    clippy::enum_variant_names,
    reason = "these four names are vanilla's ContainerInteractionState verbatim"
)]
pub enum ContainerInteractionState {
    PickupItem,
    PickupNoItem,
    PlaceItem,
    PlaceNoItem,
}

/// A container the mob has decided to walk to.
///
/// Vanilla parity: `TransportItemsBetweenContainers.TransportItemTarget`. Vanilla
/// holds one `Container`, which for a double chest is a `CompoundContainer`
/// spanning both halves; Steel holds the two independently lockable
/// [`crate::inventory::lock::ContainerRef`]s that
/// `BlockBehavior::get_attached_containers` already returns in vanilla's
/// `DoubleBlockCombiner` order, so slot order across the pair is identical.
pub struct TransportItemTarget {
    pos: BlockPos,
    containers: AttachedContainers,
    block_entity: SharedBlockEntity,
    state: BlockStateId,
}

impl TransportItemTarget {
    /// Vanilla parity: `TransportItemTarget.tryCreatePossibleTarget(BlockEntity, Level)`.
    fn try_create(world: &Arc<World>, block_entity: &SharedBlockEntity) -> Option<Self> {
        let pos = block_entity.get_block_pos();
        let containers = attached_containers_at(world, pos);
        if containers.is_empty() {
            return None;
        }
        Some(Self {
            pos,
            containers,
            block_entity: Arc::clone(block_entity),
            state: block_entity.get_block_state(),
        })
    }

    /// Vanilla parity: `TransportItemTarget.tryCreatePossibleTarget(BlockPos, Level)`.
    fn try_create_at(world: &Arc<World>, pos: BlockPos) -> Option<Self> {
        let block_entity = world.get_block_entity(pos)?;
        Self::try_create(world, &block_entity)
    }

    /// The block this target sits at.
    #[must_use]
    pub const fn pos(&self) -> BlockPos {
        self.pos
    }

    /// The block entity the mob is about to open.
    #[must_use]
    pub const fn block_entity(&self) -> &SharedBlockEntity {
        &self.block_entity
    }

    /// Vanilla parity: `Container.isEmpty` across the whole target.
    fn is_empty(&self, guard: &ContainerLockGuard) -> bool {
        self.containers.iter().all(|reference| {
            guard
                .get(reference.container_id())
                .is_none_or(Container::is_empty)
        })
    }
}

/// Runs when the mob has been standing at its target for `ticks_since_reaching_target`.
///
/// Vanilla parity: `TransportItemsBetweenContainers.OnTargetReachedInteraction`.
pub type OnTargetReachedInteraction =
    Box<dyn Fn(&dyn PathfinderMob, &TransportItemTarget, i32) + Send>;

/// Whether a block is worth walking to.
pub type BlockStatePredicate = Box<dyn Fn(BlockStateId) -> bool + Send>;
/// Whether someone else is already using this target.
pub type ShouldQueueForTarget = Box<dyn Fn(&TransportItemTarget) -> bool + Send>;
/// Runs when the mob gives up on its target and walks off.
pub type OnStartTravelling = Box<dyn Fn(&dyn PathfinderMob) + Send>;

/// Carries one item stack at a time from a source container to a destination.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.behavior.TransportItemsBetweenContainers`.
/// This is the whole of the copper golem's job.
pub struct TransportItemsBetweenContainers {
    entry_condition: [(MemoryModuleId, MemoryStatus); 4],
    speed_modifier: f64,
    horizontal_search_distance: i32,
    vertical_search_distance: i32,
    source_block_type: BlockStatePredicate,
    destination_block_type: BlockStatePredicate,
    should_queue_for_target: ShouldQueueForTarget,
    on_start_travelling: OnStartTravelling,
    on_target_interaction_actions: FxHashMap<ContainerInteractionState, OnTargetReachedInteraction>,
    target: Option<TransportItemTarget>,
    state: TransportItemState,
    interaction_state: Option<ContainerInteractionState>,
    ticks_since_reaching_target: i32,
}

impl TransportItemsBetweenContainers {
    /// Builds the behavior from the mob's own choices about what to carry
    /// between what.
    #[must_use]
    #[expect(
        clippy::too_many_arguments,
        reason = "vanilla's constructor takes exactly these eight, and splitting them would hide which mob supplies which"
    )]
    pub fn new(
        speed_modifier: f64,
        source_block_type: BlockStatePredicate,
        destination_block_type: BlockStatePredicate,
        horizontal_search_distance: i32,
        vertical_search_distance: i32,
        on_target_interaction_actions: FxHashMap<
            ContainerInteractionState,
            OnTargetReachedInteraction,
        >,
        on_start_travelling: OnStartTravelling,
        should_queue_for_target: ShouldQueueForTarget,
    ) -> Self {
        Self {
            entry_condition: [
                (
                    memory_module_types::VISITED_BLOCK_POSITIONS.id(),
                    MemoryStatus::Registered,
                ),
                (
                    memory_module_types::UNREACHABLE_TRANSPORT_BLOCK_POSITIONS.id(),
                    MemoryStatus::Registered,
                ),
                (
                    memory_module_types::TRANSPORT_ITEMS_COOLDOWN_TICKS.id(),
                    MemoryStatus::ValueAbsent,
                ),
                (
                    memory_module_types::IS_PANICKING.id(),
                    MemoryStatus::ValueAbsent,
                ),
            ],
            speed_modifier,
            horizontal_search_distance,
            vertical_search_distance,
            source_block_type,
            destination_block_type,
            should_queue_for_target,
            on_start_travelling,
            on_target_interaction_actions,
            target: None,
            state: TransportItemState::Travelling,
            interaction_state: None,
            ticks_since_reaching_target: 0,
        }
    }

    /// Vanilla parity: `isPickingUpItems`, which is just "hands empty".
    fn is_picking_up_items(mob: &dyn PathfinderMob) -> bool {
        let mut empty = true;
        mob.with_equipment_slot(EquipmentSlot::MainHand, &mut |item_stack| {
            empty = item_stack.is_empty();
        });
        empty
    }

    fn main_hand_item(mob: &dyn PathfinderMob) -> ItemStack {
        let mut held = ItemStack::empty();
        mob.with_equipment_slot(EquipmentSlot::MainHand, &mut |item_stack| {
            held = item_stack.clone();
        });
        held
    }

    fn set_main_hand_item(mob: &dyn PathfinderMob, stack: ItemStack) {
        mob.with_equipment_slot_mut(EquipmentSlot::MainHand, &mut |slot| {
            *slot = stack.clone();
        });
    }

    /// Vanilla parity: `isWantedBlock`.
    fn is_wanted_block(&self, mob: &dyn PathfinderMob, state: BlockStateId) -> bool {
        if Self::is_picking_up_items(mob) {
            (self.source_block_type)(state)
        } else {
            (self.destination_block_type)(state)
        }
    }

    fn horizontal_search_distance(&self, mob: &dyn PathfinderMob) -> i32 {
        if mob.is_passenger() {
            PASSENGER_MOB_TARGET_SEARCH_DISTANCE
        } else {
            self.horizontal_search_distance
        }
    }

    fn vertical_search_distance(&self, mob: &dyn PathfinderMob) -> i32 {
        if mob.is_passenger() {
            PASSENGER_MOB_TARGET_SEARCH_DISTANCE
        } else {
            self.vertical_search_distance
        }
    }

    /// Vanilla parity: `getTargetSearchArea`.
    fn target_search_area(&self, mob: &dyn PathfinderMob) -> WorldAabb {
        let horizontal = f64::from(self.horizontal_search_distance(mob));
        let vertical = f64::from(self.vertical_search_distance(mob));
        let (center_x, center_y, center_z) = mob.block_position().get_center();
        WorldAabb::of_size(DVec3::new(center_x, center_y, center_z), 1.0, 1.0, 1.0)
            .inflate_xyz(horizontal, vertical, horizontal)
    }

    /// Vanilla parity: `getTransportTarget`, which walks the block entities of
    /// every chunk within reach rather than every block.
    fn find_transport_target(&self, ctx: &BrainContext<'_>) -> Option<TransportItemTarget> {
        let mob = ctx.mob();
        let world = ctx.world();
        let search_area = self.target_search_area(mob);
        let visited = visited_positions(ctx);
        let unreachable = unreachable_positions(ctx);
        let mob_position = mob.position();

        let center = ChunkPos::from_block_pos(mob.block_position());
        let chunk_radius = self.horizontal_search_distance(mob).div_euclid(16) + 1;

        let mut best: Option<(TransportItemTarget, f64)> = None;
        for chunk_x in (center.0.x - chunk_radius)..=(center.0.x + chunk_radius) {
            for chunk_z in (center.0.y - chunk_radius)..=(center.0.y + chunk_radius) {
                let Some(block_entities) = world
                    .chunk_map
                    .with_full_chunk(ChunkPos::new(chunk_x, chunk_z), |chunk| {
                        chunk.get_block_entities()
                    })
                else {
                    continue;
                };

                for block_entity in block_entities {
                    let pos = block_entity.get_block_pos();
                    let (center_x, center_y, center_z) = pos.get_center();
                    let distance =
                        DVec3::new(center_x, center_y, center_z).distance_squared(mob_position);
                    if best
                        .as_ref()
                        .is_some_and(|(_, closest)| distance >= *closest)
                    {
                        continue;
                    }
                    let Some(candidate) = self.target_valid_to_pick(
                        ctx,
                        &block_entity,
                        &visited,
                        &unreachable,
                        search_area,
                    ) else {
                        continue;
                    };
                    best = Some((candidate, distance));
                }
            }
        }

        best.map(|(target, _)| target)
    }

    /// Vanilla parity: `isTargetValidToPick`. The `isContainerLocked` branch is
    /// not ported because Steel has no `LockCode` on container block entities.
    fn target_valid_to_pick(
        &self,
        ctx: &BrainContext<'_>,
        block_entity: &SharedBlockEntity,
        visited: &FxHashSet<GlobalPos>,
        unreachable: &FxHashSet<GlobalPos>,
        search_area: WorldAabb,
    ) -> Option<TransportItemTarget> {
        let pos = block_entity.get_block_pos();
        if !search_area.contains_xyz(f64::from(pos.x()), f64::from(pos.y()), f64::from(pos.z())) {
            return None;
        }

        let target = TransportItemTarget::try_create(ctx.world(), block_entity)?;
        if !self.is_wanted_block(ctx.mob(), target.state)
            || Self::is_position_already_visited(ctx.world(), visited, unreachable, &target)
        {
            return None;
        }
        Some(target)
    }

    /// Vanilla parity: `getConnectedTargets`, which folds the far half of a
    /// double chest into the same decision.
    fn connected_positions(world: &Arc<World>, target: &TransportItemTarget) -> Vec<BlockPos> {
        let mut positions = vec![target.pos];
        let chest_type = target
            .state
            .try_get_value(&BlockStateProperties::CHEST_TYPE)
            .unwrap_or(ChestType::Single);
        if chest_type == ChestType::Single {
            return positions;
        }
        let connected = target
            .pos
            .relative(ChestBlock::connected_direction(target.state));
        if TransportItemTarget::try_create_at(world, connected).is_some() {
            positions.push(connected);
        }
        positions
    }

    /// Vanilla parity: `isPositionAlreadyVisited`.
    fn is_position_already_visited(
        world: &Arc<World>,
        visited: &FxHashSet<GlobalPos>,
        unreachable: &FxHashSet<GlobalPos>,
        target: &TransportItemTarget,
    ) -> bool {
        Self::connected_positions(world, target)
            .into_iter()
            .map(|pos| GlobalPos::new(world.key.clone(), pos))
            .any(|pos| visited.contains(&pos) || unreachable.contains(&pos))
    }

    /// Vanilla parity: `isAnotherMobInteractingWithTarget`.
    fn is_another_mob_interacting(
        &self,
        ctx: &BrainContext<'_>,
        target: &TransportItemTarget,
    ) -> bool {
        if (self.should_queue_for_target)(target) {
            return true;
        }
        Self::connected_positions(ctx.world(), target)
            .into_iter()
            .filter(|pos| *pos != target.pos)
            .filter_map(|pos| TransportItemTarget::try_create_at(ctx.world(), pos))
            .any(|connected| (self.should_queue_for_target)(&connected))
    }

    /// Vanilla parity: `hasValidTarget`.
    fn has_valid_target(&mut self, ctx: &BrainContext<'_>) -> bool {
        let Some(target) = self.target.as_ref() else {
            return false;
        };
        let valid_type = self.is_wanted_block(ctx.mob(), target.state)
            && ctx
                .world()
                .get_block_entity(target.pos)
                .is_some_and(|current| Arc::ptr_eq(&current, &target.block_entity));
        if !valid_type || ChestBlock::is_chest_blocked_at(ctx.world().as_ref(), target.pos) {
            return false;
        }
        if self.state != TransportItemState::Travelling {
            return true;
        }
        if self.has_valid_travelling_path(ctx) {
            return true;
        }

        let pos = self.target.as_ref().map(|target| target.pos);
        if let Some(pos) = pos {
            self.mark_visited_block_pos_as_unreachable(ctx, pos);
        }
        false
    }

    /// Vanilla parity: `hasValidTravellingPath`.
    fn has_valid_travelling_path(&self, ctx: &BrainContext<'_>) -> bool {
        let Some(target) = self.target.as_ref() else {
            return false;
        };
        let existing_path = ctx.mob().mob_base().navigation().lock().path().cloned();
        let path = match existing_path {
            Some(path) => Some(path),
            None => ctx.mob().create_path_to(target.pos, 0),
        };

        let reach_from = match path.as_ref().and_then(|path| path.end_node()) {
            Some(end) => {
                let pos = end.as_block_pos();
                let (x, y, z) = pos.get_bottom_center();
                Self::middle_y(ctx.mob(), DVec3::new(x, y, z))
            }
            None => Self::middle_y(ctx.mob(), ctx.mob().position()),
        };

        let can_reach = Self::is_within_target_distance(
            ctx,
            Self::interaction_range(ctx.mob()),
            target,
            reach_from,
        );
        let has_not_yet_created_path = path.is_none() && !can_reach;
        has_not_yet_created_path
            || (can_reach && Self::can_see_any_target_side(ctx, target, reach_from))
    }

    /// Vanilla parity: `setMiddleYPosition`.
    fn middle_y(mob: &dyn PathfinderMob, pos: DVec3) -> DVec3 {
        pos + DVec3::new(0.0, mob.bounding_box().height() / 2.0, 0.0)
    }

    /// Vanilla parity: `getCenterPos`.
    fn center_pos(mob: &dyn PathfinderMob) -> DVec3 {
        Self::middle_y(mob, mob.position())
    }

    /// Vanilla parity: `getInteractionRange`, which grows once the path is done
    /// so a mob that stopped a block short still reaches the chest.
    fn interaction_range(mob: &dyn PathfinderMob) -> f64 {
        let finished = mob
            .mob_base()
            .navigation()
            .lock()
            .path()
            .is_some_and(Path::is_done);
        if finished {
            CLOSE_ENOUGH_TO_START_INTERACTING_WITH_TARGET_PATH_END_DISTANCE
        } else {
            CLOSE_ENOUGH_TO_START_INTERACTING_WITH_TARGET_DISTANCE
        }
    }

    /// Vanilla parity: `isWithinTargetDistance`.
    fn is_within_target_distance(
        ctx: &BrainContext<'_>,
        distance: f64,
        target: &TransportItemTarget,
        from_pos: DVec3,
    ) -> bool {
        let body_box = ctx.mob().bounding_box();
        let moved = WorldAabb::of_size(
            from_pos,
            body_box.width(),
            body_box.height(),
            body_box.depth(),
        );
        let shape = BLOCK_BEHAVIORS
            .get_behavior(target.state.get_block())
            .get_collision_shape(
                target.state,
                ctx.world().as_ref(),
                target.pos,
                BlockCollisionContext::empty(),
            );
        let Some(bounds) = shape.bounds() else {
            return false;
        };
        bounds
            .inflate_xyz(distance, 0.5, distance)
            .at_block(target.pos)
            .intersects(moved)
    }

    /// Vanilla parity: `canSeeAnyTargetSide`.
    fn can_see_any_target_side(
        ctx: &BrainContext<'_>,
        target: &TransportItemTarget,
        eye_position: DVec3,
    ) -> bool {
        let (center_x, center_y, center_z) = target.pos.get_center();
        let center = DVec3::new(center_x, center_y, center_z);
        Direction::ALL.into_iter().any(|direction| {
            let (step_x, step_y, step_z) = direction.offset();
            let hit_target = center
                + DVec3::new(
                    0.5 * f64::from(step_x),
                    0.5 * f64::from(step_y),
                    0.5 * f64::from(step_z),
                );
            let hit = ctx.world().clip(
                eye_position,
                hit_target,
                ClipBlockShape::Collider,
                ClipFluid::None,
            );
            !hit.is_miss() && hit.block_pos == target.pos
        })
    }

    // The state machine.

    /// Vanilla parity: `updateInvalidTarget`.
    fn update_invalid_target(&mut self, ctx: &BrainContext<'_>) -> bool {
        if self.has_valid_target(ctx) {
            return false;
        }

        self.stop_targeting_current_target(ctx);
        let Some(target) = self.find_transport_target(ctx) else {
            self.enter_cooldown_after_no_matching_target_found(ctx);
            return true;
        };

        let pos = target.pos;
        self.target = Some(target);
        self.on_start_travelling(ctx);
        self.set_visited_block_pos(ctx, pos);
        true
    }

    /// Vanilla parity: `onTravelToTarget`.
    fn on_travel_to_target(&mut self, ctx: &BrainContext<'_>) {
        let Some(target) = self.target.as_ref() else {
            return;
        };
        let center = Self::center_pos(ctx.mob());
        if Self::is_within_target_distance(
            ctx,
            CLOSE_ENOUGH_TO_START_QUEUING_DISTANCE,
            target,
            center,
        ) && self.is_another_mob_interacting(ctx, target)
        {
            self.start_queuing(ctx);
        } else if Self::is_within_target_distance(
            ctx,
            Self::interaction_range(ctx.mob()),
            target,
            center,
        ) {
            self.start_on_reached_target_interaction(ctx);
        } else {
            self.walk_towards_target(ctx);
        }
    }

    /// Vanilla parity: `onReachedTarget`.
    fn on_reached_target(&mut self, ctx: &BrainContext<'_>) {
        let center = Self::center_pos(ctx.mob());
        let within = self.target.as_ref().is_some_and(|target| {
            Self::is_within_target_distance(
                ctx,
                CLOSE_ENOUGH_TO_CONTINUE_INTERACTING_WITH_TARGET,
                target,
                center,
            )
        });
        if !within {
            self.on_start_travelling(ctx);
            return;
        }

        self.ticks_since_reaching_target += 1;
        self.on_target_interaction(ctx);
        if self.ticks_since_reaching_target < TARGET_INTERACTION_TIME {
            return;
        }

        self.do_reached_target_transfer(ctx);
        self.on_start_travelling(ctx);
    }

    /// Vanilla parity: `startQueuing`.
    fn start_queuing(&mut self, ctx: &BrainContext<'_>) {
        Self::stop_in_place(ctx.mob());
        self.state = TransportItemState::Queuing;
    }

    /// Vanilla parity: `resumeTravelling`.
    fn resume_travelling(&mut self, ctx: &BrainContext<'_>) {
        self.state = TransportItemState::Travelling;
        self.walk_towards_target(ctx);
    }

    /// Vanilla parity: `walkTowardsTarget`.
    fn walk_towards_target(&self, ctx: &BrainContext<'_>) {
        let Some(target) = self.target.as_ref() else {
            return;
        };
        set_walk_and_look_target_memories(
            ctx.brain(),
            PositionTracker::of_block(target.pos),
            self.speed_modifier,
            0,
        );
    }

    /// Vanilla parity: `startOnReachedTargetInteraction`.
    fn start_on_reached_target_interaction(&mut self, ctx: &BrainContext<'_>) {
        self.interaction_state = Some(self.classify_interaction(ctx));
        self.state = TransportItemState::Interacting;
    }

    /// Vanilla parity: the `doReachedTargetInteraction` dispatch, which picks
    /// one of four outcomes from what the mob holds and what the chest holds.
    fn classify_interaction(&self, ctx: &BrainContext<'_>) -> ContainerInteractionState {
        let Some(target) = self.target.as_ref() else {
            return ContainerInteractionState::PickupNoItem;
        };
        let guard = ContainerLockGuard::lock_all(&target.containers);
        if Self::is_picking_up_items(ctx.mob()) {
            if target.is_empty(&guard) {
                ContainerInteractionState::PickupNoItem
            } else {
                ContainerInteractionState::PickupItem
            }
        } else if target.is_empty(&guard)
            || Self::has_item_matching_hand_item(ctx.mob(), target, &guard)
        {
            ContainerInteractionState::PlaceItem
        } else {
            ContainerInteractionState::PlaceNoItem
        }
    }

    /// Vanilla parity: `onStartTravelling`.
    fn on_start_travelling(&mut self, ctx: &BrainContext<'_>) {
        (self.on_start_travelling)(ctx.mob());
        self.state = TransportItemState::Travelling;
        self.interaction_state = None;
        self.ticks_since_reaching_target = 0;
    }

    /// Vanilla parity: `onTargetInteraction`.
    fn on_target_interaction(&self, ctx: &BrainContext<'_>) {
        let Some(target) = self.target.as_ref() else {
            return;
        };
        ctx.brain().set_memory(
            memory_module_types::LOOK_TARGET,
            PositionTracker::of_block(target.pos),
        );
        Self::stop_in_place(ctx.mob());
        let Some(interaction_state) = self.interaction_state else {
            return;
        };
        let Some(action) = self.on_target_interaction_actions.get(&interaction_state) else {
            return;
        };
        action(ctx.mob(), target, self.ticks_since_reaching_target);
    }

    /// Vanilla parity: the `pickUpItems`/`putDownItem` half of
    /// `doReachedTargetInteraction`, run once the interaction time is up.
    fn do_reached_target_transfer(&mut self, ctx: &BrainContext<'_>) {
        let Some(target) = self.target.as_ref() else {
            return;
        };

        if Self::is_picking_up_items(ctx.mob()) {
            let picked_up = {
                let mut guard = ContainerLockGuard::lock_all(&target.containers);
                if target.is_empty(&guard) {
                    None
                } else {
                    Some(Self::pickup_item_from_container(
                        &target.containers,
                        &mut guard,
                    ))
                }
            };
            match picked_up {
                Some(stack) => {
                    Self::set_main_hand_item(ctx.mob(), stack);
                    ctx.mob().set_guaranteed_drop(EquipmentSlot::MainHand);
                    self.clear_memories_after_matching_target_found(ctx);
                }
                None => self.stop_targeting_current_target(ctx),
            }
            return;
        }

        let can_place = {
            let guard = ContainerLockGuard::lock_all(&target.containers);
            target.is_empty(&guard) || Self::has_item_matching_hand_item(ctx.mob(), target, &guard)
        };
        if !can_place {
            self.stop_targeting_current_target(ctx);
            return;
        }

        let leftover = {
            let mut guard = ContainerLockGuard::lock_all(&target.containers);
            Self::add_items_to_container(
                &target.containers,
                &mut guard,
                Self::main_hand_item(ctx.mob()),
            )
        };
        let leftover_is_empty = leftover.is_empty();
        Self::set_main_hand_item(ctx.mob(), leftover);
        if leftover_is_empty {
            self.clear_memories_after_matching_target_found(ctx);
        } else {
            self.stop_targeting_current_target(ctx);
        }
    }

    /// Vanilla parity: `hasItemMatchingHandItem`, which compares items only and
    /// ignores components.
    fn has_item_matching_hand_item(
        mob: &dyn PathfinderMob,
        target: &TransportItemTarget,
        guard: &ContainerLockGuard,
    ) -> bool {
        let held = Self::main_hand_item(mob);
        target.containers.iter().any(|reference| {
            guard
                .get(reference.container_id())
                .is_some_and(|container| {
                    container
                        .iter()
                        .any(|item_stack| ItemStack::is_same_item(item_stack, &held))
                })
        })
    }

    /// Vanilla parity: `pickupItemFromContainer`, which takes at most sixteen
    /// from the first non-empty slot.
    fn pickup_item_from_container(
        containers: &AttachedContainers,
        guard: &mut ContainerLockGuard,
    ) -> ItemStack {
        for reference in containers {
            let Some(container) = guard.get(reference.container_id()) else {
                continue;
            };
            let slot = container
                .iter()
                .position(|item_stack| !item_stack.is_empty());
            let Some(slot) = slot else {
                continue;
            };
            let count = guard
                .get(reference.container_id())
                .map_or(0, |container| container.get_item(slot).count())
                .min(TRANSPORTED_ITEM_MAX_STACK_SIZE);
            if let Some(stack) = guard.remove_item(reference.container_id(), slot, count) {
                guard.set_changed(reference.container_id());
                return stack;
            }
        }
        ItemStack::empty()
    }

    /// Vanilla parity: `addItemsToContainer`.
    fn add_items_to_container(
        containers: &AttachedContainers,
        guard: &mut ContainerLockGuard,
        mut stack: ItemStack,
    ) -> ItemStack {
        for reference in containers {
            let slot_count = guard
                .get(reference.container_id())
                .map_or(0, Container::get_container_size);
            for slot in 0..slot_count {
                let Some(container) = guard.get(reference.container_id()) else {
                    break;
                };
                let existing = container.get_item(slot).clone();
                if existing.is_empty() {
                    guard.set_item(reference.container_id(), slot, stack);
                    guard.set_changed(reference.container_id());
                    return ItemStack::empty();
                }

                if !ItemStack::is_same_item_same_components(&existing, &stack)
                    || existing.count() >= existing.max_stack_size()
                {
                    continue;
                }

                let count_that_can_be_added = existing.max_stack_size() - existing.count();
                let count_to_add = count_that_can_be_added.min(stack.count());
                let mut merged = existing;
                merged.set_count(merged.count() + count_to_add);
                // Vanilla subtracts the whole free capacity rather than what it
                // actually moved, which is equivalent: when the stack fitted,
                // the result is at or below zero and reads as empty.
                stack.set_count((stack.count() - count_that_can_be_added).max(0));
                guard.set_item(reference.container_id(), slot, merged);
                guard.set_changed(reference.container_id());
                if stack.is_empty() {
                    return ItemStack::empty();
                }
            }
        }
        stack
    }

    // Memories and cooldown.

    /// Vanilla parity: `setVisitedBlockPos`.
    fn set_visited_block_pos(&mut self, ctx: &BrainContext<'_>, target: BlockPos) {
        let mut visited = visited_positions(ctx);
        visited.insert(GlobalPos::new(ctx.world().key.clone(), target));
        if visited.len() > MAX_VISITED_POSITIONS {
            self.enter_cooldown_after_no_matching_target_found(ctx);
        } else {
            ctx.brain().set_memory_with_expiry(
                memory_module_types::VISITED_BLOCK_POSITIONS,
                visited,
                VISITED_POSITIONS_MEMORY_TIME,
            );
        }
    }

    /// Vanilla parity: `markVisitedBlockPosAsUnreachable`.
    fn mark_visited_block_pos_as_unreachable(&mut self, ctx: &BrainContext<'_>, target: BlockPos) {
        let global = GlobalPos::new(ctx.world().key.clone(), target);
        let mut visited = visited_positions(ctx);
        visited.remove(&global);
        let mut unreachable = unreachable_positions(ctx);
        unreachable.insert(global);
        if unreachable.len() > MAX_UNREACHABLE_POSITIONS {
            self.enter_cooldown_after_no_matching_target_found(ctx);
            return;
        }
        ctx.brain().set_memory_with_expiry(
            memory_module_types::VISITED_BLOCK_POSITIONS,
            visited,
            VISITED_POSITIONS_MEMORY_TIME,
        );
        ctx.brain().set_memory_with_expiry(
            memory_module_types::UNREACHABLE_TRANSPORT_BLOCK_POSITIONS,
            unreachable,
            VISITED_POSITIONS_MEMORY_TIME,
        );
    }

    /// Vanilla parity: `stopTargetingCurrentTarget`.
    fn stop_targeting_current_target(&mut self, ctx: &BrainContext<'_>) {
        self.ticks_since_reaching_target = 0;
        self.target = None;
        ctx.mob().mob_base().navigation().lock().stop();
        ctx.brain()
            .erase_memory(memory_module_types::WALK_TARGET.id());
    }

    /// Vanilla parity: `clearMemoriesAfterMatchingTargetFound`.
    fn clear_memories_after_matching_target_found(&mut self, ctx: &BrainContext<'_>) {
        self.stop_targeting_current_target(ctx);
        ctx.brain()
            .erase_memory(memory_module_types::VISITED_BLOCK_POSITIONS.id());
        ctx.brain()
            .erase_memory(memory_module_types::UNREACHABLE_TRANSPORT_BLOCK_POSITIONS.id());
    }

    /// Vanilla parity: `enterCooldownAfterNoMatchingTargetFound`.
    fn enter_cooldown_after_no_matching_target_found(&mut self, ctx: &BrainContext<'_>) {
        self.stop_targeting_current_target(ctx);
        ctx.brain().set_memory(
            memory_module_types::TRANSPORT_ITEMS_COOLDOWN_TICKS,
            IDLE_COOLDOWN,
        );
        ctx.brain()
            .erase_memory(memory_module_types::VISITED_BLOCK_POSITIONS.id());
        ctx.brain()
            .erase_memory(memory_module_types::UNREACHABLE_TRANSPORT_BLOCK_POSITIONS.id());
    }

    /// Vanilla parity: `stopInPlace`.
    fn stop_in_place(mob: &dyn PathfinderMob) {
        mob.mob_base().navigation().lock().stop();
        mob.set_travel_input(LivingTravelInput::ZERO);
        mob.set_mob_speed(0.0);
        let velocity = mob.velocity();
        mob.set_velocity(DVec3::new(0.0, velocity.y, 0.0));
    }
}

fn visited_positions(ctx: &BrainContext<'_>) -> FxHashSet<GlobalPos> {
    ctx.brain()
        .get_memory(memory_module_types::VISITED_BLOCK_POSITIONS)
        .unwrap_or_default()
}

fn unreachable_positions(ctx: &BrainContext<'_>) -> FxHashSet<GlobalPos> {
    ctx.brain()
        .get_memory(memory_module_types::UNREACHABLE_TRANSPORT_BLOCK_POSITIONS)
        .unwrap_or_default()
}

impl TimedBehavior for TransportItemsBetweenContainers {
    fn entry_condition(&self) -> &[(MemoryModuleId, MemoryStatus)] {
        &self.entry_condition
    }

    fn times_out(&self) -> bool {
        false
    }

    fn check_extra_start_conditions(&mut self, ctx: &BrainContext<'_>) -> bool {
        !ctx.mob().is_leashed()
    }

    fn can_still_use(&mut self, ctx: &BrainContext<'_>) -> bool {
        !ctx.brain()
            .has_memory_value(memory_module_types::TRANSPORT_ITEMS_COOLDOWN_TICKS.id())
            && !ctx
                .brain()
                .has_memory_value(memory_module_types::IS_PANICKING.id())
            && !ctx.mob().is_leashed()
    }

    fn start(&mut self, ctx: &BrainContext<'_>) {
        // Vanilla parity: a golem is allowed to path down to a chest in a hole
        // only while it is actually transporting.
        ctx.mob()
            .mob_base()
            .navigation()
            .lock()
            .set_can_path_to_targets_below_surface(true);
    }

    fn tick(&mut self, ctx: &BrainContext<'_>) {
        let updated_invalid_target = self.update_invalid_target(ctx);
        if self.target.is_none() {
            self.stop(ctx);
            return;
        }
        if updated_invalid_target {
            return;
        }

        if self.state == TransportItemState::Queuing {
            let queued = self
                .target
                .as_ref()
                .is_some_and(|target| self.is_another_mob_interacting(ctx, target));
            if !queued {
                self.resume_travelling(ctx);
            }
        }

        if self.state == TransportItemState::Travelling {
            self.on_travel_to_target(ctx);
        }

        if self.state == TransportItemState::Interacting {
            self.on_reached_target(ctx);
        }
    }

    fn stop(&mut self, ctx: &BrainContext<'_>) {
        self.on_start_travelling(ctx);
        ctx.mob()
            .mob_base()
            .navigation()
            .lock()
            .set_can_path_to_targets_below_surface(false);
    }

    fn debug_name(&self) -> &'static str {
        "TransportItemsBetweenContainers"
    }
}
