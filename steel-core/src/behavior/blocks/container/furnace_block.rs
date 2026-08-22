//! Furnace, smoker and blast furnace block behavior.
//!
//! Vanilla parity: `AbstractFurnaceBlock` and its three subclasses. They share
//! everything but the block entity type, the menu type and the title, so the
//! shared behavior lives in [`AbstractFurnaceBlock`] and each variant delegates.

use std::sync::{Arc, Weak};

use steel_macros::block_behavior;
use steel_registry::block_entity_type::{BlockEntityType, BlockEntityTypeRef};
use steel_registry::blocks::BlockRef;
use steel_registry::blocks::block_state_ext::BlockStateExt;
use steel_registry::blocks::properties::{BlockStateProperties, Direction, EnumProperty};
use steel_registry::menu_type::MenuTypeRef;
use steel_registry::{vanilla_block_entity_types, vanilla_menu_types};
use steel_utils::{BlockPos, BlockStateId, Downcast as _, translations};
use text_components::TextComponent;
use text_components::translation::TranslatedMessage;

use crate::behavior::InventoryAccess;
use crate::behavior::block::{BlockBehavior, BlockEntityCreation};
use crate::behavior::context::{BlockHitResult, BlockPlaceContext, InteractionResult};
use crate::block_entity::entities::FurnaceBlockEntity;
use crate::block_entity::{BLOCK_ENTITIES, BlockEntityTicker};
use crate::inventory::container::calculate_redstone_signal_from_container;
use crate::inventory::lock::{ContainerLockGuard, ContainerRef};
use crate::inventory::menu::kinds::furnace;
use crate::player::Player;
use crate::world::{LevelReader, World};

const FACING: &EnumProperty<Direction> = &BlockStateProperties::HORIZONTAL_FACING;

/// Behavior shared by every furnace variant.
struct AbstractFurnaceBlock {
    block: BlockRef,
    block_entity_type: &'static BlockEntityType,
    menu_type: MenuTypeRef,
    title: TranslatedMessage,
}

impl AbstractFurnaceBlock {
    const fn new(
        block: BlockRef,
        block_entity_type: &'static BlockEntityType,
        menu_type: MenuTypeRef,
        title: TranslatedMessage,
    ) -> Self {
        Self {
            block,
            block_entity_type,
            menu_type,
            title,
        }
    }

    /// Vanilla parity: `AbstractFurnaceBlock.getStateForPlacement`.
    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        Some(
            self.block
                .default_state()
                .set_value(FACING, context.horizontal_direction().opposite()),
        )
    }

    fn use_without_item(
        &self,
        world: &Arc<World>,
        pos: BlockPos,
        player: &Player,
    ) -> InteractionResult {
        let Some(block_entity) = world.get_block_entity(pos) else {
            return InteractionResult::Pass;
        };
        let Some(furnace_entity) = block_entity.downcast_ref::<FurnaceBlockEntity>() else {
            return InteractionResult::Pass;
        };
        let Some(container_ref) = ContainerRef::from_block_entity(block_entity.clone()) else {
            return InteractionResult::Pass;
        };

        let data = furnace_entity.data();
        let inventory = player.inventory.clone();
        let menu_type = self.menu_type;
        // The title is owned by the menu, so the shared behavior hands it a copy.
        player.open_menu(
            TextComponent::translated(self.title.clone()),
            move |context| {
                furnace(
                    inventory,
                    context.container_id,
                    container_ref,
                    menu_type,
                    data,
                )
            },
        );

        // TODO: award the INTERACT_WITH_FURNACE / _SMOKER / _BLAST_FURNACE stat once
        // player statistics exist.
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

    fn get_block_entity_ticker(
        &self,
        block_entity_type: BlockEntityTypeRef,
    ) -> Option<BlockEntityTicker> {
        BlockEntityTicker::for_matching_entity_tick(block_entity_type, self.block_entity_type)
    }

    fn get_analog_output_signal(&self, world: &dyn LevelReader, pos: BlockPos) -> i32 {
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

/// Declares one furnace variant and forwards its behavior to the shared base.
macro_rules! furnace_variant {
    ($name:ident, $be_type:expr, $menu_type:expr, $title:expr, $doc:literal) => {
        #[doc = $doc]
        #[block_behavior]
        pub struct $name {
            inner: AbstractFurnaceBlock,
        }

        impl $name {
            #[doc = "Creates the behavior for this block."]
            #[must_use]
            pub const fn new(block: BlockRef) -> Self {
                Self {
                    inner: AbstractFurnaceBlock::new(block, $be_type, $menu_type, $title),
                }
            }
        }

        impl BlockBehavior for $name {
            fn get_state_for_placement(
                &self,
                context: &BlockPlaceContext<'_>,
            ) -> Option<BlockStateId> {
                self.inner.get_state_for_placement(context)
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
                self.inner.use_without_item(world, pos, player)
            }

            fn new_block_entity(
                &self,
                level: Weak<World>,
                pos: BlockPos,
                state: BlockStateId,
            ) -> BlockEntityCreation {
                self.inner.new_block_entity(level, pos, state)
            }

            fn get_block_entity_ticker(
                &self,
                _world: &Arc<World>,
                _state: BlockStateId,
                block_entity_type: BlockEntityTypeRef,
            ) -> Option<BlockEntityTicker> {
                self.inner.get_block_entity_ticker(block_entity_type)
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
                self.inner.get_analog_output_signal(world, pos)
            }
        }
    };
}

furnace_variant!(
    FurnaceBlock,
    &vanilla_block_entity_types::FURNACE,
    &vanilla_menu_types::FURNACE,
    translations::CONTAINER_FURNACE.msg(),
    "Behavior for the furnace block."
);

furnace_variant!(
    SmokerBlock,
    &vanilla_block_entity_types::SMOKER,
    &vanilla_menu_types::SMOKER,
    translations::CONTAINER_SMOKER.msg(),
    "Behavior for the smoker block, which cooks food twice as fast."
);

furnace_variant!(
    BlastFurnaceBlock,
    &vanilla_block_entity_types::BLAST_FURNACE,
    &vanilla_menu_types::BLAST_FURNACE,
    translations::CONTAINER_BLAST_FURNACE.msg(),
    "Behavior for the blast furnace block, which smelts ore twice as fast."
);
