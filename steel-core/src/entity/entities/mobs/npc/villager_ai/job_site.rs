//! The behaviors that get a villager to a workstation and keep it there.
//!
//! Vanilla parity: `SetWalkTargetFromBlockMemory`, `GoToPotentialJobSite`,
//! `AssignProfessionFromJobSite`, `ResetProfession`, `YieldJobSite`,
//! `PoiCompetitorScan` and `WorkAtPoi`, all of which vanilla types to
//! `Villager` even though they sit in the shared `ai/behavior` package.

use std::f64::consts::FRAC_PI_2;

use glam::DVec3;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::poi::PoiTypeRef;
use steel_registry::{REGISTRY, RegistryExt as _, vanilla_blocks, vanilla_villager_professions};
use steel_utils::entity_events::EntityStatus;
use steel_utils::{BlockPos, Downcast as _, GlobalPos};

use super::villager;
use crate::entity::ai::brain::Activity;
use crate::entity::ai::brain::behavior::{
    BrainContext, MemoryModuleId, MemoryStatus, TimedBehavior, Trigger, utils,
};
use crate::entity::ai::brain::memory::{
    EntityMemory, MemoryModuleType, WalkTarget, memory_module_types,
};
use crate::entity::ai::brain::position_tracker::PositionTracker;
use crate::entity::ai::goal::default_random_pos_towards;
use crate::entity::entities::mobs::npc::VillagerEntity;
use crate::entity::{Entity as _, LivingEntity, Mob, PathfinderMob as _, SharedEntity};

/// Vanilla parity: the `int MAX_TRIES = 1000` of `SetWalkTargetFromBlockMemory`.
const MAX_TRIES_TOWARD_TARGET: u32 = 1000;
/// Vanilla parity: the `DefaultRandomPos.getPosTowards(body, 15, 7, ...)`.
const TOWARD_TARGET_HORIZONTAL_DIST: i32 = 15;
const TOWARD_TARGET_VERTICAL_DIST: i32 = 7;
/// Vanilla parity: the `bodyData.level() <= 1` of `ResetProfession`.
const MAX_LEVEL_TO_LOSE_A_JOB: i32 = 1;

/// Walks toward a remembered point of interest, giving it up if it is hopeless.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.behavior.SetWalkTargetFromBlockMemory`.
/// This is what actually sends a villager home at dusk and to its workstation
/// in the morning: the REST and WORK packages each schedule one against their
/// own memory. When the point cannot be reached for `too_long_unreachable`
/// ticks the villager gives back the POI ticket and forgets it, which is how a
/// walled-off bed eventually returns to the pool.
pub struct SetWalkTargetFromBlockMemory {
    memory: MemoryModuleType<GlobalPos>,
    speed_modifier: f64,
    close_enough_dist: i32,
    too_far_distance: i32,
    too_long_unreachable_duration: i64,
}

impl SetWalkTargetFromBlockMemory {
    /// Vanilla parity: `SetWalkTargetFromBlockMemory.create`.
    #[must_use]
    pub const fn new(
        memory: MemoryModuleType<GlobalPos>,
        speed_modifier: f64,
        close_enough_dist: i32,
        too_far_distance: i32,
        too_long_unreachable_duration: i64,
    ) -> Self {
        Self {
            memory,
            speed_modifier,
            close_enough_dist,
            too_far_distance,
            too_long_unreachable_duration,
        }
    }

    /// Vanilla parity: the `body.releasePoi(memoryType); memory.erase();
    /// cantReachSince.set(timestamp);` triple both give-up branches run.
    fn give_up(&self, ctx: &BrainContext<'_>) {
        if let Some(villager) = villager(ctx) {
            villager.release_poi(self.memory);
        }
        let brain = ctx.brain();
        brain.erase_memory(self.memory.id());
        brain.set_memory(
            memory_module_types::CANT_REACH_WALK_TARGET_SINCE,
            ctx.game_time(),
        );
    }
}

impl Trigger for SetWalkTargetFromBlockMemory {
    fn required_memories(&self) -> Vec<MemoryModuleId> {
        vec![
            memory_module_types::CANT_REACH_WALK_TARGET_SINCE.id(),
            memory_module_types::WALK_TARGET.id(),
            self.memory.id(),
        ]
    }

    fn trigger(&mut self, ctx: &BrainContext<'_>) -> bool {
        let brain = ctx.brain();
        if brain.has_memory_value(memory_module_types::WALK_TARGET.id()) {
            return false;
        }
        let Some(target) = brain.get_memory(self.memory) else {
            return false;
        };

        let cant_reach_since = brain.get_memory(memory_module_types::CANT_REACH_WALK_TARGET_SINCE);
        let given_up_on = cant_reach_since
            .is_some_and(|since| ctx.game_time() - since > self.too_long_unreachable_duration);
        if target.dimension != ctx.world().key || given_up_on {
            self.give_up(ctx);
            return true;
        }

        let mob = ctx.mob();
        let distance = target.pos.dist_manhattan(mob.block_position());
        if distance > self.too_far_distance {
            // Too far to path to in one go, so walk a leg toward it. Vanilla
            // rerolls until the leg actually lands nearer than `tooFarDistance`,
            // and gives the point up entirely after a thousand tries.
            let mut tries = 0;
            let leg = loop {
                let towards = default_random_pos_towards(
                    mob,
                    TOWARD_TARGET_HORIZONTAL_DIST,
                    TOWARD_TARGET_VERTICAL_DIST,
                    bottom_center_of(target.pos),
                    FRAC_PI_2,
                );
                if let Some(towards) = towards
                    && BlockPos::containing(towards.x, towards.y, towards.z)
                        .dist_manhattan(mob.block_position())
                        <= self.too_far_distance
                {
                    break Some(towards);
                }
                tries += 1;
                if tries == MAX_TRIES_TOWARD_TARGET {
                    break None;
                }
            };
            let Some(leg) = leg else {
                self.give_up(ctx);
                return true;
            };
            brain.set_memory(
                memory_module_types::WALK_TARGET,
                WalkTarget::of_position(leg, self.speed_modifier, self.close_enough_dist),
            );
        } else if distance > self.close_enough_dist {
            brain.set_memory(
                memory_module_types::WALK_TARGET,
                WalkTarget::of_block(target.pos, self.speed_modifier, self.close_enough_dist),
            );
        }
        true
    }

    fn debug_name(&self) -> &'static str {
        "SetWalkTargetFromBlockMemory"
    }
}

/// Vanilla parity: `Vec3.atBottomCenterOf`.
fn bottom_center_of(pos: BlockPos) -> DVec3 {
    DVec3::new(
        f64::from(pos.x()) + 0.5,
        f64::from(pos.y()),
        f64::from(pos.z()) + 0.5,
    )
}

/// Vanilla parity: `GoToPotentialJobSite.TICKS_UNTIL_TIMEOUT`.
const GO_TO_POTENTIAL_JOB_SITE_TIMEOUT: i32 = 1200;

/// Walks an unemployed villager to the workstation it has its eye on.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.behavior.GoToPotentialJobSite`.
/// Stopping -- by arriving, by timing out, or by the memory going away -- gives
/// the ticket back, because until `AssignProfessionFromJobSite` promotes the
/// claim to a real `JOB_SITE` nobody owns it.
pub struct GoToPotentialJobSite {
    speed_modifier: f64,
}

impl GoToPotentialJobSite {
    /// Vanilla parity: `new GoToPotentialJobSite(float)`.
    #[must_use]
    pub const fn new(speed_modifier: f64) -> Self {
        Self { speed_modifier }
    }
}

/// Vanilla parity: the `ImmutableMap.of(POTENTIAL_JOB_SITE, VALUE_PRESENT)`.
const POTENTIAL_JOB_SITE_PRESENT: &[(MemoryModuleId, MemoryStatus)] = &[(
    memory_module_types::POTENTIAL_JOB_SITE.id(),
    MemoryStatus::ValuePresent,
)];

impl TimedBehavior for GoToPotentialJobSite {
    fn entry_condition(&self) -> &[(MemoryModuleId, MemoryStatus)] {
        POTENTIAL_JOB_SITE_PRESENT
    }

    fn duration(&self) -> (i32, i32) {
        (
            GO_TO_POTENTIAL_JOB_SITE_TIMEOUT,
            GO_TO_POTENTIAL_JOB_SITE_TIMEOUT,
        )
    }

    fn check_extra_start_conditions(&mut self, ctx: &BrainContext<'_>) -> bool {
        // Vanilla parity: a villager only walks to a job site while it has
        // nothing more pressing on -- and a brain with no non-core activity at
        // all counts as free.
        ctx.brain()
            .active_non_core_activity()
            .is_none_or(|activity| {
                matches!(activity, Activity::Idle | Activity::Work | Activity::Play)
            })
    }

    fn can_still_use(&mut self, ctx: &BrainContext<'_>) -> bool {
        ctx.brain()
            .has_memory_value(memory_module_types::POTENTIAL_JOB_SITE.id())
    }

    fn tick(&mut self, ctx: &BrainContext<'_>) {
        let brain = ctx.brain();
        let Some(target) = brain.get_memory(memory_module_types::POTENTIAL_JOB_SITE) else {
            return;
        };
        utils::set_walk_and_look_target_memories(
            brain,
            PositionTracker::of_block(target.pos),
            self.speed_modifier,
            1,
        );
    }

    fn stop(&mut self, ctx: &BrainContext<'_>) {
        let brain = ctx.brain();
        if let Some(target) = brain.get_memory(memory_module_types::POTENTIAL_JOB_SITE)
            && target.dimension == ctx.world().key
        {
            let mut storage = ctx.world().poi_storage.lock();
            if storage.get_type(target.pos).is_some() {
                let _released = storage.release_ticket(target.pos);
            }
        }
        brain.erase_memory(memory_module_types::POTENTIAL_JOB_SITE.id());
    }

    fn debug_name(&self) -> &'static str {
        "GoToPotentialJobSite"
    }
}

/// Vanilla parity: the `closerToCenterThan(body.position(), 2.0)` of
/// `AssignProfessionFromJobSite`.
const CLOSE_ENOUGH_TO_TAKE_THE_JOB: f64 = 2.0;

/// Turns the workstation a villager walked to into its trade.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.behavior.AssignProfessionFromJobSite`.
/// The POI and the profession share a registry key, so the block it is standing
/// at names the trade.
pub struct AssignProfessionFromJobSite;

impl Trigger for AssignProfessionFromJobSite {
    fn required_memories(&self) -> Vec<MemoryModuleId> {
        vec![
            memory_module_types::POTENTIAL_JOB_SITE.id(),
            memory_module_types::JOB_SITE.id(),
        ]
    }

    fn trigger(&mut self, ctx: &BrainContext<'_>) -> bool {
        let brain = ctx.brain();
        let Some(target) = brain.get_memory(memory_module_types::POTENTIAL_JOB_SITE) else {
            return false;
        };
        let Some(villager) = villager(ctx) else {
            return false;
        };
        if !utils::block_closer_to_center_than(
            target.pos,
            ctx.mob().position(),
            CLOSE_ENOUGH_TO_TAKE_THE_JOB,
        ) {
            return false;
        }

        brain.erase_memory(memory_module_types::POTENTIAL_JOB_SITE.id());
        brain.set_memory(memory_module_types::JOB_SITE, target.clone());
        villager.broadcast_entity_event(EntityStatus::VillagerHappy);
        if villager.profession().key.path != "none" {
            return true;
        }

        let poi_type_id = {
            let storage = ctx.world().poi_storage.lock();
            storage.get_type(target.pos)
        };
        if let Some(poi_type) = poi_type_id.and_then(|id| REGISTRY.poi_types.by_id(id))
            && let Some(profession) = REGISTRY.villager_professions.by_key(&poi_type.key)
        {
            villager.set_profession(profession);
        }
        true
    }

    fn debug_name(&self) -> &'static str {
        "AssignProfessionFromJobSite"
    }
}

/// Fires a villager that has lost its workstation and never earned anything.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.behavior.ResetProfession`.
pub struct ResetProfession;

impl Trigger for ResetProfession {
    fn required_memories(&self) -> Vec<MemoryModuleId> {
        vec![memory_module_types::JOB_SITE.id()]
    }

    fn trigger(&mut self, ctx: &BrainContext<'_>) -> bool {
        if ctx
            .brain()
            .has_memory_value(memory_module_types::JOB_SITE.id())
        {
            return false;
        }
        let Some(villager) = villager(ctx) else {
            return false;
        };
        let profession = villager.profession();
        let can_be_fired = profession.key.path != "none" && profession.key.path != "nitwit";
        // Vanilla parity: `getVillagerXp() == 0 && bodyData.level() <= 1`.
        if !can_be_fired
            || villager.villager_xp() != 0
            || villager.villager_level() > MAX_LEVEL_TO_LOSE_A_JOB
        {
            return false;
        }
        villager.set_profession(&vanilla_villager_professions::NONE);
        true
    }

    fn debug_name(&self) -> &'static str {
        "ResetProfession"
    }
}

/// Hands a workstation over to a neighbour who can actually use it.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.behavior.YieldJobSite`. An
/// unemployed villager that has claimed a workstation somebody else's trade
/// matches steps aside and points them at it, which is what keeps two villagers
/// from deadlocking over one block.
pub struct YieldJobSite {
    speed_modifier: f64,
}

impl YieldJobSite {
    /// Vanilla parity: `YieldJobSite.create`.
    #[must_use]
    pub const fn new(speed_modifier: f64) -> Self {
        Self { speed_modifier }
    }
}

impl Trigger for YieldJobSite {
    fn required_memories(&self) -> Vec<MemoryModuleId> {
        vec![
            memory_module_types::POTENTIAL_JOB_SITE.id(),
            memory_module_types::JOB_SITE.id(),
            memory_module_types::NEAREST_LIVING_ENTITIES.id(),
            memory_module_types::WALK_TARGET.id(),
            memory_module_types::LOOK_TARGET.id(),
        ]
    }

    fn trigger(&mut self, ctx: &BrainContext<'_>) -> bool {
        let brain = ctx.brain();
        if brain.has_memory_value(memory_module_types::JOB_SITE.id()) {
            return false;
        }
        let Some(target) = brain.get_memory(memory_module_types::POTENTIAL_JOB_SITE) else {
            return false;
        };
        let Some(nearby) = brain.get_memory(memory_module_types::NEAREST_LIVING_ENTITIES) else {
            return false;
        };
        let Some(villager) = villager(ctx) else {
            return false;
        };
        if villager.is_baby() || villager.profession().key.path != "none" {
            return false;
        }

        let poi_pos = target.pos;
        let poi_type = {
            let storage = ctx.world().poi_storage.lock();
            storage.get_type(poi_pos)
        }
        .and_then(|id| REGISTRY.poi_types.by_id(id));
        let Some(poi_type) = poi_type else {
            // Vanilla returns `true` here: the behavior ran, it simply found no
            // POI to give away.
            return true;
        };

        let body_id = villager.id();
        let successor = nearby.iter().filter_map(EntityMemory::get).find(|entity| {
            let Some(other) = entity.downcast_ref::<VillagerEntity>() else {
                return false;
            };
            other.id() != body_id
                && LivingEntity::is_alive(other)
                && nearby_wants_job_site(ctx, other, poi_type, poi_pos)
        });
        let Some(successor) = successor else {
            return true;
        };
        let Some(successor_villager) = successor.downcast_ref::<VillagerEntity>() else {
            return true;
        };

        brain.erase_memory(memory_module_types::WALK_TARGET.id());
        brain.erase_memory(memory_module_types::LOOK_TARGET.id());
        brain.erase_memory(memory_module_types::POTENTIAL_JOB_SITE.id());
        let Some(successor_brain) = Mob::brain(successor_villager) else {
            return true;
        };
        if successor_brain.has_memory_value(memory_module_types::JOB_SITE.id()) {
            return true;
        }
        utils::set_walk_and_look_target_memories(
            successor_brain,
            PositionTracker::of_block(poi_pos),
            self.speed_modifier,
            1,
        );
        successor_brain.set_memory(
            memory_module_types::POTENTIAL_JOB_SITE,
            GlobalPos::new(ctx.world().key.clone(), poi_pos),
        );
        true
    }

    fn debug_name(&self) -> &'static str {
        "YieldJobSite"
    }
}

/// Vanilla parity: the private `YieldJobSite.nearbyWantsJobsite`.
fn nearby_wants_job_site(
    ctx: &BrainContext<'_>,
    other: &VillagerEntity,
    poi_type: PoiTypeRef,
    poi_pos: BlockPos,
) -> bool {
    let Some(brain) = Mob::brain(other) else {
        return false;
    };
    if brain.has_memory_value(memory_module_types::POTENTIAL_JOB_SITE.id()) {
        return false;
    }
    // Vanilla parity: `heldJobSite.test(type)`; the POI and the profession share
    // a key, and NONE and NITWIT hold no job site at all.
    let profession = other.profession();
    if profession.key.path == "none"
        || profession.key.path == "nitwit"
        || poi_type.key != profession.key
    {
        return false;
    }

    match brain.get_memory(memory_module_types::JOB_SITE) {
        Some(held) => held.pos == poi_pos && held.dimension == ctx.world().key,
        None => other
            .create_path_to(
                poi_pos,
                i32::try_from(poi_type.search_distance).unwrap_or(i32::MAX),
            )
            .is_some_and(|path| path.can_reach()),
    }
}

/// Breaks a tie between two villagers of the same trade over one workstation.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.behavior.PoiCompetitorScan`.
/// The one with less experience gives its claim up.
pub struct PoiCompetitorScan;

impl Trigger for PoiCompetitorScan {
    fn required_memories(&self) -> Vec<MemoryModuleId> {
        vec![
            memory_module_types::JOB_SITE.id(),
            memory_module_types::NEAREST_LIVING_ENTITIES.id(),
        ]
    }

    fn trigger(&mut self, ctx: &BrainContext<'_>) -> bool {
        let brain = ctx.brain();
        let Some(job_site) = brain.get_memory(memory_module_types::JOB_SITE) else {
            return false;
        };
        let Some(nearby) = brain.get_memory(memory_module_types::NEAREST_LIVING_ENTITIES) else {
            return false;
        };
        let Some(body) = villager(ctx) else {
            return false;
        };
        let poi_type = {
            let storage = ctx.world().poi_storage.lock();
            storage.get_type(job_site.pos)
        }
        .and_then(|id| REGISTRY.poi_types.by_id(id));
        let Some(poi_type) = poi_type else {
            return true;
        };

        let body_id = body.id();
        // Vanilla reduces the stream over `selectWinner`, so the loser of every
        // pair gives its claim up as the fold walks along. `None` is the body
        // itself, which is where vanilla's reduction starts.
        let mut winner_xp = body.villager_xp();
        let mut winner: Option<SharedEntity> = None;
        for entity in nearby.iter().filter_map(EntityMemory::get) {
            let Some(other) = entity.downcast_ref::<VillagerEntity>() else {
                continue;
            };
            if other.id() == body_id
                || !LivingEntity::is_alive(other)
                || !competes_for_same_job_site(ctx, other, &job_site, poi_type)
            {
                continue;
            }

            let other_xp = other.villager_xp();
            // Vanilla's `first.getVillagerXp() > second.getVillagerXp()` makes
            // the newcomer win a tie.
            if winner_xp > other_xp {
                give_up_job_site(other);
                continue;
            }
            match winner.take() {
                Some(previous) => {
                    if let Some(previous) = previous.downcast_ref::<VillagerEntity>() {
                        give_up_job_site(previous);
                    }
                }
                None => brain.erase_memory(memory_module_types::JOB_SITE.id()),
            }
            winner = Some(entity.clone());
            winner_xp = other_xp;
        }
        true
    }

    fn debug_name(&self) -> &'static str {
        "PoiCompetitorScan"
    }
}

/// Vanilla parity: the losing half of `PoiCompetitorScan.selectWinner`.
fn give_up_job_site(loser: &VillagerEntity) {
    if let Some(brain) = Mob::brain(loser) {
        brain.erase_memory(memory_module_types::JOB_SITE.id());
    }
}

/// Vanilla parity: the private `PoiCompetitorScan.competesForSameJobsite`.
fn competes_for_same_job_site(
    ctx: &BrainContext<'_>,
    other: &VillagerEntity,
    job_site: &GlobalPos,
    poi_type: PoiTypeRef,
) -> bool {
    let Some(brain) = Mob::brain(other) else {
        return false;
    };
    let Some(held) = brain.get_memory(memory_module_types::JOB_SITE) else {
        return false;
    };
    let profession = other.profession();
    held == *job_site
        && profession.key.path != "none"
        && profession.key.path != "nitwit"
        && poi_type.key == profession.key
        && held.dimension == ctx.world().key
}

/// Vanilla parity: `WorkAtPoi.CHECK_COOLDOWN`.
const WORK_CHECK_COOLDOWN: i64 = 300;
/// Vanilla parity: `WorkAtPoi.DISTANCE`.
const WORK_DISTANCE: f64 = 1.73;

/// Vanilla parity: `WorkAtPoi.useWorkstation`, which is empty on the base class
/// and is `WorkAtComposter`'s whole reason to exist.
fn use_workstation(ctx: &BrainContext<'_>, villager: &VillagerEntity, job_site: BlockPos) {
    if ctx.world().get_block_state(job_site).get_block().key == vanilla_blocks::COMPOSTER.key {
        villager.make_bread();
    }
}

/// Stands at the workstation and does the day's work.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.behavior.WorkAtPoi`. The
/// villager turns to face its workstation, makes its trade's work sound, and
/// restocks if it is due.
///
/// Vanilla builds a farmer's brain with `WorkAtComposter` instead, whose only
/// difference is a `useWorkstation` that acts on the composter it is standing
/// at. Steel registers one `WorkAtPoi` on every villager and has it look at the
/// block instead -- see the module docs on [`super`] for why there is no
/// `refreshBrain` to swap the two.
///
/// MISSING FOUNDATION: `WorkAtComposter.compostItems`, the other half of that
/// `useWorkstation`, needs `ComposterBlock.insertItem`/`extractProduce`, which
/// Steel's composter block behavior does not implement. A farmer therefore
/// bakes its wheat but never fills its composter.
pub struct WorkAtPoi {
    last_check: i64,
}

impl WorkAtPoi {
    /// Vanilla parity: `new WorkAtPoi()`.
    #[must_use]
    pub const fn new() -> Self {
        Self { last_check: 0 }
    }
}

impl Default for WorkAtPoi {
    fn default() -> Self {
        Self::new()
    }
}

/// Vanilla parity: the `ImmutableMap` handed to `WorkAtPoi`'s `super(...)`.
const WORK_ENTRY_CONDITION: &[(MemoryModuleId, MemoryStatus)] = &[
    (
        memory_module_types::JOB_SITE.id(),
        MemoryStatus::ValuePresent,
    ),
    (
        memory_module_types::LOOK_TARGET.id(),
        MemoryStatus::Registered,
    ),
];

/// Whether the villager is standing at the job site it remembers.
fn at_job_site(ctx: &BrainContext<'_>) -> Option<GlobalPos> {
    let target = ctx.brain().get_memory(memory_module_types::JOB_SITE)?;
    let at = target.dimension == ctx.world().key
        && utils::block_closer_to_center_than(target.pos, ctx.mob().position(), WORK_DISTANCE);
    at.then_some(target)
}

impl TimedBehavior for WorkAtPoi {
    fn entry_condition(&self) -> &[(MemoryModuleId, MemoryStatus)] {
        WORK_ENTRY_CONDITION
    }

    fn check_extra_start_conditions(&mut self, ctx: &BrainContext<'_>) -> bool {
        if ctx.game_time() - self.last_check < WORK_CHECK_COOLDOWN {
            return false;
        }
        // Vanilla parity: the coin flip that spreads work sounds out.
        if rand::random_range(0..2) != 0 {
            return false;
        }
        self.last_check = ctx.game_time();
        at_job_site(ctx).is_some()
    }

    fn start(&mut self, ctx: &BrainContext<'_>) {
        let brain = ctx.brain();
        brain.set_memory(memory_module_types::LAST_WORKED_AT_POI, ctx.game_time());
        let Some(target) = brain.get_memory(memory_module_types::JOB_SITE) else {
            return;
        };
        brain.set_memory(
            memory_module_types::LOOK_TARGET,
            PositionTracker::of_block(target.pos),
        );
        let Some(villager) = villager(ctx) else {
            return;
        };
        villager.play_work_sound();
        use_workstation(ctx, villager, target.pos);
        let game_time = ctx.world().game_time();
        if villager.should_restock(game_time) {
            villager.restock(game_time);
        }
    }

    fn can_still_use(&mut self, ctx: &BrainContext<'_>) -> bool {
        at_job_site(ctx).is_some()
    }

    fn debug_name(&self) -> &'static str {
        "WorkAtPoi"
    }
}
