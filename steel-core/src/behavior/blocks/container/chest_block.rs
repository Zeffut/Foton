//! Chest block behavior implementation.
//!
//! Two adjacent chests that face the same direction pair into a double chest
//! exposing a single 54-slot menu. Each half keeps its own block entity and its
//! own independently lockable container.

use std::sync::{Arc, Weak};

use glam::DVec3;

use smallvec::smallvec;
use steel_macros::block_behavior;
use steel_protocol::packets::game::SoundSource;
use steel_registry::block_entity_type::BlockEntityTypeRef;
use steel_registry::blocks::BlockRef;
use steel_registry::blocks::block_state_ext::BlockStateExt;
use steel_registry::blocks::properties::{
    BlockStateProperties, BoolProperty, ChestType, Direction, EnumProperty,
};
use steel_registry::fluid::FluidStateExt;
use steel_registry::sound_event::SoundEventRef;
use steel_registry::sound_events;
use steel_registry::vanilla_block_entity_types;
use steel_registry::vanilla_block_tags::BlockTag;
use steel_utils::{BlockPos, BlockStateId, translations};
use text_components::TextComponent;

use crate::behavior::InventoryAccess;
use crate::behavior::block::{
    BlockBehavior, BlockEntityCreation, schedule_water_tick_if_waterlogged,
};
use crate::behavior::context::{BlockHitResult, BlockPlaceContext, InteractionResult};
use crate::block_entity::BLOCK_ENTITIES;
use crate::fluid::get_fluid_state;
use crate::inventory::container::{Container, calculate_redstone_signal_from_containers};
use crate::inventory::lock::{AttachedContainers, ContainerLockGuard, ContainerRef};
use crate::inventory::menu::kinds::{chest, double_chest};
use crate::player::Player;
use crate::world::{LevelReader, ScheduledTickAccess, World};

/// Behavior for chest blocks.
///
/// Vanilla parity: `ChestBlock`.
#[block_behavior]
pub struct ChestBlock {
    block: BlockRef,
    /// Which chests this one pairs with.
    pairing: ChestPairing,
    /// Which block entity to create.
    ///
    /// Vanilla parity: `ChestBlock.blockEntityType`, whose only reason to be
    /// overridable is the trapped chest -- which is a chest in every other
    /// respect, pairing and blocking included.
    block_entity_type: BlockEntityTypeRef,
}

pub(super) const FACING: &EnumProperty<Direction> = &BlockStateProperties::HORIZONTAL_FACING;
pub(super) const TYPE: &EnumProperty<ChestType> = &BlockStateProperties::CHEST_TYPE;
const WATERLOGGED: &BoolProperty = &BlockStateProperties::WATERLOGGED;

/// Which blocks a chest is willing to pair with.
///
/// Vanilla parity: `ChestBlock.chestCanConnectTo`, which the copper chests
/// override so a chest pairs with any other copper chest rather than only with
/// its own block. That override is what lets an exposed chest pair with an
/// oxidized one and drag the pair down to the lower oxidation stage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ChestPairing {
    /// The base rule: only the very same block.
    SameBlock,
    /// The copper rule: anything in the copper chest tag that has a chest type.
    CopperChests,
}

/// Rows of nine slots in a single chest.
const CHEST_ROWS: usize = 3;

/// How loud the lid is.
///
/// Vanilla parity: the `0.5F` of `ChestBlockEntity.playSound`.
const LID_VOLUME: f32 = 0.5;

/// Vanilla parity: the `random.nextFloat() * 0.1F + 0.9F` every lid uses.
fn lid_pitch() -> f32 {
    rand::random::<f32>().mul_add(0.1, 0.9)
}

impl ChestBlock {
    /// Creates a new chest block behavior for the given block.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self {
            block,
            pairing: ChestPairing::SameBlock,
            block_entity_type: &vanilla_block_entity_types::CHEST,
        }
    }

    /// Creates the chest behavior a copper chest is built on.
    ///
    /// Vanilla parity: `CopperChestBlock`, which is a `ChestBlock` on the plain
    /// chest block entity that pairs across the whole copper chest tag.
    #[must_use]
    pub const fn copper(block: BlockRef) -> Self {
        Self {
            block,
            pairing: ChestPairing::CopperChests,
            block_entity_type: &vanilla_block_entity_types::CHEST,
        }
    }

    /// Plays the lid at the place vanilla plays it.
    ///
    /// Vanilla parity: `ChestBlockEntity.playSound`. The left half of a double
    /// chest stays silent and the right half plays for both, shifted half a
    /// block toward its partner, so one chest makes one sound from the middle
    /// of the pair rather than two from its ends.
    pub(super) fn play_lid(
        world: &Arc<World>,
        pos: BlockPos,
        state: BlockStateId,
        sound: SoundEventRef,
    ) {
        let chest_type = state.get_value(TYPE);
        if chest_type == ChestType::Left {
            return;
        }

        let mut position = DVec3::new(
            f64::from(pos.x()) + 0.5,
            f64::from(pos.y()) + 0.5,
            f64::from(pos.z()) + 0.5,
        );
        if chest_type == ChestType::Right {
            let (step_x, step_z) = Self::connected_direction(state).offset_xz();
            position.x += f64::from(step_x) * 0.5;
            position.z += f64::from(step_z) * 0.5;
        }

        world.play_sound_at(
            sound,
            SoundSource::Blocks,
            position,
            LID_VOLUME,
            lid_pitch(),
            None,
        );
    }

    /// Creates the chest behavior a trapped chest is built on.
    ///
    /// Vanilla parity: `TrappedChestBlock`, which is a `ChestBlock` that
    /// answers `TRAPPED_CHEST` to `blockEntityType`.
    #[must_use]
    pub const fn trapped(block: BlockRef) -> Self {
        Self {
            block,
            pairing: ChestPairing::SameBlock,
            block_entity_type: &vanilla_block_entity_types::TRAPPED_CHEST,
        }
    }

    /// Returns whether the given state is a chest this one can pair with.
    ///
    /// Vanilla parity: `ChestBlock.chestCanConnectTo`.
    pub(super) fn chest_can_connect_to(&self, state: BlockStateId) -> bool {
        match self.pairing {
            ChestPairing::SameBlock => state.get_block() == self.block,
            ChestPairing::CopperChests => {
                state.get_block().has_tag(&BlockTag::COPPER_CHESTS)
                    && state.try_get_value(TYPE).is_some()
            }
        }
    }

    /// Returns the direction of the paired half, for a non-single chest.
    ///
    /// Vanilla parity: `ChestBlock.getConnectedDirection`.
    pub(super) fn connected_direction(state: BlockStateId) -> Direction {
        let facing = state.get_value(FACING);
        if state.get_value(TYPE) == ChestType::Left {
            facing.rotate_y_clockwise()
        } else {
            facing.rotate_y_counter_clockwise()
        }
    }

    /// Returns `Left` for `Right` and vice versa; `Single` maps to itself.
    ///
    /// Vanilla parity: `ChestType.getOpposite`.
    const fn opposite_type(chest_type: &ChestType) -> ChestType {
        match chest_type {
            ChestType::Single => ChestType::Single,
            ChestType::Left => ChestType::Right,
            ChestType::Right => ChestType::Left,
        }
    }

    /// Returns the facing of a neighbour that is available for pairing.
    ///
    /// Vanilla parity: `ChestBlock.candidatePartnerFacing`.
    fn candidate_partner_facing(
        &self,
        world: &dyn LevelReader,
        pos: BlockPos,
        neighbour_direction: Direction,
    ) -> Option<Direction> {
        let state = world.get_block_state(pos.relative(neighbour_direction));
        if self.chest_can_connect_to(state) && state.get_value(TYPE) == ChestType::Single {
            Some(state.get_value(FACING))
        } else {
            None
        }
    }

    /// Resolves the chest type from the neighbors at placement time.
    ///
    /// Vanilla parity: `ChestBlock.getChestType`.
    fn get_chest_type(
        &self,
        world: &dyn LevelReader,
        pos: BlockPos,
        facing: Direction,
    ) -> ChestType {
        if self.candidate_partner_facing(world, pos, facing.rotate_y_clockwise()) == Some(facing) {
            return ChestType::Left;
        }
        if self.candidate_partner_facing(world, pos, facing.rotate_y_counter_clockwise())
            == Some(facing)
        {
            return ChestType::Right;
        }
        ChestType::Single
    }

    /// Returns whether the chest at `pos` cannot be opened.
    ///
    /// Vanilla parity: `ChestBlock.isChestBlockedAt`. The sitting-cat check is
    /// not implemented because Steel has no `Cat` entity yet.
    fn is_chest_blocked_at(world: &dyn LevelReader, pos: BlockPos) -> bool {
        // TODO: also block when a cat is sitting on the chest, matching
        // `ChestBlock.isCatSittingOnChest`, once the Cat entity exists.
        let above = pos.above();
        world.get_block_state(above).is_static_redstone_conductor()
    }

    /// Returns the container of this chest, and of its pair when it has one.
    ///
    /// The first element is always the half whose type is `Right`, matching
    /// vanilla's `DoubleBlockCombiner.BlockType.FIRST` ordering, so the slot
    /// order of the double menu is identical to vanilla.
    ///
    /// Vanilla parity: `ChestBlock.getContainer`. `override_blocked` is its
    /// `overrideBlockedChest` flag, which hoppers set so a chest with a solid
    /// block on top still accepts and gives up items even though no player can
    /// open it.
    fn combine(
        &self,
        state: BlockStateId,
        world: &dyn LevelReader,
        pos: BlockPos,
        override_blocked: bool,
    ) -> Option<(ContainerRef, Option<ContainerRef>)> {
        if !override_blocked && Self::is_chest_blocked_at(world, pos) {
            return None;
        }
        let own = world
            .get_block_entity(pos)
            .and_then(ContainerRef::from_block_entity)?;

        let chest_type = state.get_value(TYPE);
        if chest_type == ChestType::Single {
            return Some((own, None));
        }

        let neighbour_pos = pos.relative(Self::connected_direction(state));
        let neighbour_state = world.get_block_state(neighbour_pos);
        if !self.chest_can_connect_to(neighbour_state)
            || neighbour_state.get_value(TYPE) != Self::opposite_type(&chest_type)
            || neighbour_state.get_value(FACING) != state.get_value(FACING)
            || (!override_blocked && Self::is_chest_blocked_at(world, neighbour_pos))
        {
            return Some((own, None));
        }

        let Some(neighbour) = world
            .get_block_entity(neighbour_pos)
            .and_then(ContainerRef::from_block_entity)
        else {
            return Some((own, None));
        };

        if chest_type == ChestType::Right {
            Some((own, Some(neighbour)))
        } else {
            Some((neighbour, Some(own)))
        }
    }
}

impl BlockBehavior for ChestBlock {
    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        let mut chest_type = ChestType::Single;
        let mut facing = context.horizontal_direction().opposite();
        let secondary_use = context.is_secondary_use_active();
        let clicked_face = context.clicked_face();

        if clicked_face.is_horizontal() && secondary_use {
            let neighbour_facing = self.candidate_partner_facing(
                context.world.as_ref(),
                context.place_pos(),
                clicked_face.opposite(),
            );
            if let Some(neighbour_facing) = neighbour_facing
                && neighbour_facing.axis() != clicked_face.axis()
            {
                facing = neighbour_facing;
                chest_type = if facing.rotate_y_counter_clockwise() == clicked_face.opposite() {
                    ChestType::Right
                } else {
                    ChestType::Left
                };
            }
        }

        if chest_type == ChestType::Single && !secondary_use {
            chest_type = self.get_chest_type(context.world.as_ref(), context.place_pos(), facing);
        }

        let replaced_fluid_state = get_fluid_state(context.world, context.place_pos());
        Some(
            self.block
                .default_state()
                .set_value(FACING, facing)
                .set_value(TYPE, chest_type)
                .set_value(WATERLOGGED, replaced_fluid_state.is_water()),
        )
    }

    fn update_shape(
        &self,
        state: BlockStateId,
        world: &dyn ScheduledTickAccess,
        pos: BlockPos,
        direction: Direction,
        _neighbor_pos: BlockPos,
        neighbor_state: BlockStateId,
    ) -> BlockStateId {
        schedule_water_tick_if_waterlogged(state, world, pos);

        if self.chest_can_connect_to(neighbor_state) && direction.is_horizontal() {
            let neighbour_type = neighbor_state.get_value(TYPE);
            if state.get_value(TYPE) == ChestType::Single
                && neighbour_type != ChestType::Single
                && state.get_value(FACING) == neighbor_state.get_value(FACING)
                && Self::connected_direction(neighbor_state) == direction.opposite()
            {
                return state.set_value(TYPE, Self::opposite_type(&neighbour_type));
            }
        } else if Self::connected_direction(state) == direction {
            return state.set_value(TYPE, ChestType::Single);
        }

        state
    }

    fn use_without_item(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        player: &Player,
        _hit_result: &BlockHitResult,
        _inv: &mut InventoryAccess,
    ) -> InteractionResult {
        let Some((first, second)) = self.combine(state, world.as_ref(), pos, false) else {
            // Blocked by the block above: vanilla silently refuses to open.
            return InteractionResult::Consume;
        };

        let inventory = player.inventory.clone();
        match second {
            Some(second) => player.open_menu(
                TextComponent::translated(translations::CONTAINER_CHEST_DOUBLE.msg()),
                move |context| double_chest(inventory, context.container_id, first, second),
            ),
            None => player.open_menu(
                TextComponent::translated(translations::CONTAINER_CHEST.msg()),
                move |context| chest(inventory, context.container_id, first, CHEST_ROWS),
            ),
        }

        // The open count, and with it the lid animation and the open/close
        // sounds, is driven by the menu through `BlockEntityBase`.
        // TODO: Award stat OPEN_CHEST and anger nearby piglins; Steel has
        // neither a statistics registry nor piglins.

        InteractionResult::Success
    }

    fn new_block_entity(
        &self,
        level: Weak<World>,
        pos: BlockPos,
        state: BlockStateId,
    ) -> BlockEntityCreation {
        BlockEntityCreation::from_registered_factory(BLOCK_ENTITIES.create(
            self.block_entity_type,
            level,
            pos,
            state,
        ))
    }

    /// Vanilla parity: the `ChestBlock` special case of
    /// `HopperBlockEntity.getBlockContainer`, which combines both halves and
    /// ignores whatever is sitting on the lid.
    fn get_attached_containers(
        &self,
        state: BlockStateId,
        world: &dyn LevelReader,
        pos: BlockPos,
    ) -> AttachedContainers {
        let Some((first, second)) = self.combine(state, world, pos, true) else {
            return AttachedContainers::new();
        };
        if let Some(second) = second {
            smallvec![first, second]
        } else {
            smallvec![first]
        }
    }

    /// Vanilla parity: the `onOpen` of `ChestBlockEntity.openersCounter`.
    fn on_container_open(&self, world: &Arc<World>, pos: BlockPos, state: BlockStateId) {
        Self::play_lid(world, pos, state, &sound_events::BLOCK_CHEST_OPEN);
    }

    /// Vanilla parity: the `onClose` of `ChestBlockEntity.openersCounter`.
    fn on_container_close(&self, world: &Arc<World>, pos: BlockPos, state: BlockStateId) {
        Self::play_lid(world, pos, state, &sound_events::BLOCK_CHEST_CLOSE);
    }

    fn has_analog_output_signal(&self, _state: BlockStateId) -> bool {
        true
    }

    fn get_analog_output_signal(
        &self,
        state: BlockStateId,
        world: &dyn LevelReader,
        pos: BlockPos,
        _direction: Direction,
    ) -> i32 {
        // Vanilla reads the combined container, so a double chest measures all 54
        // slots as one, and a blocked chest reports nothing.
        let Some((first, second)) = self.combine(state, world, pos, false) else {
            return 0;
        };

        let refs: Vec<&ContainerRef> = match &second {
            Some(second) => vec![&first, second],
            None => vec![&first],
        };
        let guard = ContainerLockGuard::lock_all(&refs);

        let mut containers: Vec<&dyn Container> = Vec::with_capacity(refs.len());
        for container_ref in &refs {
            let Some(container) = guard.get(container_ref.container_id()) else {
                return 0;
            };
            containers.push(container);
        }
        calculate_redstone_signal_from_containers(&containers)
    }
}
