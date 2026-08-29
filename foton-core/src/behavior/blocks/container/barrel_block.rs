//! Barrel block behavior implementation.
//!
//! Opens a 27-slot container menu when right-clicked.

use std::sync::{Arc, Weak};

use foton_macros::block_behavior;
use foton_protocol::packets::game::SoundSource;
use foton_registry::blocks::BlockRef;
use foton_registry::blocks::block_state_ext::BlockStateExt;
use foton_registry::blocks::properties::BoolProperty;
use foton_registry::blocks::properties::{BlockStateProperties, Direction, EnumProperty};
use foton_registry::sound_event::SoundEventRef;
use foton_registry::{sound_events, vanilla_block_entity_types};
use foton_utils::types::UpdateFlags;
use foton_utils::{BlockPos, BlockStateId, translations};
use glam::DVec3;
use text_components::TextComponent;

use crate::behavior::InventoryAccess;
use crate::behavior::block::{BlockBehavior, BlockEntityCreation};
use crate::behavior::context::{BlockHitResult, BlockPlaceContext, InteractionResult};
use crate::block_entity::BLOCK_ENTITIES;
use crate::inventory::container::calculate_redstone_signal_from_container;
use crate::inventory::lock::{ContainerLockGuard, ContainerRef};
use crate::inventory::menu::kinds::chest;
use crate::player::Player;
use crate::world::{LevelReader, World};

/// Behavior for barrel blocks.
///
/// Barrels are container block entities with 27 slots (3x9 grid).
/// They use the same menu as chests but cannot form double containers.
#[block_behavior]
pub struct BarrelBlock {
    block: BlockRef,
}

const FACING: &EnumProperty<Direction> = &BlockStateProperties::FACING;

/// Whether the barrel is being looked into.
const OPEN: &BoolProperty = &BlockStateProperties::OPEN;

/// How loud the lid is.
///
/// Vanilla parity: the `0.5F` of `BarrelBlockEntity.playSound`.
const LID_VOLUME: f32 = 0.5;

/// Plays the lid half a block out from the face the barrel opens through.
///
/// Vanilla parity: `BarrelBlockEntity.playSound`. The offset is audible: a
/// barrel in a wall should sound from the side you can reach, not from inside
/// the wall.
fn play_lid(world: &Arc<World>, pos: BlockPos, state: BlockStateId, sound: SoundEventRef) {
    let (step_x, step_y, step_z) = state.get_value(FACING).offset();
    let position = DVec3::new(
        f64::from(pos.x()) + 0.5 + f64::from(step_x) / 2.0,
        f64::from(pos.y()) + 0.5 + f64::from(step_y) / 2.0,
        f64::from(pos.z()) + 0.5 + f64::from(step_z) / 2.0,
    );
    world.play_sound_at(
        sound,
        SoundSource::Blocks,
        position,
        LID_VOLUME,
        rand::random::<f32>().mul_add(0.1, 0.9),
        None,
    );
}

impl BarrelBlock {
    /// Creates a new barrel block behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }
}

impl BlockBehavior for BarrelBlock {
    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        // Barrel faces opposite to the player's look direction (all 6 directions).
        let facing = context.get_nearest_looking_direction().opposite();

        Some(self.block.default_state().set_value(FACING, facing))
    }

    fn use_without_item(
        &self,
        _state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        player: &Player,
        _hit_result: &BlockHitResult,
        _inv: &mut InventoryAccess,
    ) -> InteractionResult {
        // Get the block entity
        let Some(block_entity) = world.get_block_entity(pos) else {
            return InteractionResult::Pass;
        };

        // Create a container reference from the block entity
        let Some(container_ref) = ContainerRef::from_block_entity(block_entity.clone()) else {
            return InteractionResult::Pass;
        };

        // Vanilla parity: `RandomizableContainerBlockEntity.createMenu`
        // unpacks with the opening player, whose luck the roll uses.
        container_ref.unpack_loot_table(Some(player));

        // Open the chest menu (3 rows for barrel)
        let inventory = player.inventory.clone();
        player.open_menu(
            block_entity.display_name(TextComponent::translated(
                translations::CONTAINER_BARREL.msg(),
            )),
            move |context| chest(inventory, context.container_id, container_ref, 3),
        );

        // TODO: Award stat OPEN_BARREL, and anger nearby piglins; Foton has no
        // statistics registry and no piglins.
        InteractionResult::Success
    }

    /// Vanilla parity: the `onOpen` of `BarrelBlockEntity.openersCounter`.
    ///
    /// The `open` property is the only thing that makes a barrel look open, so
    /// without it a player has no idea anyone is in theirs.
    fn on_container_open(&self, world: &Arc<World>, pos: BlockPos, state: BlockStateId) {
        play_lid(world, pos, state, &sound_events::BLOCK_BARREL_OPEN);
        world.set_block(pos, state.set_value(OPEN, true), UpdateFlags::UPDATE_ALL);
    }

    /// Vanilla parity: the `onClose` of `BarrelBlockEntity.openersCounter`.
    fn on_container_close(&self, world: &Arc<World>, pos: BlockPos, state: BlockStateId) {
        play_lid(world, pos, state, &sound_events::BLOCK_BARREL_CLOSE);
        world.set_block(pos, state.set_value(OPEN, false), UpdateFlags::UPDATE_ALL);
    }

    fn new_block_entity(
        &self,
        level: Weak<World>,
        pos: BlockPos,
        state: BlockStateId,
    ) -> BlockEntityCreation {
        BlockEntityCreation::from_registered_factory(BLOCK_ENTITIES.create(
            &vanilla_block_entity_types::BARREL,
            level,
            pos,
            state,
        ))
    }

    fn has_analog_output_signal(&self, _state: BlockStateId) -> bool {
        true
    }

    fn get_analog_output_signal(
        &self,
        _state: BlockStateId,
        world: &dyn LevelReader,
        pos: BlockPos,
        _direction: Direction,
    ) -> i32 {
        // Get the block entity and calculate signal from container contents
        let Some(container_ref) = world
            .get_block_entity(pos)
            .and_then(ContainerRef::from_block_entity)
        else {
            return 0;
        };
        let guard = ContainerLockGuard::lock_all(&[&container_ref]);
        guard
            .get(container_ref.container_id())
            .map_or(0, |container| {
                calculate_redstone_signal_from_container(container)
            })
    }
}
