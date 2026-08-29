use std::sync::{Arc, Weak};

use foton_macros::block_behavior;
use foton_registry::block_entity_type::BlockEntityTypeRef;
use foton_registry::blocks::properties::{
    BlockStateProperties, BoolProperty, Direction, EnumProperty,
};
use foton_registry::blocks::{BlockRef, block_state_ext::BlockStateExt as _};
use foton_registry::fluid::FluidState;
use foton_registry::vanilla_block_tags::BlockTag;
use foton_registry::vanilla_damage_types;
use foton_registry::{
    REGISTRY, TaggedRegistryExt as _, sound_events, vanilla_block_entity_types, vanilla_blocks,
    vanilla_fluids, vanilla_game_events,
};
use foton_utils::types::InteractionHand;
use foton_utils::{BlockPos, BlockStateId, Downcast as _, types::UpdateFlags};

use crate::behavior::InventoryAccess;
use crate::behavior::block::{BlockEntityCreation, schedule_water_tick_if_waterlogged};
use crate::behavior::context::{BlockHitResult, InteractionResult};
use crate::block_entity::entities::CampfireBlockEntity;
use crate::block_entity::{BLOCK_ENTITIES, BlockEntityTicker};
use crate::player::Player;
use crate::{
    behavior::{BlockBehavior, BlockPlaceContext, block::schedule_placed_liquid_tick},
    entity::{Entity, InsideBlockEffectCollector, damage::DamageSource, projectile::Projectile},
    world::{
        ClipHitResult, LevelAccessor, LevelReader as _, ScheduledTickAccess, World,
        game_event::GameEventContext,
    },
};

/// How far below a campfire its smoke still reaches.
///
/// Vanilla parity: the `i <= 5` of `CampfireBlock.isSmokeyPos`.
const SMOKE_REACH: i32 = 5;

/// Returns whether smoke from a campfire reaches `pos`.
///
/// Vanilla parity: `CampfireBlock.isSmokeyPos`. This is what lets a player
/// harvest a hive without the bees turning on them, and it is the whole reason
/// beekeepers put a campfire under the hive.
///
/// Deviation: vanilla stops the search at a block whose collision shape
/// intersects the thin slab just under the hive, then looks one block further.
/// Foton has no shape-intersection helper reachable from here, so a full
/// collision block stands in -- which agrees with vanilla for every block a
/// player would actually put there, and differs only for shapes that are solid
/// at the top and open at the bottom.
#[must_use]
pub(crate) fn is_smokey_pos(world: &Arc<World>, pos: BlockPos) -> bool {
    for step in 1..=SMOKE_REACH {
        let below = BlockPos::new(pos.x(), pos.y() - step, pos.z());
        let state = world.get_block_state(below);
        if is_lit_campfire(state) {
            return true;
        }
        if world.is_collision_shape_full_block_at(below, state) {
            let further = BlockPos::new(below.x(), below.y() - 1, below.z());
            return is_lit_campfire(world.get_block_state(further));
        }
    }
    false
}

/// Vanilla parity: `CampfireBlock.isLitCampfire`.
#[must_use]
pub(crate) fn is_lit_campfire(state: BlockStateId) -> bool {
    REGISTRY
        .blocks
        .is_in_tag(state.get_block(), &BlockTag::CAMPFIRES)
        && state.get_value(&BlockStateProperties::LIT)
}

/// Behavior for campfires and soul campfires.
///
/// Smoke and crackle particles are vanilla `animateTick`, which is client-only
/// and has no server counterpart.
#[block_behavior]
pub struct CampfireBlock {
    block: BlockRef,
    #[json_arg(value, json = "spawn_particles")]
    _spawn_particles: bool,
    #[json_arg(value, json = "fire_damage")]
    fire_damage: i32,
}

const HORIZONTAL_FACING: &EnumProperty<Direction> = &BlockStateProperties::HORIZONTAL_FACING;
const LIT: &BoolProperty = &BlockStateProperties::LIT;
const SIGNAL_FIRE: &BoolProperty = &BlockStateProperties::SIGNAL_FIRE;
const WATERLOGGED: &BoolProperty = &BlockStateProperties::WATERLOGGED;

impl CampfireBlock {
    /// Creates a campfire block behavior.
    #[must_use]
    pub const fn new(block: BlockRef, spawn_particles: bool, fire_damage: i32) -> Self {
        Self {
            block,
            _spawn_particles: spawn_particles,
            fire_damage,
        }
    }

    #[must_use]
    fn contact_damage_amount(&self, state: BlockStateId, is_living_entity: bool) -> Option<f32> {
        if state.get_value(LIT) && is_living_entity {
            Some(self.fire_damage as f32)
        } else {
            None
        }
    }

    fn is_smoke_source(state: BlockStateId) -> bool {
        state.get_block() == &vanilla_blocks::HAY_BLOCK
    }

    fn placement_state(
        &self,
        waterlogged: bool,
        below_state: BlockStateId,
        facing: Direction,
    ) -> BlockStateId {
        self.block
            .default_state()
            .set_value(WATERLOGGED, waterlogged)
            .set_value(SIGNAL_FIRE, Self::is_smoke_source(below_state))
            .set_value(LIT, !waterlogged)
            .set_value(HORIZONTAL_FACING, facing)
    }

    fn projectile_lit_state(
        state: BlockStateId,
        projectile_is_on_fire: bool,
        may_interact: bool,
    ) -> Option<BlockStateId> {
        (projectile_is_on_fire
            && may_interact
            && !state.get_value(LIT)
            && !state.get_value(WATERLOGGED))
        .then(|| state.set_value(LIT, true))
    }
}

impl BlockBehavior for CampfireBlock {
    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        let waterlogged = context.is_water_source();
        let below_state = context.world.get_block_state(context.place_pos().below());
        Some(self.placement_state(waterlogged, below_state, context.horizontal_direction()))
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

        if direction == Direction::Down {
            state.set_value(SIGNAL_FIRE, Self::is_smoke_source(neighbor_state))
        } else {
            state
        }
    }

    fn on_projectile_hit(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        hit: &ClipHitResult,
        projectile: &dyn Projectile,
    ) {
        let Some(lit_state) = Self::projectile_lit_state(
            state,
            projectile.is_on_fire(),
            projectile.projectile_may_interact(world, hit.block_pos),
        ) else {
            return;
        };
        world.set_block(hit.block_pos, lit_state, UpdateFlags::UPDATE_ALL_IMMEDIATE);
    }

    fn entity_inside(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        entity: &dyn Entity,
        effect_collector: &mut InsideBlockEffectCollector,
        is_precise: bool,
    ) {
        if let Some(damage) = self.contact_damage_amount(state, entity.is_living_entity()) {
            entity.hurt(
                world,
                &DamageSource::environment(&vanilla_damage_types::CAMPFIRE),
                damage,
            );
        }

        self.default_entity_inside(state, world, pos, entity, effect_collector, is_precise);
    }

    fn new_block_entity(
        &self,
        level: Weak<World>,
        pos: BlockPos,
        state: BlockStateId,
    ) -> BlockEntityCreation {
        BlockEntityCreation::from_registered_factory(BLOCK_ENTITIES.create(
            &vanilla_block_entity_types::CAMPFIRE,
            level,
            pos,
            state,
        ))
    }

    /// Vanilla parity: `CampfireBlock.getTicker`, which picks the callback from
    /// the live state rather than branching inside the block entity. A lit
    /// campfire cooks; an unlit one walks its progress back down.
    ///
    /// The selection is re-run whenever the state changes, so dousing a
    /// campfire swaps the ticker on the same tick the `lit` property flips.
    fn get_block_entity_ticker(
        &self,
        _world: &Arc<World>,
        state: BlockStateId,
        block_entity_type: BlockEntityTypeRef,
    ) -> Option<BlockEntityTicker> {
        let tick = if state.get_value(LIT) {
            CampfireBlockEntity::cook_tick
        } else {
            CampfireBlockEntity::cooldown_tick
        };
        BlockEntityTicker::for_matching_tick(
            block_entity_type,
            &vanilla_block_entity_types::CAMPFIRE,
            tick,
        )
    }

    /// Vanilla parity: `CampfireBlock.useItemOn`.
    ///
    /// The `awardStat(INTERACT_WITH_CAMPFIRE)` of vanilla has no counterpart:
    /// Foton has no statistics system.
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
            return InteractionResult::TryEmptyHandInteraction;
        };
        let Some(campfire) = block_entity.downcast_ref::<CampfireBlockEntity>() else {
            return InteractionResult::TryEmptyHandInteraction;
        };

        // Vanilla tests `RecipePropertySet.CAMPFIRE_INPUT`, which is built from
        // exactly the campfire recipes' ingredients, so the recipe lookup is
        // the same question. Anything else falls through to the empty-hand
        // interaction rather than being swallowed.
        let held = inv.with_item(|item| item.clone());
        if REGISTRY.recipes.find_campfire_recipe(&held).is_none() {
            return InteractionResult::TryEmptyHandInteraction;
        }

        if !campfire.place_food(world, Some(player), &held) {
            return InteractionResult::Consume;
        }

        // Vanilla's `consumeAndReturn` shrinks the held stack unless the holder
        // has infinite materials; the block entity only took a copy.
        if !player.has_infinite_materials() {
            inv.with_item(|item| item.shrink(1));
        }
        InteractionResult::SuccessServer
    }

    fn place_liquid(
        &self,
        level: &dyn LevelAccessor,
        pos: BlockPos,
        state: BlockStateId,
        fluid_state: FluidState,
    ) -> bool {
        if state.try_get_value(WATERLOGGED) != Some(false)
            || fluid_state.fluid_id != &vanilla_fluids::WATER
        {
            return false;
        }

        if state.get_value(LIT) {
            level.play_block_sound(
                &sound_events::ENTITY_GENERIC_EXTINGUISH_FIRE,
                pos,
                1.0,
                1.0,
                None,
            );
            level.game_event(
                &vanilla_game_events::BLOCK_CHANGE,
                pos,
                &GameEventContext::new(None, Some(state.set_value(LIT, false))),
            );
        }

        level.set_block_state(
            pos,
            state.set_value(WATERLOGGED, true).set_value(LIT, false),
            UpdateFlags::UPDATE_ALL,
        );
        schedule_placed_liquid_tick(level, pos, fluid_state);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestLevel;
    use foton_registry::{
        blocks::block_state_ext::BlockStateExt, init_vanilla_registry, vanilla_blocks,
    };

    #[test]
    fn lit_campfire_damages_living_entities() {
        init_vanilla_registry();
        let campfire = CampfireBlock::new(&vanilla_blocks::CAMPFIRE, true, 1);
        let state = vanilla_blocks::CAMPFIRE
            .default_state()
            .set_value(LIT, true);

        assert_eq!(campfire.contact_damage_amount(state, true), Some(1.0));
    }

    #[test]
    fn unlit_campfire_does_not_damage_entities() {
        init_vanilla_registry();
        let campfire = CampfireBlock::new(&vanilla_blocks::CAMPFIRE, true, 1);
        let state = vanilla_blocks::CAMPFIRE
            .default_state()
            .set_value(LIT, false);

        assert_eq!(campfire.contact_damage_amount(state, true), None);
    }

    #[test]
    fn campfire_does_not_damage_non_living_entities() {
        init_vanilla_registry();
        let campfire = CampfireBlock::new(&vanilla_blocks::SOUL_CAMPFIRE, false, 2);
        let state = vanilla_blocks::SOUL_CAMPFIRE
            .default_state()
            .set_value(LIT, true);

        assert_eq!(campfire.contact_damage_amount(state, false), None);
    }

    #[test]
    fn burning_projectile_lights_only_dry_unlit_campfires() {
        init_vanilla_registry();

        let unlit = vanilla_blocks::CAMPFIRE
            .default_state()
            .set_value(LIT, false)
            .set_value(WATERLOGGED, false);
        let lit = unlit.set_value(LIT, true);
        let waterlogged = unlit.set_value(WATERLOGGED, true);

        assert_eq!(
            CampfireBlock::projectile_lit_state(unlit, true, true),
            Some(lit)
        );
        assert_eq!(
            CampfireBlock::projectile_lit_state(unlit, false, true),
            None
        );
        assert_eq!(
            CampfireBlock::projectile_lit_state(unlit, true, false),
            None
        );
        assert_eq!(CampfireBlock::projectile_lit_state(lit, true, true), None);
        assert_eq!(
            CampfireBlock::projectile_lit_state(waterlogged, true, true),
            None
        );
    }

    #[test]
    fn placement_state_sets_facing_and_signal_fire() {
        init_vanilla_registry();
        let campfire = CampfireBlock::new(&vanilla_blocks::CAMPFIRE, true, 1);

        let state = campfire.placement_state(
            false,
            vanilla_blocks::HAY_BLOCK.default_state(),
            Direction::East,
        );

        assert_eq!(state.get_value(HORIZONTAL_FACING), Direction::East);
        assert!(state.get_value(SIGNAL_FIRE));
        assert!(state.get_value(LIT));
        assert!(!state.get_value(WATERLOGGED));
    }

    #[test]
    fn update_shape_recomputes_signal_fire_from_below() {
        init_vanilla_registry();
        let campfire = CampfireBlock::new(&vanilla_blocks::CAMPFIRE, true, 1);
        let level = TestLevel::default();
        let state = vanilla_blocks::CAMPFIRE
            .default_state()
            .set_value(SIGNAL_FIRE, false)
            .set_value(WATERLOGGED, false);

        let updated = campfire.update_shape(
            state,
            &level,
            BlockPos::ZERO,
            Direction::Down,
            BlockPos::ZERO.below(),
            vanilla_blocks::HAY_BLOCK.default_state(),
        );

        assert!(updated.get_value(SIGNAL_FIRE));
    }

    #[test]
    fn water_placement_extinguishes_lit_campfire() {
        init_vanilla_registry();
        let level = TestLevel::default();
        let campfire = CampfireBlock::new(&vanilla_blocks::CAMPFIRE, true, 1);
        let state = vanilla_blocks::CAMPFIRE
            .default_state()
            .set_value(LIT, true)
            .set_value(WATERLOGGED, false);
        let pos = BlockPos::new(1, 2, 3);

        assert!(campfire.place_liquid(
            &level,
            pos,
            state,
            FluidState::source(&vanilla_fluids::WATER),
        ));

        let placed = level
            .last_placed_state()
            .expect("campfire should be updated");
        assert!(!placed.get_value(LIT));
        assert!(placed.get_value(WATERLOGGED));
        assert_eq!(
            level
                .block_sounds
                .borrow()
                .iter()
                .map(|sound| sound.sound)
                .collect::<Vec<_>>(),
            vec![&sound_events::ENTITY_GENERIC_EXTINGUISH_FIRE]
        );
        assert_eq!(
            level
                .scheduled_fluid_ticks
                .borrow()
                .iter()
                .map(|tick| (tick.pos, tick.fluid, tick.delay))
                .collect::<Vec<_>>(),
            vec![(pos, &vanilla_fluids::WATER, 5)]
        );
        assert_eq!(
            level
                .game_events
                .borrow()
                .iter()
                .map(|event| event.event)
                .collect::<Vec<_>>(),
            vec![&vanilla_game_events::BLOCK_CHANGE]
        );
    }
}
