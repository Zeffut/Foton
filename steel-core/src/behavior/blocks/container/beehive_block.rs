//! Beehive block behavior implementation.

use std::sync::{Arc, Weak};

use steel_macros::block_behavior;
use steel_registry::block_entity_type::BlockEntityTypeRef;
use steel_registry::blocks::BlockRef;
use steel_registry::blocks::block_state_ext::BlockStateExt;
use steel_registry::blocks::properties::{
    BlockStateProperties, Direction, EnumProperty, IntProperty,
};
use steel_registry::item_stack::ItemStack;
use steel_registry::{
    sound_events, vanilla_block_entity_types, vanilla_game_events, vanilla_items,
};
use steel_utils::types::{InteractionHand, UpdateFlags};
use steel_utils::{BlockPos, BlockStateId, Downcast as _, WorldAabb};

use crate::behavior::InventoryAccess;
use crate::behavior::block::{BlockBehavior, BlockEntityCreation};
use crate::behavior::blocks::building::campfire_block::is_smokey_pos;
use crate::behavior::context::{BlockHitResult, BlockPlaceContext, InteractionResult};
use crate::block_entity::entities::{BeeReleaseStatus, BeehiveBlockEntity};
use crate::block_entity::{BLOCK_ENTITIES, BlockEntityTicker};
use crate::entity::Mob;
use crate::entity::entities::BeeEntity;
use crate::player::Player;
use crate::world::game_event::GameEventContext;
use crate::world::{LevelReader, World};

/// The honey level at which a hive can be harvested.
///
/// Vanilla parity: the `honeyLevel >= 5` of `BeehiveBlock.useItemOn`.
const FULL_HONEY_LEVEL: u8 = 5;

/// How much honeycomb a full hive gives.
///
/// Vanilla parity: the `harvest_beehive` loot table, which is a flat three.
/// Steel has no loot tables, so the number is written out.
const HONEYCOMB_PER_HARVEST: i32 = 3;

/// How far around a broken or harvested hive its bees are roused.
///
/// Vanilla parity: the `new AABB(pos).inflate(8.0, 6.0, 8.0)` of
/// `BeehiveBlock.angerNearbyBees`.
const ANGER_RANGE_XZ: f64 = 8.0;
/// The vertical half of that box.
const ANGER_RANGE_Y: f64 = 6.0;

/// Behavior for beehive and bee nest blocks.
#[block_behavior]
pub struct BeehiveBlock {
    block: BlockRef,
}

const HORIZONTAL_FACING: &EnumProperty<Direction> = &BlockStateProperties::HORIZONTAL_FACING;
const LEVEL_HONEY: &IntProperty = &BlockStateProperties::LEVEL_HONEY;

impl BeehiveBlock {
    /// Creates a new beehive block behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }

    /// Empties the hive and drops its honey level back to zero.
    ///
    /// Vanilla parity: `BeehiveBlock.releaseBeesAndResetHoneyLevel`.
    fn release_bees_and_reset_honey_level(
        world: &Arc<World>,
        state: BlockStateId,
        pos: BlockPos,
        player: Option<&Player>,
        release_status: BeeReleaseStatus,
    ) {
        Self::reset_honey_level(world, state, pos);
        let Some(block_entity) = world.get_block_entity(pos) else {
            return;
        };
        let Some(hive) = block_entity.downcast_ref::<BeehiveBlockEntity>() else {
            return;
        };
        hive.empty_all_living_from_hive(player, state, release_status);
    }

    /// Vanilla parity: `BeehiveBlock.resetHoneyLevel`.
    fn reset_honey_level(world: &Arc<World>, state: BlockStateId, pos: BlockPos) {
        world.set_block(
            pos,
            state.set_value(LEVEL_HONEY, 0),
            UpdateFlags::UPDATE_ALL,
        );
    }

    /// Vanilla parity: `BeehiveBlock.hiveContainsBees`.
    fn hive_contains_bees(world: &Arc<World>, pos: BlockPos) -> bool {
        world.get_block_entity(pos).is_some_and(|block_entity| {
            block_entity
                .downcast_ref::<BeehiveBlockEntity>()
                .is_some_and(|hive| !hive.is_empty())
        })
    }

    /// Turns every bee within eight blocks on a random nearby player.
    ///
    /// Vanilla parity: `BeehiveBlock.angerNearbyBees`. This is the half of the
    /// harvest that reaches bees already out flying, as opposed to the ones the
    /// hive itself lets out.
    fn anger_nearby_bees(world: &Arc<World>, pos: BlockPos) {
        let area = WorldAabb::new(
            f64::from(pos.x()),
            f64::from(pos.y()),
            f64::from(pos.z()),
            f64::from(pos.x() + 1),
            f64::from(pos.y() + 1),
            f64::from(pos.z() + 1),
        )
        .inflate_xyz(ANGER_RANGE_XZ, ANGER_RANGE_Y, ANGER_RANGE_XZ);

        let bees = world.get_entities_in_aabb_matching(&area, |entity| {
            entity.downcast_ref::<BeeEntity>().is_some()
        });
        if bees.is_empty() {
            return;
        }
        let players =
            world.get_entities_in_aabb_matching(&area, |entity| entity.as_player().is_some());
        if players.is_empty() {
            return;
        }

        for bee_entity in bees {
            let Some(bee) = bee_entity.downcast_ref::<BeeEntity>() else {
                continue;
            };
            if bee.target().is_some() {
                continue;
            }
            let index = rand::random_range(0..players.len());
            bee.set_target(Some(&players[index]));
        }
    }
}

impl BlockBehavior for BeehiveBlock {
    /// Vanilla parity: `BeehiveBlock.useItemOn`. A full hive gives honeycomb to
    /// shears and a honey bottle to a glass bottle, and empties either way.
    ///
    /// Whether the harvester gets away with it is decided by the campfire below:
    /// with smoke the hive merely loses its honey, without it every bee in range
    /// turns on the player and the hive empties itself as an emergency.
    fn use_item_on(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        player: &Player,
        _hand: InteractionHand,
        _hit_result: &BlockHitResult,
        inv: &mut InventoryAccess,
    ) -> InteractionResult {
        if state.get_value(LEVEL_HONEY) < FULL_HONEY_LEVEL {
            return InteractionResult::TryEmptyHandInteraction;
        }

        let held = inv.with_item(|item| item.copy_with_count(item.count()));
        let emptied = if held.is(&vanilla_items::SHEARS) {
            for _ in 0..HONEYCOMB_PER_HARVEST {
                world.drop_item_stack(pos, ItemStack::new(&vanilla_items::HONEYCOMB));
            }
            world.play_block_sound(&sound_events::BLOCK_BEEHIVE_SHEAR, pos, 1.0, 1.0, None);
            if !player.has_infinite_materials() {
                inv.with_item(|item| item.hurt_and_break(1, false));
            }
            world.game_event(
                &vanilla_game_events::SHEAR,
                pos,
                &GameEventContext::new(Some(player), None),
            );
            true
        } else if held.is(&vanilla_items::GLASS_BOTTLE) {
            inv.with_item(|item| item.shrink(1));
            player.add_item_or_drop(ItemStack::new(&vanilla_items::HONEY_BOTTLE));
            world.play_block_sound(&sound_events::ITEM_BOTTLE_FILL, pos, 1.0, 1.0, None);
            world.game_event(
                &vanilla_game_events::FLUID_PICKUP,
                pos,
                &GameEventContext::new(Some(player), None),
            );
            true
        } else {
            false
        };

        if !emptied {
            return InteractionResult::TryEmptyHandInteraction;
        }

        // TODO: Award stat ITEM_USED; Steel has no statistics registry.
        if is_smokey_pos(world, pos) {
            Self::reset_honey_level(world, state, pos);
            return InteractionResult::Success;
        }

        if Self::hive_contains_bees(world, pos) {
            Self::anger_nearby_bees(world, pos);
        }
        Self::release_bees_and_reset_honey_level(
            world,
            state,
            pos,
            Some(player),
            BeeReleaseStatus::Emergency,
        );
        InteractionResult::Success
    }

    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        Some(
            self.block
                .default_state()
                .set_value(HORIZONTAL_FACING, context.horizontal_direction().opposite()),
        )
    }

    fn new_block_entity(
        &self,
        level: Weak<World>,
        pos: BlockPos,
        state: BlockStateId,
    ) -> BlockEntityCreation {
        BlockEntityCreation::from_registered_factory(BLOCK_ENTITIES.create(
            &vanilla_block_entity_types::BEEHIVE,
            level,
            pos,
            state,
        ))
    }

    /// Vanilla parity: the ticker `BeehiveBlock.getTicker` installs, which is
    /// what counts an occupant's stay down and lets it back out.
    fn get_block_entity_ticker(
        &self,
        _world: &Arc<World>,
        _state: BlockStateId,
        block_entity_type: BlockEntityTypeRef,
    ) -> Option<BlockEntityTicker> {
        BlockEntityTicker::for_matching_entity_tick(
            block_entity_type,
            &vanilla_block_entity_types::BEEHIVE,
        )
    }

    fn has_analog_output_signal(&self, _state: BlockStateId) -> bool {
        true
    }

    fn get_analog_output_signal(
        &self,
        state: BlockStateId,
        _world: &dyn LevelReader,
        _pos: BlockPos,
        _direction: Direction,
    ) -> i32 {
        state.get_value(LEVEL_HONEY).into()
    }
}

#[cfg(test)]
mod tests {
    use steel_registry::{init_vanilla_registry, vanilla_blocks};

    use super::*;
    use crate::test_support::fresh_test_world;

    #[test]
    fn a_beehive_asks_for_a_ticker_so_its_occupants_can_come_back_out() {
        // Without this the block entity never ticks, so a bee that went in stays
        // in forever and the hive never gains honey. Nothing else notices: the
        // hive still stores, saves and loads its occupants perfectly well.
        init_vanilla_registry();
        let world = fresh_test_world("beehive_ticker");
        let behavior = BeehiveBlock::new(&vanilla_blocks::BEEHIVE);

        let ticker = behavior.get_block_entity_ticker(
            &world,
            vanilla_blocks::BEEHIVE.default_state(),
            &vanilla_block_entity_types::BEEHIVE,
        );

        assert!(ticker.is_some());
    }
}
