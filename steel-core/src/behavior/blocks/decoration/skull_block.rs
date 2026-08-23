//! Skull and head block behaviors.
//!
//! Vanilla parity: `AbstractSkullBlock` and the seven classes under it --
//! `SkullBlock`, `WallSkullBlock`, `PlayerHeadBlock`, `PlayerWallHeadBlock`,
//! `PiglinWallSkullBlock`, `WitherSkullBlock` and `WitherWallSkullBlock`.
//! All fourteen skull blocks share one block entity, so the classes differ
//! only in how the block is oriented and in what a broken one gives back.
//!
//! Not implemented: the wither summoning that `WitherSkullBlock.checkSpawn`
//! performs. Steel has no wither entity, so there is nothing to summon; the
//! hook is marked where it belongs rather than half-built.

use std::sync::{Arc, Weak};

use steel_macros::block_behavior;
use steel_registry::blocks::properties::{
    BlockStateProperties, BoolProperty, Direction, EnumProperty, IntProperty,
};
use steel_registry::blocks::{BlockRef, block_state_ext::BlockStateExt as _};
use steel_registry::data_components::vanilla_components::{CUSTOM_NAME, NOTE_BLOCK_SOUND, PROFILE};
use steel_registry::item_stack::ItemStack;
use steel_registry::items::ItemRef;
use steel_registry::vanilla_block_entity_types;
use steel_registry::{REGISTRY, RegistryExt as _};
use steel_utils::angle::convert_to_rotation_segment;
use steel_utils::types::UpdateFlags;
use steel_utils::{BlockPos, BlockStateId, Downcast as _, Identifier};

use crate::behavior::block::{BlockEntityCreation, BlockLootContext};
use crate::behavior::context::PlacementSource;
use crate::behavior::{BlockBehavior, BlockPlaceContext, BlockStateBehaviorExt as _};
use crate::block_entity::BLOCK_ENTITIES;
use crate::block_entity::entities::SkullBlockEntity;
use crate::entity::ai::path::PathComputationType;
use crate::world::{SignalGetter as _, World};

const POWERED: &BoolProperty = &BlockStateProperties::POWERED;
const ROTATION_16: &IntProperty = &BlockStateProperties::ROTATION_16;
const FACING: &EnumProperty<Direction> = &BlockStateProperties::HORIZONTAL_FACING;

/// Creates the block entity every skull carries.
///
/// Vanilla parity: `AbstractSkullBlock.newBlockEntity`. Standing and wall
/// forms share one block-entity type, which is why they share this.
fn new_skull_block_entity(
    level: Weak<World>,
    pos: BlockPos,
    state: BlockStateId,
) -> BlockEntityCreation {
    BlockEntityCreation::from_registered_factory(BLOCK_ENTITIES.create(
        &vanilla_block_entity_types::SKULL,
        level,
        pos,
        state,
    ))
}

/// Carries a head item's profile, note-block sound and name onto the skull
/// just placed.
///
/// Vanilla parity: `SkullBlockEntity.applyImplicitComponents`, which vanilla
/// reaches through `BlockItem.updateBlockEntityComponents`. Without this a
/// player head loses whose it is the moment it is put down.
fn apply_item_to_skull(world: &Arc<World>, pos: BlockPos, source: &PlacementSource<'_>) {
    let (owner, note_block_sound, name) = source.with_item(|item| {
        (
            item.get(PROFILE).cloned(),
            item.get(NOTE_BLOCK_SOUND).cloned(),
            item.get(CUSTOM_NAME).cloned(),
        )
    });
    if owner.is_none() && note_block_sound.is_none() && name.is_none() {
        return;
    }

    let Some(block_entity) = world.get_block_entity(pos) else {
        return;
    };
    let Some(skull) = block_entity.downcast_ref::<SkullBlockEntity>() else {
        return;
    };
    skull.set_from_item(owner, note_block_sound, name);
}

/// Returns the item a skull block drops.
///
/// Vanilla parity: a wall skull has no item of its own -- `wallVariant` in
/// `Blocks` points its loot at the standing block -- so the wall form's key
/// has to be translated rather than looked up. Vanilla spells the pair two
/// ways, `_wall_skull` beside `_skull` and `_wall_head` beside `_head`.
fn skull_item_for(block: BlockRef) -> Option<ItemRef> {
    let path = block.key.path.as_ref();
    if let Some(kind) = path.strip_suffix("_wall_skull") {
        return REGISTRY
            .items
            .by_key(&Identifier::vanilla(format!("{kind}_skull")));
    }
    if let Some(kind) = path.strip_suffix("_wall_head") {
        return REGISTRY
            .items
            .by_key(&Identifier::vanilla(format!("{kind}_head")));
    }
    REGISTRY.items.by_key(&block.key)
}

/// Drops a skull carrying whatever the block entity remembered.
///
/// Vanilla parity: the `copy_components` loot function in each skull's loot
/// table. Every skull copies `custom_name`; only `blocks/player_head` also
/// copies `profile` and `note_block_sound`, which is why `copy_profile` is a
/// property of the block rather than of the block entity. Steel's loot engine
/// parses `copy_components` but its `ItemStack::copy_components` is still a
/// stub, so the copy happens here.
fn skull_drops(
    block: BlockRef,
    context: &BlockLootContext<'_>,
    copy_profile: bool,
) -> Option<Vec<ItemStack>> {
    let mut dropped = ItemStack::new(skull_item_for(block)?);

    if let Some(block_entity) = context.world().get_block_entity(context.pos())
        && let Some(skull) = block_entity.downcast_ref::<SkullBlockEntity>()
    {
        if copy_profile {
            if let Some(owner) = skull.owner_profile() {
                dropped.set(PROFILE, owner);
            }
            if let Some(sound) = skull.note_block_sound() {
                dropped.set(NOTE_BLOCK_SOUND, sound);
            }
        }
        if let Some(name) = skull.custom_name() {
            dropped.set(CUSTOM_NAME, name);
        }
    }

    Some(vec![dropped])
}

/// Returns the default state already carrying the redstone signal at `pos`.
///
/// Vanilla parity: `AbstractSkullBlock.getStateForPlacement`.
fn powered_default_state(block: BlockRef, context: &BlockPlaceContext<'_>) -> BlockStateId {
    block.default_state().set_value(
        POWERED,
        context.world.has_neighbor_signal(context.place_pos()),
    )
}

/// Orients a skull standing on the floor.
///
/// Vanilla parity: `SkullBlock.getStateForPlacement`. Note the absence of the
/// half turn `BannerBlock` applies: a skull looks the way the player does. A
/// standing skull needs nothing to stand on, so unlike the wall form this
/// never refuses.
fn standing_skull_placement(block: BlockRef, context: &BlockPlaceContext<'_>) -> BlockStateId {
    powered_default_state(block, context)
        .set_value(ROTATION_16, convert_to_rotation_segment(context.rotation()))
}

/// Orients a skull hung on a wall, or refuses the placement.
///
/// Vanilla parity: `WallSkullBlock.getStateForPlacement`. Vanilla keeps
/// reassigning its local state as it walks the looking directions, but only
/// ever returns it from inside the loop, so the carried value is dead and the
/// loop is written here without it.
fn wall_skull_placement(block: BlockRef, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
    let base = powered_default_state(block, context);

    for direction in context.get_nearest_looking_directions() {
        if !direction.is_horizontal() {
            continue;
        }
        let support = direction.relative(context.place_pos());
        if !context
            .world
            .get_block_state(support)
            .can_be_replaced(context)
        {
            return Some(base.set_value(FACING, direction.opposite()));
        }
    }

    None
}

/// Follows the redstone signal at `pos` into the `powered` property.
///
/// Vanilla parity: `AbstractSkullBlock.neighborChanged`. Vanilla passes flag
/// 2, clients only: the property drives nothing but the dragon and piglin head
/// animation on the client, so it must not start another round of neighbor
/// updates.
fn update_powered(state: BlockStateId, world: &Arc<World>, pos: BlockPos) {
    let signal = world.has_neighbor_signal(pos);
    if signal == state.get_value(POWERED) {
        return;
    }
    world.set_block(
        pos,
        state.set_value(POWERED, signal),
        UpdateFlags::UPDATE_CLIENTS,
    );
}

/// Vanilla `SkullBlock`: the skeleton, zombie, creeper, dragon and piglin
/// heads standing on the floor.
///
/// Vanilla carries a `SkullBlock.Type` here. It selects the collision shape
/// and the model the client draws, both of which Steel takes from generated
/// block data, so the behavior has no use for it.
#[block_behavior]
pub struct SkullBlock {
    block: BlockRef,
}

impl SkullBlock {
    /// Creates a standing skull behavior for `block`.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }
}

impl BlockBehavior for SkullBlock {
    fn new_block_entity(
        &self,
        level: Weak<World>,
        pos: BlockPos,
        state: BlockStateId,
    ) -> BlockEntityCreation {
        new_skull_block_entity(level, pos, state)
    }

    fn set_placed_by(
        &self,
        _state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        source: &PlacementSource<'_>,
    ) {
        apply_item_to_skull(world, pos, source);
    }

    fn get_drops(
        &self,
        _state: BlockStateId,
        context: &BlockLootContext<'_>,
    ) -> Option<Vec<ItemStack>> {
        skull_drops(self.block, context, false)
    }

    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        Some(standing_skull_placement(self.block, context))
    }

    fn handle_neighbor_changed(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        _source_block: BlockRef,
        _moved_by_piston: bool,
    ) {
        update_powered(state, world, pos);
    }

    fn is_pathfindable(&self, _state: BlockStateId, _type: PathComputationType) -> bool {
        false
    }
}

/// Vanilla `WallSkullBlock`: the same heads hung on a wall.
#[block_behavior]
pub struct WallSkullBlock {
    block: BlockRef,
}

impl WallSkullBlock {
    /// Creates a wall skull behavior for `block`.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }
}

impl BlockBehavior for WallSkullBlock {
    fn new_block_entity(
        &self,
        level: Weak<World>,
        pos: BlockPos,
        state: BlockStateId,
    ) -> BlockEntityCreation {
        new_skull_block_entity(level, pos, state)
    }

    fn set_placed_by(
        &self,
        _state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        source: &PlacementSource<'_>,
    ) {
        apply_item_to_skull(world, pos, source);
    }

    fn get_drops(
        &self,
        _state: BlockStateId,
        context: &BlockLootContext<'_>,
    ) -> Option<Vec<ItemStack>> {
        skull_drops(self.block, context, false)
    }

    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        wall_skull_placement(self.block, context)
    }

    fn handle_neighbor_changed(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        _source_block: BlockRef,
        _moved_by_piston: bool,
    ) {
        update_powered(state, world, pos);
    }

    fn is_pathfindable(&self, _state: BlockStateId, _type: PathComputationType) -> bool {
        false
    }
}

/// Vanilla `PiglinWallSkullBlock`: a wall piglin head.
///
/// The class exists in vanilla only to widen the collision shape, and Steel
/// takes shapes from generated block data, so this is a wall skull under
/// another name. It stays a separate behavior because the block-to-class
/// table names it.
#[block_behavior]
pub struct PiglinWallSkullBlock {
    inner: WallSkullBlock,
}

impl PiglinWallSkullBlock {
    /// Creates a piglin wall head behavior for `block`.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self {
            inner: WallSkullBlock::new(block),
        }
    }
}

impl BlockBehavior for PiglinWallSkullBlock {
    fn new_block_entity(
        &self,
        level: Weak<World>,
        pos: BlockPos,
        state: BlockStateId,
    ) -> BlockEntityCreation {
        self.inner.new_block_entity(level, pos, state)
    }

    fn set_placed_by(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        source: &PlacementSource<'_>,
    ) {
        self.inner.set_placed_by(state, world, pos, source);
    }

    fn get_drops(
        &self,
        state: BlockStateId,
        context: &BlockLootContext<'_>,
    ) -> Option<Vec<ItemStack>> {
        self.inner.get_drops(state, context)
    }

    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        self.inner.get_state_for_placement(context)
    }

    fn handle_neighbor_changed(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        source_block: BlockRef,
        moved_by_piston: bool,
    ) {
        self.inner
            .handle_neighbor_changed(state, world, pos, source_block, moved_by_piston);
    }

    fn is_pathfindable(&self, state: BlockStateId, computation_type: PathComputationType) -> bool {
        self.inner.is_pathfindable(state, computation_type)
    }
}

/// Vanilla `PlayerHeadBlock`: a player head standing on the floor.
///
/// The only skull whose loot table copies the profile back onto the item.
#[block_behavior]
pub struct PlayerHeadBlock {
    block: BlockRef,
}

impl PlayerHeadBlock {
    /// Creates a standing player head behavior for `block`.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }
}

impl BlockBehavior for PlayerHeadBlock {
    fn new_block_entity(
        &self,
        level: Weak<World>,
        pos: BlockPos,
        state: BlockStateId,
    ) -> BlockEntityCreation {
        new_skull_block_entity(level, pos, state)
    }

    fn set_placed_by(
        &self,
        _state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        source: &PlacementSource<'_>,
    ) {
        apply_item_to_skull(world, pos, source);
    }

    fn get_drops(
        &self,
        _state: BlockStateId,
        context: &BlockLootContext<'_>,
    ) -> Option<Vec<ItemStack>> {
        skull_drops(self.block, context, true)
    }

    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        Some(standing_skull_placement(self.block, context))
    }

    fn handle_neighbor_changed(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        _source_block: BlockRef,
        _moved_by_piston: bool,
    ) {
        update_powered(state, world, pos);
    }

    fn is_pathfindable(&self, _state: BlockStateId, _type: PathComputationType) -> bool {
        false
    }
}

/// Vanilla `PlayerWallHeadBlock`: a player head hung on a wall.
#[block_behavior]
pub struct PlayerWallHeadBlock {
    block: BlockRef,
}

impl PlayerWallHeadBlock {
    /// Creates a wall player head behavior for `block`.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }
}

impl BlockBehavior for PlayerWallHeadBlock {
    fn new_block_entity(
        &self,
        level: Weak<World>,
        pos: BlockPos,
        state: BlockStateId,
    ) -> BlockEntityCreation {
        new_skull_block_entity(level, pos, state)
    }

    fn set_placed_by(
        &self,
        _state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        source: &PlacementSource<'_>,
    ) {
        apply_item_to_skull(world, pos, source);
    }

    fn get_drops(
        &self,
        _state: BlockStateId,
        context: &BlockLootContext<'_>,
    ) -> Option<Vec<ItemStack>> {
        skull_drops(self.block, context, true)
    }

    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        wall_skull_placement(self.block, context)
    }

    fn handle_neighbor_changed(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        _source_block: BlockRef,
        _moved_by_piston: bool,
    ) {
        update_powered(state, world, pos);
    }

    fn is_pathfindable(&self, _state: BlockStateId, _type: PathComputationType) -> bool {
        false
    }
}

/// Vanilla `WitherSkullBlock`: a wither skeleton skull standing on the floor.
#[block_behavior]
pub struct WitherSkullBlock {
    inner: SkullBlock,
}

impl WitherSkullBlock {
    /// Creates a standing wither skeleton skull behavior for `block`.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self {
            inner: SkullBlock::new(block),
        }
    }
}

impl BlockBehavior for WitherSkullBlock {
    fn new_block_entity(
        &self,
        level: Weak<World>,
        pos: BlockPos,
        state: BlockStateId,
    ) -> BlockEntityCreation {
        self.inner.new_block_entity(level, pos, state)
    }

    fn set_placed_by(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        source: &PlacementSource<'_>,
    ) {
        self.inner.set_placed_by(state, world, pos, source);
        // Vanilla calls `WitherSkullBlock.checkSpawn` here, which matches the
        // soul sand and skull pattern and spawns a wither. Steel has no wither
        // entity, so there is nothing to spawn and no pattern matcher to run;
        // this is where that hook goes once the entity exists.
    }

    fn get_drops(
        &self,
        state: BlockStateId,
        context: &BlockLootContext<'_>,
    ) -> Option<Vec<ItemStack>> {
        self.inner.get_drops(state, context)
    }

    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        self.inner.get_state_for_placement(context)
    }

    fn handle_neighbor_changed(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        source_block: BlockRef,
        moved_by_piston: bool,
    ) {
        self.inner
            .handle_neighbor_changed(state, world, pos, source_block, moved_by_piston);
    }

    fn is_pathfindable(&self, state: BlockStateId, computation_type: PathComputationType) -> bool {
        self.inner.is_pathfindable(state, computation_type)
    }
}

/// Vanilla `WitherWallSkullBlock`: a wither skeleton skull hung on a wall.
#[block_behavior]
pub struct WitherWallSkullBlock {
    inner: WallSkullBlock,
}

impl WitherWallSkullBlock {
    /// Creates a wall wither skeleton skull behavior for `block`.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self {
            inner: WallSkullBlock::new(block),
        }
    }
}

impl BlockBehavior for WitherWallSkullBlock {
    fn new_block_entity(
        &self,
        level: Weak<World>,
        pos: BlockPos,
        state: BlockStateId,
    ) -> BlockEntityCreation {
        self.inner.new_block_entity(level, pos, state)
    }

    fn set_placed_by(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        source: &PlacementSource<'_>,
    ) {
        self.inner.set_placed_by(state, world, pos, source);
        // Vanilla calls `WitherSkullBlock.checkSpawn` here as well. Out of
        // scope for the same reason: Steel has no wither entity.
    }

    fn get_drops(
        &self,
        state: BlockStateId,
        context: &BlockLootContext<'_>,
    ) -> Option<Vec<ItemStack>> {
        self.inner.get_drops(state, context)
    }

    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        self.inner.get_state_for_placement(context)
    }

    fn handle_neighbor_changed(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        source_block: BlockRef,
        moved_by_piston: bool,
    ) {
        self.inner
            .handle_neighbor_changed(state, world, pos, source_block, moved_by_piston);
    }

    fn is_pathfindable(&self, state: BlockStateId, computation_type: PathComputationType) -> bool {
        self.inner.is_pathfindable(state, computation_type)
    }
}

#[cfg(test)]
mod tests {
    use glam::DVec3;
    use steel_registry::data_components::vanilla_components::{PlayerSkinPatch, ResolvableProfile};
    use steel_registry::{init_vanilla_registry, vanilla_blocks, vanilla_items};
    use steel_utils::ChunkPos;
    use steel_utils::types::{InteractionHand, UpdateFlags};
    use text_components::TextComponent;

    use super::*;
    use crate::behavior::BlockHitResult;
    use crate::behavior::context::PlacementOrientation;
    use crate::behavior::init_behaviors;
    use crate::block_entity::init_block_entities;
    use crate::test_support::{fresh_test_world, insert_ready_full_chunk};

    /// A skull sitting in a real world, block entity and all.
    fn placed_skull(name: &'static str, pos: BlockPos, block: BlockRef) -> Arc<World> {
        init_vanilla_registry();
        init_behaviors();
        init_block_entities();
        let world = fresh_test_world(name);
        insert_ready_full_chunk(&world, ChunkPos::new(0, 0));
        let _ = world.set_block(pos, block.default_state(), UpdateFlags::UPDATE_ALL);
        world
    }

    /// An empty world with room for a skull, for the placement questions that
    /// do not need one to be standing yet.
    fn empty_world(name: &'static str) -> Arc<World> {
        init_vanilla_registry();
        init_behaviors();
        init_block_entities();
        let world = fresh_test_world(name);
        insert_ready_full_chunk(&world, ChunkPos::new(0, 0));
        world
    }

    /// The profile of a named player, the sort a head item carries.
    fn profile_of(name: &str) -> ResolvableProfile {
        ResolvableProfile::dynamic_name(name.to_owned(), PlayerSkinPatch::default())
            .expect("a short plain name is a valid profile name")
    }

    /// Runs `body` with a placement source holding `item`.
    fn with_placement<R>(item: &mut ItemStack, body: impl FnOnce(&PlacementSource<'_>) -> R) -> R {
        let source = PlacementSource::direct(
            None,
            InteractionHand::MainHand,
            item,
            PlacementOrientation::Player {
                rotation: 0.0,
                pitch: 0.0,
            },
            false,
        );
        body(&source)
    }

    /// Builds the context a player clicking `pos` would produce.
    fn place_context<'a>(
        world: &'a Arc<World>,
        pos: BlockPos,
        item: &'a mut ItemStack,
    ) -> BlockPlaceContext<'a> {
        let hit_result = BlockHitResult {
            location: DVec3::ZERO,
            direction: Direction::Up,
            block_pos: pos,
            miss: false,
            inside: false,
            world_border_hit: false,
        };
        let source = PlacementSource::direct(
            None,
            InteractionHand::MainHand,
            item,
            PlacementOrientation::Player {
                rotation: 0.0,
                pitch: 0.0,
            },
            false,
        );
        BlockPlaceContext::new(world, source, &hit_result)
    }

    /// The ordinary case: nothing was stamped on the skull, so exactly one
    /// plain skull comes back. A second stack or a stray component would stop
    /// it stacking with a skull from anywhere else.
    #[test]
    fn a_plain_skull_drops_a_single_plain_skull() {
        let pos = BlockPos::new(8, 70, 8);
        let world = placed_skull("skull_plain", pos, &vanilla_blocks::SKELETON_SKULL);
        let behavior = SkullBlock::new(&vanilla_blocks::SKELETON_SKULL);

        let drops = behavior
            .get_drops(
                vanilla_blocks::SKELETON_SKULL.default_state(),
                &BlockLootContext::new(&world, pos),
            )
            .expect("a skull overrides its own loot");

        assert_eq!(drops.len(), 1, "one skull, not a pile");
        let dropped = drops.into_iter().next().expect("the skull");
        assert!(dropped.is(&vanilla_items::SKELETON_SKULL));
        assert!(dropped.get(PROFILE).is_none());
        assert!(dropped.get(CUSTOM_NAME).is_none());
    }

    /// The whole point of the block entity: a head keeps whose it is when it
    /// is put down and gives it back when it is broken. Losing the profile on
    /// the way down would make every head a blank one; losing it on the way
    /// out would quietly replace the player's head with someone else's.
    #[test]
    fn a_head_remembers_whose_it_is_from_placement_to_breaking() {
        let pos = BlockPos::new(8, 70, 8);
        let world = placed_skull(
            "skull_profile_round_trip",
            pos,
            &vanilla_blocks::PLAYER_HEAD,
        );
        let behavior = PlayerHeadBlock::new(&vanilla_blocks::PLAYER_HEAD);
        let state = vanilla_blocks::PLAYER_HEAD.default_state();

        let owner = profile_of("Steelhead");
        let mut item = ItemStack::new(&vanilla_items::PLAYER_HEAD);
        item.set(PROFILE, owner.clone());
        with_placement(&mut item, |source| {
            behavior.set_placed_by(state, &world, pos, source);
        });

        let drops = behavior
            .get_drops(state, &BlockLootContext::new(&world, pos))
            .expect("a head overrides its own loot");
        let dropped = drops.into_iter().next().expect("the head");

        assert!(dropped.is(&vanilla_items::PLAYER_HEAD));
        assert_eq!(dropped.get(PROFILE), Some(&owner));
    }

    /// A wall head has no item of its own, so it has to drop the standing one
    /// -- and it shares the block entity, so the profile has to survive that
    /// translation too.
    #[test]
    fn a_wall_head_drops_the_standing_head_with_its_profile_intact() {
        let pos = BlockPos::new(8, 70, 8);
        let world = placed_skull("skull_wall_drop", pos, &vanilla_blocks::PLAYER_WALL_HEAD);
        let behavior = PlayerWallHeadBlock::new(&vanilla_blocks::PLAYER_WALL_HEAD);
        let state = vanilla_blocks::PLAYER_WALL_HEAD.default_state();

        let owner = profile_of("Steelhead");
        let mut item = ItemStack::new(&vanilla_items::PLAYER_HEAD);
        item.set(PROFILE, owner.clone());
        with_placement(&mut item, |source| {
            behavior.set_placed_by(state, &world, pos, source);
        });

        let drops = behavior
            .get_drops(state, &BlockLootContext::new(&world, pos))
            .expect("a wall head overrides its own loot");
        let dropped = drops.into_iter().next().expect("the head");

        assert!(
            dropped.is(&vanilla_items::PLAYER_HEAD),
            "the wall block has no item, so the standing one drops"
        );
        assert_eq!(dropped.get(PROFILE), Some(&owner));
    }

    /// Vanilla's loot tables copy `profile` and `note_block_sound` for the
    /// player head alone; every other skull copies only its name. A skeleton
    /// skull that somehow carries a profile must not hand it back, or the
    /// dropped item would stop stacking with an ordinary skull.
    #[test]
    fn a_skeleton_skull_gives_back_its_name_but_never_a_profile() {
        let pos = BlockPos::new(8, 70, 8);
        let world = placed_skull("skull_name_only", pos, &vanilla_blocks::SKELETON_SKULL);
        let behavior = SkullBlock::new(&vanilla_blocks::SKELETON_SKULL);
        let state = vanilla_blocks::SKELETON_SKULL.default_state();

        let name = TextComponent::plain("Yorick");
        let mut item = ItemStack::new(&vanilla_items::SKELETON_SKULL);
        item.set(PROFILE, profile_of("Steelhead"));
        item.set(CUSTOM_NAME, name.clone());
        with_placement(&mut item, |source| {
            behavior.set_placed_by(state, &world, pos, source);
        });

        let drops = behavior
            .get_drops(state, &BlockLootContext::new(&world, pos))
            .expect("a skull overrides its own loot");
        let dropped = drops.into_iter().next().expect("the skull");

        assert_eq!(dropped.get(CUSTOM_NAME), Some(&name));
        assert!(dropped.get(PROFILE).is_none());
    }

    /// A wall head turns its face away from whatever it hangs on, and the
    /// only wall on offer decides which way that is.
    #[test]
    fn a_wall_head_faces_away_from_the_only_wall_it_can_hang_on() {
        let pos = BlockPos::new(8, 70, 8);
        let world = empty_world("skull_wall_placement");
        let _ = world.set_block(
            Direction::North.relative(pos),
            vanilla_blocks::STONE.default_state(),
            UpdateFlags::UPDATE_ALL,
        );

        let behavior = WallSkullBlock::new(&vanilla_blocks::SKELETON_WALL_SKULL);
        let mut item = ItemStack::new(&vanilla_items::SKELETON_SKULL);
        let context = place_context(&world, pos, &mut item);

        let state = behavior
            .get_state_for_placement(&context)
            .expect("the stone to the north is something to hang from");
        assert_eq!(state.get_value(FACING), Direction::South);
    }

    /// With nothing to hang on, vanilla refuses the placement outright rather
    /// than leaving a head floating.
    #[test]
    fn a_wall_head_with_no_wall_refuses_to_be_placed() {
        let pos = BlockPos::new(8, 70, 8);
        let world = empty_world("skull_wall_unsupported");

        let behavior = WallSkullBlock::new(&vanilla_blocks::SKELETON_WALL_SKULL);
        let mut item = ItemStack::new(&vanilla_items::SKELETON_SKULL);
        let context = place_context(&world, pos, &mut item);

        assert!(behavior.get_state_for_placement(&context).is_none());
    }

    /// A standing skull looks the way the player does. The banner, which
    /// shares the rotation property, adds half a turn; the skull does not, and
    /// copying the banner here would leave every skull facing backwards.
    #[test]
    fn a_standing_skull_takes_the_players_rotation_with_no_half_turn() {
        let pos = BlockPos::new(8, 70, 8);
        let world = empty_world("skull_standing_rotation");

        let behavior = SkullBlock::new(&vanilla_blocks::SKELETON_SKULL);
        let mut item = ItemStack::new(&vanilla_items::SKELETON_SKULL);
        let context = place_context(&world, pos, &mut item);

        let state = behavior
            .get_state_for_placement(&context)
            .expect("a standing skull needs nothing to stand on");
        assert_eq!(state.get_value(ROTATION_16), 0);
        assert!(!state.get_value(POWERED));
    }
}
