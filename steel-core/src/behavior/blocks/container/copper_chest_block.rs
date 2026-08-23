//! Copper chest behaviors.
//!
//! Vanilla parity: `CopperChestBlock` and `WeatheringCopperChestBlock`. A copper
//! chest is a chest in every respect the wooden one is; what it adds is a wider
//! pairing rule -- any block in the copper chest tag, not just its own -- and
//! the rule that when two different stages pair up, the pair settles on the
//! least oxidized of the two.

use std::sync::{Arc, Weak};

use steel_macros::block_behavior;
use steel_registry::blocks::BlockRef;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::blocks::properties::{ChestType, Direction};
use steel_registry::sound_event::SoundEventRef;
use steel_utils::{BlockPos, BlockStateId};

use super::chest_block::{ChestBlock, TYPE};
use crate::behavior::block::{BlockBehavior, BlockEntityCreation};
use crate::behavior::blocks::{WeatherState, WeatheringCopper};
use crate::behavior::context::{BlockHitResult, BlockPlaceContext, InteractionResult};
use crate::behavior::waxables::get_normal_from_waxed_variant;
use crate::behavior::{BLOCK_BEHAVIORS, InventoryAccess};
use crate::inventory::lock::AttachedContainers;
use crate::player::Player;
use crate::world::{LevelReader, ScheduledTickAccess, World};

/// The vanilla `CopperChestBlock` capability.
///
/// Vanilla parity: the `instanceof CopperChestBlock` checks in
/// `CopperChestBlock.getLeastOxidizedChestOfConnectedBlocks`, which have to ask
/// the chest next to it for its oxidation stage and whether it is waxed.
pub trait CopperChest {
    /// Vanilla parity: `CopperChestBlock.getState`.
    fn weather_state(&self) -> WeatherState;

    /// Vanilla parity: `CopperChestBlock.isWaxed`.
    fn is_waxed(&self) -> bool;
}

/// Returns the copper chest capability of whatever block a state belongs to.
fn copper_chest_of(state: BlockStateId) -> Option<&'static dyn CopperChest> {
    BLOCK_BEHAVIORS
        .get_behavior(state.get_block())
        .as_copper_chest()
}

/// Behavior for the waxed copper chests.
///
/// Vanilla parity: `CopperChestBlock`, whose unwaxed subclass adds oxidation.
#[block_behavior]
pub struct CopperChestBlock {
    /// The chest this is, in every respect but pairing and oxidation.
    chest: ChestBlock,
    #[json_arg(r#enum = "WeatherState", json = "weather_state")]
    weather_state: WeatherState,
    /// Vanilla parity: `ChestBlock.getOpenChestSound`, which the weathered and
    /// oxidized copper chests answer differently from the rest.
    #[json_arg(sound_events, json = "open_sound")]
    open_sound: SoundEventRef,
    /// Vanilla parity: `ChestBlock.getCloseChestSound`.
    #[json_arg(sound_events, json = "close_sound")]
    close_sound: SoundEventRef,
}

impl CopperChestBlock {
    /// Creates a waxed copper chest behavior.
    #[must_use]
    pub const fn new(
        block: BlockRef,
        weather_state: WeatherState,
        open_sound: SoundEventRef,
        close_sound: SoundEventRef,
    ) -> Self {
        Self {
            chest: ChestBlock::copper(block),
            weather_state,
            open_sound,
            close_sound,
        }
    }

    /// Vanilla parity: `CopperChestBlock.unwaxBlock`.
    ///
    /// An unwaxed chest is already its own answer; a waxed one answers with the
    /// unwaxed block wearing the same properties.
    fn unwaxed_state(state: BlockStateId, waxed: bool) -> Option<BlockStateId> {
        if !waxed {
            return Some(state);
        }

        let unwaxed = get_normal_from_waxed_variant(state.get_block())?;
        Some(unwaxed.default_state().with_properties_of(state))
    }

    /// Vanilla parity: `CopperChestBlock.getLeastOxidizedChestOfConnectedBlocks`.
    ///
    /// When two copper chests pair, both halves become the less oxidized of the
    /// two, and a mixed waxed/unwaxed pair unwaxes first so the comparison is
    /// between oxidation stages rather than between wax states.
    fn least_oxidized_of_pair(
        state: BlockStateId,
        world: &dyn LevelReader,
        pos: BlockPos,
    ) -> BlockStateId {
        if state.get_value(TYPE) == ChestType::Single {
            return state;
        }

        let connected_state =
            world.get_block_state(pos.relative(ChestBlock::connected_direction(state)));
        let (Some(own), Some(connected)) =
            (copper_chest_of(state), copper_chest_of(connected_state))
        else {
            return state;
        };

        let mut updated = state;
        let mut connected_predicted = connected_state;
        if own.is_waxed() != connected.is_waxed() {
            updated = Self::unwaxed_state(state, own.is_waxed()).unwrap_or(updated);
            connected_predicted = Self::unwaxed_state(connected_state, connected.is_waxed())
                .unwrap_or(connected_predicted);
        }

        let least_oxidized = if own.weather_state() <= connected.weather_state() {
            updated.get_block()
        } else {
            connected_predicted.get_block()
        };

        least_oxidized.default_state().with_properties_of(updated)
    }
}

impl CopperChest for CopperChestBlock {
    fn weather_state(&self) -> WeatherState {
        self.weather_state
    }

    fn is_waxed(&self) -> bool {
        true
    }
}

impl BlockBehavior for CopperChestBlock {
    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        let state = self.chest.get_state_for_placement(context)?;
        Some(Self::least_oxidized_of_pair(
            state,
            context.world.as_ref(),
            context.place_pos(),
        ))
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
        let updated =
            self.chest
                .update_shape(state, world, pos, direction, neighbor_pos, neighbor_state);

        if self.chest.chest_can_connect_to(neighbor_state)
            && updated.get_value(TYPE) != ChestType::Single
            && ChestBlock::connected_direction(updated) == direction
        {
            return neighbor_state
                .get_block()
                .default_state()
                .with_properties_of(updated);
        }

        updated
    }

    fn use_without_item(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        player: &Player,
        hit_result: &BlockHitResult,
        inv: &mut InventoryAccess,
    ) -> InteractionResult {
        self.chest
            .use_without_item(state, world, pos, player, hit_result, inv)
    }

    fn new_block_entity(
        &self,
        level: Weak<World>,
        pos: BlockPos,
        state: BlockStateId,
    ) -> BlockEntityCreation {
        self.chest.new_block_entity(level, pos, state)
    }

    fn get_attached_containers(
        &self,
        state: BlockStateId,
        world: &dyn LevelReader,
        pos: BlockPos,
    ) -> AttachedContainers {
        self.chest.get_attached_containers(state, world, pos)
    }

    fn on_container_open(&self, world: &Arc<World>, pos: BlockPos, state: BlockStateId) {
        ChestBlock::play_lid(world, pos, state, self.open_sound);
    }

    fn on_container_close(&self, world: &Arc<World>, pos: BlockPos, state: BlockStateId) {
        ChestBlock::play_lid(world, pos, state, self.close_sound);
    }

    fn has_analog_output_signal(&self, state: BlockStateId) -> bool {
        self.chest.has_analog_output_signal(state)
    }

    fn get_analog_output_signal(
        &self,
        state: BlockStateId,
        world: &dyn LevelReader,
        pos: BlockPos,
        direction: Direction,
    ) -> i32 {
        self.chest
            .get_analog_output_signal(state, world, pos, direction)
    }

    fn as_copper_chest(&self) -> Option<&dyn CopperChest> {
        Some(self)
    }
}

/// Behavior for the unwaxed copper chests.
///
/// Vanilla parity: `WeatheringCopperChestBlock`, a `CopperChestBlock` that
/// oxidizes -- but only while nobody has it open, and only from the half a
/// double chest counts as its first.
#[block_behavior]
pub struct WeatheringCopperChestBlock {
    /// The copper chest this is, in every respect but oxidizing.
    copper_chest: CopperChestBlock,
    #[json_arg(r#enum = "WeatherState", json = "weather_state")]
    weathering: WeatheringCopper,
    /// Held here as well as inside `copper_chest` because the lid sounds are
    /// the one thing this block plays itself.
    #[json_arg(sound_events, json = "open_sound")]
    open_sound: SoundEventRef,
    /// See [`Self::open_sound`].
    #[json_arg(sound_events, json = "close_sound")]
    close_sound: SoundEventRef,
}

impl WeatheringCopperChestBlock {
    /// Creates an unwaxed copper chest behavior.
    #[must_use]
    pub const fn new(
        block: BlockRef,
        weather_state: WeatherState,
        open_sound: SoundEventRef,
        close_sound: SoundEventRef,
    ) -> Self {
        Self {
            copper_chest: CopperChestBlock::new(block, weather_state, open_sound, close_sound),
            weathering: WeatheringCopper::new(weather_state),
            open_sound,
            close_sound,
        }
    }
}

impl CopperChest for WeatheringCopperChestBlock {
    fn weather_state(&self) -> WeatherState {
        self.copper_chest.weather_state()
    }

    fn is_waxed(&self) -> bool {
        false
    }
}

impl BlockBehavior for WeatheringCopperChestBlock {
    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        self.copper_chest.get_state_for_placement(context)
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
        self.copper_chest
            .update_shape(state, world, pos, direction, neighbor_pos, neighbor_state)
    }

    fn use_without_item(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        player: &Player,
        hit_result: &BlockHitResult,
        inv: &mut InventoryAccess,
    ) -> InteractionResult {
        self.copper_chest
            .use_without_item(state, world, pos, player, hit_result, inv)
    }

    fn new_block_entity(
        &self,
        level: Weak<World>,
        pos: BlockPos,
        state: BlockStateId,
    ) -> BlockEntityCreation {
        self.copper_chest.new_block_entity(level, pos, state)
    }

    fn get_attached_containers(
        &self,
        state: BlockStateId,
        world: &dyn LevelReader,
        pos: BlockPos,
    ) -> AttachedContainers {
        self.copper_chest.get_attached_containers(state, world, pos)
    }

    fn on_container_open(&self, world: &Arc<World>, pos: BlockPos, state: BlockStateId) {
        ChestBlock::play_lid(world, pos, state, self.open_sound);
    }

    fn on_container_close(&self, world: &Arc<World>, pos: BlockPos, state: BlockStateId) {
        ChestBlock::play_lid(world, pos, state, self.close_sound);
    }

    fn has_analog_output_signal(&self, state: BlockStateId) -> bool {
        self.copper_chest.has_analog_output_signal(state)
    }

    fn get_analog_output_signal(
        &self,
        state: BlockStateId,
        world: &dyn LevelReader,
        pos: BlockPos,
        direction: Direction,
    ) -> i32 {
        self.copper_chest
            .get_analog_output_signal(state, world, pos, direction)
    }

    /// Vanilla parity: `WeatheringCopperChestBlock.randomTick`. The right half
    /// of a double chest never oxidizes on its own -- the left half carries the
    /// pair -- and an open chest is left alone so its contents are not shuffled
    /// out from under a viewer.
    fn random_tick(&self, state: BlockStateId, world: &Arc<World>, pos: BlockPos) {
        if state.get_value(TYPE) == ChestType::Right {
            return;
        }

        let Some(block_entity) = world.get_block_entity(pos) else {
            return;
        };
        if block_entity.base().opener_count() != 0 {
            return;
        }

        self.weathering.change_over_time(state, world, pos);
    }

    fn as_copper_chest(&self) -> Option<&dyn CopperChest> {
        Some(self)
    }
}

#[cfg(test)]
mod tests {
    use steel_registry::blocks::properties::{BlockStateProperties, EnumProperty};
    use steel_registry::{init_vanilla_registry, vanilla_blocks};

    use super::*;
    use crate::behavior::init_behaviors;
    use crate::test_support::TestLevel;

    const FACING: &EnumProperty<Direction> = &BlockStateProperties::HORIZONTAL_FACING;

    fn half(block: BlockRef, chest_type: ChestType) -> BlockStateId {
        block
            .default_state()
            .set_value(FACING, Direction::North)
            .set_value(TYPE, chest_type)
    }

    #[test]
    fn copper_chests_pair_across_oxidation_stages_and_waxing() {
        init_vanilla_registry();
        init_behaviors();

        let waxed = ChestBlock::copper(&vanilla_blocks::WAXED_COPPER_CHEST);
        assert!(waxed.chest_can_connect_to(half(
            &vanilla_blocks::OXIDIZED_COPPER_CHEST,
            ChestType::Single
        )));
        assert!(!waxed.chest_can_connect_to(half(&vanilla_blocks::CHEST, ChestType::Single)));

        // The wooden chest keeps the base rule.
        let wooden = ChestBlock::new(&vanilla_blocks::CHEST);
        assert!(
            !wooden.chest_can_connect_to(half(&vanilla_blocks::TRAPPED_CHEST, ChestType::Single))
        );
    }

    #[test]
    fn a_mixed_pair_settles_on_the_least_oxidized_unwaxed_chest() {
        init_vanilla_registry();
        init_behaviors();

        let pos = BlockPos::new(0, 64, 0);
        // A north-facing left half pairs with the block to its west.
        let own = half(
            &vanilla_blocks::WAXED_OXIDIZED_COPPER_CHEST,
            ChestType::Left,
        );
        let partner_pos = pos.relative(ChestBlock::connected_direction(own));
        let level = TestLevel::default().with_block(
            partner_pos,
            half(&vanilla_blocks::EXPOSED_COPPER_CHEST, ChestType::Right),
        );

        let settled = CopperChestBlock::least_oxidized_of_pair(own, &level, pos);
        assert_eq!(
            settled.get_block(),
            &vanilla_blocks::EXPOSED_COPPER_CHEST,
            "the waxed oxidized half should unwax and take the exposed stage"
        );
        assert_eq!(settled.get_value(TYPE), ChestType::Left);
        assert_eq!(settled.get_value(FACING), Direction::North);
    }

    #[test]
    fn a_single_copper_chest_keeps_its_own_stage() {
        init_vanilla_registry();
        init_behaviors();

        let pos = BlockPos::new(0, 64, 0);
        let own = half(&vanilla_blocks::OXIDIZED_COPPER_CHEST, ChestType::Single);
        let level = TestLevel::default();

        assert_eq!(
            CopperChestBlock::least_oxidized_of_pair(own, &level, pos),
            own
        );
    }
}
