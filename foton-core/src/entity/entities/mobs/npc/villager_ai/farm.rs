//! The farmer's field.
//!
//! Vanilla parity: `net.minecraft.world.entity.ai.behavior.HarvestFarmland`,
//! which vanilla types to `Villager` even though it sits in the shared
//! `ai/behavior` package.

use std::sync::Arc;

use foton_protocol::packets::game::SoundSource;
use foton_registry::blocks::block_state_ext::BlockStateExt as _;
use foton_registry::vanilla_item_tags::ItemTag;
use foton_registry::{
    sound_events, vanilla_blocks, vanilla_game_events, vanilla_game_rules,
    vanilla_villager_professions,
};
use foton_utils::BlockPos;
use foton_utils::types::UpdateFlags;

use super::villager;
use crate::behavior::{BlockStateBehaviorExt as _, ITEM_BEHAVIORS};
use crate::entity::InventoryCarrier as _;
use crate::entity::ai::brain::behavior::{
    BrainContext, MemoryModuleId, MemoryStatus, TimedBehavior, utils,
};
use crate::entity::ai::brain::memory::{WalkTarget, memory_module_types};
use crate::entity::ai::brain::position_tracker::PositionTracker;
use crate::entity::entities::mobs::npc::VillagerEntity;
use crate::inventory::container::Container as _;
use crate::world::World;
use crate::world::game_event::GameEventContext;

/// Vanilla parity: `HarvestFarmland.SPEED_MODIFIER`.
const SPEED_MODIFIER: f64 = 0.5;
/// Vanilla parity: `HarvestFarmland.HARVEST_DURATION`.
const HARVEST_DURATION: i32 = 200;
/// Vanilla parity: the `closerToCenterThan(body.position(), 1.0)` that decides
/// the villager has arrived at the block it is working.
const REACH: f64 = 1.0;
/// Vanilla parity: the `nextOkStartTime = timestamp + 40L` of `stop`.
const COOLDOWN_AFTER_STOP: i64 = 40;
/// Vanilla parity: the `nextOkStartTime = timestamp + 20L` the villager waits
/// before starting on the next block of the field.
const COOLDOWN_BETWEEN_BLOCKS: i64 = 20;
/// Vanilla parity: the `closeEnoughDist` of the behavior's two `WalkTarget`s.
const CLOSE_ENOUGH_DIST: i32 = 1;
/// Vanilla parity: the `-1..=1` box `checkExtraStartConditions` scans.
const SCAN_RADIUS: i32 = 1;

/// Vanilla parity: the `ImmutableMap` handed to `HarvestFarmland`'s `super(...)`.
const HARVEST_ENTRY_CONDITION: &[(MemoryModuleId, MemoryStatus)] = &[
    (
        memory_module_types::LOOK_TARGET.id(),
        MemoryStatus::ValueAbsent,
    ),
    (
        memory_module_types::WALK_TARGET.id(),
        MemoryStatus::ValueAbsent,
    ),
    (
        memory_module_types::SECONDARY_JOB_SITE.id(),
        MemoryStatus::ValuePresent,
    ),
];

/// Harvests a ripe crop and replants the bare farmland it leaves behind.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.behavior.HarvestFarmland`.
/// The `SECONDARY_JOB_SITE` entry condition is what ties this to the
/// `SecondaryPoiSensor`: a farmer only works a field it can see from its
/// composter.
pub struct HarvestFarmland {
    above_farmland_pos: Option<BlockPos>,
    next_ok_start_time: i64,
    time_worked_so_far: i32,
    valid_farmland_around_villager: Vec<BlockPos>,
}

impl HarvestFarmland {
    /// Vanilla parity: `new HarvestFarmland()`.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            above_farmland_pos: None,
            next_ok_start_time: 0,
            time_worked_so_far: 0,
            valid_farmland_around_villager: Vec::new(),
        }
    }

    /// Vanilla parity: `HarvestFarmland.getValidFarmland`.
    fn pick_valid_farmland(&self) -> Option<BlockPos> {
        let field = &self.valid_farmland_around_villager;
        field
            .get(rand::random_range(0..field.len().max(1)))
            .copied()
    }

    /// Vanilla parity: `HarvestFarmland.validPos` -- a ripe crop to pull, or the
    /// bare farmland left where one was pulled.
    fn valid_pos(world: &World, pos: BlockPos) -> bool {
        let state = world.get_block_state(pos);
        state.crop_is_max_age() == Some(true) || (state.is_air() && is_farmland(world, pos.below()))
    }

    /// Vanilla parity: the two `setMemory` calls `start` and `tick` share.
    fn walk_to(ctx: &BrainContext<'_>, pos: BlockPos) {
        let brain = ctx.brain();
        brain.set_memory(
            memory_module_types::LOOK_TARGET,
            PositionTracker::of_block(pos),
        );
        brain.set_memory(
            memory_module_types::WALK_TARGET,
            WalkTarget::of_block(pos, SPEED_MODIFIER, CLOSE_ENOUGH_DIST),
        );
    }

    /// Vanilla parity: the seed half of `HarvestFarmland.tick`, which plants the
    /// first plantable seed the villager is carrying and stops there.
    fn plant_a_seed(world: &Arc<World>, villager: &VillagerEntity, pos: BlockPos) {
        let planted = {
            let mut inventory = villager.carried_inventory().lock();
            let mut planted = None;
            for slot in 0..inventory.get_container_size() {
                let stack = inventory.get_item(slot);
                if stack.is_empty() || !stack.item().has_tag(&ItemTag::VILLAGER_PLANTABLE_SEEDS) {
                    continue;
                }
                let Some(block) = ITEM_BEHAVIORS.get_behavior(stack.item()).placed_block() else {
                    continue;
                };
                inventory.remove_item(slot, 1);
                planted = Some(block.default_state());
                break;
            }
            planted
        };
        let Some(planted) = planted else {
            return;
        };

        world.set_block(pos, planted, UpdateFlags::UPDATE_ALL);
        world.game_event(
            &vanilla_game_events::BLOCK_PLACE,
            pos,
            &GameEventContext::new(Some(villager), Some(planted)),
        );
        world.play_sound(
            &sound_events::ITEM_CROP_PLANT,
            SoundSource::Blocks,
            pos,
            1.0,
            1.0,
            None,
        );
    }
}

impl Default for HarvestFarmland {
    fn default() -> Self {
        Self::new()
    }
}

impl TimedBehavior for HarvestFarmland {
    fn entry_condition(&self) -> &[(MemoryModuleId, MemoryStatus)] {
        HARVEST_ENTRY_CONDITION
    }

    /// Vanilla parity: `HarvestFarmland.checkExtraStartConditions`.
    fn check_extra_start_conditions(&mut self, ctx: &BrainContext<'_>) -> bool {
        let world = ctx.world();
        if !world.get_game_rule(&vanilla_game_rules::MOB_GRIEFING) {
            return false;
        }
        let Some(villager) = villager(ctx) else {
            return false;
        };
        if villager.profession().key != vanilla_villager_professions::FARMER.key {
            return false;
        }

        let center = ctx.mob().block_position();
        self.valid_farmland_around_villager.clear();
        for x in -SCAN_RADIUS..=SCAN_RADIUS {
            for y in -SCAN_RADIUS..=SCAN_RADIUS {
                for z in -SCAN_RADIUS..=SCAN_RADIUS {
                    let pos = center.offset(x, y, z);
                    if Self::valid_pos(world, pos) {
                        self.valid_farmland_around_villager.push(pos);
                    }
                }
            }
        }

        self.above_farmland_pos = self.pick_valid_farmland();
        self.above_farmland_pos.is_some()
    }

    /// Vanilla parity: `HarvestFarmland.start`.
    fn start(&mut self, ctx: &BrainContext<'_>) {
        if ctx.game_time() <= self.next_ok_start_time {
            return;
        }
        let Some(pos) = self.above_farmland_pos else {
            return;
        };
        Self::walk_to(ctx, pos);
    }

    /// Vanilla parity: `HarvestFarmland.tick`.
    fn tick(&mut self, ctx: &BrainContext<'_>) {
        let world = ctx.world();
        let arrived = self
            .above_farmland_pos
            .is_none_or(|pos| utils::block_closer_to_center_than(pos, ctx.mob().position(), REACH));
        if !arrived {
            return;
        }

        if let Some(pos) = self.above_farmland_pos
            && ctx.game_time() > self.next_ok_start_time
        {
            // Vanilla reads the state once, up front, and the replant and the
            // move-on branch below both test that same reading. That is why a
            // villager never pulls a crop and replants the square in one tick.
            let state = world.get_block_state(pos);
            let crop_age = state.crop_is_max_age();
            if crop_age == Some(true) {
                world.destroy_block_by_entity(pos, true, ctx.mob().as_entity_event_source());
            }

            if state.is_air()
                && is_farmland(world, pos.below())
                && let Some(villager) = villager(ctx)
                && villager.has_farm_seeds()
            {
                Self::plant_a_seed(world, villager, pos);
            }

            if crop_age == Some(false) {
                self.valid_farmland_around_villager.retain(|&at| at != pos);
                self.above_farmland_pos = self.pick_valid_farmland();
                if let Some(next) = self.above_farmland_pos {
                    self.next_ok_start_time = ctx.game_time() + COOLDOWN_BETWEEN_BLOCKS;
                    Self::walk_to(ctx, next);
                }
            }
        }

        self.time_worked_so_far += 1;
    }

    /// Vanilla parity: `HarvestFarmland.stop`.
    fn stop(&mut self, ctx: &BrainContext<'_>) {
        let brain = ctx.brain();
        brain.erase_memory(memory_module_types::LOOK_TARGET.id());
        brain.erase_memory(memory_module_types::WALK_TARGET.id());
        self.time_worked_so_far = 0;
        self.next_ok_start_time = ctx.game_time() + COOLDOWN_AFTER_STOP;
    }

    /// Vanilla parity: `HarvestFarmland.canStillUse`.
    fn can_still_use(&mut self, _ctx: &BrainContext<'_>) -> bool {
        self.time_worked_so_far < HARVEST_DURATION
    }

    fn debug_name(&self) -> &'static str {
        "HarvestFarmland"
    }
}

/// Vanilla parity: the `blockBelow instanceof FarmlandBlock` of `validPos` and
/// `tick`. `Blocks.FARMLAND` is the only block of that class.
fn is_farmland(world: &World, pos: BlockPos) -> bool {
    world.get_block_state(pos).get_block().key == vanilla_blocks::FARMLAND.key
}
