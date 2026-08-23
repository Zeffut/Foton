use std::sync::{Arc, Weak};

use steel_macros::block_behavior;
use steel_registry::blocks::properties::{
    BlockStateProperties, Direction, EnumProperty, IntProperty,
};
use steel_registry::blocks::{BlockRef, block_state_ext::BlockStateExt};
use steel_registry::data_components::components::BannerPatternLayers;
use steel_registry::data_components::vanilla_components::{BANNER_PATTERNS, CUSTOM_NAME};
use steel_registry::item_stack::ItemStack;
use steel_registry::items::ItemRef;
use steel_registry::{REGISTRY, RegistryExt as _};
use steel_registry::{vanilla_block_entity_types, vanilla_blocks};
use steel_utils::angle::convert_to_rotation_segment;
use steel_utils::{BlockPos, BlockStateId, Downcast as _, Identifier};

use crate::behavior::block::{BlockEntityCreation, BlockLootContext};
use crate::behavior::context::PlacementSource;
use crate::behavior::{BlockBehavior, BlockPlaceContext};
use crate::block_entity::BLOCK_ENTITIES;
use crate::block_entity::entities::BannerBlockEntity;
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

/// Carries an item's pattern layers and name onto the banner just placed.
///
/// Vanilla parity: the `applyImplicitComponents` of `BannerBlockEntity`.
/// Without this the loom's work is lost the moment the banner is put down.
fn apply_item_to_banner(world: &Arc<World>, pos: BlockPos, source: &PlacementSource<'_>) {
    let (patterns, name) = source.with_item(|item| {
        (
            item.get(BANNER_PATTERNS)
                .cloned()
                .unwrap_or_else(BannerPatternLayers::empty),
            item.get(CUSTOM_NAME).cloned(),
        )
    });
    if patterns.layers().is_empty() && name.is_none() {
        return;
    }

    let Some(block_entity) = world.get_block_entity(pos) else {
        return;
    };
    let Some(banner) = block_entity.downcast_ref::<BannerBlockEntity>() else {
        return;
    };
    banner.set_from_item(patterns, name);
}

/// Returns the item a banner block drops.
///
/// Vanilla parity: a wall banner has no item of its own -- its loot table
/// names the standing one -- so the wall form's key has to be translated
/// rather than looked up.
fn banner_item_for(block: BlockRef) -> Option<ItemRef> {
    let path = block.key.path.as_ref();
    if let Some(color) = path.strip_suffix("_wall_banner") {
        let standing = Identifier::vanilla(format!("{color}_banner"));
        return REGISTRY.items.by_key(&standing);
    }
    REGISTRY.items.by_key(&block.key)
}

/// Drops a banner carrying whatever was stamped on it.
///
/// Vanilla parity: the `COPY_COMPONENTS` loot function every banner's loot
/// table applies. Steel has no loot tables, so the copy happens here.
fn banner_drops(block: BlockRef, context: &BlockLootContext<'_>) -> Option<Vec<ItemStack>> {
    let mut dropped = ItemStack::new(banner_item_for(block)?);

    if let Some(block_entity) = context.world().get_block_entity(context.pos())
        && let Some(banner) = block_entity.downcast_ref::<BannerBlockEntity>()
    {
        let patterns = banner.patterns();
        if !patterns.layers().is_empty() {
            dropped.set(BANNER_PATTERNS, patterns);
        }
        if let Some(name) = banner.custom_name() {
            dropped.set(CUSTOM_NAME, name);
        }
    }

    Some(vec![dropped])
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

    fn set_placed_by(
        &self,
        _state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        source: &PlacementSource<'_>,
    ) {
        apply_item_to_banner(world, pos, source);
    }

    fn get_drops(
        &self,
        _state: BlockStateId,
        context: &BlockLootContext<'_>,
    ) -> Option<Vec<ItemStack>> {
        banner_drops(self.block, context)
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

    fn set_placed_by(
        &self,
        _state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        source: &PlacementSource<'_>,
    ) {
        apply_item_to_banner(world, pos, source);
    }

    fn get_drops(
        &self,
        _state: BlockStateId,
        context: &BlockLootContext<'_>,
    ) -> Option<Vec<ItemStack>> {
        banner_drops(self.block, context)
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
    use steel_registry::data_components::components::BannerPatternLayer;
    use steel_registry::dye_color::DyeColor;
    use steel_registry::registry::holder::RegistryHolder;
    use steel_registry::{init_vanilla_registry, vanilla_items};
    use steel_utils::ChunkPos;
    use steel_utils::types::{InteractionHand, UpdateFlags};

    use super::*;
    use crate::behavior::context::{PlacementOrientation, PlacementSource};
    use crate::behavior::init_behaviors;
    use crate::block_entity::init_block_entities;
    use crate::test_support::{fresh_test_world, insert_ready_full_chunk};

    /// A banner standing in a real world.
    fn placed_banner(name: &'static str, pos: BlockPos) -> Arc<World> {
        init_vanilla_registry();
        init_behaviors();
        init_block_entities();
        let world = fresh_test_world(name);
        insert_ready_full_chunk(&world, ChunkPos::new(0, 0));
        let _ = world.set_block(
            pos,
            vanilla_blocks::WHITE_BANNER.default_state(),
            UpdateFlags::UPDATE_ALL,
        );
        world
    }

    /// One red flower layer, the sort of thing a loom makes.
    fn flower_layer() -> BannerPatternLayer {
        let pattern = REGISTRY
            .banner_patterns
            .by_key(&steel_utils::Identifier::vanilla_static("flower"))
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

    /// The whole point of the block entity: a banner the loom stamped keeps
    /// its layers when it is put down and gives them back when it is broken.
    /// Losing them on the way down would make the loom pointless; losing them
    /// on the way out would delete the player's work.
    #[test]
    fn the_pattern_survives_being_placed_and_broken() {
        let pos = BlockPos::new(8, 70, 8);
        let world = placed_banner("banner_round_trip", pos);
        let behavior = BannerBlock::new(&vanilla_blocks::WHITE_BANNER);
        let mut item = banner_with_a_layer();

        let source = PlacementSource::direct(
            None,
            InteractionHand::MainHand,
            &mut item,
            PlacementOrientation::Player {
                rotation: 0.0,
                pitch: 0.0,
            },
            false,
        );
        behavior.set_placed_by(
            vanilla_blocks::WHITE_BANNER.default_state(),
            &world,
            pos,
            &source,
        );

        let drops = behavior
            .get_drops(
                vanilla_blocks::WHITE_BANNER.default_state(),
                &BlockLootContext::new(&world, pos),
            )
            .expect("a banner overrides its own loot");

        assert_eq!(drops.len(), 1, "one banner, not a pile");
        let dropped = drops.into_iter().next().expect("the banner");
        assert!(dropped.is(&vanilla_items::WHITE_BANNER));
        let layers = dropped
            .get(BANNER_PATTERNS)
            .expect("the dropped banner carries its patterns");
        assert_eq!(layers.layers().len(), 1);
        assert_eq!(layers.layers()[0].color(), DyeColor::Red);
    }

    /// A plain banner drops plain. The component itself is always there --
    /// vanilla gives every banner item a default `banner_patterns: []` -- so
    /// what matters is that the list is empty, or the banner would not stack
    /// with a fresh one.
    #[test]
    fn a_plain_banner_drops_with_no_layers() {
        let pos = BlockPos::new(8, 70, 8);
        let world = placed_banner("banner_plain", pos);
        let behavior = BannerBlock::new(&vanilla_blocks::WHITE_BANNER);

        let drops = behavior
            .get_drops(
                vanilla_blocks::WHITE_BANNER.default_state(),
                &BlockLootContext::new(&world, pos),
            )
            .expect("a banner overrides its own loot");

        let dropped = drops.into_iter().next().expect("the banner");
        assert_eq!(
            dropped
                .get(BANNER_PATTERNS)
                .map_or(0, |layers| layers.layers().len()),
            0
        );
    }

    /// The wall form shares the block entity, so it has to keep the patterns
    /// too -- a banner hung on a wall is the same banner.
    #[test]
    fn a_wall_banner_keeps_its_pattern_as_well() {
        let pos = BlockPos::new(8, 70, 8);
        let world = placed_banner("banner_wall", pos);
        let _ = world.set_block(
            pos,
            vanilla_blocks::WHITE_WALL_BANNER.default_state(),
            UpdateFlags::UPDATE_ALL,
        );
        let behavior = WallBannerBlock::new(&vanilla_blocks::WHITE_WALL_BANNER);
        let mut item = banner_with_a_layer();

        let source = PlacementSource::direct(
            None,
            InteractionHand::MainHand,
            &mut item,
            PlacementOrientation::Player {
                rotation: 0.0,
                pitch: 0.0,
            },
            false,
        );
        behavior.set_placed_by(
            vanilla_blocks::WHITE_WALL_BANNER.default_state(),
            &world,
            pos,
            &source,
        );

        let drops = behavior
            .get_drops(
                vanilla_blocks::WHITE_WALL_BANNER.default_state(),
                &BlockLootContext::new(&world, pos),
            )
            .expect("a wall banner overrides its own loot");
        let dropped = drops.into_iter().next().expect("the banner");

        // Vanilla drops the standing form from a wall banner, since the wall
        // banner has no item of its own.
        assert_eq!(
            dropped
                .get(BANNER_PATTERNS)
                .map_or(0, |layers| layers.layers().len()),
            1
        );
    }
}
