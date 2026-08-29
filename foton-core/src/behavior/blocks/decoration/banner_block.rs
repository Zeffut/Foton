use std::sync::Weak;

use foton_macros::block_behavior;
use foton_registry::REGISTRY;
use foton_registry::blocks::properties::{
    BlockStateProperties, Direction, EnumProperty, IntProperty,
};
use foton_registry::blocks::{BlockRef, block_state_ext::BlockStateExt};
use foton_registry::{vanilla_block_entity_types, vanilla_blocks};
use foton_utils::angle::convert_to_rotation_segment;
use foton_utils::{BlockPos, BlockStateId};

use crate::behavior::block::BlockEntityCreation;
use crate::behavior::{BlockBehavior, BlockPlaceContext};
use crate::block_entity::BLOCK_ENTITIES;
use crate::world::{LevelReader, ScheduledTickAccess, World};

/// Creates the block entity every banner carries.
///
/// Vanilla parity: `AbstractBannerBlock.newBlockEntity`. The standing and wall
/// forms share one block-entity type, which is why they share this.
fn new_banner_block_entity(
    level: Weak<World>,
    pos: BlockPos,
    state: BlockStateId,
) -> BlockEntityCreation {
    BlockEntityCreation::from_registered_factory(BLOCK_ENTITIES.create(
        &vanilla_block_entity_types::BANNER,
        level,
        pos,
        state,
    ))
}

const FACING: &EnumProperty<Direction> = &BlockStateProperties::HORIZONTAL_FACING;

/// Shared behavior for standing banner blocks
#[block_behavior]
pub struct BannerBlock {
    block: BlockRef,
}

const ROTATION_16: &IntProperty = &BlockStateProperties::ROTATION_16;

impl BannerBlock {
    /// Creates a new banner block behavior
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }
}

impl BlockBehavior for BannerBlock {
    fn new_block_entity(
        &self,
        level: Weak<World>,
        pos: BlockPos,
        state: BlockStateId,
    ) -> BlockEntityCreation {
        new_banner_block_entity(level, pos, state)
    }

    fn is_possible_to_respawn_in_this(&self, _state: BlockStateId) -> bool {
        true
    }

    fn can_survive(&self, _state: BlockStateId, world: &dyn LevelReader, pos: BlockPos) -> bool {
        world.get_block_state(pos.below()).is_solid()
    }

    fn update_shape(
        &self,
        state: BlockStateId,
        world: &dyn ScheduledTickAccess,
        pos: BlockPos,
        direction: Direction,
        _neighbor_pos: BlockPos,
        _neighbor_state: BlockStateId,
    ) -> BlockStateId {
        if direction == Direction::Down && !self.can_survive(state, world, pos) {
            return REGISTRY.blocks.get_default_state_id(&vanilla_blocks::AIR);
        }
        state
    }

    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        Some(self.block.default_state().set_value(
            ROTATION_16,
            convert_to_rotation_segment(context.rotation() + 180.0),
        ))
    }
}

/// Shared behavior for wall banner blocks
#[block_behavior]
pub struct WallBannerBlock {
    block: BlockRef,
}

impl WallBannerBlock {
    /// Creates a new wall banner block behavior
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }
}

impl BlockBehavior for WallBannerBlock {
    fn new_block_entity(
        &self,
        level: Weak<World>,
        pos: BlockPos,
        state: BlockStateId,
    ) -> BlockEntityCreation {
        new_banner_block_entity(level, pos, state)
    }

    fn is_possible_to_respawn_in_this(&self, _state: BlockStateId) -> bool {
        true
    }

    fn can_survive(&self, state: BlockStateId, world: &dyn LevelReader, pos: BlockPos) -> bool {
        let facing = state.get_value(FACING);
        world
            .get_block_state(facing.opposite().relative(pos))
            .is_solid()
    }

    fn update_shape(
        &self,
        state: BlockStateId,
        world: &dyn ScheduledTickAccess,
        pos: BlockPos,
        direction: Direction,
        _neighbor_pos: BlockPos,
        _neighbor_state: BlockStateId,
    ) -> BlockStateId {
        let facing = state.get_value(FACING);
        if direction == facing.opposite() && !self.can_survive(state, world, pos) {
            return REGISTRY.blocks.get_default_state_id(&vanilla_blocks::AIR);
        }
        state
    }

    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        for direction in context.get_nearest_looking_directions() {
            if !direction.is_horizontal() {
                continue;
            }

            let state = self
                .block
                .default_state()
                .set_value(FACING, direction.opposite());
            if self.can_survive(state, context.world.as_ref(), context.place_pos()) {
                return Some(state);
            }
        }

        None
    }
}
#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use foton_registry::data_components::components::{BannerPatternLayer, BannerPatternLayers};
    use foton_registry::data_components::vanilla_components::BANNER_PATTERNS;
    use foton_registry::dye_color::DyeColor;
    use foton_registry::item_stack::ItemStack;
    use foton_registry::registry::holder::RegistryHolder;
    use foton_registry::{RegistryExt as _, init_vanilla_registry, vanilla_items};
    use foton_utils::ChunkPos;
    use foton_utils::types::UpdateFlags;

    use super::*;
    use crate::behavior::BlockLootContext;
    use crate::behavior::init_behaviors;
    use crate::block_entity::{SharedBlockEntity, init_block_entities};
    use crate::test_support::{fresh_test_world, insert_ready_full_chunk};

    /// Brings up everything a banner needs before its block states can even be
    /// named: `vanilla_blocks::WHITE_BANNER.default_state()` reads the registry.
    fn setup() {
        init_vanilla_registry();
        init_behaviors();
        init_block_entities();
    }

    /// A banner standing in a real world, with its block entity.
    fn placed_banner(
        name: &'static str,
        pos: BlockPos,
        state: BlockStateId,
    ) -> (Arc<World>, SharedBlockEntity) {
        let world = fresh_test_world(name);
        insert_ready_full_chunk(&world, ChunkPos::new(0, 0));
        assert!(world.set_block(pos, state, UpdateFlags::UPDATE_ALL));
        let block_entity = world
            .get_block_entity(pos)
            .unwrap_or_else(|| panic!("a placed banner should have a block entity"));
        (world, block_entity)
    }

    /// One red flower layer, the sort of thing a loom makes.
    fn flower_layer() -> BannerPatternLayer {
        let pattern = REGISTRY
            .banner_patterns
            .by_key(&foton_utils::Identifier::vanilla_static("flower"))
            .expect("the flower pattern is a vanilla banner pattern");
        BannerPatternLayer::new(RegistryHolder::Reference(pattern), DyeColor::Red)
    }

    fn banner_with_a_layer() -> ItemStack {
        let mut stack = ItemStack::new(&vanilla_items::WHITE_BANNER);
        stack.set(
            BANNER_PATTERNS,
            BannerPatternLayers::new(vec![flower_layer()]),
        );
        stack
    }

    /// The number of layers on the single item this banner state drops.
    fn dropped_layers(
        world: &Arc<World>,
        pos: BlockPos,
        state: BlockStateId,
        block_entity: &SharedBlockEntity,
    ) -> (ItemStack, usize) {
        let drops = BlockLootContext::new(world, pos)
            .with_tool(&ItemStack::empty())
            .with_block_entity(Some(block_entity))
            .get_drops(state);
        assert_eq!(drops.len(), 1, "one banner, not a pile");
        let dropped = drops.into_iter().next().expect("the banner");
        let layers = dropped
            .get(BANNER_PATTERNS)
            .map_or(0, |layers| layers.layers().len());
        (dropped, layers)
    }

    /// The whole point of the block entity: a banner the loom stamped keeps
    /// its layers when it is put down and gives them back when it is broken.
    /// Losing them on the way down would make the loom pointless; losing them
    /// on the way out would delete the player's work.
    #[test]
    fn the_pattern_survives_being_placed_and_broken() {
        setup();
        let pos = BlockPos::new(8, 70, 8);
        let state = vanilla_blocks::WHITE_BANNER.default_state();
        let (world, block_entity) = placed_banner("banner_round_trip", pos, state);

        block_entity.apply_components_from_item_stack(&banner_with_a_layer());

        let (dropped, layers) = dropped_layers(&world, pos, state, &block_entity);
        assert!(dropped.is(&vanilla_items::WHITE_BANNER));
        assert_eq!(layers, 1, "the loom's layer has to come back with the item");
        let dropped_layers = dropped
            .get(BANNER_PATTERNS)
            .expect("the banner item always answers its patterns");
        assert_eq!(dropped_layers.layers()[0].color(), DyeColor::Red);
    }

    /// A plain banner drops plain. The component itself is always there --
    /// vanilla gives every banner item a default `banner_patterns: []` -- so
    /// what matters is that the list is empty, or the banner would not stack
    /// with a fresh one.
    #[test]
    fn a_plain_banner_drops_with_no_layers() {
        setup();
        let pos = BlockPos::new(8, 70, 8);
        let state = vanilla_blocks::WHITE_BANNER.default_state();
        let (world, block_entity) = placed_banner("banner_plain", pos, state);

        let (_, layers) = dropped_layers(&world, pos, state, &block_entity);
        assert_eq!(layers, 0);
    }

    /// The wall form shares the block entity, so it has to keep the patterns
    /// too -- a banner hung on a wall is the same banner.
    ///
    /// It is also the only form that proves the loot table id: there is no
    /// `blocks/white_wall_banner`, because vanilla registers the wall form with
    /// `dropsLike(WHITE_BANNER)`. Guessing the id from the block name finds
    /// nothing and drops nothing at all, which is what the `drops.len() == 1`
    /// inside `dropped_layers` catches.
    #[test]
    fn a_wall_banner_keeps_its_pattern_as_well() {
        setup();
        let pos = BlockPos::new(8, 70, 8);
        let state = vanilla_blocks::WHITE_WALL_BANNER.default_state();
        let (world, block_entity) = placed_banner("banner_wall", pos, state);

        block_entity.apply_components_from_item_stack(&banner_with_a_layer());

        // Vanilla drops the standing form from a wall banner, since the wall
        // banner has no item of its own.
        let (dropped, layers) = dropped_layers(&world, pos, state, &block_entity);
        assert!(dropped.is(&vanilla_items::WHITE_BANNER));
        assert_eq!(layers, 1);
    }
}
