//! Framework-level brain tests.
//!
//! These drive a brain directly rather than through a mob, so each one pins
//! down one rule of `Brain.tick` on its own: when a memory expires, when an
//! activity may become active, and when a behavior is stopped. The copper
//! golem exercises the same machinery end to end in
//! `crate::entity::tests::brains`.

use std::io::Cursor;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Weak};

use foton_registry::entity_type::EntityTypeRef;
use foton_registry::{init_vanilla_registry, vanilla_entities};
use foton_utils::locks::SyncMutex;
use foton_utils::{BlockPos, GlobalPos};
use glam::DVec3;
use rustc_hash::FxHashSet;
use simdnbt::borrow::read_compound as read_borrowed_compound;
use simdnbt::owned::NbtCompound;

use super::behavior::{
    Behavior, BehaviorControl, DoNothing, OneShot, RunOne, TimedBehavior, Trigger,
};
use super::context::BrainContext;
use super::memory::{MemoryModuleId, MemoryStatus, memory_module_types};
use super::{Activity, ActivityData, Brain};
use crate::entity::{
    Entity, EntityBase, LivingEntity, LivingEntityBase, Mob, MobBase, PathfinderMob,
};
use crate::test_support::fresh_test_world;
use crate::world::World;

/// A mob that exists only to own a brain.
struct TestBrainMob {
    base: EntityBase,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    mob_flags: SyncMutex<i8>,
    health: SyncMutex<f32>,
    brain: Brain,
}

impl TestBrainMob {
    fn new(brain: Brain) -> Self {
        init_vanilla_registry();
        Self {
            base: EntityBase::new(
                1,
                DVec3::ZERO,
                vanilla_entities::PIG.dimensions,
                Weak::new(),
            ),
            living_base: LivingEntityBase::new(&vanilla_entities::PIG),
            mob_base: MobBase::new(),
            mob_flags: SyncMutex::new(0),
            health: SyncMutex::new(10.0),
            brain,
        }
    }
}

crate::entity::impl_test_downcast_type!(TestBrainMob);

impl Entity for TestBrainMob {
    fn base(&self) -> &EntityBase {
        &self.base
    }

    fn entity_type(&self) -> EntityTypeRef {
        &vanilla_entities::PIG
    }
}

impl LivingEntity for TestBrainMob {
    fn living_base(&self) -> &LivingEntityBase {
        &self.living_base
    }

    fn get_health(&self) -> f32 {
        *self.health.lock()
    }

    fn set_health(&self, health: f32) {
        *self.health.lock() = health;
    }
}

impl Mob for TestBrainMob {
    fn mob_base(&self) -> &MobBase {
        &self.mob_base
    }

    fn brain(&self) -> Option<&Brain> {
        Some(&self.brain)
    }

    fn mob_flags(&self) -> i8 {
        *self.mob_flags.lock()
    }

    fn set_mob_flags(&self, flags: i8) {
        *self.mob_flags.lock() = flags;
    }
}

impl PathfinderMob for TestBrainMob {}

/// A behavior that counts its calls and can be told to give up.
struct RecordingBehavior {
    entry_condition: Vec<(MemoryModuleId, MemoryStatus)>,
    starts: &'static AtomicUsize,
    stops: &'static AtomicUsize,
    can_still_use: bool,
}

impl RecordingBehavior {
    fn new(
        entry_condition: Vec<(MemoryModuleId, MemoryStatus)>,
        starts: &'static AtomicUsize,
        stops: &'static AtomicUsize,
    ) -> Self {
        Self {
            entry_condition,
            starts,
            stops,
            can_still_use: true,
        }
    }

    const fn giving_up_immediately(mut self) -> Self {
        self.can_still_use = false;
        self
    }
}

impl TimedBehavior for RecordingBehavior {
    fn entry_condition(&self) -> &[(MemoryModuleId, MemoryStatus)] {
        &self.entry_condition
    }

    fn duration(&self) -> (i32, i32) {
        (i32::MAX, i32::MAX)
    }

    fn can_still_use(&mut self, _ctx: &BrainContext<'_>) -> bool {
        self.can_still_use
    }

    fn start(&mut self, _ctx: &BrainContext<'_>) {
        self.starts.fetch_add(1, Ordering::Relaxed);
    }

    fn stop(&mut self, _ctx: &BrainContext<'_>) {
        self.stops.fetch_add(1, Ordering::Relaxed);
    }

    fn debug_name(&self) -> &'static str {
        "RecordingBehavior"
    }
}

/// A trigger that counts how many times it fired.
struct RecordingTrigger {
    fired: &'static AtomicUsize,
}

impl Trigger for RecordingTrigger {
    fn trigger(&mut self, _ctx: &BrainContext<'_>) -> bool {
        self.fired.fetch_add(1, Ordering::Relaxed);
        true
    }

    fn debug_name(&self) -> &'static str {
        "RecordingTrigger"
    }
}

fn brain_world(key: &'static str) -> Arc<World> {
    init_vanilla_registry();
    fresh_test_world(key)
}

static IDLE_STARTS: AtomicUsize = AtomicUsize::new(0);
static IDLE_STOPS: AtomicUsize = AtomicUsize::new(0);
static GIVING_UP_STARTS: AtomicUsize = AtomicUsize::new(0);
static GIVING_UP_STOPS: AtomicUsize = AtomicUsize::new(0);
static ONE_SHOT_FIRED: AtomicUsize = AtomicUsize::new(0);
static RUN_ONE_FIRST: AtomicUsize = AtomicUsize::new(0);
static RUN_ONE_SECOND: AtomicUsize = AtomicUsize::new(0);

#[test]
fn a_memory_with_a_time_to_live_is_forgotten_when_it_runs_out() {
    let world = brain_world("brain_memory_expiry");
    let brain = Brain::new(
        &[],
        vec![ActivityData::create(
            Activity::Idle,
            0,
            vec![Behavior::boxed(RecordingBehavior::new(
                vec![(
                    memory_module_types::GAZE_COOLDOWN_TICKS.id(),
                    MemoryStatus::Registered,
                )],
                &IDLE_STARTS,
                &IDLE_STOPS,
            ))],
        )],
    );
    brain.set_memory_with_expiry(memory_module_types::GAZE_COOLDOWN_TICKS, 7, 2);
    let mob = TestBrainMob::new(brain);

    assert_eq!(
        mob.brain
            .get_memory(memory_module_types::GAZE_COOLDOWN_TICKS),
        Some(7)
    );

    mob.brain.tick(&world, &mob);
    assert_eq!(
        mob.brain
            .get_memory(memory_module_types::GAZE_COOLDOWN_TICKS),
        Some(7),
        "one tick only spends one of the two ticks the memory was given"
    );

    mob.brain.tick(&world, &mob);
    mob.brain.tick(&world, &mob);
    assert_eq!(
        mob.brain
            .get_memory(memory_module_types::GAZE_COOLDOWN_TICKS),
        None,
        "the memory should be gone once its time to live is spent"
    );
}

#[test]
fn an_activity_becomes_active_only_when_its_memories_are_in_the_required_state() {
    let world = brain_world("brain_activity_requirements");
    let brain = Brain::new(
        &[],
        vec![
            ActivityData::create(Activity::Idle, 0, vec![]),
            ActivityData::create(
                Activity::Work,
                0,
                vec![Behavior::boxed(RecordingBehavior::new(
                    vec![(memory_module_types::HOME.id(), MemoryStatus::Registered)],
                    &IDLE_STARTS,
                    &IDLE_STOPS,
                ))],
            )
            .with_conditions(vec![(
                memory_module_types::HOME.id(),
                MemoryStatus::ValuePresent,
            )]),
        ],
    );

    brain.set_active_activity_if_possible(Activity::Work);
    assert!(
        !brain.is_active(Activity::Work),
        "work requires a home, and the brain has none"
    );
    assert!(
        brain.is_active(Activity::Idle),
        "a rejected activity should fall back to the default"
    );

    brain.set_memory(
        memory_module_types::HOME,
        GlobalPos::new(world.key.clone(), BlockPos::new(1, 2, 3)),
    );
    brain.set_active_activity_if_possible(Activity::Work);
    assert!(brain.is_active(Activity::Work));
    assert!(
        brain.is_active(Activity::Core),
        "the core activity runs alongside whatever else is active"
    );
}

#[test]
fn a_behavior_that_can_no_longer_be_used_is_stopped_on_the_next_tick() {
    GIVING_UP_STARTS.store(0, Ordering::Relaxed);
    GIVING_UP_STOPS.store(0, Ordering::Relaxed);
    let world = brain_world("brain_can_still_use");
    let brain = Brain::new(
        &[],
        vec![ActivityData::create(
            Activity::Idle,
            0,
            vec![Behavior::boxed(
                RecordingBehavior::new(Vec::new(), &GIVING_UP_STARTS, &GIVING_UP_STOPS)
                    .giving_up_immediately(),
            )],
        )],
    );
    let mob = TestBrainMob::new(brain);

    mob.brain.tick(&world, &mob);
    assert_eq!(GIVING_UP_STARTS.load(Ordering::Relaxed), 1);
    assert_eq!(
        GIVING_UP_STOPS.load(Ordering::Relaxed),
        1,
        "the behavior is started and then ticked in the same brain tick, and its own \
         `can_still_use` is what ends it"
    );

    mob.brain.tick(&world, &mob);
    assert_eq!(
        GIVING_UP_STARTS.load(Ordering::Relaxed),
        2,
        "a stopped behavior is offered the chance to start again"
    );
}

#[test]
fn a_one_shot_stops_itself_the_tick_after_it_triggers() {
    ONE_SHOT_FIRED.store(0, Ordering::Relaxed);
    let world = brain_world("brain_one_shot");
    let brain = Brain::new(
        &[],
        vec![ActivityData::create(
            Activity::Idle,
            0,
            vec![OneShot::boxed(RecordingTrigger {
                fired: &ONE_SHOT_FIRED,
            })],
        )],
    );
    let mob = TestBrainMob::new(brain);

    mob.brain.tick(&world, &mob);
    mob.brain.tick(&world, &mob);

    assert_eq!(
        ONE_SHOT_FIRED.load(Ordering::Relaxed),
        2,
        "a one shot runs once per brain tick: it stops itself in the same tick it started"
    );
}

#[test]
fn run_one_starts_only_the_first_child_that_accepts() {
    RUN_ONE_FIRST.store(0, Ordering::Relaxed);
    RUN_ONE_SECOND.store(0, Ordering::Relaxed);
    let world = brain_world("brain_run_one");
    let brain = Brain::new(
        &[],
        vec![ActivityData::with_priorities(
            Activity::Idle,
            vec![(
                0,
                Box::new(RunOne::unconditional(vec![
                    (
                        Behavior::boxed(RecordingBehavior::new(
                            Vec::new(),
                            &RUN_ONE_FIRST,
                            &IDLE_STOPS,
                        )),
                        1,
                    ),
                    (
                        Behavior::boxed(RecordingBehavior::new(
                            Vec::new(),
                            &RUN_ONE_SECOND,
                            &IDLE_STOPS,
                        )),
                        1,
                    ),
                ])) as Box<dyn BehaviorControl>,
            )],
        )],
    );
    let mob = TestBrainMob::new(brain);

    mob.brain.tick(&world, &mob);

    assert_eq!(
        RUN_ONE_FIRST.load(Ordering::Relaxed) + RUN_ONE_SECOND.load(Ordering::Relaxed),
        1,
        "a RunOne gate starts exactly one of its children, whichever the shuffle put first"
    );
}

#[test]
fn leaving_an_activity_erases_the_memories_it_declared() {
    let world = brain_world("brain_activity_cleanup");
    let brain = Brain::new(
        &[],
        vec![
            ActivityData::create(Activity::Idle, 0, vec![]),
            ActivityData::create(
                Activity::Work,
                0,
                vec![Behavior::boxed(RecordingBehavior::new(
                    vec![(memory_module_types::HOME.id(), MemoryStatus::Registered)],
                    &IDLE_STARTS,
                    &IDLE_STOPS,
                ))],
            )
            .gated_by(memory_module_types::HOME.id()),
        ],
    );

    brain.set_memory(
        memory_module_types::HOME,
        GlobalPos::new(world.key.clone(), BlockPos::new(1, 2, 3)),
    );
    brain.set_active_activity_if_possible(Activity::Work);
    assert!(brain.is_active(Activity::Work));

    brain.use_default_activity();

    assert!(brain.is_active(Activity::Idle));
    assert_eq!(
        brain.get_memory(memory_module_types::HOME),
        None,
        "an activity that declared a memory to erase when stopped should have erased it"
    );
}

#[test]
fn only_the_memories_vanilla_gives_a_codec_survive_a_save_and_load() {
    let world = brain_world("brain_save_load");
    let make = || {
        Brain::new(
            &[],
            vec![ActivityData::create(
                Activity::Idle,
                0,
                vec![Behavior::boxed(RecordingBehavior::new(
                    vec![
                        (
                            memory_module_types::GAZE_COOLDOWN_TICKS.id(),
                            MemoryStatus::Registered,
                        ),
                        (
                            memory_module_types::TRANSPORT_ITEMS_COOLDOWN_TICKS.id(),
                            MemoryStatus::Registered,
                        ),
                        (
                            memory_module_types::VISITED_BLOCK_POSITIONS.id(),
                            MemoryStatus::Registered,
                        ),
                    ],
                    &IDLE_STARTS,
                    &IDLE_STOPS,
                ))],
            )],
        )
    };

    let saved = make();
    saved.set_memory(memory_module_types::GAZE_COOLDOWN_TICKS, 12);
    saved.set_memory(memory_module_types::TRANSPORT_ITEMS_COOLDOWN_TICKS, 34);
    let mut visited = FxHashSet::default();
    visited.insert(GlobalPos::new(world.key.clone(), BlockPos::new(4, 5, 6)));
    saved.set_memory_with_expiry(
        memory_module_types::VISITED_BLOCK_POSITIONS,
        visited.clone(),
        6000,
    );

    let mut nbt = NbtCompound::new();
    saved.save(&mut nbt);
    let mut bytes = Vec::new();
    nbt.write(&mut bytes);
    let borrowed = read_borrowed_compound(&mut Cursor::new(bytes.as_slice()))
        .expect("test NBT should reborrow");

    let loaded = make();
    loaded.load((&borrowed).into());

    assert_eq!(
        loaded.get_memory(memory_module_types::GAZE_COOLDOWN_TICKS),
        Some(12)
    );
    assert_eq!(
        loaded.get_memory(memory_module_types::VISITED_BLOCK_POSITIONS),
        Some(visited)
    );
    assert_eq!(
        loaded.time_until_expiry(memory_module_types::VISITED_BLOCK_POSITIONS),
        6000,
        "an expiring memory keeps the rest of its life across a save"
    );
    assert_eq!(
        loaded.get_memory(memory_module_types::TRANSPORT_ITEMS_COOLDOWN_TICKS),
        None,
        "vanilla registers the transport cooldown without a codec, so a reloaded golem \
         is ready to work rather than mid-cooldown"
    );
}

#[test]
fn do_nothing_holds_its_slot_until_its_duration_is_up() {
    let world = brain_world("brain_do_nothing");
    let brain = Brain::new(
        &[],
        vec![ActivityData::create(
            Activity::Idle,
            0,
            vec![Box::new(DoNothing::new(3, 3))],
        )],
    );
    let mob = TestBrainMob::new(brain);

    mob.brain.tick(&world, &mob);
    assert_eq!(mob.brain.running_behaviors(), vec!["DoNothing"]);
}
