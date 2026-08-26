//! The behaviors only a warden runs.
//!
//! Vanilla parity: `net.minecraft.world.entity.ai.behavior.warden`. Each one is short;
//! together they are the whole visible warden -- it comes out of the ground, sniffs,
//! roars, screams at range and digs back down.

use glam::DVec3;
use steel_registry::entity_data::EntityPose;
use steel_registry::entity_type::EntityAttachment;
use steel_registry::particle_type::ParticleData;
use steel_registry::vanilla_attributes;
use steel_registry::{sound_events, vanilla_damage_types, vanilla_particle_types};
use steel_utils::Downcast as _;
use steel_utils::entity_events::EntityStatus;

use crate::entity::ai::brain::behavior::{
    BehaviorControl, BrainContext, MemoryModuleId, MemoryStatus, OneShot, TimedBehavior, Trigger,
};
use crate::entity::ai::brain::memory::{EntityMemory, Unit, memory_module_types};
use crate::entity::ai::brain::position_tracker::PositionTracker;
use crate::entity::callback::RemovalReason;
use crate::entity::damage::DamageSource;
use crate::entity::{Entity, Mob, SharedEntity};

use super::entity::WardenEntity;
use super::warden_ai;

/// Runs `action` with the warden this brain drives.
fn with_warden<R>(ctx: &BrainContext<'_>, action: impl FnOnce(&WardenEntity) -> R) -> Option<R> {
    ctx.mob().downcast_ref::<WardenEntity>().map(action)
}

/// Vanilla `Digging`.
///
/// The warden burrows back into the ground and deletes itself. Nothing else in the game
/// removes a healthy mob on purpose, which is why the removal is the whole of `stop`.
pub struct Digging {
    duration: i32,
}

impl Digging {
    /// Digs for `duration` ticks and then leaves.
    #[must_use]
    pub const fn new(duration: i32) -> Self {
        Self { duration }
    }
}

impl TimedBehavior for Digging {
    fn entry_condition(&self) -> &[(MemoryModuleId, MemoryStatus)] {
        &DIGGING_CONDITION
    }

    fn duration(&self) -> (i32, i32) {
        (self.duration, self.duration)
    }

    fn can_still_use(&mut self, ctx: &BrainContext<'_>) -> bool {
        ctx.mob().removal_reason().is_none()
    }

    fn check_extra_start_conditions(&mut self, ctx: &BrainContext<'_>) -> bool {
        let mob = ctx.mob();
        mob.on_ground() || mob.is_in_water() || mob.is_in_lava()
    }

    fn start(&mut self, ctx: &BrainContext<'_>) {
        let mob = ctx.mob();
        if mob.on_ground() {
            mob.set_pose(EntityPose::Digging);
            mob.play_sound(&sound_events::ENTITY_WARDEN_DIG, 5.0, 1.0);
            return;
        }
        // A warden that ran out of dig cooldown in mid-air just complains.
        mob.play_sound(&sound_events::ENTITY_WARDEN_AGITATED, 5.0, 1.0);
        self.stop(ctx);
    }

    fn stop(&mut self, ctx: &BrainContext<'_>) {
        if ctx.mob().removal_reason().is_none() {
            ctx.mob().set_removed(RemovalReason::Discarded);
        }
    }

    fn debug_name(&self) -> &'static str {
        "warden_digging"
    }
}

/// Vanilla `Emerging`.
pub struct Emerging {
    duration: i32,
}

impl Emerging {
    /// Emerges over `duration` ticks.
    #[must_use]
    pub const fn new(duration: i32) -> Self {
        Self { duration }
    }
}

impl TimedBehavior for Emerging {
    fn entry_condition(&self) -> &[(MemoryModuleId, MemoryStatus)] {
        &EMERGING_CONDITION
    }

    fn duration(&self) -> (i32, i32) {
        (self.duration, self.duration)
    }

    fn can_still_use(&mut self, _ctx: &BrainContext<'_>) -> bool {
        true
    }

    fn start(&mut self, ctx: &BrainContext<'_>) {
        ctx.mob().set_pose(EntityPose::Emerging);
        ctx.mob()
            .play_sound(&sound_events::ENTITY_WARDEN_EMERGE, 5.0, 1.0);
    }

    fn stop(&mut self, ctx: &BrainContext<'_>) {
        if ctx.mob().pose() == EntityPose::Emerging {
            ctx.mob().set_pose(EntityPose::Standing);
        }
    }

    fn debug_name(&self) -> &'static str {
        "warden_emerging"
    }
}

/// Vanilla `ForceUnmount`.
///
/// A warden about to dig cannot do it from inside a boat.
pub struct ForceUnmount;

impl TimedBehavior for ForceUnmount {
    fn entry_condition(&self) -> &[(MemoryModuleId, MemoryStatus)] {
        &[]
    }

    fn check_extra_start_conditions(&mut self, ctx: &BrainContext<'_>) -> bool {
        ctx.mob().is_passenger()
    }

    fn start(&mut self, ctx: &BrainContext<'_>) {
        ctx.mob().stop_riding();
    }

    fn debug_name(&self) -> &'static str {
        "warden_force_unmount"
    }
}

/// Vanilla `Roar.TICKS_BEFORE_PLAYING_ROAR_SOUND`.
const TICKS_BEFORE_PLAYING_ROAR_SOUND: i64 = 25;
/// Vanilla `Roar.ROAR_ANGER_INCREASE`.
const ROAR_ANGER_INCREASE: i32 = 20;

/// Vanilla `Roar`.
///
/// The roar is the warden committing: it ends by promoting the roar target to the attack
/// target, which is the moment a player stops being a suspicion and starts being prey.
pub struct Roar;

impl TimedBehavior for Roar {
    fn entry_condition(&self) -> &[(MemoryModuleId, MemoryStatus)] {
        &ROAR_CONDITION
    }

    fn duration(&self) -> (i32, i32) {
        (warden_ai::ROAR_DURATION, warden_ai::ROAR_DURATION)
    }

    fn can_still_use(&mut self, _ctx: &BrainContext<'_>) -> bool {
        true
    }

    fn start(&mut self, ctx: &BrainContext<'_>) {
        let brain = ctx.brain();
        brain.set_memory_with_expiry(
            memory_module_types::ROAR_SOUND_DELAY,
            Unit,
            TICKS_BEFORE_PLAYING_ROAR_SOUND,
        );
        brain.erase_memory(memory_module_types::WALK_TARGET.id());
        let Some(target) = brain
            .get_memory(memory_module_types::ROAR_TARGET)
            .and_then(|memory| memory.get())
        else {
            return;
        };
        brain.set_memory(
            memory_module_types::LOOK_TARGET,
            PositionTracker::of_entity(&target, true),
        );
        ctx.mob().set_pose(EntityPose::Roaring);
        with_warden(ctx, |warden| {
            warden.increase_anger_at_by(Some(target.as_ref()), ROAR_ANGER_INCREASE, false);
        });
    }

    fn tick(&mut self, ctx: &BrainContext<'_>) {
        let brain = ctx.brain();
        if brain.has_memory_value(memory_module_types::ROAR_SOUND_DELAY.id())
            || brain.has_memory_value(memory_module_types::ROAR_SOUND_COOLDOWN.id())
        {
            return;
        }
        brain.set_memory_with_expiry(
            memory_module_types::ROAR_SOUND_COOLDOWN,
            Unit,
            i64::from(warden_ai::ROAR_DURATION) - TICKS_BEFORE_PLAYING_ROAR_SOUND,
        );
        ctx.mob()
            .play_sound(&sound_events::ENTITY_WARDEN_ROAR, 3.0, 1.0);
    }

    fn stop(&mut self, ctx: &BrainContext<'_>) {
        if ctx.mob().pose() == EntityPose::Roaring {
            ctx.mob().set_pose(EntityPose::Standing);
        }
        let target = ctx
            .brain()
            .get_memory(memory_module_types::ROAR_TARGET)
            .and_then(|memory| memory.get());
        if let Some(target) = target {
            with_warden(ctx, |warden| warden.set_attack_target(&target));
        }
        ctx.brain()
            .erase_memory(memory_module_types::ROAR_TARGET.id());
    }

    fn debug_name(&self) -> &'static str {
        "warden_roar"
    }
}

/// Vanilla `SetRoarTarget`.
pub struct SetRoarTarget {
    find_target: fn(&WardenEntity) -> Option<SharedEntity>,
}

impl SetRoarTarget {
    /// Takes whatever `find_target` names as the thing to roar at.
    #[must_use]
    pub const fn new(find_target: fn(&WardenEntity) -> Option<SharedEntity>) -> Self {
        Self { find_target }
    }
}

impl Trigger for SetRoarTarget {
    fn required_memories(&self) -> Vec<MemoryModuleId> {
        vec![
            memory_module_types::ROAR_TARGET.id(),
            memory_module_types::ATTACK_TARGET.id(),
            memory_module_types::CANT_REACH_WALK_TARGET_SINCE.id(),
        ]
    }

    fn trigger(&mut self, ctx: &BrainContext<'_>) -> bool {
        let brain = ctx.brain();
        if brain.has_memory_value(memory_module_types::ROAR_TARGET.id())
            || brain.has_memory_value(memory_module_types::ATTACK_TARGET.id())
        {
            return false;
        }

        let Some(Some(target)) = with_warden(ctx, |warden| {
            (self.find_target)(warden)
                .filter(|target| warden.can_target_entity(Some(target.as_ref())))
        }) else {
            return false;
        };

        brain.set_memory(memory_module_types::ROAR_TARGET, EntityMemory::new(&target));
        brain.erase_memory(memory_module_types::CANT_REACH_WALK_TARGET_SINCE.id());
        true
    }

    fn debug_name(&self) -> &'static str {
        "warden_set_roar_target"
    }
}

/// Vanilla `SetWardenLookTarget`.
///
/// A blind mob still turns toward what it heard, and that is what this does: the roar
/// target if there is one, otherwise the last disturbance.
pub struct SetWardenLookTarget;

impl Trigger for SetWardenLookTarget {
    fn required_memories(&self) -> Vec<MemoryModuleId> {
        vec![
            memory_module_types::LOOK_TARGET.id(),
            memory_module_types::DISTURBANCE_LOCATION.id(),
            memory_module_types::ROAR_TARGET.id(),
            memory_module_types::ATTACK_TARGET.id(),
        ]
    }

    fn trigger(&mut self, ctx: &BrainContext<'_>) -> bool {
        let brain = ctx.brain();
        if brain.has_memory_value(memory_module_types::ATTACK_TARGET.id()) {
            return false;
        }

        let target = brain
            .get_memory(memory_module_types::ROAR_TARGET)
            .and_then(|memory| memory.get())
            .map(|entity| entity.block_position())
            .or_else(|| brain.get_memory(memory_module_types::DISTURBANCE_LOCATION));
        let Some(target) = target else {
            return false;
        };

        brain.set_memory(
            memory_module_types::LOOK_TARGET,
            PositionTracker::of_block(target),
        );
        true
    }

    fn debug_name(&self) -> &'static str {
        "warden_set_look_target"
    }
}

/// Vanilla `TryToSniff.SNIFF_COOLDOWN`, `UniformInt.of(100, 200)`.
const SNIFF_COOLDOWN_MIN: i64 = 100;
const SNIFF_COOLDOWN_MAX: i64 = 200;

/// Vanilla `TryToSniff`.
pub struct TryToSniff;

impl Trigger for TryToSniff {
    fn required_memories(&self) -> Vec<MemoryModuleId> {
        vec![
            memory_module_types::IS_SNIFFING.id(),
            memory_module_types::WALK_TARGET.id(),
            memory_module_types::SNIFF_COOLDOWN.id(),
            memory_module_types::NEAREST_ATTACKABLE.id(),
            memory_module_types::DISTURBANCE_LOCATION.id(),
        ]
    }

    fn trigger(&mut self, ctx: &BrainContext<'_>) -> bool {
        let brain = ctx.brain();
        if brain.has_memory_value(memory_module_types::SNIFF_COOLDOWN.id())
            || !brain.has_memory_value(memory_module_types::NEAREST_ATTACKABLE.id())
            || brain.has_memory_value(memory_module_types::DISTURBANCE_LOCATION.id())
        {
            return false;
        }

        brain.set_memory(memory_module_types::IS_SNIFFING, Unit);
        brain.set_memory_with_expiry(
            memory_module_types::SNIFF_COOLDOWN,
            Unit,
            rand::random_range(SNIFF_COOLDOWN_MIN..=SNIFF_COOLDOWN_MAX),
        );
        brain.erase_memory(memory_module_types::WALK_TARGET.id());
        ctx.mob().set_pose(EntityPose::Sniffing);
        true
    }

    fn debug_name(&self) -> &'static str {
        "warden_try_to_sniff"
    }
}

/// Vanilla `Sniffing.ANGER_FROM_SNIFFING_MAX_DISTANCE_XZ`.
const ANGER_FROM_SNIFFING_MAX_DISTANCE_XZ: f64 = 6.0;
/// Vanilla `Sniffing.ANGER_FROM_SNIFFING_MAX_DISTANCE_Y`.
const ANGER_FROM_SNIFFING_MAX_DISTANCE_Y: f64 = 20.0;

/// Vanilla `Sniffing`.
///
/// The sniff is how a warden finds someone standing still: it costs the player nothing to
/// be quiet, but a warden that sniffs them out is angry at them anyway.
pub struct Sniffing {
    duration: i32,
}

impl Sniffing {
    /// Sniffs for `duration` ticks.
    #[must_use]
    pub const fn new(duration: i32) -> Self {
        Self { duration }
    }
}

impl TimedBehavior for Sniffing {
    fn entry_condition(&self) -> &[(MemoryModuleId, MemoryStatus)] {
        &SNIFFING_CONDITION
    }

    fn duration(&self) -> (i32, i32) {
        (self.duration, self.duration)
    }

    fn can_still_use(&mut self, _ctx: &BrainContext<'_>) -> bool {
        true
    }

    fn start(&mut self, ctx: &BrainContext<'_>) {
        ctx.mob()
            .play_sound(&sound_events::ENTITY_WARDEN_SNIFF, 5.0, 1.0);
    }

    fn stop(&mut self, ctx: &BrainContext<'_>) {
        if ctx.mob().pose() == EntityPose::Sniffing {
            ctx.mob().set_pose(EntityPose::Standing);
        }
        ctx.brain()
            .erase_memory(memory_module_types::IS_SNIFFING.id());

        let Some(found) = ctx
            .brain()
            .get_memory(memory_module_types::NEAREST_ATTACKABLE)
            .and_then(|memory| memory.get())
        else {
            return;
        };
        with_warden(ctx, |warden| {
            if !warden.can_target_entity(Some(found.as_ref())) {
                return;
            }
            if closer_than(
                warden.position(),
                found.position(),
                ANGER_FROM_SNIFFING_MAX_DISTANCE_XZ,
                ANGER_FROM_SNIFFING_MAX_DISTANCE_Y,
            ) {
                warden.increase_anger_at(Some(found.as_ref()));
            }
            if !ctx
                .brain()
                .has_memory_value(memory_module_types::DISTURBANCE_LOCATION.id())
            {
                warden_ai::set_disturbance_location(warden, found.block_position());
            }
        });
    }

    fn debug_name(&self) -> &'static str {
        "warden_sniffing"
    }
}

/// Vanilla `SonicBoom.DISTANCE_XZ`.
const SONIC_BOOM_DISTANCE_XZ: f64 = 15.0;
/// Vanilla `SonicBoom.DISTANCE_Y`.
const SONIC_BOOM_DISTANCE_Y: f64 = 20.0;
/// Vanilla `SonicBoom.KNOCKBACK_VERTICAL`.
const KNOCKBACK_VERTICAL: f64 = 0.5;
/// Vanilla `SonicBoom.KNOCKBACK_HORIZONTAL`.
const KNOCKBACK_HORIZONTAL: f64 = 2.5;
/// Vanilla `SonicBoom.COOLDOWN`.
pub const SONIC_BOOM_COOLDOWN: i64 = 40;
/// Vanilla `SonicBoom.TICKS_BEFORE_PLAYING_SOUND`, `Mth.ceil(34.0)`.
const TICKS_BEFORE_PLAYING_SOUND: i32 = 34;
/// Vanilla `SonicBoom.DURATION`, `Mth.ceil(60.0F)`.
const SONIC_BOOM_DURATION: i32 = 60;
/// Vanilla `SonicBoom` hurts for ten, straight through armor.
const SONIC_BOOM_DAMAGE: f32 = 10.0;
/// Vanilla adds seven to the floored distance when drawing the beam.
const SONIC_BOOM_PARTICLE_OVERSHOOT: i32 = 7;

/// Vanilla `SonicBoom`.
///
/// The one attack in the game that ignores armor entirely. It charges for thirty-four
/// ticks, which is the window a player has to break line of sight.
pub struct SonicBoom;

impl TimedBehavior for SonicBoom {
    fn entry_condition(&self) -> &[(MemoryModuleId, MemoryStatus)] {
        &SONIC_BOOM_CONDITION
    }

    fn duration(&self) -> (i32, i32) {
        (SONIC_BOOM_DURATION, SONIC_BOOM_DURATION)
    }

    fn can_still_use(&mut self, _ctx: &BrainContext<'_>) -> bool {
        true
    }

    fn check_extra_start_conditions(&mut self, ctx: &BrainContext<'_>) -> bool {
        let Some(target) = attack_target(ctx) else {
            return false;
        };
        closer_than(
            ctx.mob().position(),
            target.position(),
            SONIC_BOOM_DISTANCE_XZ,
            SONIC_BOOM_DISTANCE_Y,
        )
    }

    fn start(&mut self, ctx: &BrainContext<'_>) {
        let brain = ctx.brain();
        brain.set_memory_with_expiry(
            memory_module_types::ATTACK_COOLING_DOWN,
            true,
            SONIC_BOOM_DURATION.into(),
        );
        brain.set_memory_with_expiry(
            memory_module_types::SONIC_BOOM_SOUND_DELAY,
            Unit,
            TICKS_BEFORE_PLAYING_SOUND.into(),
        );
        ctx.mob().broadcast_entity_event(EntityStatus::SonicCharge);
        ctx.mob()
            .play_sound(&sound_events::ENTITY_WARDEN_SONIC_CHARGE, 3.0, 1.0);
    }

    fn tick(&mut self, ctx: &BrainContext<'_>) {
        if let Some(target) = attack_target(ctx) {
            ctx.mob()
                .mob_base()
                .controls()
                .lock()
                .look_control
                .set_look_at(target.position(), 10.0, 40.0);
        }

        let brain = ctx.brain();
        if brain.has_memory_value(memory_module_types::SONIC_BOOM_SOUND_DELAY.id())
            || brain.has_memory_value(memory_module_types::SONIC_BOOM_SOUND_COOLDOWN.id())
        {
            return;
        }
        brain.set_memory_with_expiry(
            memory_module_types::SONIC_BOOM_SOUND_COOLDOWN,
            Unit,
            i64::from(SONIC_BOOM_DURATION - TICKS_BEFORE_PLAYING_SOUND),
        );

        let Some(target) = attack_target(ctx) else {
            return;
        };
        let Some(fires) = with_warden(ctx, |warden| {
            warden.can_target_entity(Some(target.as_ref()))
                && closer_than(
                    warden.position(),
                    target.position(),
                    SONIC_BOOM_DISTANCE_XZ,
                    SONIC_BOOM_DISTANCE_Y,
                )
        }) else {
            return;
        };
        if !fires {
            return;
        }

        Self::fire(ctx, &target);
    }

    fn stop(&mut self, ctx: &BrainContext<'_>) {
        warden_ai::set_sonic_boom_cooldown(ctx.brain(), SONIC_BOOM_COOLDOWN);
    }

    fn debug_name(&self) -> &'static str {
        "warden_sonic_boom"
    }
}

impl SonicBoom {
    fn fire(ctx: &BrainContext<'_>, target: &SharedEntity) {
        let mob = ctx.mob();
        let source = mob.position() + warden_chest_attachment(mob);
        let Some(target_living) = target.as_living_entity() else {
            return;
        };
        let delta = target.position() + DVec3::new(0.0, target.get_eye_height(), 0.0) - source;
        let direction = delta.normalize_or_zero();
        let steps = delta.length().floor() as i32 + SONIC_BOOM_PARTICLE_OVERSHOOT;

        for step in 1..steps {
            let particle_pos = source + direction * f64::from(step);
            ctx.world().send_particles(
                ParticleData::simple(&vanilla_particle_types::SONIC_BOOM),
                particle_pos,
                1,
                DVec3::ZERO,
                0.0,
            );
        }

        mob.play_sound(&sound_events::ENTITY_WARDEN_SONIC_BOOM, 3.0, 1.0);
        let damage_source = DamageSource::environment(&vanilla_damage_types::SONIC_BOOM)
            .with_causing_entity(mob.id())
            .with_direct_entity(mob.id());
        if !target_living.hurt_server(ctx.world(), &damage_source, SONIC_BOOM_DAMAGE) {
            return;
        }

        // Vanilla scales the push by the target's knockback resistance rather than going
        // through `knockback`, so armor that resists knockback still resists this.
        let resistance = target_living
            .attributes()
            .lock()
            .get_value(vanilla_attributes::KNOCKBACK_RESISTANCE)
            .unwrap_or(0.0);
        let vertical = KNOCKBACK_VERTICAL * (1.0 - resistance);
        let horizontal = KNOCKBACK_HORIZONTAL * (1.0 - resistance);
        target.set_velocity(
            target.velocity()
                + DVec3::new(
                    direction.x * horizontal,
                    direction.y * vertical,
                    direction.z * horizontal,
                ),
        );
    }
}

/// Vanilla `body.getAttachments().get(EntityAttachment.WARDEN_CHEST, 0, body.getYRot())`,
/// which is where the sonic boom leaves from.
fn warden_chest_attachment(mob: &dyn Mob) -> DVec3 {
    let dimensions = mob.base().dimensions();
    dimensions.attachments.get_clamped(
        EntityAttachment::WardenChest,
        0,
        mob.rotation().0,
        dimensions,
    )
}

fn attack_target(ctx: &BrainContext<'_>) -> Option<SharedEntity> {
    ctx.brain()
        .get_memory(memory_module_types::ATTACK_TARGET)
        .and_then(|memory| memory.get())
}

/// Vanilla `Entity.closerThan(entity, horizontal, vertical)`.
fn closer_than(from: DVec3, to: DVec3, horizontal: f64, vertical: f64) -> bool {
    let dx = to.x - from.x;
    let dz = to.z - from.z;
    let dy = to.y - from.y;
    dx.hypot(dz) < horizontal && dy.abs() < vertical
}

/// Vanilla `Digging`'s entry condition.
static DIGGING_CONDITION: [(MemoryModuleId, MemoryStatus); 2] = [
    (
        memory_module_types::ATTACK_TARGET.id(),
        MemoryStatus::ValueAbsent,
    ),
    (
        memory_module_types::WALK_TARGET.id(),
        MemoryStatus::ValueAbsent,
    ),
];

/// Vanilla `Emerging`'s entry condition.
static EMERGING_CONDITION: [(MemoryModuleId, MemoryStatus); 3] = [
    (
        memory_module_types::IS_EMERGING.id(),
        MemoryStatus::ValuePresent,
    ),
    (
        memory_module_types::WALK_TARGET.id(),
        MemoryStatus::ValueAbsent,
    ),
    (
        memory_module_types::LOOK_TARGET.id(),
        MemoryStatus::Registered,
    ),
];

/// Vanilla `Roar`'s entry condition.
static ROAR_CONDITION: [(MemoryModuleId, MemoryStatus); 4] = [
    (
        memory_module_types::ROAR_TARGET.id(),
        MemoryStatus::ValuePresent,
    ),
    (
        memory_module_types::ATTACK_TARGET.id(),
        MemoryStatus::ValueAbsent,
    ),
    (
        memory_module_types::ROAR_SOUND_COOLDOWN.id(),
        MemoryStatus::Registered,
    ),
    (
        memory_module_types::ROAR_SOUND_DELAY.id(),
        MemoryStatus::Registered,
    ),
];

/// Vanilla `Sniffing`'s entry condition.
static SNIFFING_CONDITION: [(MemoryModuleId, MemoryStatus); 7] = [
    (
        memory_module_types::IS_SNIFFING.id(),
        MemoryStatus::ValuePresent,
    ),
    (
        memory_module_types::ATTACK_TARGET.id(),
        MemoryStatus::ValueAbsent,
    ),
    (
        memory_module_types::WALK_TARGET.id(),
        MemoryStatus::ValueAbsent,
    ),
    (
        memory_module_types::LOOK_TARGET.id(),
        MemoryStatus::Registered,
    ),
    (
        memory_module_types::NEAREST_ATTACKABLE.id(),
        MemoryStatus::Registered,
    ),
    (
        memory_module_types::DISTURBANCE_LOCATION.id(),
        MemoryStatus::Registered,
    ),
    (
        memory_module_types::SNIFF_COOLDOWN.id(),
        MemoryStatus::Registered,
    ),
];

/// Vanilla `SonicBoom`'s entry condition.
static SONIC_BOOM_CONDITION: [(MemoryModuleId, MemoryStatus); 4] = [
    (
        memory_module_types::ATTACK_TARGET.id(),
        MemoryStatus::ValuePresent,
    ),
    (
        memory_module_types::SONIC_BOOM_COOLDOWN.id(),
        MemoryStatus::ValueAbsent,
    ),
    (
        memory_module_types::SONIC_BOOM_SOUND_COOLDOWN.id(),
        MemoryStatus::Registered,
    ),
    (
        memory_module_types::SONIC_BOOM_SOUND_DELAY.id(),
        MemoryStatus::Registered,
    ),
];

/// Vanilla `WardenAi.DIG_COOLDOWN_SETTER`, which refreshes the cooldown only when the
/// warden already has one -- a warden with no cooldown is one that is allowed to leave.
pub struct DigCooldownSetter;

impl Trigger for DigCooldownSetter {
    fn required_memories(&self) -> Vec<MemoryModuleId> {
        vec![memory_module_types::DIG_COOLDOWN.id()]
    }

    fn trigger(&mut self, ctx: &BrainContext<'_>) -> bool {
        if ctx
            .brain()
            .has_memory_value(memory_module_types::DIG_COOLDOWN.id())
        {
            ctx.brain().set_memory_with_expiry(
                memory_module_types::DIG_COOLDOWN,
                Unit,
                warden_ai::DIGGING_COOLDOWN.into(),
            );
        }
        true
    }

    fn debug_name(&self) -> &'static str {
        "warden_dig_cooldown_setter"
    }
}

/// Boxes [`DigCooldownSetter`] ready for an activity list.
#[must_use]
pub fn dig_cooldown_setter() -> Box<dyn BehaviorControl> {
    OneShot::boxed(DigCooldownSetter)
}
