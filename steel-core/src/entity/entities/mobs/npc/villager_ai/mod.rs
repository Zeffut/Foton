//! The villager's brain: the activity packages that make up its day.
//!
//! Vanilla parity: `net.minecraft.world.entity.ai.behavior.VillagerGoalPackages`
//! plus the `Brain.Provider` in `Villager`.
//!
//! # What drives it
//!
//! The `villager_schedule` timeline carries two activity-valued tracks, and
//! [`Brain::update_activity_from_schedule`] reads whichever one the villager's
//! [`ScheduleAttribute`] names. An adult steps IDLE -> WORK -> MEET -> IDLE ->
//! REST across the day; a baby steps IDLE -> PLAY -> IDLE -> PLAY -> REST.
//! Every package but PANIC ends with `UpdateActivityFromSchedule` at priority
//! 99, so the lowest-priority thing a villager does is notice the hour.
//!
//! # Two deliberate differences from vanilla
//!
//! **No `refreshBrain`.** Vanilla rebuilds a villager's whole brain whenever its
//! profession changes or it grows up, because the job-site predicates, the
//! composter special case and the baby/adult package split are all baked in at
//! construction. Steel instead: hands the tick's context to the job-site
//! predicates (see [`AcquirePoi`]); lets one [`WorkAtPoi`] look at the block it
//! is standing at; and registers both WORK and PLAY on every villager, which is
//! safe because the schedule attribute never names PLAY for an adult and WORK
//! needs a `JOB_SITE` a baby cannot hold. Growing up therefore only has to swap
//! the schedule attribute, which `age_boundary_changed` does.
//!
//! **The farming weights are the farmer's.** Vanilla weights `HarvestFarmland`
//! and `UseBonemeal` differently for a farmer and for everybody else, because
//! the weight is fixed when the brain is built. Both refuse to start for a
//! non-farmer -- `HarvestFarmland` tests the profession, and only the farmer
//! asks for bone meal -- and a `RunOne` entry that never starts does not change
//! the relative order of the ones that do, so the farmer's weights are used
//! throughout.
//!
//! [`Brain::update_activity_from_schedule`]: crate::entity::ai::brain::Brain::update_activity_from_schedule
//! [`ScheduleAttribute`]: crate::entity::ai::brain::ScheduleAttribute

mod breed;
mod farm;
mod job_site;
mod panic;
mod trade;

use steel_registry::entity_type::{EntityTypeRef, MobCategory};
use steel_registry::poi::PoiTypeRef;
use steel_registry::vanilla_poi_type_tags::PoiTag;
use steel_registry::{REGISTRY, TaggedRegistryExt as _, vanilla_entities, vanilla_poi_types};
use steel_utils::entity_events::EntityStatus;
use steel_utils::{BlockPos, Downcast as _};

use crate::behavior::BlockStateBehaviorExt as _;
use crate::entity::LivingEntity;
use crate::entity::ai::brain::behavior::{
    AcquirePoi, Behavior, BehaviorControl, DoNothing, GateBehavior, GoToWantedItem, InteractWith,
    LookAtTargetSink, MoveToTargetSink, OneShot, OrderPolicy, RunOne, RunningPolicy,
    SetEntityLookTarget, SetLookAndInteract, SetWalkTargetAwayFrom, SetWalkTargetFromLookTarget,
    SleepInBed, SocializeAtBell, StrollAroundPoi, StrollToPoi, StrollToPoiList, Swim, TriggerGate,
    UpdateActivityFromSchedule, ValidateNearbyPoi, VillageBoundRandomStroll, WakeUp,
};
use crate::entity::ai::brain::memory::{
    EntityMemory, MemoryModuleType, MemoryStatus, memory_module_types,
};
use crate::entity::ai::brain::sensor::SensorType;
use crate::entity::ai::brain::{Activity, ActivityData, Brain, BrainContext};
use crate::entity::entities::mobs::npc::VillagerEntity;
use crate::world::World;

pub use breed::VillagerMakeLove;
pub use farm::HarvestFarmland;
pub use job_site::{
    AssignProfessionFromJobSite, GoToPotentialJobSite, PoiCompetitorScan, ResetProfession,
    SetWalkTargetFromBlockMemory, WorkAtPoi, YieldJobSite,
};
pub use panic::{VillagerCalmDown, VillagerPanicTrigger};
pub use trade::{LookAndFollowTradingPlayerSink, ShowTradesToPlayer, TradeWithVillager};

use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::blocks::properties::BlockStateProperties;

/// Vanilla parity: `Villager.SPEED_MODIFIER`, the `0.5F` every package is built
/// with.
pub const SPEED_MODIFIER: f64 = 0.5;
/// Vanilla parity: `VillagerGoalPackages.STROLL_SPEED_MODIFIER`.
const STROLL_SPEED_MODIFIER: f64 = 0.4;
/// Vanilla parity: `VillagerGoalPackages.INTERACT_WALKUP_DIST`.
const INTERACT_WALKUP_DIST: i32 = 2;
/// Vanilla parity: the `4` of every `SetLookAndInteract.create(PLAYER, 4)`.
const PLAYER_INTERACT_RANGE: i32 = 4;
/// Vanilla parity: the `new LookAtTargetSink(45, 90)` of the core package.
const LOOK_AT_TARGET_MIN_DURATION: i32 = 45;
const LOOK_AT_TARGET_MAX_DURATION: i32 = 90;
/// Vanilla parity: the `8.0F` every `SetEntityLookTarget` in a look package uses.
const LOOK_DISTANCE: f64 = 8.0;
/// Vanilla parity: the `8` range every `InteractWith.of` in a package uses.
const INTERACT_RANGE: i32 = 8;
/// Vanilla parity: the `new DoNothing(30, 60)` of the look packages.
const LOOK_DO_NOTHING_MIN: i32 = 30;
const LOOK_DO_NOTHING_MAX: i32 = 60;
/// Vanilla parity: the `new DoNothing(20, 40)` of the play package.
const PLAY_DO_NOTHING_MIN: i32 = 20;
const PLAY_DO_NOTHING_MAX: i32 = 40;
/// Vanilla parity: the `GoToWantedItem.create(speedModifier, false, 4)`.
const WANTED_ITEM_MAX_DIST: i32 = 4;
/// Vanilla parity: the `speedModifier * 1.5F` a panicking villager runs at.
const PANIC_SPEED_MULTIPLIER: f64 = 1.5;
/// Vanilla parity: the `SetWalkTargetAwayFrom.entity(..., 6, false)` distance.
const PANIC_DESIRED_DISTANCE: i32 = 6;
/// Vanilla parity: the `new ShowTradesToPlayer(400, 1600)`.
const SHOW_TRADES_MIN_DURATION: i32 = 400;
const SHOW_TRADES_MAX_DURATION: i32 = 1600;
/// Vanilla parity: the `new MoveToTargetSink(80, 120)` of the play package.
const PLAY_MOVE_MIN_TIMEOUT: i32 = 80;
const PLAY_MOVE_MAX_TIMEOUT: i32 = 120;
/// Vanilla parity: the `StrollAroundPoi.create(MEETING_POINT, 0.4F, 40)`.
const MEETING_POINT_STROLL_DISTANCE: i32 = 40;
/// Vanilla parity: the `maxDistanceFromPoi` of the work package's
/// `StrollToPoiList.create(SECONDARY_JOB_SITE, speedModifier, 1, 6, JOB_SITE)`.
const SECONDARY_JOB_SITE_MAX_DIST: i32 = 6;
/// Vanilla parity: the weight `getWorkPackage` gives `HarvestFarmland` for a
/// farmer -- see the module docs on why the farmer's weights are used
/// throughout.
const FARMING_WEIGHT: i32 = 2;
/// Vanilla parity: the `VillageBoundRandomStroll.create(runawaySpeed, 2, 2)` of
/// the panic package, which keeps a frightened villager's hops short.
const PANIC_STROLL_DIST: i32 = 2;

/// Vanilla parity: the sensor list of `Villager.BRAIN_PROVIDER`.
///
/// MISSING FOUNDATION: vanilla also asks for `NEAREST_BED`, `VILLAGER_BABIES`
/// and `GOLEM_DETECTED`. The first two feed behaviors Steel has not ported
/// (`JumpOnBed`, `PlayTagWithOtherKids`) and the last feeds iron-golem
/// spawning, which Steel does not have.
const SENSORS: &[SensorType] = &[
    SensorType::NearestLivingEntities,
    SensorType::NearestPlayers,
    SensorType::NearestItems,
    SensorType::HurtBy,
    SensorType::VillagerHostiles,
    SensorType::SecondaryPois,
];

/// Reaches for the villager behind a brain context.
///
/// Every behavior in this module is registered on a villager's own brain, so
/// the downcast only fails if one is put on some other mob -- in which case
/// doing nothing is the right answer.
fn villager<'a>(ctx: &'a BrainContext<'a>) -> Option<&'a VillagerEntity> {
    ctx.mob().downcast_ref::<VillagerEntity>()
}

/// Whether this profession holds no workstation at all.
///
/// Vanilla parity: the `PoiType.NONE` predicate `VillagerProfession.NONE` and
/// `NITWIT` are registered with as their `heldJobSite`.
fn is_jobless(profession_path: &str) -> bool {
    profession_path == "none" || profession_path == "nitwit"
}

/// Vanilla parity: `VillagerProfession.heldJobSite`, which every named
/// profession registers as `poiType -> poiType.is(jobSite)` -- and the POI and
/// the profession share a registry key.
fn held_job_site(ctx: &BrainContext<'_>, poi_type: PoiTypeRef) -> bool {
    let Some(villager) = villager(ctx) else {
        return false;
    };
    let profession = villager.profession();
    !is_jobless(&profession.key.path) && poi_type.key == profession.key
}

/// Vanilla parity: `VillagerProfession.acquirableJobSite`, which is the held one
/// for a named profession, `ALL_ACQUIRABLE_JOBS` for `NONE`, and nothing for
/// `NITWIT`.
fn acquirable_job_site(ctx: &BrainContext<'_>, poi_type: PoiTypeRef) -> bool {
    let Some(villager) = villager(ctx) else {
        return false;
    };
    let profession = villager.profession();
    match profession.key.path.as_ref() {
        "nitwit" => false,
        "none" => REGISTRY
            .poi_types
            .is_in_tag(poi_type, &PoiTag::ACQUIRABLE_JOB_SITE),
        _ => poi_type.key == profession.key,
    }
}

/// Vanilla parity: the `p -> p.is(PoiTypes.HOME)` of the core package.
fn is_home_poi(_ctx: &BrainContext<'_>, poi_type: PoiTypeRef) -> bool {
    poi_type.key == vanilla_poi_types::HOME.key
}

/// Vanilla parity: the `p -> p.is(PoiTypes.MEETING)` of the core package.
fn is_meeting_poi(_ctx: &BrainContext<'_>, poi_type: PoiTypeRef) -> bool {
    poi_type.key == vanilla_poi_types::MEETING.key
}

/// Vanilla parity: the private `VillagerGoalPackages.validateBedPoi`.
fn validate_bed_poi(world: &World, pos: BlockPos) -> bool {
    let state = world.get_block_state(pos);
    state.is_bed() && !state.get_value(&BlockStateProperties::OCCUPIED)
}

/// Builds a villager's brain.
///
/// Vanilla parity: `Villager.BRAIN_PROVIDER` plus `registerBrainGoals`. The
/// schedule attribute is set by the caller, which knows whether this villager
/// is a baby.
#[must_use]
pub fn make_brain() -> Brain {
    Brain::new_with_memories(
        SENSORS,
        // Nothing writes these until the villager itself does, so they have to
        // be registered up front or the reads would miss.
        &[
            memory_module_types::LAST_SLEPT.id(),
            memory_module_types::LAST_WOKEN.id(),
            memory_module_types::LAST_WORKED_AT_POI.id(),
            memory_module_types::HOME.id(),
            memory_module_types::JOB_SITE.id(),
            memory_module_types::POTENTIAL_JOB_SITE.id(),
            memory_module_types::MEETING_POINT.id(),
        ],
        vec![
            core_package(),
            work_package(),
            rest_package(),
            meet_package(),
            idle_package(),
            play_package(),
            panic_package(),
        ],
    )
}

/// Vanilla parity: `VillagerGoalPackages.getCorePackage`.
///
/// MISSING FOUNDATION: vanilla also runs `InteractWithDoor`, `ReactToBell` and
/// `SetRaidStatus` here. Doors need the `DOORS_TO_CLOSE` bookkeeping Steel does
/// not do, and the other two need the bell event and the raid seam.
fn core_package() -> ActivityData {
    ActivityData::with_priorities(
        Activity::Core,
        vec![
            (0, Behavior::boxed(Swim::new(0.8))),
            (
                0,
                Behavior::boxed(LookAtTargetSink::new(
                    LOOK_AT_TARGET_MIN_DURATION,
                    LOOK_AT_TARGET_MAX_DURATION,
                )),
            ),
            (0, Behavior::boxed(VillagerPanicTrigger)),
            (0, OneShot::boxed(WakeUp)),
            (
                0,
                OneShot::boxed(ValidateNearbyPoi::new(
                    held_job_site,
                    memory_module_types::JOB_SITE,
                )),
            ),
            (
                0,
                OneShot::boxed(ValidateNearbyPoi::new(
                    acquirable_job_site,
                    memory_module_types::POTENTIAL_JOB_SITE,
                )),
            ),
            (1, Behavior::boxed(MoveToTargetSink::new())),
            (2, OneShot::boxed(PoiCompetitorScan)),
            (
                3,
                Behavior::boxed(LookAndFollowTradingPlayerSink::new(SPEED_MODIFIER)),
            ),
            (
                5,
                OneShot::boxed(GoToWantedItem::new(
                    SPEED_MODIFIER,
                    false,
                    WANTED_ITEM_MAX_DIST,
                )),
            ),
            (
                6,
                OneShot::boxed(AcquirePoi::with_validated_memory(
                    acquirable_job_site,
                    memory_module_types::JOB_SITE,
                    memory_module_types::POTENTIAL_JOB_SITE,
                    true,
                    None,
                )),
            ),
            (
                7,
                Behavior::boxed(GoToPotentialJobSite::new(SPEED_MODIFIER)),
            ),
            (8, OneShot::boxed(YieldJobSite::new(SPEED_MODIFIER))),
            (
                10,
                OneShot::boxed(
                    AcquirePoi::new(
                        is_home_poi,
                        memory_module_types::HOME,
                        false,
                        Some(EntityStatus::VillagerHappy),
                    )
                    .with_valid_poi(validate_bed_poi),
                ),
            ),
            (
                10,
                OneShot::boxed(AcquirePoi::new(
                    is_meeting_poi,
                    memory_module_types::MEETING_POINT,
                    true,
                    Some(EntityStatus::VillagerHappy),
                )),
            ),
            (10, OneShot::boxed(AssignProfessionFromJobSite)),
            (10, OneShot::boxed(ResetProfession)),
        ],
    )
}

/// Vanilla parity: `VillagerGoalPackages.getWorkPackage`.
///
/// MISSING FOUNDATION: vanilla's `RunOne` also holds `UseBonemeal`, which needs
/// the bone-meal application seam, and the package ends with `GiveGiftToHero`,
/// which needs the hero-of-the-village effect.
fn work_package() -> ActivityData {
    ActivityData::with_priorities(
        Activity::Work,
        vec![
            minimal_look_behavior(),
            (
                5,
                Box::new(RunOne::unconditional(vec![
                    (Behavior::boxed(WorkAtPoi::new()), 7),
                    (
                        OneShot::boxed(StrollAroundPoi::new(
                            memory_module_types::JOB_SITE,
                            STROLL_SPEED_MODIFIER,
                            4,
                        )),
                        2,
                    ),
                    (
                        OneShot::boxed(StrollToPoi::new(
                            memory_module_types::JOB_SITE,
                            STROLL_SPEED_MODIFIER,
                            1,
                            10,
                        )),
                        5,
                    ),
                    (
                        OneShot::boxed(StrollToPoiList::new(
                            memory_module_types::SECONDARY_JOB_SITE,
                            SPEED_MODIFIER,
                            1,
                            SECONDARY_JOB_SITE_MAX_DIST,
                            memory_module_types::JOB_SITE,
                        )),
                        5,
                    ),
                    (Behavior::boxed(HarvestFarmland::new()), FARMING_WEIGHT),
                ])),
            ),
            (
                10,
                Behavior::boxed(ShowTradesToPlayer::new(
                    SHOW_TRADES_MIN_DURATION,
                    SHOW_TRADES_MAX_DURATION,
                )),
            ),
            (
                10,
                OneShot::boxed(SetLookAndInteract::new(
                    &vanilla_entities::PLAYER,
                    PLAYER_INTERACT_RANGE,
                )),
            ),
            (
                2,
                OneShot::boxed(SetWalkTargetFromBlockMemory::new(
                    memory_module_types::JOB_SITE,
                    SPEED_MODIFIER,
                    9,
                    100,
                    1200,
                )),
            ),
            (99, OneShot::boxed(UpdateActivityFromSchedule)),
        ],
    )
    // Vanilla parity: the `ImmutableSet.of(Pair.of(JOB_SITE, VALUE_PRESENT))`
    // the WORK activity is registered with. Without a workstation there is no
    // work to go to, so the schedule's WORK hours fall back to the default
    // activity instead.
    .with_conditions(vec![(
        memory_module_types::JOB_SITE.id(),
        MemoryStatus::ValuePresent,
    )])
}

/// Vanilla parity: `VillagerGoalPackages.getRestPackage`.
///
/// MISSING FOUNDATION: vanilla's homeless `RunOne` --
/// `SetClosestHomeAsWalkTarget`, `InsideBrownianWalk`, `GoToClosestVillage` --
/// is not here; the first needs the `NEAREST_BED` sensor and the last two need
/// the village-center queries Steel does not have. A villager with no bed stands
/// still at night rather than wandering toward one.
fn rest_package() -> ActivityData {
    ActivityData::with_priorities(
        Activity::Rest,
        vec![
            (
                2,
                OneShot::boxed(SetWalkTargetFromBlockMemory::new(
                    memory_module_types::HOME,
                    SPEED_MODIFIER,
                    1,
                    150,
                    1200,
                )),
            ),
            (
                3,
                OneShot::boxed(ValidateNearbyPoi::new(
                    is_home_poi,
                    memory_module_types::HOME,
                )),
            ),
            (3, Behavior::boxed(SleepInBed::new())),
            minimal_look_behavior(),
            (99, OneShot::boxed(UpdateActivityFromSchedule)),
        ],
    )
}

/// Vanilla parity: `VillagerGoalPackages.getMeetPackage`.
///
/// MISSING FOUNDATION: `GiveGiftToHero` is not ported yet -- it needs the
/// hero-of-the-village effect.
fn meet_package() -> ActivityData {
    ActivityData::with_priorities(
        Activity::Meet,
        vec![
            (
                2,
                OneShot::boxed(TriggerGate::trigger_one_shuffled(vec![
                    (
                        Box::new(StrollAroundPoi::new(
                            memory_module_types::MEETING_POINT,
                            STROLL_SPEED_MODIFIER,
                            MEETING_POINT_STROLL_DISTANCE,
                        )),
                        2,
                    ),
                    (Box::new(SocializeAtBell), 2),
                ])),
            ),
            (
                10,
                Behavior::boxed(ShowTradesToPlayer::new(
                    SHOW_TRADES_MIN_DURATION,
                    SHOW_TRADES_MAX_DURATION,
                )),
            ),
            (
                10,
                OneShot::boxed(SetLookAndInteract::new(
                    &vanilla_entities::PLAYER,
                    PLAYER_INTERACT_RANGE,
                )),
            ),
            (
                2,
                OneShot::boxed(SetWalkTargetFromBlockMemory::new(
                    memory_module_types::MEETING_POINT,
                    SPEED_MODIFIER,
                    6,
                    100,
                    200,
                )),
            ),
            (
                3,
                OneShot::boxed(ValidateNearbyPoi::new(
                    is_meeting_poi,
                    memory_module_types::MEETING_POINT,
                )),
            ),
            // Vanilla parity: the `GateBehavior` that erases `INTERACTION_TARGET`
            // when it stops, so a swap that is interrupted does not leave the
            // pair bound to each other.
            (
                3,
                Box::new(GateBehavior::new(
                    Vec::new(),
                    vec![memory_module_types::INTERACTION_TARGET.id()],
                    OrderPolicy::Ordered,
                    RunningPolicy::RunOne,
                    vec![(Behavior::boxed(TradeWithVillager::new()), 1)],
                )),
            ),
            full_look_behavior(),
            (99, OneShot::boxed(UpdateActivityFromSchedule)),
        ],
    )
    // Vanilla parity: the `ImmutableSet.of(Pair.of(MEETING_POINT, VALUE_PRESENT))`
    // the MEET activity is registered with -- a village with no bell has
    // nowhere to gather, so the schedule's MEET hours fall back to IDLE.
    .with_conditions(vec![(
        memory_module_types::MEETING_POINT.id(),
        MemoryStatus::ValuePresent,
    )])
}

/// Vanilla parity: `VillagerGoalPackages.getIdlePackage`.
///
/// MISSING FOUNDATION: `JumpOnBed` needs the `NEAREST_BED` sensor and
/// `GiveGiftToHero` needs the hero-of-the-village effect.
fn idle_package() -> ActivityData {
    ActivityData::with_priorities(
        Activity::Idle,
        vec![
            (
                2,
                Box::new(RunOne::unconditional(vec![
                    (
                        OneShot::boxed(interact_with(
                            &vanilla_entities::VILLAGER,
                            memory_module_types::INTERACTION_TARGET,
                        )),
                        2,
                    ),
                    (
                        OneShot::boxed(InteractWith::of_matching(
                            &vanilla_entities::VILLAGER,
                            INTERACT_RANGE,
                            can_breed,
                            can_breed,
                            memory_module_types::BREED_TARGET,
                            SPEED_MODIFIER,
                            INTERACT_WALKUP_DIST,
                        )),
                        1,
                    ),
                    (
                        OneShot::boxed(interact_with(
                            &vanilla_entities::CAT,
                            memory_module_types::INTERACTION_TARGET,
                        )),
                        1,
                    ),
                    (
                        OneShot::boxed(VillageBoundRandomStroll::new(SPEED_MODIFIER)),
                        1,
                    ),
                    (
                        OneShot::boxed(SetWalkTargetFromLookTarget::new(
                            SPEED_MODIFIER,
                            INTERACT_WALKUP_DIST,
                        )),
                        1,
                    ),
                    (
                        Box::new(DoNothing::new(LOOK_DO_NOTHING_MIN, LOOK_DO_NOTHING_MAX)),
                        1,
                    ),
                ])),
            ),
            (
                3,
                OneShot::boxed(SetLookAndInteract::new(
                    &vanilla_entities::PLAYER,
                    PLAYER_INTERACT_RANGE,
                )),
            ),
            (
                3,
                Behavior::boxed(ShowTradesToPlayer::new(
                    SHOW_TRADES_MIN_DURATION,
                    SHOW_TRADES_MAX_DURATION,
                )),
            ),
            // Vanilla parity: the `GateBehavior` that erases `INTERACTION_TARGET`
            // when it stops, so a swap that is interrupted does not leave the
            // pair bound to each other.
            (
                3,
                Box::new(GateBehavior::new(
                    Vec::new(),
                    vec![memory_module_types::INTERACTION_TARGET.id()],
                    OrderPolicy::Ordered,
                    RunningPolicy::RunOne,
                    vec![(Behavior::boxed(TradeWithVillager::new()), 1)],
                )),
            ),
            // Vanilla parity: the `GateBehavior` that erases `BREED_TARGET` when
            // it stops, so a courtship that is interrupted does not leave the
            // pair bound to each other.
            (
                3,
                Box::new(GateBehavior::new(
                    Vec::new(),
                    vec![memory_module_types::BREED_TARGET.id()],
                    OrderPolicy::Ordered,
                    RunningPolicy::RunOne,
                    vec![(Behavior::boxed(VillagerMakeLove::new()), 1)],
                )),
            ),
            full_look_behavior(),
            (99, OneShot::boxed(UpdateActivityFromSchedule)),
        ],
    )
}

/// Vanilla parity: the `AgeableMob::canBreed` used as both the self filter and
/// the target filter of the breeding `InteractWith`.
fn can_breed(candidate: &dyn LivingEntity) -> bool {
    candidate
        .downcast_ref::<VillagerEntity>()
        .is_some_and(VillagerEntity::can_breed)
}

/// Vanilla parity: `VillagerGoalPackages.getPlayPackage`.
///
/// MISSING FOUNDATION: `PlayTagWithOtherKids` needs the `VILLAGER_BABIES`
/// sensor and `JumpOnBed` needs `NEAREST_BED`, so a baby wanders and stares
/// rather than playing tag.
fn play_package() -> ActivityData {
    ActivityData::with_priorities(
        Activity::Play,
        vec![
            (
                0,
                Behavior::boxed(MoveToTargetSink::with_timeout(
                    PLAY_MOVE_MIN_TIMEOUT,
                    PLAY_MOVE_MAX_TIMEOUT,
                )),
            ),
            full_look_behavior(),
            (
                5,
                Box::new(RunOne::unconditional(vec![
                    (
                        OneShot::boxed(interact_with(
                            &vanilla_entities::VILLAGER,
                            memory_module_types::INTERACTION_TARGET,
                        )),
                        2,
                    ),
                    (
                        OneShot::boxed(interact_with(
                            &vanilla_entities::CAT,
                            memory_module_types::INTERACTION_TARGET,
                        )),
                        1,
                    ),
                    (
                        OneShot::boxed(VillageBoundRandomStroll::new(SPEED_MODIFIER)),
                        1,
                    ),
                    (
                        OneShot::boxed(SetWalkTargetFromLookTarget::new(
                            SPEED_MODIFIER,
                            INTERACT_WALKUP_DIST,
                        )),
                        1,
                    ),
                    (
                        Box::new(DoNothing::new(PLAY_DO_NOTHING_MIN, PLAY_DO_NOTHING_MAX)),
                        2,
                    ),
                ])),
            ),
            (99, OneShot::boxed(UpdateActivityFromSchedule)),
        ],
    )
}

/// Vanilla parity: `VillagerGoalPackages.getPanicPackage`.
fn panic_package() -> ActivityData {
    let runaway_speed = SPEED_MODIFIER * PANIC_SPEED_MULTIPLIER;
    ActivityData::with_priorities(
        Activity::Panic,
        vec![
            (0, OneShot::boxed(VillagerCalmDown)),
            (
                1,
                OneShot::boxed(SetWalkTargetAwayFrom::entity(
                    memory_module_types::NEAREST_HOSTILE,
                    runaway_speed,
                    PANIC_DESIRED_DISTANCE,
                    false,
                )),
            ),
            (
                1,
                OneShot::boxed(SetWalkTargetAwayFrom::entity(
                    memory_module_types::HURT_BY_ENTITY,
                    runaway_speed,
                    PANIC_DESIRED_DISTANCE,
                    false,
                )),
            ),
            (
                3,
                OneShot::boxed(VillageBoundRandomStroll::with_range(
                    runaway_speed,
                    PANIC_STROLL_DIST,
                    PANIC_STROLL_DIST,
                )),
            ),
            minimal_look_behavior(),
        ],
    )
}

/// Vanilla parity: `InteractWith.of(type, 8, memory, speedModifier, 2)`.
fn interact_with(
    entity_type: EntityTypeRef,
    memory: MemoryModuleType<EntityMemory>,
) -> InteractWith {
    InteractWith::of(
        entity_type,
        INTERACT_RANGE,
        memory,
        SPEED_MODIFIER,
        INTERACT_WALKUP_DIST,
    )
}

/// Vanilla parity: the private `VillagerGoalPackages.getFullLookBehavior`.
fn full_look_behavior() -> (i32, Box<dyn BehaviorControl>) {
    let of_category = |category: MobCategory, weight: i32| {
        (
            OneShot::boxed(SetEntityLookTarget::matching(
                move |candidate| candidate.entity_type().mob_category == category,
                LOOK_DISTANCE,
            )),
            weight,
        )
    };
    (
        5,
        Box::new(RunOne::unconditional(vec![
            (
                OneShot::boxed(SetEntityLookTarget::of_type(
                    &vanilla_entities::CAT,
                    LOOK_DISTANCE,
                )),
                8,
            ),
            (
                OneShot::boxed(SetEntityLookTarget::of_type(
                    &vanilla_entities::VILLAGER,
                    LOOK_DISTANCE,
                )),
                2,
            ),
            (
                OneShot::boxed(SetEntityLookTarget::of_type(
                    &vanilla_entities::PLAYER,
                    LOOK_DISTANCE,
                )),
                2,
            ),
            of_category(MobCategory::Creature, 1),
            of_category(MobCategory::WaterCreature, 1),
            of_category(MobCategory::Axolotls, 1),
            of_category(MobCategory::UndergroundWaterCreature, 1),
            of_category(MobCategory::WaterAmbient, 1),
            of_category(MobCategory::Monster, 1),
            (
                Box::new(DoNothing::new(LOOK_DO_NOTHING_MIN, LOOK_DO_NOTHING_MAX)),
                2,
            ),
        ])),
    )
}

/// Vanilla parity: the private `VillagerGoalPackages.getMinimalLookBehavior`.
fn minimal_look_behavior() -> (i32, Box<dyn BehaviorControl>) {
    (
        5,
        Box::new(RunOne::unconditional(vec![
            (
                OneShot::boxed(SetEntityLookTarget::of_type(
                    &vanilla_entities::VILLAGER,
                    LOOK_DISTANCE,
                )),
                2,
            ),
            (
                OneShot::boxed(SetEntityLookTarget::of_type(
                    &vanilla_entities::PLAYER,
                    LOOK_DISTANCE,
                )),
                2,
            ),
            (
                Box::new(DoNothing::new(LOOK_DO_NOTHING_MIN, LOOK_DO_NOTHING_MAX)),
                8,
            ),
        ])),
    )
}
