//! Carved pumpkin behavior.
//!
//! Vanilla parity: `CarvedPumpkinBlock`. Placing the head is the last move in
//! building a golem, so this block owns the three golem patterns and the code
//! that swaps the blocks for the mob.

use std::sync::{Arc, LazyLock};

use glam::DVec3;
use steel_macros::block_behavior;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::blocks::properties::{
    BlockStateProperties, ChestType, Direction, EnumProperty,
};
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::vanilla_block_tags::BlockTag;
use steel_registry::{blocks::BlockRef, level_events, vanilla_blocks, vanilla_entities};
use steel_utils::types::UpdateFlags;
use steel_utils::{BlockPos, BlockStateId, Downcast as _};

use crate::behavior::block::BlockBehavior;
use crate::behavior::blocks::WeatherState;
use crate::behavior::context::BlockPlaceContext;
use crate::behavior::waxables::get_normal_from_waxed_variant;
use crate::behavior::weathering::get_weather_state;
use crate::entity::entities::{CopperGolemEntity, IronGolemEntity};
use crate::entity::{ENTITIES, SharedEntity, next_entity_id};
use crate::world::block_pattern::{
    BlockPattern, BlockPatternBuilder, BlockPatternMatch, has_state,
};
use crate::world::{LevelReader, World};

const HORIZONTAL_FACING: &EnumProperty<Direction> = &BlockStateProperties::HORIZONTAL_FACING;
const CHEST_FACING: &EnumProperty<Direction> = &BlockStateProperties::HORIZONTAL_FACING;
const CHEST_TYPE: &EnumProperty<ChestType> = &BlockStateProperties::CHEST_TYPE;

/// Returns whether a state is one of the two pumpkin heads a golem accepts.
///
/// Vanilla parity: `CarvedPumpkinBlock.PUMPKINS_PREDICATE`.
fn is_golem_head(state: BlockStateId) -> bool {
    let block = state.get_block();
    block == &vanilla_blocks::CARVED_PUMPKIN || block == &vanilla_blocks::JACK_O_LANTERN
}

/// Vanilla parity: `CarvedPumpkinBlock.getOrCreateSnowGolemBase`.
static SNOW_GOLEM_BASE: LazyLock<BlockPattern> = LazyLock::new(|| {
    BlockPatternBuilder::start()
        .aisle(&[" ", "#", "#"])
        .where_char(
            '#',
            has_state(|state| state.get_block() == &vanilla_blocks::SNOW_BLOCK),
        )
        .build()
});

/// Vanilla parity: `CarvedPumpkinBlock.getOrCreateSnowGolemFull`.
static SNOW_GOLEM_FULL: LazyLock<BlockPattern> = LazyLock::new(|| {
    BlockPatternBuilder::start()
        .aisle(&["^", "#", "#"])
        .where_char('^', has_state(is_golem_head))
        .where_char(
            '#',
            has_state(|state| state.get_block() == &vanilla_blocks::SNOW_BLOCK),
        )
        .build()
});

/// Vanilla parity: `CarvedPumpkinBlock.getOrCreateIronGolemBase`.
static IRON_GOLEM_BASE: LazyLock<BlockPattern> = LazyLock::new(|| {
    BlockPatternBuilder::start()
        .aisle(&["~ ~", "###", "~#~"])
        .where_char(
            '#',
            has_state(|state| state.get_block() == &vanilla_blocks::IRON_BLOCK),
        )
        .where_char('~', has_state(|state| state.is_air()))
        .build()
});

/// Vanilla parity: `CarvedPumpkinBlock.getOrCreateIronGolemFull`.
static IRON_GOLEM_FULL: LazyLock<BlockPattern> = LazyLock::new(|| {
    BlockPatternBuilder::start()
        .aisle(&["~^~", "###", "~#~"])
        .where_char('^', has_state(is_golem_head))
        .where_char(
            '#',
            has_state(|state| state.get_block() == &vanilla_blocks::IRON_BLOCK),
        )
        .where_char('~', has_state(|state| state.is_air()))
        .build()
});

/// Vanilla parity: `CarvedPumpkinBlock.getOrCreateCopperGolemBase`.
static COPPER_GOLEM_BASE: LazyLock<BlockPattern> = LazyLock::new(|| {
    BlockPatternBuilder::start()
        .aisle(&[" ", "#"])
        .where_char(
            '#',
            has_state(|state| state.get_block().has_tag(&BlockTag::COPPER)),
        )
        .build()
});

/// Vanilla parity: `CarvedPumpkinBlock.getOrCreateCopperGolemFull`.
static COPPER_GOLEM_FULL: LazyLock<BlockPattern> = LazyLock::new(|| {
    BlockPatternBuilder::start()
        .aisle(&["^", "#"])
        .where_char('^', has_state(is_golem_head))
        .where_char(
            '#',
            has_state(|state| state.get_block().has_tag(&BlockTag::COPPER)),
        )
        .build()
});

/// Carved pumpkin.
#[block_behavior]
pub struct CarvedPumpkinBlock {
    block: BlockRef,
}

impl CarvedPumpkinBlock {
    /// Creates the carved pumpkin behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }

    /// Returns whether a golem's body is standing under `top_pos`.
    ///
    /// Vanilla parity: `CarvedPumpkinBlock.canSpawnGolem`.
    #[must_use]
    pub fn can_spawn_golem(level: &dyn LevelReader, top_pos: BlockPos) -> bool {
        SNOW_GOLEM_BASE.find(level, top_pos).is_some()
            || IRON_GOLEM_BASE.find(level, top_pos).is_some()
            || COPPER_GOLEM_BASE.find(level, top_pos).is_some()
    }

    /// Turns a finished golem frame into the golem.
    ///
    /// Vanilla parity: `CarvedPumpkinBlock.trySpawnGolem`.
    fn try_spawn_golem(world: &Arc<World>, top_pos: BlockPos) {
        if let Some(found) = SNOW_GOLEM_FULL.find(world.as_ref(), top_pos) {
            let spawn_pos = found.block(0, 2, 0).pos();
            if let Some(golem) = create_golem(world, &vanilla_entities::SNOW_GOLEM, spawn_pos) {
                spawn_golem_in_world(world, &found, &golem);
            }
            return;
        }

        if let Some(found) = IRON_GOLEM_FULL.find(world.as_ref(), top_pos) {
            let spawn_pos = found.block(1, 2, 0).pos();
            if let Some(golem) = create_golem(world, &vanilla_entities::IRON_GOLEM, spawn_pos) {
                if let Some(iron_golem) = golem.downcast_ref::<IronGolemEntity>() {
                    iron_golem.set_player_created(true);
                }
                spawn_golem_in_world(world, &found, &golem);
            }
            return;
        }

        if let Some(found) = COPPER_GOLEM_FULL.find(world.as_ref(), top_pos) {
            let spawn_pos = found.block(0, 0, 0).pos();
            let Some(golem) = create_golem(world, &vanilla_entities::COPPER_GOLEM, spawn_pos)
            else {
                return;
            };
            let weather_state = weather_state_from_pattern(&found);
            let chest_pos = found.block(0, 1, 0).pos();
            let copper_block = found.block(0, 1, 0).state().get_block();
            let facing = found.block(0, 0, 0).state().get_value(HORIZONTAL_FACING);

            spawn_golem_in_world(world, &found, &golem);
            replace_copper_block_with_chest(world, chest_pos, copper_block, facing);
            if let Some(copper_golem) = golem.downcast_ref::<CopperGolemEntity>() {
                copper_golem.spawn(weather_state);
            }
        }
    }

    /// Clears every block a golem was built from.
    ///
    /// Vanilla parity: `CarvedPumpkinBlock.clearPatternBlocks`, which the
    /// wither's summon also calls.
    pub fn clear_pattern_blocks(world: &Arc<World>, found: &BlockPatternMatch<'_>) {
        for x in 0..found.width() {
            for y in 0..found.height() {
                let block = found.block(x as i32, y as i32, 0);
                let pos = block.pos();
                let old_state = block.state();
                world.set_block(
                    pos,
                    vanilla_blocks::AIR.default_state(),
                    UpdateFlags::UPDATE_CLIENTS,
                );
                world.level_event(
                    level_events::PARTICLES_DESTROY_BLOCK,
                    pos,
                    level_events::encode_block_state_data(u32::from(old_state.0)),
                    None,
                );
            }
        }
    }

    /// Tells the neighbours of a cleared golem frame that it is gone.
    ///
    /// Vanilla parity: `CarvedPumpkinBlock.updatePatternBlocks`.
    pub fn update_pattern_blocks(world: &Arc<World>, found: &BlockPatternMatch<'_>) {
        for x in 0..found.width() {
            for y in 0..found.height() {
                let pos = found.block(x as i32, y as i32, 0).pos();
                world.update_neighbors_at(pos, &vanilla_blocks::AIR);
            }
        }
    }
}

/// Builds the golem where its feet will be, without putting it in the world yet.
///
/// Vanilla creates the entity and then calls `snapTo`; Steel's entity factory
/// takes the position, so the two steps collapse into one.
fn create_golem(
    world: &Arc<World>,
    entity_type: EntityTypeRef,
    spawn_pos: BlockPos,
) -> Option<SharedEntity> {
    let position = DVec3::new(
        f64::from(spawn_pos.x()) + 0.5,
        f64::from(spawn_pos.y()) + 0.05,
        f64::from(spawn_pos.z()) + 0.5,
    );
    ENTITIES.create(
        entity_type,
        next_entity_id(),
        position,
        Arc::downgrade(world),
    )
}

/// Swaps the frame for the golem.
///
/// Vanilla parity: `CarvedPumpkinBlock.spawnGolemInWorld`. The advancement
/// trigger vanilla fires here has no equivalent in Steel yet.
fn spawn_golem_in_world(world: &Arc<World>, found: &BlockPatternMatch<'_>, golem: &SharedEntity) {
    CarvedPumpkinBlock::clear_pattern_blocks(world, found);
    golem.set_rotation((0.0, 0.0));
    if let Err(error) = world.try_add_entity(Arc::clone(golem)) {
        log::debug!("golem could not be built: {error}");
    }
    CarvedPumpkinBlock::update_pattern_blocks(world, found);
}

/// Returns how oxidized the copper the golem was built from is.
///
/// Vanilla parity: `CarvedPumpkinBlock.getWeatherStateFromPattern`, including
/// its fall back through `HoneycombItem.WAX_OFF_BY_BLOCK` for waxed copper.
fn weather_state_from_pattern(found: &BlockPatternMatch<'_>) -> WeatherState {
    let block = found.block(0, 1, 0).state().get_block();
    get_weather_state(block)
        .or_else(|| get_normal_from_waxed_variant(block).and_then(get_weather_state))
        .unwrap_or(WeatherState::Unaffected)
}

/// Puts a copper chest where the copper block the golem was built from stood.
///
/// Vanilla parity: `CarvedPumpkinBlock.replaceCopperBlockWithChest` together
/// with `CopperChestBlock.getFromCopperBlock`.
///
/// Steel has no `CopperChestBlock` behavior yet, so the two parts of vanilla's
/// call that depend on one — `getChestType`, which pairs the new chest with an
/// adjacent copper chest, and `getLeastOxidizedChestOfConnectedBlocks`, which
/// levels the pair's oxidation — are left out. No copper chest can be adjacent
/// while the block has no behavior to place one, so the outcome only differs
/// for a chest put there with `/setblock`.
fn replace_copper_block_with_chest(
    world: &Arc<World>,
    pos: BlockPos,
    copper_block: BlockRef,
    facing: Direction,
) {
    let chest = copper_chest_for(copper_block);
    let state = chest
        .default_state()
        .set_value(CHEST_FACING, facing)
        .set_value(CHEST_TYPE, ChestType::Single);
    world.set_block(pos, state, UpdateFlags::UPDATE_CLIENTS);
}

/// Maps a copper block to the copper chest it becomes.
///
/// Vanilla parity: `CopperChestBlock.COPPER_TO_COPPER_CHEST_MAPPING`, which
/// zips the copper block family onto the copper chest family; anything else in
/// the `copper` tag falls back to the unoxidized chest.
fn copper_chest_for(copper_block: BlockRef) -> BlockRef {
    if copper_block == &vanilla_blocks::EXPOSED_COPPER {
        &vanilla_blocks::EXPOSED_COPPER_CHEST
    } else if copper_block == &vanilla_blocks::WEATHERED_COPPER {
        &vanilla_blocks::WEATHERED_COPPER_CHEST
    } else if copper_block == &vanilla_blocks::OXIDIZED_COPPER {
        &vanilla_blocks::OXIDIZED_COPPER_CHEST
    } else if copper_block == &vanilla_blocks::WAXED_COPPER_BLOCK {
        &vanilla_blocks::WAXED_COPPER_CHEST
    } else if copper_block == &vanilla_blocks::WAXED_EXPOSED_COPPER {
        &vanilla_blocks::WAXED_EXPOSED_COPPER_CHEST
    } else if copper_block == &vanilla_blocks::WAXED_WEATHERED_COPPER {
        &vanilla_blocks::WAXED_WEATHERED_COPPER_CHEST
    } else if copper_block == &vanilla_blocks::WAXED_OXIDIZED_COPPER {
        &vanilla_blocks::WAXED_OXIDIZED_COPPER_CHEST
    } else {
        &vanilla_blocks::COPPER_CHEST
    }
}

impl BlockBehavior for CarvedPumpkinBlock {
    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        Some(
            self.block
                .default_state()
                .set_value(HORIZONTAL_FACING, context.horizontal_direction().opposite()),
        )
    }

    /// Vanilla parity: `CarvedPumpkinBlock.onPlace`.
    fn on_place(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        old_state: BlockStateId,
        _moved_by_piston: bool,
    ) {
        if old_state.get_block() == state.get_block() {
            return;
        }
        Self::try_spawn_golem(world, pos);
    }
}

#[cfg(test)]
mod tests {
    use steel_registry::{init_vanilla_registry, vanilla_entities};
    use steel_utils::{ChunkPos, WorldAabb};

    use super::*;
    use crate::behavior::init_behaviors;
    use crate::entity::init_entities;
    use crate::test_support::{fresh_test_world, insert_ready_full_chunk};

    /// The block the golem's feet end up on, which is the spawn coordinate the
    /// test world is happy with.
    const FEET: BlockPos = BlockPos::new(8, 64, 8);

    fn prepared_world(key: &'static str) -> Arc<World> {
        init_vanilla_registry();
        init_behaviors();
        init_entities();
        let world = fresh_test_world(key);
        insert_ready_full_chunk(&world, ChunkPos::new(0, 0));
        world
    }

    fn place_iron_body(world: &Arc<World>, with_both_arms: bool) {
        let iron = vanilla_blocks::IRON_BLOCK.default_state();
        world.set_block(FEET, iron, UpdateFlags::UPDATE_ALL);
        world.set_block(FEET.above(), iron, UpdateFlags::UPDATE_ALL);
        world.set_block(FEET.above().east(), iron, UpdateFlags::UPDATE_ALL);
        if with_both_arms {
            world.set_block(FEET.above().west(), iron, UpdateFlags::UPDATE_ALL);
        }
    }

    fn golem_count(world: &Arc<World>) -> usize {
        let search = WorldAabb::new(0.0, 60.0, 0.0, 16.0, 72.0, 16.0);
        world
            .get_entities_in_aabb_matching(&search, |entity| {
                entity.entity_type() == &vanilla_entities::IRON_GOLEM
            })
            .len()
    }

    #[test]
    fn a_finished_iron_golem_frame_spawns_a_player_created_golem_and_eats_its_blocks() {
        let world = prepared_world("carved_pumpkin_iron_golem");
        place_iron_body(&world, true);
        let head = FEET.above_n(2);

        world.set_block(
            head,
            vanilla_blocks::CARVED_PUMPKIN.default_state(),
            UpdateFlags::UPDATE_ALL,
        );

        for consumed in [
            head,
            FEET,
            FEET.above(),
            FEET.above().east(),
            FEET.above().west(),
        ] {
            assert!(
                world.get_block_state(consumed).is_air(),
                "{consumed:?} should have been consumed by the golem"
            );
        }

        let search = WorldAabb::new(0.0, 60.0, 0.0, 16.0, 72.0, 16.0);
        let golems = world.get_entities_in_aabb_matching(&search, |entity| {
            entity.entity_type() == &vanilla_entities::IRON_GOLEM
        });
        assert_eq!(golems.len(), 1);
        let golem = golems[0]
            .downcast_ref::<IronGolemEntity>()
            .expect("the spawned entity should be an iron golem");
        assert!(golem.is_player_created());
    }

    #[test]
    fn an_iron_golem_frame_missing_an_arm_keeps_its_blocks_and_spawns_nothing() {
        let world = prepared_world("carved_pumpkin_iron_golem_malformed");
        place_iron_body(&world, false);
        let head = FEET.above_n(2);

        world.set_block(
            head,
            vanilla_blocks::CARVED_PUMPKIN.default_state(),
            UpdateFlags::UPDATE_ALL,
        );

        assert_eq!(
            world.get_block_state(head).get_block(),
            &vanilla_blocks::CARVED_PUMPKIN
        );
        assert_eq!(
            world.get_block_state(FEET).get_block(),
            &vanilla_blocks::IRON_BLOCK
        );
        assert_eq!(golem_count(&world), 0);
    }

    #[test]
    fn a_snow_column_topped_with_a_jack_o_lantern_still_builds_a_snow_golem() {
        let world = prepared_world("carved_pumpkin_snow_golem");
        let snow = vanilla_blocks::SNOW_BLOCK.default_state();
        world.set_block(FEET, snow, UpdateFlags::UPDATE_ALL);
        world.set_block(FEET.above(), snow, UpdateFlags::UPDATE_ALL);
        let head = FEET.above_n(2);

        world.set_block(
            head,
            vanilla_blocks::JACK_O_LANTERN.default_state(),
            UpdateFlags::UPDATE_ALL,
        );

        let search = WorldAabb::new(0.0, 60.0, 0.0, 16.0, 72.0, 16.0);
        let golems = world.get_entities_in_aabb_matching(&search, |entity| {
            entity.entity_type() == &vanilla_entities::SNOW_GOLEM
        });
        assert_eq!(golems.len(), 1);
        assert!(world.get_block_state(FEET).is_air());
    }

    #[test]
    fn a_copper_block_under_a_pumpkin_builds_a_copper_golem_over_a_copper_chest() {
        let world = prepared_world("carved_pumpkin_copper_golem");
        world.set_block(
            FEET,
            vanilla_blocks::WEATHERED_COPPER.default_state(),
            UpdateFlags::UPDATE_ALL,
        );
        let head = FEET.above();

        world.set_block(
            head,
            vanilla_blocks::CARVED_PUMPKIN.default_state(),
            UpdateFlags::UPDATE_ALL,
        );

        let search = WorldAabb::new(0.0, 60.0, 0.0, 16.0, 72.0, 16.0);
        let golems = world.get_entities_in_aabb_matching(&search, |entity| {
            entity.entity_type() == &vanilla_entities::COPPER_GOLEM
        });
        assert_eq!(golems.len(), 1);
        let golem = golems[0]
            .downcast_ref::<CopperGolemEntity>()
            .expect("the spawned entity should be a copper golem");
        assert_eq!(golem.weather_state(), WeatherState::Weathered);
        assert_eq!(
            world.get_block_state(FEET).get_block(),
            &vanilla_blocks::WEATHERED_COPPER_CHEST
        );
    }
}
