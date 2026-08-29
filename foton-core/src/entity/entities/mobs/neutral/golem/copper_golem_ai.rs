//! The copper golem's brain.
//!
//! Vanilla parity: `net.minecraft.world.entity.animal.golem.CopperGolemAi`.

use foton_registry::blocks::block_state_ext::BlockStateExt as _;
use foton_registry::sound_event::SoundEventRef;
use foton_registry::vanilla_block_tags::BlockTag;
use foton_registry::{sound_events, vanilla_blocks, vanilla_entities};
use foton_utils::Downcast as _;
use foton_utils::value_providers::UniformIntProvider;
use rustc_hash::FxHashMap;

use super::copper_golem::{CopperGolemEntity, CopperGolemState};
use crate::entity::LivingEntity as _;
use crate::entity::PathfinderMob;
use crate::entity::ai::brain::behavior::transport_items_between_containers::{
    ContainerInteractionState, OnTargetReachedInteraction, TARGET_INTERACTION_TIME,
    TransportItemTarget,
};
use crate::entity::ai::brain::behavior::{
    AnimalPanic, Behavior, CountDownCooldownTicks, DoNothing, LookAtTargetSink, MoveToTargetSink,
    OneShot, RandomStroll, RunOne, SetEntityLookTargetSometimes, TransportItemsBetweenContainers,
};
use crate::entity::ai::brain::memory::{MemoryStatus, memory_module_types};
use crate::entity::ai::brain::sensor::SensorType;
use crate::entity::ai::brain::{Activity, ActivityData, Brain};

/// Vanilla parity: `CopperGolemAi.SPEED_MULTIPLIER_WHEN_PANICKING`.
const SPEED_MULTIPLIER_WHEN_PANICKING: f64 = 1.5;
/// Vanilla parity: `CopperGolemAi.SPEED_MULTIPLIER_WHEN_IDLING`.
const SPEED_MULTIPLIER_WHEN_IDLING: f64 = 1.0;
/// Vanilla parity: `CopperGolemAi.TRANSPORT_ITEM_HORIZONTAL_SEARCH_RADIUS`.
const TRANSPORT_ITEM_HORIZONTAL_SEARCH_RADIUS: i32 = 32;
/// Vanilla parity: `CopperGolemAi.TRANSPORT_ITEM_VERTICAL_SEARCH_RADIUS`.
const TRANSPORT_ITEM_VERTICAL_SEARCH_RADIUS: i32 = 8;
/// Vanilla parity: `CopperGolemAi.TICK_TO_START_ON_REACHED_INTERACTION`.
const TICK_TO_START_ON_REACHED_INTERACTION: i32 = 1;
/// Vanilla parity: `CopperGolemAi.TICK_TO_PLAY_ON_REACHED_SOUND`.
const TICK_TO_PLAY_ON_REACHED_SOUND: i32 = 9;
/// Vanilla parity: the `LookAtTargetSink(45, 90)` of the core activity.
const LOOK_AT_TARGET_MIN_DURATION: i32 = 45;
const LOOK_AT_TARGET_MAX_DURATION: i32 = 90;
/// Vanilla parity: the `SetEntityLookTargetSometimes.create(PLAYER, 6.0F, UniformInt.of(40, 80))`
/// of the idle activity.
const GAZE_AT_PLAYER_RANGE: f64 = 6.0;
const GAZE_INTERVAL: UniformIntProvider = UniformIntProvider {
    min_inclusive: 40,
    max_inclusive: 80,
};
/// Vanilla parity: the `RandomStroll.stroll(1.0F, 2, 2)` of the idle activity.
const IDLE_STROLL_HORIZONTAL_RANGE: i32 = 2;
const IDLE_STROLL_VERTICAL_RANGE: i32 = 2;
/// Vanilla parity: the `DoNothing(30, 60)` of the idle activity.
const IDLE_DO_NOTHING_MIN: i32 = 30;
const IDLE_DO_NOTHING_MAX: i32 = 60;
/// Vanilla parity: the `nextInt(60, 100)` the constructor seeds the cooldown with.
const SPAWN_COOLDOWN_MIN: i32 = 60;
const SPAWN_COOLDOWN_MAX: i32 = 100;

/// Builds the copper golem's brain.
///
/// Vanilla parity: `CopperGolem.BRAIN_PROVIDER` plus the
/// `getBrain().setMemory(TRANSPORT_ITEMS_COOLDOWN_TICKS, ...)` of its
/// constructor. Vanilla's `Brain.Provider` is a static that `makeBrain` calls
/// with the body; the copper golem's activity supplier ignores the body, and a
/// Rust brain is a field of the mob so it cannot see the mob while it is being
/// built, so the provider collapses into this one function.
#[must_use]
pub fn make_brain() -> Brain {
    let brain = Brain::new(
        &[SensorType::NearestLivingEntities, SensorType::HurtBy],
        vec![core_activity(), idle_activity()],
    );
    brain.set_memory(
        memory_module_types::TRANSPORT_ITEMS_COOLDOWN_TICKS,
        rand::random_range(SPAWN_COOLDOWN_MIN..SPAWN_COOLDOWN_MAX),
    );
    brain
}

/// Picks the activity the golem should be in.
///
/// Vanilla parity: `CopperGolemAi.updateActivity`.
pub fn update_activity(brain: &Brain) {
    brain.set_active_activity_to_first_valid(&[Activity::Idle]);
}

/// Vanilla parity: `CopperGolemAi.initCoreActivity`.
fn core_activity() -> ActivityData {
    ActivityData::create(
        Activity::Core,
        0,
        vec![
            Behavior::boxed(AnimalPanic::new(SPEED_MULTIPLIER_WHEN_PANICKING)),
            Behavior::boxed(LookAtTargetSink::new(
                LOOK_AT_TARGET_MIN_DURATION,
                LOOK_AT_TARGET_MAX_DURATION,
            )),
            Behavior::boxed(MoveToTargetSink::new()),
            Behavior::boxed(CountDownCooldownTicks::new(
                memory_module_types::GAZE_COOLDOWN_TICKS,
            )),
            Behavior::boxed(CountDownCooldownTicks::new(
                memory_module_types::TRANSPORT_ITEMS_COOLDOWN_TICKS,
            )),
        ],
    )
}

/// Vanilla parity: `CopperGolemAi.initIdleActivity`.
///
/// `InteractWithDoor` is the one behavior of the vanilla core activity that is
/// missing: it needs `MemoryModuleType.DOORS_TO_CLOSE` driven from the path's
/// previous and next nodes, and Foton's `Path` exposes those, but the door half
/// of the port belongs with the villagers that share the door rather than with
/// the golem, which only ever opens one on its way past.
fn idle_activity() -> ActivityData {
    ActivityData::with_priorities(
        Activity::Idle,
        vec![
            (
                0,
                Behavior::boxed(TransportItemsBetweenContainers::new(
                    SPEED_MULTIPLIER_WHEN_IDLING,
                    Box::new(|state| state.get_block().has_tag(&BlockTag::COPPER_CHESTS)),
                    Box::new(|state| {
                        state.get_block() == &vanilla_blocks::CHEST
                            || state.get_block() == &vanilla_blocks::TRAPPED_CHEST
                    }),
                    TRANSPORT_ITEM_HORIZONTAL_SEARCH_RADIUS,
                    TRANSPORT_ITEM_VERTICAL_SEARCH_RADIUS,
                    target_reached_interactions(),
                    Box::new(|body| {
                        let Some(golem) = body.downcast_ref::<CopperGolemEntity>() else {
                            return;
                        };
                        golem.clear_opened_chest_pos();
                        golem.set_state(CopperGolemState::Idle);
                    }),
                    // Vanilla asks the chest whether any `ContainerUser` has it
                    // open. Foton's `ContainerOpenersCounter` port is a plain
                    // count on `BlockEntityBase` with no opener list, so a
                    // non-zero count is the same answer: someone is inside.
                    Box::new(|target| target.block_entity().base().opener_count() > 0),
                )),
            ),
            (
                1,
                OneShot::boxed(SetEntityLookTargetSometimes::of_type(
                    &vanilla_entities::PLAYER,
                    GAZE_AT_PLAYER_RANGE,
                    GAZE_INTERVAL,
                )),
            ),
            (
                2,
                Box::new(RunOne::gated(
                    vec![
                        (
                            memory_module_types::WALK_TARGET.id(),
                            MemoryStatus::ValueAbsent,
                        ),
                        (
                            memory_module_types::TRANSPORT_ITEMS_COOLDOWN_TICKS.id(),
                            MemoryStatus::ValuePresent,
                        ),
                    ],
                    vec![
                        (
                            OneShot::boxed(RandomStroll::stroll_within(
                                SPEED_MULTIPLIER_WHEN_IDLING,
                                IDLE_STROLL_HORIZONTAL_RANGE,
                                IDLE_STROLL_VERTICAL_RANGE,
                            )),
                            1,
                        ),
                        (
                            Box::new(DoNothing::new(IDLE_DO_NOTHING_MIN, IDLE_DO_NOTHING_MAX)),
                            1,
                        ),
                    ],
                )),
            ),
        ],
    )
}

/// Vanilla parity: `CopperGolemAi.getTargetReachedInteractions`.
///
/// The two pickup sounds are registered under paths that read backwards:
/// `SoundEvents.COPPER_GOLEM_ITEM_GET` is `entity.copper_golem.no_item_get` and
/// `COPPER_GOLEM_ITEM_NO_GET` is `entity.copper_golem.no_item_no_get`. The
/// registry keys are what the client resolves, so they are matched here rather
/// than corrected.
fn target_reached_interactions() -> FxHashMap<ContainerInteractionState, OnTargetReachedInteraction>
{
    let mut actions: FxHashMap<ContainerInteractionState, OnTargetReachedInteraction> =
        FxHashMap::default();
    actions.insert(
        ContainerInteractionState::PickupItem,
        on_reached_target_interaction(
            CopperGolemState::GettingItem,
            Some(&sound_events::ENTITY_COPPER_GOLEM_NO_ITEM_GET),
        ),
    );
    actions.insert(
        ContainerInteractionState::PickupNoItem,
        on_reached_target_interaction(
            CopperGolemState::GettingNoItem,
            Some(&sound_events::ENTITY_COPPER_GOLEM_NO_ITEM_NO_GET),
        ),
    );
    actions.insert(
        ContainerInteractionState::PlaceItem,
        on_reached_target_interaction(
            CopperGolemState::DroppingItem,
            Some(&sound_events::ENTITY_COPPER_GOLEM_ITEM_DROP),
        ),
    );
    actions.insert(
        ContainerInteractionState::PlaceNoItem,
        on_reached_target_interaction(
            CopperGolemState::DroppingNoItem,
            Some(&sound_events::ENTITY_COPPER_GOLEM_ITEM_NO_DROP),
        ),
    );
    actions
}

/// Vanilla parity: `CopperGolemAi.onReachedTargetInteraction`.
fn on_reached_target_interaction(
    state: CopperGolemState,
    sound: Option<SoundEventRef>,
) -> OnTargetReachedInteraction {
    Box::new(
        move |body: &dyn PathfinderMob, target: &TransportItemTarget, ticks: i32| {
            let Some(golem) = body.downcast_ref::<CopperGolemEntity>() else {
                return;
            };

            if ticks == TICK_TO_START_ON_REACHED_INTERACTION {
                target.block_entity().base().increment_openers();
                golem.set_opened_chest_pos(target.pos());
                golem.set_state(state);
            }

            if ticks == TICK_TO_PLAY_ON_REACHED_SOUND
                && let Some(sound) = sound
            {
                golem.make_sound(Some(sound));
            }

            if ticks == TARGET_INTERACTION_TIME {
                // Vanilla asks the counter whether this body is among its
                // openers; Foton's counter has no opener list, so the golem's
                // own record of which chest it opened answers the same
                // question and keeps the count balanced.
                if golem.opened_chest_pos() == Some(target.pos()) {
                    target.block_entity().base().decrement_openers();
                }
                golem.clear_opened_chest_pos();
            }
        },
    )
}
