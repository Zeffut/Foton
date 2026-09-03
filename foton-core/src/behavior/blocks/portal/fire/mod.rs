//! Fire block behavior implementation.
//!
//! Vanilla splits fire into `BaseFireBlock` (portal logic, placement checks) and `FireBlock`
//! (spreading, aging). This combines the portal-relevant parts from `BaseFireBlock`.

mod flammability;

use std::str::FromStr as _;
use std::sync::Arc;

use foton_macros::block_behavior;
use foton_registry::blocks::BlockRef;
use foton_registry::blocks::block_state_ext::BlockStateExt;
use foton_registry::blocks::properties::{BlockStateProperties, BoolProperty, IntProperty};
use foton_registry::vanilla_block_tags::BlockTag;
use foton_registry::vanilla_blocks;
use foton_registry::vanilla_damage_types;
use foton_registry::vanilla_dimension_types;
use foton_utils::axis::Axis;
use foton_utils::types::UpdateFlags;
use foton_utils::{BlockPos, BlockStateId, Direction, Identifier};
use rand::{Rng, RngExt as _};

use self::flammability::flammability;
use crate::behavior::block::BlockBehavior;
use crate::behavior::blocks::redstone::TntBlock;
use crate::behavior::context::BlockPlaceContext;
use crate::entity::damage::DamageSource;
use crate::entity::{Entity, InsideBlockEffectCollector, InsideBlockEffectType};
use crate::event::Event as _;
use crate::portal::portal_shape::{PortalShape, nether_portal_config};
use crate::world::{LevelReader, ScheduledTickAccess, World};

/// Vanilla parity: `FireBlock.MAX_AGE`.
const MAX_AGE: u8 = 15;

const AGE: &IntProperty = &BlockStateProperties::AGE_15;
const WATERLOGGED: &BoolProperty = &BlockStateProperties::WATERLOGGED;

/// Vanilla parity: `FireBlock.PROPERTY_BY_DIRECTION`, which is `PipeBlock`'s map
/// with `DOWN` filtered out -- fire never draws a face against the block it
/// stands on.
const fn face_property(direction: Direction) -> Option<&'static BoolProperty> {
    Some(match direction {
        Direction::North => &BlockStateProperties::NORTH,
        Direction::East => &BlockStateProperties::EAST,
        Direction::South => &BlockStateProperties::SOUTH,
        Direction::West => &BlockStateProperties::WEST,
        Direction::Up => &BlockStateProperties::UP,
        Direction::Down => return None,
    })
}

/// Vanilla parity: `FireBlock.getFireTickDelay`.
fn fire_tick_delay(rng: &mut impl Rng) -> i32 {
    30 + rng.random_range(0..10)
}

/// Behavior for fire blocks.
#[block_behavior]
pub struct FireBlock {
    block: BlockRef,
}

fn remove_fire_with_event(world: &Arc<World>, pos: BlockPos) {
    let mut event = crate::event::BlockFadeEvent::new(world.key.to_string(), pos);
    world.fire_event(&mut event);
    if !event.is_cancelled() {
        world.remove_block(pos, false);
    }
}

impl FireBlock {
    /// Creates a new fire block behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }

    /// Returns true if the world supports nether portal creation.
    ///
    /// Vanilla expresses this in terms of dimensions; Foton checks the loaded
    /// world's vanilla dimension type.
    pub(crate) fn in_portal_world(world: &World) -> bool {
        world.dimension_type == &vanilla_dimension_types::OVERWORLD
            || world.dimension_type == &vanilla_dimension_types::THE_NETHER
    }

    /// Checks if fire can be placed at `pos`, matching vanilla's `BaseFireBlock.canBePlacedAt`.
    /// Position must be air AND (fire can survive there OR it's a valid portal location).
    pub(crate) fn can_be_placed_at(
        world: &Arc<World>,
        pos: BlockPos,
        forward_dir: Direction,
    ) -> bool {
        if !world.get_block_state(pos).is_air() {
            return false;
        }
        Self::selected_fire_can_survive_at(world.as_ref(), pos)
            || Self::is_portal(world, pos, forward_dir)
    }

    /// Foton equivalent of vanilla's `BaseFireBlock.getState` for selecting
    /// between soul fire and regular fire.
    pub(crate) fn get_state(world: &dyn LevelReader, pos: BlockPos) -> BlockStateId {
        if SoulFireBlock::can_survive_at(world, pos) {
            vanilla_blocks::SOUL_FIRE.default_state()
        } else {
            Self::placement_state(world, pos)
        }
    }

    fn selected_fire_can_survive_at(world: &dyn LevelReader, pos: BlockPos) -> bool {
        SoulFireBlock::can_survive_at(world, pos) || Self::can_survive_at(world, pos)
    }

    /// Matches vanilla's `FireBlock.canSurvive`: block below has a sturdy top face,
    /// or an adjacent block is flammable.
    fn can_survive_at(world: &dyn LevelReader, pos: BlockPos) -> bool {
        let below_pos = pos.below();
        world.is_face_sturdy(world.get_block_state(below_pos), below_pos, Direction::Up)
            || Self::is_valid_fire_location(world, pos)
    }

    /// Matches vanilla's `FireBlock.getStateForPlacement(BlockGetter, BlockPos)`:
    /// fire standing on nothing solid leans on whatever around it can burn, and
    /// the faces it leans on are what the client draws.
    fn placement_state(world: &dyn LevelReader, pos: BlockPos) -> BlockStateId {
        let below_pos = pos.below();
        let below_state = world.get_block_state(below_pos);
        let default = vanilla_blocks::FIRE.default_state();
        if can_burn(below_state) || world.is_face_sturdy(below_state, below_pos, Direction::Up) {
            return default;
        }

        let mut state = default;
        for direction in Direction::ALL {
            let Some(property) = face_property(direction) else {
                continue;
            };
            let neighbor = world.get_block_state(direction.relative(pos));
            state = state.set_value(property, can_burn(neighbor));
        }
        state
    }

    /// Matches vanilla's `FireBlock.getStateWithAge`.
    fn state_with_age(world: &dyn LevelReader, pos: BlockPos, age: u8) -> BlockStateId {
        let state = Self::get_state(world, pos);
        if state.get_block() == &vanilla_blocks::FIRE {
            state.set_value(AGE, age)
        } else {
            state
        }
    }

    /// Matches vanilla's `FireBlock.isValidFireLocation`.
    fn is_valid_fire_location(world: &dyn LevelReader, pos: BlockPos) -> bool {
        Direction::ALL
            .into_iter()
            .any(|direction| can_burn(world.get_block_state(direction.relative(pos))))
    }

    /// Matches vanilla's `FireBlock.getIgniteOdds(LevelReader, BlockPos)`: how
    /// readily an empty position catches from whatever surrounds it.
    fn ignite_odds_at(world: &dyn LevelReader, pos: BlockPos) -> i32 {
        if !world.get_block_state(pos).is_air() {
            return 0;
        }
        Direction::ALL
            .into_iter()
            .map(|direction| ignite_odds(world.get_block_state(direction.relative(pos))))
            .max()
            .unwrap_or(0)
    }

    /// Matches vanilla's `FireBlock.isNearRain`.
    fn is_near_rain(world: &World, pos: BlockPos) -> bool {
        world.is_raining_at(pos)
            || world.is_raining_at(Direction::West.relative(pos))
            || world.is_raining_at(Direction::East.relative(pos))
            || world.is_raining_at(Direction::North.relative(pos))
            || world.is_raining_at(Direction::South.relative(pos))
    }

    /// Matches vanilla's `FireBlock.checkBurnOut`: one neighbour either catches
    /// in fire's place or is simply gone.
    fn check_burn_out(world: &Arc<World>, pos: BlockPos, chance: i32, rng: &mut impl Rng, age: u8) {
        let odds = burn_odds(world.get_block_state(pos));
        if rng.random_range(0..chance) >= odds {
            return;
        }

        let old_block = world.get_block_state(pos).get_block();
        let mut event = crate::event::BlockBurnEvent::new(world.key.to_string(), pos);
        world.fire_event(&mut event);
        if event.is_cancelled() {
            return;
        }
        if rng.random_range(0..i32::from(age) + 10) < 5 && !world.is_raining_at(pos) {
            let new_age = MAX_AGE.min(age + rng.random_range(0..5) / 4);
            world.set_block(
                pos,
                Self::state_with_age(world.as_ref(), pos, new_age),
                UpdateFlags::UPDATE_ALL,
            );
        } else {
            world.remove_block(pos, false);
        }

        if old_block == &vanilla_blocks::TNT {
            TntBlock::prime(world, pos, None);
        }
    }

    /// Whether the block under this fire keeps it alight forever.
    ///
    /// Vanilla parity: the `dimensionType().infiniburn()` test in `FireBlock.tick`,
    /// which is netherrack in the overworld and the nether, bedrock in the end.
    fn burns_forever_on(world: &World, below_state: BlockStateId) -> bool {
        let Some(tag) = world.dimension_type.infiniburn.strip_prefix('#') else {
            return false;
        };
        let Ok(tag) = Identifier::from_str(tag) else {
            return false;
        };
        below_state.get_block().has_tag(&tag)
    }

    /// Matches vanilla's `BaseFireBlock.isPortal`: checks if placing fire here could form a portal.
    /// Requires a portal-capable world, adjacent obsidian, and a valid empty portal shape.
    fn is_portal(world: &Arc<World>, pos: BlockPos, forward_dir: Direction) -> bool {
        if !Self::in_portal_world(world) {
            return false;
        }

        let has_obsidian = Direction::ALL.iter().any(|&dir| {
            world.get_block_state(pos.relative(dir)).get_block() == &vanilla_blocks::OBSIDIAN
        });
        if !has_obsidian {
            return false;
        }

        let preferred_axis = if forward_dir.get_axis().is_horizontal() {
            forward_dir.rotate_y_counter_clockwise().get_axis()
        } else if rand::random::<bool>() {
            Axis::X
        } else {
            Axis::Z
        };

        let config = nether_portal_config();
        PortalShape::find_empty_portal_shape_with_axis(world, pos, preferred_axis, &config)
            .is_some()
    }

    fn queue_entity_contact_effects(
        effect_collector: &mut InsideBlockEffectCollector,
        fire_damage: f32,
    ) {
        effect_collector.apply(InsideBlockEffectType::ClearFreeze);
        effect_collector.apply(InsideBlockEffectType::FireIgnite);
        effect_collector.run_after(
            InsideBlockEffectType::FireIgnite,
            Box::new(move |entity| {
                if !entity.fire_immune()
                    && let Some(entity_world) = entity.level()
                {
                    entity.hurt(
                        &entity_world,
                        &DamageSource::environment(&vanilla_damage_types::IN_FIRE),
                        fire_damage,
                    );
                }
            }),
        );
    }
}

/// Vanilla parity: `FireBlock.getIgniteOdds(BlockState)`. Water in the block
/// puts out any chance of it catching.
fn ignite_odds(state: BlockStateId) -> i32 {
    if state.try_get_value(WATERLOGGED) == Some(true) {
        return 0;
    }
    flammability(state.get_block()).ignite_odds
}

/// Vanilla parity: `FireBlock.getBurnOdds(BlockState)`.
fn burn_odds(state: BlockStateId) -> i32 {
    if state.try_get_value(WATERLOGGED) == Some(true) {
        return 0;
    }
    flammability(state.get_block()).burn_odds
}

/// Vanilla parity: `FireBlock.canBurn`.
fn can_burn(state: BlockStateId) -> bool {
    ignite_odds(state) > 0
}

impl BlockBehavior for FireBlock {
    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        Some(Self::get_state(context.world.as_ref(), context.place_pos()))
    }

    fn can_survive(&self, _state: BlockStateId, world: &dyn LevelReader, pos: BlockPos) -> bool {
        Self::can_survive_at(world, pos)
    }

    fn update_shape(
        &self,
        state: BlockStateId,
        world: &dyn ScheduledTickAccess,
        pos: BlockPos,
        _direction: Direction,
        _neighbor_pos: BlockPos,
        _neighbor_state: BlockStateId,
    ) -> BlockStateId {
        if Self::can_survive_at(world, pos) {
            Self::state_with_age(world, pos, state.get_value(AGE))
        } else {
            vanilla_blocks::AIR.default_state()
        }
    }

    fn entity_inside(
        &self,
        _state: BlockStateId,
        _world: &Arc<World>,
        _pos: BlockPos,
        _entity: &dyn Entity,
        effect_collector: &mut InsideBlockEffectCollector,
        _is_precise: bool,
    ) {
        Self::queue_entity_contact_effects(effect_collector, 1.0);
    }

    fn on_place(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        old_state: BlockStateId,
        _moved_by_piston: bool,
    ) {
        // Vanilla parity: `BaseFireBlock.onPlace` only attempts portal creation
        // when fire is newly placed, but `FireBlock.onPlace` schedules its next
        // tick either way -- an age bump re-enters here and has to keep ticking.
        if old_state.get_block() != state.get_block() {
            if Self::in_portal_world(world)
                && let Some(shape) =
                    PortalShape::find_empty_portal_shape(world, pos, &nether_portal_config())
            {
                shape.place_portal_blocks(world);
                return;
            }

            if !self.can_survive(state, world.as_ref(), pos) {
                world.remove_block(pos, false);
            }
        }

        world.schedule_block_tick_default(pos, self.block, fire_tick_delay(&mut rand::rng()));
    }

    /// Vanilla parity: `FireBlock.tick`.
    fn tick(&self, state: BlockStateId, world: &Arc<World>, pos: BlockPos) {
        let mut rng = rand::rng();
        world.schedule_block_tick_default(pos, self.block, fire_tick_delay(&mut rng));

        // `doFireTick`, plus the radius that keeps unattended chunks from burning.
        if !world.can_spread_fire_around(pos) {
            return;
        }

        // Vanilla removes the block here and carries on with the rest of the
        // tick regardless; the spread it then rolls is what makes fire creep
        // sideways off a ledge.
        if !Self::can_survive_at(world.as_ref(), pos) {
            remove_fire_with_event(world, pos);
        }

        let below_pos = pos.below();
        let burns_forever = Self::burns_forever_on(world, world.get_block_state(below_pos));
        let age = state.get_value(AGE);

        if !burns_forever
            && world.is_raining()
            && Self::is_near_rain(world, pos)
            && rng.random::<f32>() < 0.2 + f32::from(age) * 0.03
        {
            remove_fire_with_event(world, pos);
            return;
        }

        let new_age = MAX_AGE.min(age + rng.random_range(0..3) / 2);
        if age != new_age {
            world.set_block(pos, state.set_value(AGE, new_age), UpdateFlags::UPDATE_NONE);
        }

        if !burns_forever {
            if !Self::is_valid_fire_location(world.as_ref(), pos) {
                let below_state = world.get_block_state(below_pos);
                if !world.is_face_sturdy(below_state, below_pos, Direction::Up) || age > 3 {
                    remove_fire_with_event(world, pos);
                }
                return;
            }

            if age == MAX_AGE
                && rng.random_range(0..4) == 0
                && !can_burn(world.get_block_state(below_pos))
            {
                remove_fire_with_event(world, pos);
                return;
            }
        }

        let increased_burnout = world.increased_fire_burnout_at(pos);
        let extra = if increased_burnout { -50 } else { 0 };
        Self::check_burn_out(world, pos.east(), 300 + extra, &mut rng, age);
        Self::check_burn_out(world, pos.west(), 300 + extra, &mut rng, age);
        Self::check_burn_out(world, below_pos, 250 + extra, &mut rng, age);
        Self::check_burn_out(world, pos.above(), 250 + extra, &mut rng, age);
        Self::check_burn_out(world, pos.north(), 300 + extra, &mut rng, age);
        Self::check_burn_out(world, pos.south(), 300 + extra, &mut rng, age);

        let difficulty = i32::from(u8::from(world.difficulty()));
        for dx in -1..=1 {
            for dz in -1..=1 {
                for dy in -1..=4 {
                    if dx == 0 && dy == 0 && dz == 0 {
                        continue;
                    }

                    // Fire reaches further up than sideways, and each step above
                    // the first costs another hundred rolls.
                    let mut rate = 100;
                    if dy > 1 {
                        rate += (dy - 1) * 100;
                    }

                    let test_pos = pos.offset(dx, dy, dz);
                    let ignite_odds = Self::ignite_odds_at(world.as_ref(), test_pos);
                    if ignite_odds <= 0 {
                        continue;
                    }

                    let mut odds = (ignite_odds + 40 + difficulty * 7) / (i32::from(age) + 30);
                    if increased_burnout {
                        odds /= 2;
                    }

                    if odds > 0
                        && rng.random_range(0..rate) <= odds
                        && (!world.is_raining() || !Self::is_near_rain(world, test_pos))
                    {
                        let spread_age = MAX_AGE.min(age + rng.random_range(0..5) / 4);
                        world.set_block(
                            test_pos,
                            Self::state_with_age(world.as_ref(), test_pos, spread_age),
                            UpdateFlags::UPDATE_ALL,
                        );
                    }
                }
            }
        }
    }
}

/// Behavior for soul fire.
///
/// Vanilla keeps this as `SoulFireBlock`, separate from normal `FireBlock`: it
/// never ages and never spreads, it only asks whether it is still standing on
/// soul sand.
#[block_behavior]
pub struct SoulFireBlock {
    block: BlockRef,
}

impl SoulFireBlock {
    /// Creates a new soul fire block behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }

    fn can_survive_at(world: &dyn LevelReader, pos: BlockPos) -> bool {
        let block_below = world.get_block_state(pos.below()).get_block();
        block_below.has_tag(&BlockTag::SOUL_FIRE_BASE_BLOCKS)
    }
}

impl BlockBehavior for SoulFireBlock {
    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        let state = self.block.default_state();
        self.can_survive(state, context.world, context.place_pos())
            .then_some(state)
    }

    fn can_survive(&self, _state: BlockStateId, world: &dyn LevelReader, pos: BlockPos) -> bool {
        Self::can_survive_at(world, pos)
    }

    fn update_shape(
        &self,
        _state: BlockStateId,
        world: &dyn ScheduledTickAccess,
        pos: BlockPos,
        _direction: Direction,
        _neighbor_pos: BlockPos,
        _neighbor_state: BlockStateId,
    ) -> BlockStateId {
        if Self::can_survive_at(world, pos) {
            self.block.default_state()
        } else {
            vanilla_blocks::AIR.default_state()
        }
    }

    fn on_place(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        old_state: BlockStateId,
        _moved_by_piston: bool,
    ) {
        // Vanilla parity: `BaseFireBlock.onPlace`. Soul fire cannot light a
        // portal, but the shape is shared, and the survival check is what stops
        // a soul fire that was placed on the wrong block from lingering.
        if old_state.get_block() == state.get_block() {
            return;
        }
        if !Self::can_survive_at(world.as_ref(), pos) {
            remove_fire_with_event(world, pos);
        }
    }

    fn entity_inside(
        &self,
        _state: BlockStateId,
        _world: &Arc<World>,
        _pos: BlockPos,
        _entity: &dyn Entity,
        effect_collector: &mut InsideBlockEffectCollector,
        _is_precise: bool,
    ) {
        FireBlock::queue_entity_contact_effects(effect_collector, 2.0);
    }
}

#[cfg(test)]
mod tests;
