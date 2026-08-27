//! Blaze entity.
//!
//! Vanilla parity: `Blaze` and `Blaze.BlazeAttackGoal`. A blaze hovers, rises
//! to whatever height its target is at, and fires its fireballs in volleys of
//! three with a long pause between them.
//!
//! **Gap**: `Blaze.getLightLevelDependentMagicValue` returns a flat `1.0F`, so
//! a blaze reads every block as fully lit when it scores a walk target. Steel's
//! `PathfinderMob::get_walk_target_value` default only implements vanilla's
//! darkness formula for animals and returns `0.0` for everything else, so there
//! is nothing for the override to change yet.

use std::sync::{Arc, Weak};

use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;
use steel_macros::entity_behavior;
use steel_protocol::packets::game::SoundSource;
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::sound_event::SoundEventRef;
use steel_registry::vanilla_entity_data::BlazeEntityData;
use steel_registry::{level_events, sound_events, vanilla_attributes, vanilla_entities};
use steel_utils::locks::SyncMutex;
use steel_utils::{BlockPos, Downcast as _, DowncastType, DowncastTypeKey};

use crate::entity::Enemy;
use crate::entity::EntitySpawnReason;
use crate::entity::ai::goal::{
    Goal, GoalControls, HurtByTargetGoal, LookAtPlayerGoal, MoveTowardsRestrictionGoal,
    NearestAttackableTargetGoal, RandomLookAroundGoal, WaterAvoidingRandomStrollGoal,
};
use crate::entity::ai::path::PathType;
use crate::entity::damage::DamageSource;
use crate::entity::entities::SmallFireballEntity;
use crate::entity::spawn_rules::check_monster_spawn_rules;
use crate::entity::{
    Entity, EntityBase, EntityBaseLoad, EntitySyncedData, HurtingProjectile, LivingEntity,
    LivingEntityBase, Mob, MobBase, PathfinderMob, SharedEntity, next_entity_id,
};
use crate::physics::MoveResult;
use crate::world::World;

/// Experience a blaze drops.
///
/// Vanilla parity: the `this.xpReward = 10` of the constructor.
const XP_REWARD: i32 = 10;

/// The synchronized bit that says a blaze is charging a volley.
///
/// Vanilla parity: the `& 1` of `Blaze.isCharged`.
const FLAG_CHARGED: i8 = 1;

/// Speed multiplier while returning home.
///
/// Vanilla parity: `new MoveTowardsRestrictionGoal(this, 1.0)`.
const RESTRICTION_SPEED_MODIFIER: f64 = 1.0;

/// Speed multiplier while wandering.
///
/// Vanilla parity: `new WaterAvoidingRandomStrollGoal(this, 1.0, 0.0F)`.
const STROLL_SPEED_MODIFIER: f64 = 1.0;

/// Chance a wander target may sit in water.
///
/// Vanilla parity: the `0.0F` probability of the same call. A blaze never
/// deliberately picks a spot in water, because water kills it.
const STROLL_WATER_PROBABILITY: f32 = 0.0;

/// Distance at which a blaze watches a player.
///
/// Vanilla parity: `new LookAtPlayerGoal(this, Player.class, 8.0F)`.
const LOOK_AT_PLAYER_RANGE: f64 = 8.0;

/// How much of a falling blaze's descent survives each tick.
///
/// Vanilla parity: the `multiply(1.0, 0.6, 1.0)` of `Blaze.aiStep`, which is
/// what makes a blaze drift down rather than fall.
const FALL_DAMPING: f64 = 0.6;

/// Ticks between two rolls of the height a blaze is willing to hover at.
///
/// Vanilla parity: the `this.nextHeightOffsetChangeTick = 100` of
/// `Blaze.customServerAiStep`.
const HEIGHT_OFFSET_CHANGE_INTERVAL: i32 = 100;

/// Center of the hover-height roll, in blocks.
///
/// Vanilla parity: the `random.triangle(0.5, 6.891)` of the same method.
const HEIGHT_OFFSET_MODE: f64 = 0.5;

/// Spread of the hover-height roll, in blocks.
const HEIGHT_OFFSET_DEVIATION: f64 = 6.891;

/// The blaze's starting hover allowance, before the first roll.
///
/// Vanilla parity: the `private float allowedHeightOffset = 0.5F` field.
const DEFAULT_ALLOWED_HEIGHT_OFFSET: f32 = 0.5;

/// Vertical speed a rising blaze eases toward.
///
/// Vanilla parity: the `0.3F` of `(0.3F - movement.y) * 0.3F`.
const RISE_TARGET_SPEED: f64 = 0.3;

/// How much of the gap to [`RISE_TARGET_SPEED`] a blaze closes each tick.
const RISE_APPROACH_RATE: f64 = 0.3;

/// Squared distance below which a blaze punches rather than shoots.
///
/// Vanilla parity: the `distance < 4.0` of `BlazeAttackGoal.tick`.
const MELEE_RANGE_SQR: f64 = 4.0;

/// Ticks between two melee swings.
///
/// Vanilla parity: the `this.attackTime = 20` of the melee branch.
const MELEE_COOLDOWN_TICKS: i32 = 20;

/// Ticks a blaze charges before the first fireball of a volley.
///
/// Vanilla parity: the `this.attackTime = 60` of `attackStep == 1`.
const CHARGE_TICKS: i32 = 60;

/// Ticks between the fireballs of one volley.
///
/// Vanilla parity: the `this.attackTime = 6` of `attackStep <= 4`.
const VOLLEY_INTERVAL_TICKS: i32 = 6;

/// Ticks a blaze rests after finishing a volley.
///
/// Vanilla parity: the `this.attackTime = 100` of the last step.
const VOLLEY_COOLDOWN_TICKS: i32 = 100;

/// How many steps a volley runs for before it resets.
///
/// Vanilla parity: the `attackStep <= 4` bound. Step one is the charge, steps
/// two through four each throw a fireball.
const VOLLEY_LAST_STEP: i32 = 4;

/// How far a fireball may stray from a straight line to the target.
///
/// Vanilla parity: the `2.297 * sqd` deviation of the shot, where `sqd` is the
/// fourth root of the squared distance halved -- so a distant target is aimed
/// at far more loosely than a near one.
const FIREBALL_SPREAD: f64 = 2.297;

/// Ticks a blaze keeps chasing a target it has lost sight of.
///
/// Vanilla parity: the `this.lastSeen < 5` of `BlazeAttackGoal.tick`.
const LAST_SEEN_GRACE_TICKS: i32 = 5;

/// Speed multiplier while closing on a target.
///
/// Vanilla parity: the `1.0` handed to `setWantedPosition` in both branches.
const CHASE_SPEED_MODIFIER: f64 = 1.0;

/// Draws from a triangular distribution centered on `mode`.
///
/// Vanilla parity: `RandomSource.triangle`.
fn triangle(mode: f64, deviation: f64) -> f64 {
    mode + deviation * (rand::random::<f64>() - rand::random::<f64>())
}

/// A blaze.
#[entity_behavior(class = "Blaze")]
pub struct BlazeEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    entity_data: SyncMutex<BlazeEntityData>,
    /// How far above its own eyes a blaze tolerates its target before it rises.
    ///
    /// Vanilla parity: `Blaze.allowedHeightOffset`.
    allowed_height_offset: SyncMutex<f32>,
    /// Ticks left before the next hover-height roll.
    ///
    /// Vanilla parity: `Blaze.nextHeightOffsetChangeTick`.
    next_height_offset_change_tick: SyncMutex<i32>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `BlazeEntity`.
unsafe impl DowncastType for BlazeEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/blaze");
}

impl BlazeEntity {
    /// Creates a blaze at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Creates a blaze from saved base data.
    #[must_use]
    pub fn from_saved(entity_type: EntityTypeRef, load: EntityBaseLoad) -> Self {
        Self::new_with_base(
            EntityBase::from_load(load, entity_type.dimensions),
            entity_type,
        )
    }

    fn new_with_base(base: EntityBase, entity_type: EntityTypeRef) -> Self {
        let living_base = LivingEntityBase::new(entity_type);
        let mob_base = MobBase::new();
        let mut entity_data = BlazeEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);
        mob_base.set_xp_reward(XP_REWARD);

        {
            // Vanilla parity: the four `setPathfindingMalus` calls of the
            // constructor. A blaze refuses water outright and walks through
            // fire without noticing it.
            let mut malus = mob_base.pathfinding_malus().lock();
            malus.set(PathType::Water, -1.0);
            malus.set(PathType::Lava, 8.0);
            malus.set(PathType::FireInNeighbor, 0.0);
            malus.set(PathType::Fire, 0.0);
        }

        {
            // Keep vanilla Blaze goal priorities in the same order.
            let mut goals = mob_base.goal_selector().lock();
            goals.add_goal(4, BlazeAttackGoal::new());
            goals.add_goal(
                5,
                MoveTowardsRestrictionGoal::new(RESTRICTION_SPEED_MODIFIER),
            );
            goals.add_goal(
                7,
                WaterAvoidingRandomStrollGoal::with_probability(
                    STROLL_SPEED_MODIFIER,
                    STROLL_WATER_PROBABILITY,
                ),
            );
            goals.add_goal(8, LookAtPlayerGoal::new(LOOK_AT_PLAYER_RANGE));
            goals.add_goal(8, RandomLookAroundGoal::new());
        }

        {
            let mut targets = mob_base.target_selector().lock();
            targets.add_goal(1, HurtByTargetGoal::new().set_alert_others([]));
            targets.add_goal(
                2,
                NearestAttackableTargetGoal::new_for_players(true, |_, _, _| true),
            );
        }

        Self {
            base,
            entity_type,
            living_base,
            mob_base,
            entity_data: SyncMutex::new(entity_data),
            allowed_height_offset: SyncMutex::new(DEFAULT_ALLOWED_HEIGHT_OFFSET),
            next_height_offset_change_tick: SyncMutex::new(0),
        }
    }

    /// Returns whether this blaze is charging a volley.
    ///
    /// Vanilla parity: `Blaze.isCharged`, which is also what makes the client
    /// draw the blaze alight.
    #[must_use]
    pub fn is_charged(&self) -> bool {
        *self.entity_data.lock().flags.get() & FLAG_CHARGED != 0
    }

    /// Vanilla parity: `Blaze.setCharged`.
    pub fn set_charged(&self, charged: bool) {
        let mut data = self.entity_data.lock();
        let flags = *data.flags.get();
        let updated = if charged {
            flags | FLAG_CHARGED
        } else {
            flags & !FLAG_CHARGED
        };
        data.flags.set(updated);
    }

    /// Rolls a fresh hover allowance every hundred ticks and climbs toward a
    /// target that is above it.
    ///
    /// Vanilla parity: `Blaze.customServerAiStep`.
    fn tick_hover(&self) {
        let rolled = {
            let mut next = self.next_height_offset_change_tick.lock();
            *next -= 1;
            if *next <= 0 {
                *next = HEIGHT_OFFSET_CHANGE_INTERVAL;
                true
            } else {
                false
            }
        };
        if rolled {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "vanilla stores the triangle roll in a float field"
            )]
            let offset = triangle(HEIGHT_OFFSET_MODE, HEIGHT_OFFSET_DEVIATION) as f32;
            *self.allowed_height_offset.lock() = offset;
        }

        let Some(target) = self.target() else {
            return;
        };
        let Some(living_target) = target.as_living_entity() else {
            return;
        };
        let allowed = f64::from(*self.allowed_height_offset.lock());
        if living_target.get_eye_y() <= self.get_eye_y() + allowed
            || !Mob::can_attack(self, living_target)
        {
            return;
        }

        let movement = self.velocity();
        self.set_velocity(
            movement
                + DVec3::new(
                    0.0,
                    (RISE_TARGET_SPEED - movement.y) * RISE_APPROACH_RATE,
                    0.0,
                ),
        );
        self.mark_velocity_sync();
    }
}

/// Punches up close, and throws volleys of three fireballs from further out.
///
/// Vanilla parity: `Blaze.BlazeAttackGoal`.
struct BlazeAttackGoal {
    /// Where in the volley the blaze is: `1` charges, `2`..=`4` throw.
    attack_step: i32,
    /// Ticks until the next step of the volley.
    attack_time: i32,
    /// Ticks since the target was last visible.
    last_seen: i32,
}

impl BlazeAttackGoal {
    const fn new() -> Self {
        Self {
            attack_step: 0,
            attack_time: 0,
            last_seen: 0,
        }
    }

    /// Throws one fireball at `target`, wide of the mark by a distance-scaled
    /// spread.
    ///
    /// Vanilla parity: the `SmallFireball` block of `BlazeAttackGoal.tick`.
    fn shoot_fireball(mob: &dyn PathfinderMob, target: &SharedEntity, distance_sqr: f64) {
        let Some(world) = mob.level() else {
            return;
        };

        let position = mob.position();
        let target_position = target.position();
        let mob_mid_y = position.y + mob.bounding_box().height() * 0.5;
        let target_mid_y = target_position.y + target.bounding_box().height() * 0.5;
        let xd = target_position.x - position.x;
        let yd = target_mid_y - mob_mid_y;
        let zd = target_position.z - position.z;
        let spread = distance_sqr.sqrt().sqrt() * 0.5;

        if !mob.is_silent() {
            world.level_event(
                level_events::SOUND_BLAZE_FIREBALL,
                mob.block_position(),
                0,
                None,
            );
        }

        let direction = DVec3::new(
            triangle(xd, FIREBALL_SPREAD * spread),
            yd,
            triangle(zd, FIREBALL_SPREAD * spread),
        );
        // Vanilla builds the fireball at the blaze's feet, then lifts it to the
        // blaze's middle without touching x or z.
        let fireball = Arc::new(SmallFireballEntity::new(
            &vanilla_entities::SMALL_FIREBALL,
            next_entity_id(),
            DVec3::new(position.x, mob_mid_y + 0.5, position.z),
            Arc::downgrade(&world),
        ));
        if let Some(owner) = world.get_entity_by_id(mob.id()) {
            fireball.shoot_from_owner(&owner, direction);
        } else {
            fireball.set_rotation(mob.rotation());
            fireball.assign_directional_movement(direction);
        }

        let entity: SharedEntity = fireball;
        if let Err(error) = world.try_add_entity(entity) {
            log::debug!("blaze failed to throw a fireball: {error}");
        }
    }
}

impl Goal for BlazeAttackGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::MOVE | GoalControls::LOOK
    }

    /// Vanilla parity: `BlazeAttackGoal.canUse`.
    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        let Some(target) = mob.target() else {
            return false;
        };
        target
            .as_living_entity()
            .is_some_and(|living| LivingEntity::is_alive(living) && Mob::can_attack(mob, living))
    }

    fn start(&mut self, _mob: &dyn PathfinderMob) {
        self.attack_step = 0;
    }

    fn stop(&mut self, mob: &dyn PathfinderMob) {
        if let Some(blaze) = mob.downcast_ref::<BlazeEntity>() {
            blaze.set_charged(false);
        }
        self.last_seen = 0;
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    /// Vanilla parity: `BlazeAttackGoal.tick`.
    fn tick(&mut self, mob: &dyn PathfinderMob) {
        self.attack_time -= 1;
        let Some(target) = mob.target() else {
            return;
        };

        let has_line_of_sight = mob.has_line_of_sight_cached(target.as_ref());
        if has_line_of_sight {
            self.last_seen = 0;
        } else {
            self.last_seen += 1;
        }

        let target_position = target.position();
        let distance_sqr = mob.position().distance_squared(target_position);
        let follow_range = mob
            .attributes()
            .lock()
            .required_value(vanilla_attributes::FOLLOW_RANGE);

        if distance_sqr < MELEE_RANGE_SQR {
            if !has_line_of_sight {
                return;
            }

            if self.attack_time <= 0 {
                self.attack_time = MELEE_COOLDOWN_TICKS;
                if let Some(world) = mob.level() {
                    let _ = mob.do_hurt_target(world.as_ref(), &target);
                }
            }

            mob.mob_base()
                .controls()
                .lock()
                .move_control
                .set_wanted_position(target_position, CHASE_SPEED_MODIFIER);
            return;
        }

        if distance_sqr < follow_range * follow_range && has_line_of_sight {
            if self.attack_time <= 0 {
                self.attack_step += 1;
                if self.attack_step == 1 {
                    self.attack_time = CHARGE_TICKS;
                    if let Some(blaze) = mob.downcast_ref::<BlazeEntity>() {
                        blaze.set_charged(true);
                    }
                } else if self.attack_step <= VOLLEY_LAST_STEP {
                    self.attack_time = VOLLEY_INTERVAL_TICKS;
                } else {
                    self.attack_time = VOLLEY_COOLDOWN_TICKS;
                    self.attack_step = 0;
                    if let Some(blaze) = mob.downcast_ref::<BlazeEntity>() {
                        blaze.set_charged(false);
                    }
                }

                if self.attack_step > 1 {
                    Self::shoot_fireball(mob, &target, distance_sqr);
                }
            }

            mob.mob_base().controls().lock().look_control.set_look_at(
                DVec3::new(target_position.x, target.get_eye_y(), target_position.z),
                10.0,
                10.0,
            );
            return;
        }

        if self.last_seen < LAST_SEEN_GRACE_TICKS {
            mob.mob_base()
                .controls()
                .lock()
                .move_control
                .set_wanted_position(target_position, CHASE_SPEED_MODIFIER);
        }
    }
}

impl Entity for BlazeEntity {
    fn base(&self) -> &EntityBase {
        &self.base
    }

    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
    }

    fn synced_data(&self) -> Option<&dyn EntitySyncedData> {
        Some(&self.entity_data)
    }

    fn base_tick(&self) {
        Mob::base_tick_mob(self);
    }

    fn sound_source(&self) -> SoundSource {
        SoundSource::Hostile
    }

    /// Vanilla parity: `Blaze.isOnFire`, which reports the charge flag rather
    /// than the entity's own burning state.
    fn is_on_fire(&self) -> bool {
        self.is_charged()
    }

    /// Vanilla parity: `Blaze` inherits `Mob.addAdditionalSaveData` unchanged,
    /// so the shared half is the whole of it.
    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_mob(nbt);
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_mob(nbt);
    }
}

impl LivingEntity for BlazeEntity {
    fn living_base(&self) -> &LivingEntityBase {
        &self.living_base
    }

    /// Vanilla parity: `Mob.serverAiStep`, which is where a mob's goals run.
    fn server_ai_step(&self) {
        Mob::mob_server_ai_step(self);
    }

    /// Vanilla parity: `Blaze.aiStep`, which damps a blaze's descent before
    /// the base step runs.
    fn ai_step(&self) -> Option<MoveResult> {
        let movement = self.velocity();
        if !self.on_ground() && movement.y < 0.0 {
            self.set_velocity(DVec3::new(
                movement.x,
                movement.y * FALL_DAMPING,
                movement.z,
            ));
        }

        self.default_ai_step()
    }

    /// Vanilla parity: `Blaze.isSensitiveToWater`. Rain alone kills a blaze.
    fn is_sensitive_to_water(&self) -> bool {
        true
    }

    fn get_health(&self) -> f32 {
        *self.entity_data.lock().living_entity().health.get()
    }

    fn set_health(&self, health: f32) {
        let max_health = self.get_max_health();
        let clamped = health.clamp(0.0, max_health);
        self.entity_data
            .lock()
            .living_entity_mut()
            .health
            .set(clamped);
    }

    fn hurt_sound(&self, _source: &DamageSource) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_BLAZE_HURT)
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_BLAZE_DEATH)
    }
}

impl Mob for BlazeEntity {
    /// Vanilla parity: `Blaze` derives from `Monster`.
    fn is_monster(&self) -> bool {
        true
    }

    fn mob_base(&self) -> &MobBase {
        &self.mob_base
    }

    fn tick_goal_selectors(&self) {
        PathfinderMob::tick_pathfinder_goal_selectors(self);
    }

    fn tick_path_navigation(&self) {
        PathfinderMob::tick_pathfinder_path_navigation(self);
    }

    /// Vanilla parity: `Blaze.customServerAiStep`.
    fn custom_server_ai_step(&self) {
        self.tick_hover();
    }

    fn ambient_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_BLAZE_AMBIENT)
    }

    fn mob_flags(&self) -> i8 {
        *self.entity_data.lock().mob().mob_flags.get()
    }

    fn set_mob_flags(&self, flags: i8) {
        self.entity_data.lock().mob_mut().mob_flags.set(flags);
    }

    fn check_spawn_rules(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        pos: BlockPos,
    ) -> bool {
        check_monster_spawn_rules(world, spawn_reason, pos)
    }
}

impl PathfinderMob for BlazeEntity {}

impl Enemy for BlazeEntity {}

#[cfg(test)]
mod tests;
