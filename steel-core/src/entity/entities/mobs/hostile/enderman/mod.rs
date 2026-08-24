//! Enderman entity.
//!
//! Vanilla parity: `EnderMan`. Present in more biomes than any other mob, and
//! the only one whose whole character comes from a single rule: it ignores you
//! until you look it in the eye, and then it will not let go. The grudge is
//! [`NeutralMob`]'s; what is here is the stare that starts it, the teleport
//! that carries it, and the water that ends it.

use std::sync::Weak;

use glam::DVec3;
use steel_macros::entity_behavior;
use steel_protocol::packets::game::SoundSource;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::sound_event::SoundEventRef;
use steel_registry::vanilla_entity_data::EndermanEntityData;
use steel_registry::{sound_events, vanilla_attributes, vanilla_damage_types};
use steel_utils::locks::SyncMutex;
use steel_utils::{Downcast as _, DowncastType, DowncastTypeKey};

use crate::entity::Enemy;
use crate::entity::SharedEntity;
use crate::entity::ai::goal::{
    FloatGoal, Goal, GoalControls, HurtByTargetGoal, LookAtPlayerGoal, MeleeAttackGoal,
    RandomLookAroundGoal, WaterAvoidingRandomStrollGoal,
};
use crate::entity::damage::DamageSource;
use crate::entity::living_entity::is_looking_at;
use crate::entity::neutral_mob::{NeutralMob, PersistentAnger};
use crate::entity::{
    Entity, EntityBase, EntityBaseLoad, EntitySyncedData, LivingEntity, LivingEntityBase, Mob,
    MobBase, PathfinderMob,
};
use crate::world::{LevelReader as _, World};
use steel_registry::fluid::is_water_fluid;

/// How narrow the stare cone is.
///
/// Vanilla parity: the `0.025` of `isBeingStaredBy`. It is divided by the
/// distance, so a player across a field has to be far more precisely on target
/// than one standing in front of the enderman.
const STARE_CONE: f64 = 0.025;

/// Speed multiplier while chasing.
///
/// Vanilla parity: the attacking movement-speed modifier, `0.15` additive.
const ATTACKING_SPEED_BONUS: f64 = 0.15;

/// Ticks a chase runs before daylight may drive the enderman off.
///
/// Vanilla parity: the `targetChangeTime + 600` of `customServerAiStep`. It is
/// why an enderman that has just been provoked does not blink away at once.
const DAYLIGHT_GRACE_TICKS: i32 = 600;

/// Brightness above which daylight starts to bother an enderman.
///
/// Vanilla parity: the `br > 0.5F` of `customServerAiStep`.
const DAYLIGHT_BRIGHTNESS: f32 = 0.5;

/// Damage water does per tick.
///
/// Vanilla parity: the `1.0F` of `LivingEntity.baseTick` for water-sensitive
/// mobs. Rain and water both hurt, which is why an enderman caught in a shower
/// teleports repeatedly.
const WATER_DAMAGE: f32 = 1.0;

/// Shortest grudge, in ticks.
///
/// Vanilla parity: `PERSISTENT_ANGER_TIME`, twenty to thirty-nine seconds.
const ANGER_MIN_TICKS: i64 = 20 * 20;
/// Longest grudge, in ticks.
const ANGER_MAX_TICKS: i64 = 39 * 20;

/// An enderman.
#[entity_behavior(class = "EnderMan")]
pub struct EndermanEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    entity_data: SyncMutex<EndermanEntityData>,
    anger: PersistentAnger,
    /// Tick the current target was taken on, for the daylight grace period.
    target_change_time: SyncMutex<i32>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `EndermanEntity`.
unsafe impl DowncastType for EndermanEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/enderman");
}

impl EndermanEntity {
    /// Creates an enderman at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Creates an enderman from saved base data.
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
        let mut entity_data = EndermanEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);

        {
            // Vanilla parity: the goal order of `EnderMan.registerGoals`.
            let mut goals = mob_base.goal_selector().lock();
            goals.add_goal(0, FloatGoal::new(&mob_base));
            goals.add_goal(1, EndermanFreezeWhenLookedAt);
            goals.add_goal(2, MeleeAttackGoal::new(1.0, false));
            goals.add_goal(7, WaterAvoidingRandomStrollGoal::new(1.0));
            goals.add_goal(8, LookAtPlayerGoal::new(8.0));
            goals.add_goal(8, RandomLookAroundGoal::new());
            // TODO: vanilla also carries blocks, at priorities 10 and 11. That
            // needs a synced optional block state and the block's own drops;
            // neither is wired yet.
        }

        {
            let mut targets = mob_base.target_selector().lock();
            targets.add_goal(1, EndermanLookForPlayerGoal);
            targets.add_goal(2, HurtByTargetGoal::new());
            // TODO: vanilla also hunts endermites at priority 3; the endermite
            // is not implemented.
        }

        Self {
            base,
            entity_type,
            living_base,
            mob_base,
            entity_data: SyncMutex::new(entity_data),
            anger: PersistentAnger::new(),
            target_change_time: SyncMutex::new(0),
        }
    }

    /// Returns whether the enderman has its arms up and its mouth open.
    ///
    /// Vanilla parity: `isCreepy`, which is set whenever it has a target.
    #[must_use]
    pub fn is_creepy(&self) -> bool {
        *self.entity_data.lock().ender_man().creepy.get()
    }

    fn set_creepy(&self, creepy: bool) {
        self.entity_data.lock().ender_man_mut().creepy.set(creepy);
    }

    /// Returns whether a player has met the enderman's eyes.
    ///
    /// Vanilla parity: `hasBeenStaredAt`, which the client uses for the shriek.
    #[must_use]
    pub fn has_been_stared_at(&self) -> bool {
        *self.entity_data.lock().ender_man().stared_at.get()
    }

    fn set_been_stared_at(&self) {
        self.entity_data.lock().ender_man_mut().stared_at.set(true);
    }

    /// Returns whether this player is looking the enderman in the eye.
    ///
    /// Vanilla parity: `isBeingStaredBy`. The gaze has to land on the
    /// enderman's own eye height, not anywhere on its body, which is why
    /// looking at its feet is safe.
    fn is_being_stared_by(&self, player: &dyn LivingEntity) -> bool {
        // TODO: vanilla exempts a player wearing a carved pumpkin via
        // PLAYER_NOT_WEARING_DISGUISE_ITEM; equipment predicates are not wired.
        is_looking_at(self, player, STARE_CONE, true, false, &[self.get_eye_y()])
    }

    /// Blinks to a random spot within sixty-four blocks.
    ///
    /// Vanilla parity: the no-argument `teleport`.
    fn teleport_randomly(&self) -> bool {
        if !Entity::is_alive(self) {
            return false;
        }
        let position = self.position();
        let target = DVec3::new(
            (rand::random::<f64>() - 0.5).mul_add(64.0, position.x),
            position.y + f64::from(rand::random_range(0..64) - 32),
            (rand::random::<f64>() - 0.5).mul_add(64.0, position.z),
        );
        self.teleport_to_spot(target)
    }

    /// Blinks away from something, roughly sixteen blocks back.
    ///
    /// Vanilla parity: `teleportTowards`, which despite the name moves the
    /// enderman away: it is how one escapes an arrow or a splash of water.
    fn teleport_away_from(&self, from: DVec3, from_eye_y: f64) -> bool {
        let position = self.position();
        let away = DVec3::new(
            position.x - from.x,
            (position.y + 0.5) - from_eye_y,
            position.z - from.z,
        );
        if away.length_squared() <= 0.0 {
            return false;
        }
        let away = away.normalize();

        let target = DVec3::new(
            (rand::random::<f64>() - 0.5).mul_add(8.0, position.x) - away.x * 16.0,
            position.y + f64::from(rand::random_range(0..16) - 8) - away.y * 16.0,
            (rand::random::<f64>() - 0.5).mul_add(8.0, position.z) - away.z * 16.0,
        );
        self.teleport_to_spot(target)
    }

    /// Tries one teleport, refusing water and open air.
    ///
    /// Vanilla parity: the three-argument `teleport`.
    fn teleport_to_spot(&self, target: DVec3) -> bool {
        let Some(world) = self.level() else {
            return false;
        };

        // Vanilla walks down to the first block that stops movement and refuses
        // the spot if it is wet, so an enderman never lands in a lake.
        let mut pos = steel_utils::BlockPos::containing(target.x, target.y, target.z);
        while pos.y() > world.get_min_y() && !world.get_block_state(pos).blocks_motion() {
            pos = pos.below();
        }

        let landing = world.get_block_state(pos);
        if !landing.blocks_motion() || is_water_fluid(landing.get_fluid_state().fluid_id) {
            return false;
        }

        if !self.random_teleport(target) {
            return false;
        }

        self.play_sound(&sound_events::ENTITY_ENDERMAN_TELEPORT, 1.0, 1.0);
        true
    }
}

/// Stands stock still while a player is staring.
///
/// Vanilla parity: `EnderMan.EndermanFreezeWhenLookedAt`. This is the behaviour
/// players describe as the enderman "noticing" them.
struct EndermanFreezeWhenLookedAt;

impl Goal for EndermanFreezeWhenLookedAt {
    fn controls(&self) -> GoalControls {
        GoalControls::JUMP | GoalControls::MOVE
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        let Some(enderman) = mob.downcast_ref::<EndermanEntity>() else {
            return false;
        };
        let Some(target) = mob.target() else {
            return false;
        };
        let Some(living) = target.as_living_entity() else {
            return false;
        };
        target.as_player().is_some() && enderman.is_being_stared_by(living)
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        mob.mob_base().navigation().lock().stop();
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        let Some(target) = mob.target() else {
            return;
        };
        let position = target.position();
        mob.mob_base().controls().lock().look_control.set_look_at(
            DVec3::new(position.x, target.get_eye_y(), position.z),
            // Vanilla parity: `getMaxHeadYRot`/`getMaxHeadXRot` for a mob,
            // which is a full-speed turn of the head toward the starer.
            10.0,
            40.0,
        );
    }
}

/// Takes as a target whoever is staring, and whoever it is already angry at.
///
/// Vanilla parity: `EnderMan.EndermanLookForPlayerGoal`.
struct EndermanLookForPlayerGoal;

impl Goal for EndermanLookForPlayerGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::TARGET
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        let Some(enderman) = mob.downcast_ref::<EndermanEntity>() else {
            return false;
        };
        let Some(world) = mob.level() else {
            return false;
        };

        let staring = world.nearest_player(mob.position(), 64.0, |player| {
            enderman.is_being_stared_by(player) || enderman.is_angry_at(player, &world)
        });

        let Some(player) = staring else {
            return false;
        };
        let target: SharedEntity = player;
        if let Some(living) = target.as_living_entity()
            && enderman.is_being_stared_by(living)
        {
            enderman.set_been_stared_at();
        }
        mob.set_target(Some(&target))
    }

    fn can_continue_to_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        mob.target().is_some()
    }
}

impl Entity for EndermanEntity {
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

        // Vanilla parity: `isSensitiveToWater`. Water and rain both burn, and
        // the enderman blinks away rather than standing in it.
        let Some(world) = self.level() else {
            return;
        };
        let pos = self.block_position();
        let wet = self.is_in_water() || world.is_raining_at(pos);
        if wet && Entity::is_alive(self) {
            let source = DamageSource::environment(&vanilla_damage_types::DROWN);
            self.hurt_server(&world, &source, WATER_DAMAGE);
            self.teleport_randomly();
        }
    }

    fn sound_source(&self) -> SoundSource {
        SoundSource::Hostile
    }
}

impl LivingEntity for EndermanEntity {
    fn living_base(&self) -> &LivingEntityBase {
        &self.living_base
    }

    /// Vanilla parity: `Mob.serverAiStep`, which is where a mob's goals run.
    /// Without this the goal selector is never ticked and every goal this mob
    /// registers is dead code.
    fn server_ai_step(&self) {
        Mob::mob_server_ai_step(self);
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

    /// Vanilla parity: `EnderMan.isSensitiveToWater`.
    fn is_sensitive_to_water(&self) -> bool {
        true
    }

    fn hurt_sound(&self, _source: &DamageSource) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_ENDERMAN_HURT)
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_ENDERMAN_DEATH)
    }

    /// Blinks away from whatever hit it.
    ///
    /// Vanilla parity: the `hurtServer` override, which is why an enderman
    /// cannot be shot: the arrow lands and it is already somewhere else.
    fn before_actually_hurt(&self, source: &DamageSource, _amount: f32) {
        let Some(world) = self.level() else {
            return;
        };
        let Some(attacker) = source
            .causing_entity_id
            .and_then(|id| world.get_entity_by_id(id))
        else {
            self.teleport_randomly();
            return;
        };
        self.teleport_away_from(attacker.position(), attacker.get_eye_y());
    }
}

impl Mob for EndermanEntity {
    /// Vanilla parity: `EnderMan` derives from `Monster`.
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

    fn ambient_sound(&self) -> Option<SoundEventRef> {
        Some(if self.is_creepy() {
            &sound_events::ENTITY_ENDERMAN_SCREAM
        } else {
            &sound_events::ENTITY_ENDERMAN_AMBIENT
        })
    }

    /// Marks the enderman as roused and speeds it up.
    ///
    /// Vanilla parity: the `setTarget` override. The speed modifier is
    /// transient: it is added when a target is taken and removed when it is
    /// dropped, so a calm enderman moves at its ordinary pace.
    fn set_target(&self, target: Option<&SharedEntity>) -> bool {
        let changed = self.mob_base().set_target(target, |_| true);

        if target.is_none() {
            *self.target_change_time.lock() = 0;
            self.set_creepy(false);
            self.entity_data.lock().ender_man_mut().stared_at.set(false);
            self.attributes().lock().set_base_value(
                vanilla_attributes::MOVEMENT_SPEED,
                self.entity_type
                    .default_attributes
                    .iter()
                    .find(|(key, _)| *key == "minecraft:movement_speed")
                    .map_or(0.3, |(_, value)| *value),
            );
        } else {
            *self.target_change_time.lock() = self.tick_count();
            self.set_creepy(true);
            let base = self
                .attributes()
                .lock()
                .required_value(vanilla_attributes::MOVEMENT_SPEED);
            self.attributes().lock().set_base_value(
                vanilla_attributes::MOVEMENT_SPEED,
                base + ATTACKING_SPEED_BONUS,
            );
        }

        changed
    }

    /// Runs the anger clock, and flees the sun.
    ///
    /// Vanilla parity: `aiStep` plus `customServerAiStep`. Daylight only drives
    /// an enderman off once its grudge has had thirty seconds to cool, which is
    /// why one that has just been provoked stays and fights in the open.
    fn custom_server_ai_step(&self) {
        let Some(world) = self.level() else {
            return;
        };
        self.update_persistent_anger(&world, true);

        let grace_over =
            self.tick_count() >= *self.target_change_time.lock() + DAYLIGHT_GRACE_TICKS;
        if !world.is_bright_outside() || !grace_over {
            return;
        }

        let pos = self.block_position();
        let brightness = world.light_level_dependent_magic_value(pos);
        if brightness > DAYLIGHT_BRIGHTNESS
            && world.can_see_sky(pos)
            && rand::random::<f32>() * 30.0 < (brightness - 0.4) * 2.0
        {
            self.set_target(None);
            self.teleport_randomly();
        }
    }

    fn mob_flags(&self) -> i8 {
        *self.entity_data.lock().mob().mob_flags.get()
    }

    fn set_mob_flags(&self, flags: i8) {
        self.entity_data.lock().mob_mut().mob_flags.set(flags);
    }
}

impl PathfinderMob for EndermanEntity {}

impl NeutralMob for EndermanEntity {
    fn persistent_anger(&self) -> &PersistentAnger {
        &self.anger
    }

    /// Vanilla parity: `startPersistentAngerTimer`, twenty to thirty-nine
    /// seconds.
    fn start_persistent_anger_timer(&self) {
        self.set_time_to_remain_angry(rand::random_range(ANGER_MIN_TICKS..=ANGER_MAX_TICKS));
    }
}

impl Enemy for EndermanEntity {}
