//! Decorated pot block behavior.
//!
//! Vanilla parity: `DecoratedPotBlock`. A pot's whole identity -- the four
//! sherds pressed into its sides -- lives on a block entity and has to survive
//! the round trip through an item, and it is one of the few blocks that drops
//! something different depending on how it was broken: hit it with a pickaxe
//! and it shatters back into the four pieces it was made of, take it with silk
//! touch and the pot comes back whole.
//!
//! Not ported here: `getCloneItemStack`, because Steel's signature has neither
//! world nor position and so cannot reach the block entity the sherds live on;
//! the `Stats.ITEM_USED` award of `useItemOn`, because Steel has no statistics
//! foundation; and `onProjectileHit`, which cracks a pot an arrow hits.

use std::sync::{Arc, Weak};

use glam::DVec3;
use steel_macros::block_behavior;
use steel_registry::blocks::BlockRef;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::blocks::properties::{
    BlockStateProperties, BoolProperty, Direction, EnumProperty,
};
use steel_registry::data_components::components::PotDecorations;
use steel_registry::data_components::vanilla_components::POT_DECORATIONS;
use steel_registry::item_stack::ItemStack;
use steel_registry::particle_type::ParticleData;
use steel_registry::vanilla_enchantment_tags::EnchantmentTag;
use steel_registry::vanilla_item_tags::ItemTag;
use steel_registry::{
    REGISTRY, RegistryExt as _, TaggedRegistryExt as _, sound_events, vanilla_block_entity_types,
    vanilla_game_events, vanilla_particle_types,
};
use steel_utils::types::{InteractionHand, UpdateFlags};
use steel_utils::{BlockPos, BlockStateId, Downcast as _};

use crate::behavior::block::{BlockLootContext, schedule_water_tick_if_waterlogged};
use crate::behavior::{
    BlockBehavior, BlockEntityCreation, BlockHitResult, BlockPlaceContext, InteractionResult,
    InventoryAccess, PlacementSource,
};
use crate::block_entity::BLOCK_ENTITIES;
use crate::block_entity::entities::{DecoratedPotBlockEntity, WobbleStyle};
use crate::entity::ai::path::PathComputationType;
use crate::inventory::container::calculate_redstone_signal_from_container;
use crate::inventory::lock::{ContainerLockGuard, ContainerRef};
use crate::player::Player;
use crate::world::game_event::GameEventContext;
use crate::world::{LevelReader, ScheduledTickAccess, World};

/// Which way the decorated front faces.
const HORIZONTAL_FACING: &EnumProperty<Direction> = &BlockStateProperties::HORIZONTAL_FACING;
/// Whether the pot has been hit hard enough to come apart when it breaks.
const CRACKED: &BoolProperty = &BlockStateProperties::CRACKED;
/// Whether the pot is standing in water.
const WATERLOGGED: &BoolProperty = &BlockStateProperties::WATERLOGGED;

/// How many items one use puts into the pot.
///
/// Vanilla parity: the literal `1` of `DecoratedPotBlock.useItemOn`.
const ITEMS_PER_INSERT: i32 = 1;
/// Volume of the insert and refuse sounds.
const SOUND_VOLUME: f32 = 1.0;
/// Pitch of the refuse sound.
const REFUSE_PITCH: f32 = 1.0;
/// Base pitch of the insert sound, before the fullness bend.
///
/// Vanilla parity: the `0.7F + 0.5F * pitchBend` of `DecoratedPotBlock.useItemOn`.
const INSERT_BASE_PITCH: f32 = 0.7;
/// How far the insert sound's pitch rises as the pot fills.
const INSERT_PITCH_BEND: f32 = 0.5;
/// How many dust particles puff out of the pot's mouth.
const DUST_PLUME_COUNT: i32 = 7;
/// Height above the block origin the dust plume comes out at.
const DUST_PLUME_HEIGHT: f64 = 1.2;
/// Horizontal center of the pot's mouth.
const BLOCK_CENTER: f64 = 0.5;

/// Behavior for the decorated pot.
#[block_behavior]
pub struct DecoratedPotBlock {
    block: BlockRef,
}

impl DecoratedPotBlock {
    /// Creates a decorated pot block behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }

    /// Returns whether the tool takes the pot apart rather than picking it up.
    ///
    /// Vanilla parity: the condition of `DecoratedPotBlock.playerWillDestroy`
    /// -- `ItemTags.BREAKS_DECORATED_POTS`, which is the swords, pickaxes and
    /// the rest of the tools, minus anything carrying
    /// `EnchantmentTags.PREVENTS_DECORATED_POT_SHATTERING`, which is silk
    /// touch.
    fn shatters_the_pot(tool: &ItemStack) -> bool {
        tool.item().has_tag(&ItemTag::BREAKS_DECORATED_POTS) && !Self::prevents_shattering(tool)
    }

    /// Returns whether the tool's enchantments keep the pot in one piece.
    ///
    /// Vanilla parity: `EnchantmentHelper.hasTag(stack,
    /// EnchantmentTags.PREVENTS_DECORATED_POT_SHATTERING)`.
    fn prevents_shattering(tool: &ItemStack) -> bool {
        let Some(enchantments) = tool.get_enchantments() else {
            return false;
        };
        enchantments.iter().any(|(key, _)| {
            REGISTRY
                .enchantments
                .by_key(key)
                .is_some_and(|enchantment| {
                    REGISTRY.enchantments.is_in_tag(
                        enchantment,
                        &EnchantmentTag::PREVENTS_DECORATED_POT_SHATTERING,
                    )
                })
        })
    }

    /// Returns whether the pot accepts what is being offered to it.
    ///
    /// Vanilla parity: the condition of `DecoratedPotBlock.useItemOn`. A pot
    /// takes anything while it is empty and only more of the same after that,
    /// which is what makes it a one-item filter rather than a small chest.
    fn accepts(stored: &ItemStack, offered: &ItemStack) -> bool {
        if offered.is_empty() {
            return false;
        }
        stored.is_empty()
            || (ItemStack::is_same_item_same_components(stored, offered)
                && stored.count() < stored.max_stack_size())
    }
}

impl BlockBehavior for DecoratedPotBlock {
    /// Vanilla parity: `DecoratedPotBlock.getStateForPlacement`. The decorated
    /// front turns towards the player, unlike most blocks, which face away.
    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        Some(
            self.block
                .default_state()
                .set_value(HORIZONTAL_FACING, context.horizontal_direction())
                .set_value(WATERLOGGED, context.is_water_source())
                .set_value(CRACKED, false),
        )
    }

    /// Vanilla parity: `DecoratedPotBlock.updateShape`, which exists only to
    /// keep the water the pot is standing in flowing.
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

    /// Vanilla parity: `DecoratedPotBlock.isPathfindable`. A mob will not walk
    /// through a pot even though there is room above it.
    fn is_pathfindable(
        &self,
        _state: BlockStateId,
        _computation_type: PathComputationType,
    ) -> bool {
        false
    }

    /// Vanilla parity: `DecoratedPotBlock.newBlockEntity`.
    fn new_block_entity(
        &self,
        level: Weak<World>,
        pos: BlockPos,
        state: BlockStateId,
    ) -> BlockEntityCreation {
        BlockEntityCreation::from_registered_factory(BLOCK_ENTITIES.create(
            &vanilla_block_entity_types::DECORATED_POT,
            level,
            pos,
            state,
        ))
    }

    /// Presses the item's sherds into the pot that was just placed.
    ///
    /// Vanilla parity: the `applyImplicitComponents` of
    /// `DecoratedPotBlockEntity`. Vanilla assigns unconditionally; an all-brick
    /// pot is skipped here because that is already what a fresh block entity
    /// holds, and writing it back would mark the chunk dirty and sweep the
    /// comparators for nothing every time any pot is placed.
    fn set_placed_by(
        &self,
        _state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        source: &PlacementSource<'_>,
    ) {
        let Some(decorations) = source.with_item(|item| item.get(POT_DECORATIONS).cloned()) else {
            return;
        };
        if decorations == PotDecorations::EMPTY {
            return;
        }
        let Some(block_entity) = world.get_block_entity(pos) else {
            return;
        };
        let Some(pot) = block_entity.downcast_ref::<DecoratedPotBlockEntity>() else {
            return;
        };
        pot.set_decorations(decorations);
    }

    /// Cracks the pot the player is about to break with the wrong tool.
    ///
    /// Vanilla parity: `DecoratedPotBlock.playerWillDestroy`. The state this
    /// returns is what the loot is resolved from, which is how one block ends
    /// up with two completely different drops.
    fn player_will_destroy(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        player: &Player,
    ) -> BlockStateId {
        let shatters = {
            let inventory = player.inventory.lock();
            Self::shatters_the_pot(inventory.get_item_in_hand(InteractionHand::MainHand))
        };
        if !shatters {
            return state;
        }

        let cracked = state.set_value(CRACKED, true);
        // Vanilla's flag 260 is Steel's `UPDATE_NONE`: the pot is about to be
        // removed anyway, so neither the neighbors nor the client need to be
        // told about a crack that lasts one call.
        world.set_block(pos, cracked, UpdateFlags::UPDATE_NONE);
        cracked
    }

    /// Returns either the pot or the four pieces it was made of.
    ///
    /// Vanilla parity: `DecoratedPotBlock.getDrops` together with the
    /// `decorated_pot` loot table it feeds, whose two alternatives are selected
    /// by the `cracked` property -- the `sherds` dynamic drop when it is set,
    /// and the pot with `copy_components` of `pot_decorations` when it is not.
    ///
    /// Steel has no loot tables, so both alternatives are resolved here. The
    /// choice still comes from the block state exactly as in vanilla:
    /// [`Self::player_will_destroy`] is the only thing that sets `cracked`, so
    /// a pot destroyed by something other than a player -- an explosion, a
    /// piston -- drops whole, which is what vanilla does too.
    fn get_drops(
        &self,
        state: BlockStateId,
        context: &BlockLootContext<'_>,
    ) -> Option<Vec<ItemStack>> {
        let decorations =
            context
                .world()
                .get_block_entity(context.pos())
                .and_then(|block_entity| {
                    block_entity
                        .downcast_ref::<DecoratedPotBlockEntity>()
                        .map(DecoratedPotBlockEntity::decorations)
                });

        if state.get_value(CRACKED) {
            // Vanilla parity: `PotDecorations.ordered`, which fills every
            // undecorated side back in with the brick it was made from, so a
            // shattered pot gives back exactly four items -- and nothing at all
            // with no block entity to ask, because vanilla's `sherds` dynamic
            // drop then has nobody to supply it.
            return Some(decorations.map_or_else(Vec::new, |decorations| {
                decorations
                    .ordered()
                    .into_iter()
                    .map(ItemStack::new)
                    .collect()
            }));
        }

        let mut pot = ItemStack::new(REGISTRY.items.by_key(&self.block.key)?);
        // Vanilla's `copy_components` copies nothing when the block entity is
        // missing, which leaves the item's own empty decorations in place.
        pot.set(
            POT_DECORATIONS,
            decorations.unwrap_or(PotDecorations::EMPTY),
        );
        Some(vec![pot])
    }

    /// Vanilla parity: `DecoratedPotBlock.affectNeighborsAfterRemoval`, whose
    /// one line is the `Containers` after-destroy neighbor update -- a
    /// comparator reading the pot has to notice that it is gone.
    fn affect_neighbors_after_removal(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        _moved_by_piston: bool,
    ) {
        world.update_neighbor_for_output_signal(pos, state.get_block());
    }

    /// Puts one item into the pot.
    ///
    /// Vanilla parity: `DecoratedPotBlock.useItemOn`, minus its client branch,
    /// which Steel has no counterpart for.
    fn use_item_on(
        &self,
        _state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        player: &Player,
        _hand: InteractionHand,
        _hit_result: &BlockHitResult,
        inv: &mut InventoryAccess,
    ) -> InteractionResult {
        let Some(block_entity) = world.get_block_entity(pos) else {
            return InteractionResult::Pass;
        };
        let Some(pot) = block_entity.downcast_ref::<DecoratedPotBlockEntity>() else {
            return InteractionResult::Pass;
        };

        let stored = pot.the_item();
        if !inv.with_item(|offered| Self::accepts(&stored, offered)) {
            return InteractionResult::TryEmptyHandInteraction;
        }

        pot.wobble(world, WobbleStyle::Positive);

        // Vanilla parity: `ItemStack.consumeAndReturn(1, player)`, which hands
        // back one item and only charges a player who is not in creative.
        let keeps_the_stack = player.has_infinite_materials();
        let taken = inv.with_item(|offered| {
            let taken = offered.copy_with_count(ITEMS_PER_INSERT);
            if !keeps_the_stack {
                offered.shrink(ITEMS_PER_INSERT);
            }
            taken
        });

        let inserted = if stored.is_empty() {
            taken
        } else {
            let mut grown = stored;
            grown.grow(ITEMS_PER_INSERT);
            grown
        };
        // Vanilla parity: the pitch rises with how full the pot is, which is
        // how a player hears a pot filling without being able to look inside.
        let fullness = inserted.count() as f32 / inserted.max_stack_size() as f32;
        pot.set_the_item(inserted);

        world.play_block_sound(
            &sound_events::BLOCK_DECORATED_POT_INSERT,
            pos,
            SOUND_VOLUME,
            INSERT_BASE_PITCH + INSERT_PITCH_BEND * fullness,
            None,
        );
        world.send_particles(
            ParticleData::simple(&vanilla_particle_types::DUST_PLUME),
            DVec3::new(
                f64::from(pos.x()) + BLOCK_CENTER,
                f64::from(pos.y()) + DUST_PLUME_HEIGHT,
                f64::from(pos.z()) + BLOCK_CENTER,
            ),
            DUST_PLUME_COUNT,
            DVec3::ZERO,
            0.0,
        );
        world.game_event(
            &vanilla_game_events::BLOCK_CHANGE,
            pos,
            &GameEventContext::new(Some(player), None),
        );
        InteractionResult::Success
    }

    /// Vanilla parity: `DecoratedPotBlock.useWithoutItem`. An empty hand takes
    /// nothing back out of a pot; it only rocks and thuds, which is the game
    /// saying that the way back in is to break it.
    fn use_without_item(
        &self,
        _state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        player: &Player,
        _hit_result: &BlockHitResult,
        _inv: &mut InventoryAccess,
    ) -> InteractionResult {
        let Some(block_entity) = world.get_block_entity(pos) else {
            return InteractionResult::Pass;
        };
        let Some(pot) = block_entity.downcast_ref::<DecoratedPotBlockEntity>() else {
            return InteractionResult::Pass;
        };

        world.play_block_sound(
            &sound_events::BLOCK_DECORATED_POT_INSERT_FAIL,
            pos,
            SOUND_VOLUME,
            REFUSE_PITCH,
            None,
        );
        pot.wobble(world, WobbleStyle::Negative);
        world.game_event(
            &vanilla_game_events::BLOCK_CHANGE,
            pos,
            &GameEventContext::new(Some(player), None),
        );
        InteractionResult::Success
    }

    /// Vanilla parity: `BaseEntityBlock.triggerEvent`, which hands the event to
    /// the block entity. Returning `true` is what sends the wobble to clients.
    fn trigger_event(
        &self,
        _state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        param_a: i32,
        param_b: i32,
    ) -> bool {
        world
            .get_block_entity(pos)
            .is_some_and(|block_entity| block_entity.trigger_event(param_a, param_b))
    }

    fn has_analog_output_signal(&self, _state: BlockStateId) -> bool {
        true
    }

    /// Vanilla parity: `AbstractContainerMenu.getRedstoneSignalFromBlockEntity`.
    /// One slot means a single stack fills the pot, so a comparator behind one
    /// is a counter for whatever a hopper above it is feeding in.
    fn get_analog_output_signal(
        &self,
        _state: BlockStateId,
        world: &dyn LevelReader,
        pos: BlockPos,
        _direction: Direction,
    ) -> i32 {
        let Some(container_ref) = world
            .get_block_entity(pos)
            .and_then(ContainerRef::from_block_entity)
        else {
            return 0;
        };
        let guard = ContainerLockGuard::lock_all(&[&container_ref]);
        guard
            .get(container_ref.container_id())
            .map_or(0, calculate_redstone_signal_from_container)
    }
}

#[cfg(test)]
mod tests {
    use steel_registry::data_components::components::ItemEnchantments;
    use steel_registry::data_components::vanilla_components::ENCHANTMENTS;
    use steel_registry::{
        init_vanilla_registry, vanilla_blocks, vanilla_enchantments, vanilla_items,
    };
    use steel_utils::ChunkPos;

    use super::*;
    use crate::behavior::context::PlacementOrientation;
    use crate::behavior::init_behaviors;
    use crate::block_entity::init_block_entities;
    use crate::test_support::{fresh_test_world, insert_ready_full_chunk};

    const POT_POS: BlockPos = BlockPos::new(8, 70, 8);

    /// A pot standing in a real world.
    fn placed_pot(name: &'static str) -> Arc<World> {
        init_vanilla_registry();
        init_behaviors();
        init_block_entities();
        let world = fresh_test_world(name);
        insert_ready_full_chunk(&world, ChunkPos::new(0, 0));
        let _ = world.set_block(
            POT_POS,
            vanilla_blocks::DECORATED_POT.default_state(),
            UpdateFlags::UPDATE_ALL,
        );
        world
    }

    /// Two sherds and two blank sides, the sort of pot a player actually digs
    /// the pieces for.
    fn two_sherds() -> PotDecorations {
        PotDecorations::from_ordered(&[
            &vanilla_items::ANGLER_POTTERY_SHERD,
            &vanilla_items::BRICK,
            &vanilla_items::ARCHER_POTTERY_SHERD,
            &vanilla_items::BRICK,
        ])
        .expect("four decorations fit")
    }

    fn pot_item_with_sherds() -> ItemStack {
        let mut stack = ItemStack::new(&vanilla_items::DECORATED_POT);
        stack.set(POT_DECORATIONS, two_sherds());
        stack
    }

    fn place_with(behavior: &DecoratedPotBlock, world: &Arc<World>, item: &mut ItemStack) {
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
        behavior.set_placed_by(
            vanilla_blocks::DECORATED_POT.default_state(),
            world,
            POT_POS,
            &source,
        );
    }

    /// The whole round trip: a pot crafted from sherds keeps them when it is
    /// put down and hands them back when it is picked up again. Losing them on
    /// the way down would make archaeology pointless; losing them on the way
    /// out would delete work that cost a brush and a block of suspicious sand.
    #[test]
    fn the_sherds_survive_being_placed_and_broken() {
        let world = placed_pot("decorated_pot_round_trip");
        let behavior = DecoratedPotBlock::new(&vanilla_blocks::DECORATED_POT);
        place_with(&behavior, &world, &mut pot_item_with_sherds());

        let drops = behavior
            .get_drops(
                vanilla_blocks::DECORATED_POT.default_state(),
                &BlockLootContext::new(&world, POT_POS),
            )
            .expect("a decorated pot overrides its own loot");

        assert_eq!(drops.len(), 1, "one pot, not a pile of pieces");
        let dropped = drops.into_iter().next().expect("the pot");
        assert!(dropped.is(&vanilla_items::DECORATED_POT));
        assert_eq!(
            dropped.get(POT_DECORATIONS),
            Some(&two_sherds()),
            "the dropped pot lost its sherds"
        );
    }

    /// A pot nobody decorated drops a pot nobody decorated, and it has to be
    /// indistinguishable from a freshly crafted one or it would not stack.
    #[test]
    fn a_plain_pot_drops_a_plain_pot() {
        let world = placed_pot("decorated_pot_plain");
        let behavior = DecoratedPotBlock::new(&vanilla_blocks::DECORATED_POT);

        let drops = behavior
            .get_drops(
                vanilla_blocks::DECORATED_POT.default_state(),
                &BlockLootContext::new(&world, POT_POS),
            )
            .expect("a decorated pot overrides its own loot");

        let dropped = drops.into_iter().next().expect("the pot");
        assert_eq!(dropped.get(POT_DECORATIONS), Some(&PotDecorations::EMPTY));
        assert!(ItemStack::is_same_item_same_components(
            &dropped,
            &ItemStack::new(&vanilla_items::DECORATED_POT)
        ));
    }

    /// Cracked is the block's other half: broken with a tool rather than lifted
    /// with silk touch, the pot comes apart into the four pieces it was made
    /// of, blank sides included as the bricks they were.
    #[test]
    fn a_cracked_pot_shatters_into_its_four_pieces() {
        let world = placed_pot("decorated_pot_shatter");
        let behavior = DecoratedPotBlock::new(&vanilla_blocks::DECORATED_POT);
        place_with(&behavior, &world, &mut pot_item_with_sherds());

        let cracked = vanilla_blocks::DECORATED_POT
            .default_state()
            .set_value(CRACKED, true);
        let drops = behavior
            .get_drops(cracked, &BlockLootContext::new(&world, POT_POS))
            .expect("a decorated pot overrides its own loot");

        assert_eq!(drops.len(), 4, "a pot has four sides and gives back four");
        assert!(drops[0].is(&vanilla_items::ANGLER_POTTERY_SHERD));
        assert!(drops[1].is(&vanilla_items::BRICK));
        assert!(drops[2].is(&vanilla_items::ARCHER_POTTERY_SHERD));
        assert!(drops[3].is(&vanilla_items::BRICK));
        assert!(
            drops.iter().all(|drop| drop.count() == 1),
            "one of each side, not a stack of them"
        );
    }

    /// Which of the two drops happens is decided by the tool, and silk touch is
    /// the exception that saves the pot -- the whole reason for putting silk
    /// touch on a pickaxe before going near one.
    #[test]
    fn a_pickaxe_cracks_the_pot_and_silk_touch_does_not() {
        init_vanilla_registry();

        let bare_pickaxe = ItemStack::new(&vanilla_items::DIAMOND_PICKAXE);
        assert!(
            DecoratedPotBlock::shatters_the_pot(&bare_pickaxe),
            "a pickaxe is in `breaks_decorated_pots`"
        );

        let mut silk_touch = ItemStack::new(&vanilla_items::DIAMOND_PICKAXE);
        let mut enchantments = ItemEnchantments::empty();
        enchantments.set(vanilla_enchantments::SILK_TOUCH.key.clone(), 1);
        silk_touch.set(ENCHANTMENTS, enchantments);
        assert!(
            !DecoratedPotBlock::shatters_the_pot(&silk_touch),
            "silk touch is in `prevents_decorated_pot_shattering`"
        );

        let feather = ItemStack::new(&vanilla_items::FEATHER);
        assert!(
            !DecoratedPotBlock::shatters_the_pot(&feather),
            "a feather takes nothing apart"
        );
    }

    /// The pot's one slot is what a comparator behind it measures, so a full
    /// stack has to read fifteen and an empty pot nothing. Having exactly one
    /// slot is what makes a decorated pot a usable item counter.
    #[test]
    fn a_comparator_reads_the_single_slot() {
        let world = placed_pot("decorated_pot_comparator");
        let behavior = DecoratedPotBlock::new(&vanilla_blocks::DECORATED_POT);
        let state = vanilla_blocks::DECORATED_POT.default_state();
        let read =
            || behavior.get_analog_output_signal(state, world.as_ref(), POT_POS, Direction::North);

        assert_eq!(read(), 0, "an empty pot powers nothing");

        let block_entity = world
            .get_block_entity(POT_POS)
            .expect("a placed pot has a block entity");
        let pot = block_entity
            .downcast_ref::<DecoratedPotBlockEntity>()
            .expect("it is a decorated pot");

        pot.set_the_item(ItemStack::new(&vanilla_items::DIAMOND));
        assert_eq!(read(), 1, "one item is still a signal");

        pot.set_the_item(ItemStack::with_count(&vanilla_items::DIAMOND, 64));
        assert_eq!(read(), 15, "a full stack fills the only slot");
    }
}
