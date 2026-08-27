//! Vanilla `LocateHidingPlace` and `SetHiddenState`.

use steel_registry::{RegistryEntry as _, vanilla_poi_types};
use steel_utils::GlobalPos;

use crate::entity::ai::brain::behavior::{BrainContext, Trigger, utils};
use crate::entity::ai::brain::memory::{MemoryModuleId, WalkTarget, memory_module_types};
use crate::poi::poi_storage::OccupationStatus;

/// How long a villager keeps trying to reach a hiding place after the bell.
///
/// Vanilla parity: `SetHiddenState.HIDE_TIMEOUT`, measured from the
/// `HEARD_BELL_TIME` rather than from when the hiding started -- so a villager
/// that cannot get indoors gives up fifteen seconds after the alarm instead of
/// standing in a doorway forever.
const HIDE_TIMEOUT: i64 = 300;

/// Ticks per second, for the `seconds` `SetHiddenState` is built with.
const TICKS_PER_SECOND: i32 = 20;

/// Vanilla parity: the `p -> p.is(PoiTypes.HOME)` both searches use.
fn is_home_poi(poi_type_id: usize) -> bool {
    poi_type_id == vanilla_poi_types::HOME.id()
}

/// Sends a villager to the nearest bed it can find, and remembers where.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.behavior.LocateHidingPlace`.
///
/// The search is three fallbacks deep: a bed already within reach, then any bed
/// in the radius at random, then the villager's own home. Occupancy is ignored
/// throughout -- a raid is no time to be fussy about whose bed it is -- and the
/// four memories it erases are the ones that would otherwise keep the villager
/// walking somewhere else: its path, what it was looking at, and whoever it was
/// in the middle of trading with or courting.
pub struct LocateHidingPlace {
    radius: i32,
    speed_modifier: f64,
    close_enough_dist: i32,
}

impl LocateHidingPlace {
    /// Vanilla parity: `LocateHidingPlace.create`.
    #[must_use]
    pub const fn new(radius: i32, speed_modifier: f64, close_enough_dist: i32) -> Self {
        Self {
            radius,
            speed_modifier,
            close_enough_dist,
        }
    }
}

impl Trigger for LocateHidingPlace {
    fn required_memories(&self) -> Vec<MemoryModuleId> {
        vec![
            memory_module_types::WALK_TARGET.id(),
            memory_module_types::HOME.id(),
            memory_module_types::HIDING_PLACE.id(),
            memory_module_types::PATH.id(),
            memory_module_types::LOOK_TARGET.id(),
            memory_module_types::BREED_TARGET.id(),
            memory_module_types::INTERACTION_TARGET.id(),
        ]
    }

    fn trigger(&mut self, ctx: &BrainContext<'_>) -> bool {
        let brain = ctx.brain();
        // Vanilla parity: the `i.absent(WALK_TARGET)` of the group -- a
        // villager already walking somewhere is left to get there.
        if brain.has_memory_value(memory_module_types::WALK_TARGET.id()) {
            return false;
        }

        let mob = ctx.mob();
        let body_block = mob.block_position();
        let body_position = mob.position();
        let close_enough = f64::from(self.close_enough_dist);

        let found = {
            let storage = ctx.world().poi_storage.lock();
            storage
                .find(
                    &is_home_poi,
                    &|_| true,
                    body_block,
                    self.close_enough_dist + 1,
                    OccupationStatus::Any,
                )
                .filter(|&pos| utils::block_closer_to_center_than(pos, body_position, close_enough))
                .or_else(|| {
                    storage.get_random(
                        &is_home_poi,
                        &|_| true,
                        OccupationStatus::Any,
                        body_block,
                        self.radius,
                        &mut rand::rng(),
                    )
                })
        };
        // Vanilla parity: the last `.or(...)`, which falls back to the bed this
        // villager already owns however far away it is.
        let Some(pos) = found.or_else(|| {
            brain
                .get_memory(memory_module_types::HOME)
                .map(|home| home.pos)
        }) else {
            return true;
        };

        brain.erase_memory(memory_module_types::PATH.id());
        brain.erase_memory(memory_module_types::LOOK_TARGET.id());
        brain.erase_memory(memory_module_types::BREED_TARGET.id());
        brain.erase_memory(memory_module_types::INTERACTION_TARGET.id());
        brain.set_memory(
            memory_module_types::HIDING_PLACE,
            GlobalPos::new(ctx.world().key.clone(), pos),
        );
        if !utils::block_closer_to_center_than(pos, body_position, close_enough) {
            brain.set_memory(
                memory_module_types::WALK_TARGET,
                WalkTarget::of_block(pos, self.speed_modifier, self.close_enough_dist),
            );
        }
        true
    }

    fn debug_name(&self) -> &'static str {
        "LocateHidingPlace"
    }
}

/// Counts a villager's time indoors and lets it out again when that is up.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.behavior.SetHiddenState`.
///
/// Two clocks end the hiding, and both have to be watched: the villager has
/// spent its fifteen seconds actually at the hiding place, or five seconds have
/// passed since the bell without it getting there. Whichever runs out first
/// drops both memories and hands the villager back to its schedule -- which is
/// the only way out of the HIDE activity, since that package has no
/// `UpdateActivityFromSchedule` of its own.
pub struct SetHiddenState {
    stay_hidden_ticks: i32,
    close_enough_dist: f64,
    /// Vanilla parity: the `MutableInt ticksHidden` the builder closes over.
    ticks_hidden: i32,
}

impl SetHiddenState {
    /// Vanilla parity: `SetHiddenState.create(seconds, closeEnoughDist)`.
    #[must_use]
    pub const fn new(seconds: i32, close_enough_dist: i32) -> Self {
        Self {
            stay_hidden_ticks: seconds * TICKS_PER_SECOND,
            close_enough_dist: close_enough_dist as f64,
            ticks_hidden: 0,
        }
    }
}

impl Trigger for SetHiddenState {
    fn required_memories(&self) -> Vec<MemoryModuleId> {
        vec![
            memory_module_types::HIDING_PLACE.id(),
            memory_module_types::HEARD_BELL_TIME.id(),
        ]
    }

    fn trigger(&mut self, ctx: &BrainContext<'_>) -> bool {
        let brain = ctx.brain();
        let Some(hiding_place) = brain.get_memory(memory_module_types::HIDING_PLACE) else {
            return false;
        };
        let Some(heard_bell_time) = brain.get_memory(memory_module_types::HEARD_BELL_TIME) else {
            return false;
        };

        let timed_out_trying_to_hide = heard_bell_time + HIDE_TIMEOUT <= ctx.game_time();
        if self.ticks_hidden <= self.stay_hidden_ticks && !timed_out_trying_to_hide {
            if utils::block_closer_than(
                hiding_place.pos,
                ctx.mob().block_position(),
                self.close_enough_dist,
            ) {
                self.ticks_hidden += 1;
            }
            return true;
        }

        brain.erase_memory(memory_module_types::HEARD_BELL_TIME.id());
        brain.erase_memory(memory_module_types::HIDING_PLACE.id());
        brain.update_activity_from_schedule(ctx.world(), ctx.game_time());
        self.ticks_hidden = 0;
        true
    }

    fn debug_name(&self) -> &'static str {
        "SetHiddenState"
    }
}
