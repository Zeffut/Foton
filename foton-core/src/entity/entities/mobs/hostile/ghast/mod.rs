//! Ghast entity.
//!
//! Vanilla parity: `Ghast`, `Ghast.GhastShootFireballGoal` and the two public
//! goals it shares with the happy ghast. A ghast drifts through the nether on
//! nothing but its move control, faces wherever it is heading, and lobs a
//! fireball that a player can bat straight back into it.
//!
//! **Gaps**: `Ghast.getMaxSpawnClusterSize` caps a natural spawn at one ghast,
//! and Foton's natural spawner has no per-entity cluster-size hook to hang that
//! on yet.

use std::sync::{Arc, Weak};

use foton_macros::entity_behavior;
use foton_protocol::packets::game::SoundSource;
use foton_registry::entity_type::EntityTypeRef;
use foton_registry::sound_event::SoundEventRef;
use foton_registry::vanilla_entity_data::GhastEntityData;
use foton_registry::{level_events, sound_events, vanilla_entities};
use foton_utils::locks::SyncMutex;
use foton_utils::types::Difficulty;
use foton_utils::{BlockPos, BlockStateId, Downcast as _, DowncastType, DowncastTypeKey};
use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;

use crate::entity::Enemy;
use crate::entity::EntitySpawnReason;
use crate::entity::ai::control::GhastMoveControl;
use crate::entity::ai::goal::{
    GhastLookGoal, Goal, GoalControls, NearestAttackableTargetGoal, RandomFloatAroundGoal,
};
use crate::entity::damage::DamageSource;
use crate::entity::entities::LargeFireballEntity;
use crate::entity::spawn_rules::check_monster_spawn_rules;
use crate::entity::{
    Entity, EntityBase, EntityBaseLoad, EntitySyncedData, HurtingProjectile, LivingEntity,
    LivingEntityBase, Mob, MobBase, PathfinderMob, SharedEntity, next_entity_id,
};
use crate::physics::MoveResult;
use crate::world::World;

/// Experience a ghast drops.
///
/// Vanilla parity: the `this.xpReward = 5` of the constructor.
const XP_REWARD: i32 = 5;

/// Blast power of the fireball a ghast throws.
///
/// Vanilla parity: `Ghast.DEFAULT_EXPLOSION_POWER`.
const DEFAULT_EXPLOSION_POWER: i32 = 1;

/// How loudly a ghast cries.
///
/// Vanilla parity: `Ghast.getSoundVolume`, five times the usual, which is why
/// a ghast is audible across a whole nether cavern.
const SOUND_VOLUME: f32 = 5.0;

/// How far above or below itself a ghast will look for a target.
///
/// Vanilla parity: the `Math.abs(target.getY() - this.getY()) <= 4.0` selector
/// of the target goal.
const TARGET_MAX_Y_DIFFERENCE: f64 = 4.0;

/// How often the target goal rescans.
///
/// Vanilla parity: the `10` of the `NearestAttackableTargetGoal` call.
const TARGET_SCAN_INTERVAL: i32 = 10;

/// One in this many spawn attempts survives the extra roll.
///
/// Vanilla parity: the `random.nextInt(20) == 0` of `checkGhastSpawnRules`.
const SPAWN_ROLL: i32 = 20;

/// How far in front of the ghast its fireball appears.
///
/// Vanilla parity: the `4.0` offset along the view vector in
/// `GhastShootFireballGoal.tick`.
const FIREBALL_MUZZLE_DISTANCE: f64 = 4.0;

/// Squared distance beyond which a ghast holds its fire.
///
/// Vanilla parity: the `distanceToSqr(ghast) < 4096.0` of the shoot goal.
const SHOOT_RANGE_SQR: f64 = 4096.0;

/// Charge tick at which the client hears the warning.
///
/// Vanilla parity: the `chargeTime == 10` of the shoot goal.
const WARNING_CHARGE_TICK: i32 = 10;

/// Charge tick at which the fireball leaves.
const RELEASE_CHARGE_TICK: i32 = 20;

/// Charge counter a ghast drops to after firing, which is its cooldown.
///
/// Vanilla parity: the `this.chargeTime = -40` of the shoot goal.
const POST_SHOT_CHARGE: i32 = -40;

/// How far a ghast can be pulled on a lead before it snaps.
///
/// Vanilla parity: `Ghast.leashSnapDistance`.
const LEASH_SNAP_DISTANCE: f64 = 16.0;

/// How far a ghast can be pulled before the lead starts pulling back.
///
/// Vanilla parity: `Ghast.leashElasticDistance`.
const LEASH_ELASTIC_DISTANCE: f64 = 10.0;

/// Damage a fireball a player batted back does to the ghast that threw it.
///
/// Vanilla parity: the `super.hurtServer(level, source, 1000.0F)` of
/// `Ghast.hurtServer`, which is what makes deflecting a fireball a one-hit
/// kill however much health the ghast has left.
const REFLECTED_FIREBALL_DAMAGE: f32 = 1000.0;

/// How hard a ghast pushes against the air.
///
/// Vanilla parity: the `travelFlying(input, 0.02F)` of `Ghast.travel`.
const AIR_TRAVEL_SPEED: f32 = 0.02;

/// A ghast.
#[entity_behavior(class = "Ghast")]
pub struct GhastEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    entity_data: SyncMutex<GhastEntityData>,
    /// Radius of the blast this ghast's fireballs leave.
    ///
    /// Vanilla parity: `Ghast.explosionPower`.
    explosion_power: SyncMutex<i32>,
    /// Ticks left before the move control's next shove.
    ///
    /// Vanilla parity: `GhastMoveControl.floatDuration`, which vanilla keeps on
    /// the control object. Foton recreates its controls each tick, so the state
    /// they carry lives on the mob.
    float_duration: SyncMutex<i32>,
}

// SAFETY: This key is owned by Foton and uniquely identifies `GhastEntity`.
unsafe impl DowncastType for GhastEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:entity/ghast");
}

impl GhastEntity {
    /// Creates a ghast at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Creates a ghast from saved base data.
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
        let mut entity_data = GhastEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);
        mob_base.set_xp_reward(XP_REWARD);

        {
            // Keep vanilla Ghast goal priorities in the same order.
            let mut goals = mob_base.goal_selector().lock();
            goals.add_goal(5, RandomFloatAroundGoal::new());
            goals.add_goal(7, GhastLookGoal);
            goals.add_goal(7, GhastShootFireballGoal::new());
        }

        {
            let mut targets = mob_base.target_selector().lock();
            // Vanilla parity: a ghast only picks a player within four blocks of
            // its own height, which is what keeps it from diving at someone
            // walking the nether floor far below.
            targets.add_goal(
                1,
                NearestAttackableTargetGoal::new_for_players_with_interval(
                    TARGET_SCAN_INTERVAL,
                    true,
                    false,
                    |ghast, target, _| {
                        ghast.is_some_and(|ghast| {
                            (target.position().y - ghast.position().y).abs()
                                <= TARGET_MAX_Y_DIFFERENCE
                        })
                    },
                ),
            );
        }

        Self {
            base,
            entity_type,
            living_base,
            mob_base,
            entity_data: SyncMutex::new(entity_data),
            explosion_power: SyncMutex::new(DEFAULT_EXPLOSION_POWER),
            float_duration: SyncMutex::new(0),
        }
    }

    /// Returns whether this ghast is winding a fireball up.
    ///
    /// Vanilla parity: `Ghast.isCharging`, which is what makes the client draw
    /// its mouth open.
    #[must_use]
    pub fn is_charging(&self) -> bool {
        *self.entity_data.lock().is_charging.get()
    }

    /// Vanilla parity: `Ghast.setCharging`.
    pub fn set_charging(&self, charging: bool) {
        self.entity_data.lock().is_charging.set(charging);
    }

    /// Returns the blast power of this ghast's fireballs.
    ///
    /// Vanilla parity: `Ghast.getExplosionPower`.
    #[must_use]
    pub fn explosion_power(&self) -> i32 {
        *self.explosion_power.lock()
    }

    /// Returns whether `source` is a fireball a player batted back.
    ///
    /// Vanilla parity: `Ghast.isReflectedFireball`. It is the only thing that
    /// can kill a ghast through invulnerability, and the only thing that kills
    /// one in a single hit.
    fn is_reflected_fireball(&self, source: &DamageSource) -> bool {
        let Some(world) = self.level() else {
            return false;
        };
        let direct_is_fireball = source
            .direct_entity_id
            .and_then(|id| world.get_entity_by_id(id))
            .is_some_and(|direct| direct.downcast_ref::<LargeFireballEntity>().is_some());
        let causer_is_player = source
            .causing_entity_id
            .and_then(|id| world.get_entity_by_id(id))
            .is_some_and(|causer| causer.as_player().is_some());

        direct_is_fireball && causer_is_player
    }
}

/// Winds up for a second and lets a fireball go.
///
/// Vanilla parity: `Ghast.GhastShootFireballGoal`.
struct GhastShootFireballGoal {
    /// Ticks of wind-up, running negative while the ghast is on cooldown.
    charge_time: i32,
}

impl GhastShootFireballGoal {
    const fn new() -> Self {
        Self { charge_time: 0 }
    }

    /// Throws the fireball, four blocks out along the ghast's own heading.
    ///
    /// Vanilla parity: the `chargeTime == 20` branch of the goal's tick.
    fn release(mob: &dyn PathfinderMob, target: &SharedEntity) {
        let Some(world) = mob.level() else {
            return;
        };

        let position = mob.position();
        let view_vector = mob.look_angle();
        let mob_mid_y = position.y + mob.bounding_box().height() * 0.5;
        let target_position = target.position();
        let target_mid_y = target_position.y + target.bounding_box().height() * 0.5;

        let muzzle = DVec3::new(
            position.x + view_vector.x * FIREBALL_MUZZLE_DISTANCE,
            mob_mid_y + 0.5,
            position.z + view_vector.z * FIREBALL_MUZZLE_DISTANCE,
        );
        let direction = DVec3::new(
            target_position.x - muzzle.x,
            target_mid_y - (0.5 + mob_mid_y),
            target_position.z - muzzle.z,
        );

        if !mob.is_silent() {
            world.level_event(
                level_events::SOUND_GHAST_FIREBALL,
                mob.block_position(),
                0,
                None,
            );
        }

        let fireball = Arc::new(LargeFireballEntity::new(
            &vanilla_entities::FIREBALL,
            next_entity_id(),
            muzzle,
            Arc::downgrade(&world),
        ));
        if let Some(ghast) = mob.downcast_ref::<GhastEntity>() {
            fireball.set_explosion_power(ghast.explosion_power());
        }
        if let Some(owner) = world.get_entity_by_id(mob.id()) {
            fireball.shoot_from_owner(&owner, direction);
        } else {
            fireball.set_rotation(mob.rotation());
            fireball.assign_directional_movement(direction);
        }

        let entity: SharedEntity = fireball;
        if let Err(error) = world.try_add_entity(entity) {
            log::debug!("ghast failed to throw a fireball: {error}");
        }
    }
}

impl Goal for GhastShootFireballGoal {
    /// Vanilla parity: `GhastShootFireballGoal` never calls `setFlags`, so it
    /// holds no control and runs beside the look and move goals.
    fn controls(&self) -> GoalControls {
        GoalControls::EMPTY
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        mob.target().is_some()
    }

    fn start(&mut self, _mob: &dyn PathfinderMob) {
        self.charge_time = 0;
    }

    fn stop(&mut self, mob: &dyn PathfinderMob) {
        if let Some(ghast) = mob.downcast_ref::<GhastEntity>() {
            ghast.set_charging(false);
        }
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    /// Vanilla parity: `GhastShootFireballGoal.tick`.
    fn tick(&mut self, mob: &dyn PathfinderMob) {
        let Some(target) = mob.target() else {
            return;
        };

        let in_range = target.position().distance_squared(mob.position()) < SHOOT_RANGE_SQR;
        if in_range && mob.has_line_of_sight(target.as_ref()) {
            self.charge_time += 1;
            if self.charge_time == WARNING_CHARGE_TICK
                && !mob.is_silent()
                && let Some(world) = mob.level()
            {
                world.level_event(
                    level_events::SOUND_GHAST_WARNING,
                    mob.block_position(),
                    0,
                    None,
                );
            }

            if self.charge_time == RELEASE_CHARGE_TICK {
                Self::release(mob, &target);
                self.charge_time = POST_SHOT_CHARGE;
            }
        } else if self.charge_time > 0 {
            self.charge_time -= 1;
        }

        if let Some(ghast) = mob.downcast_ref::<GhastEntity>() {
            ghast.set_charging(self.charge_time > WARNING_CHARGE_TICK);
        }
    }
}

impl Entity for GhastEntity {
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

    /// Vanilla parity: `Ghast.supportQuadLeashAsHolder`. A leashable that also
    /// answers `support_quad_leash` hangs off a ghast on four ropes.
    fn support_quad_leash_as_holder(&self) -> bool {
        true
    }

    /// Vanilla parity: `Ghast.checkFallDamage` is empty.
    fn check_fall_damage(
        &self,
        _vertical_movement: f64,
        _on_ground: bool,
        _on_state: BlockStateId,
        _pos: BlockPos,
        _world: &Arc<World>,
    ) {
    }

    /// Vanilla parity: `Ghast.onClimbable`.
    fn on_climbable(&self) -> bool {
        false
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_mob(nbt);
        #[expect(
            clippy::cast_possible_truncation,
            reason = "vanilla stores the explosion power as a byte"
        )]
        nbt.insert("ExplosionPower", self.explosion_power() as i8);
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_mob(nbt);
        *self.explosion_power.lock() = nbt
            .byte("ExplosionPower")
            .map_or(DEFAULT_EXPLOSION_POWER, i32::from);
    }
}

impl LivingEntity for GhastEntity {
    fn living_base(&self) -> &LivingEntityBase {
        &self.living_base
    }

    /// Vanilla parity: `Mob.serverAiStep`, which is where a mob's goals run.
    fn server_ai_step(&self) {
        Mob::mob_server_ai_step(self);
    }

    /// Vanilla parity: `Ghast.travel`.
    fn travel(&self, input: DVec3) -> Option<MoveResult> {
        self.travel_flying(input, AIR_TRAVEL_SPEED)
    }

    /// Vanilla parity: `Ghast.isInvulnerableTo`. A ghast's own fireball cannot
    /// hurt it, and neither can anything else while it is invulnerable -- but a
    /// fireball a player batted back gets through both.
    fn is_invulnerable_to(&self, _world: &World, source: &DamageSource) -> bool {
        if self.is_invulnerable() && !source.bypasses_invulnerability() {
            return true;
        }

        !self.is_reflected_fireball(source) && self.is_invulnerable_to_base(source)
    }

    /// Vanilla parity: `Ghast.hurtServer`, which turns a reflected fireball
    /// into a thousand points of damage rather than the fireball's six.
    fn hurt_server(&self, world: &World, source: &DamageSource, amount: f32) -> bool {
        if self.is_reflected_fireball(source) {
            // Vanilla reports the hit as landed whatever the base call says,
            // because a reflected fireball is meant to be unanswerable.
            self.living_hurt_server(world, source, REFLECTED_FIREBALL_DAMAGE);
            return true;
        }

        if self.is_invulnerable_to(world, source) {
            return false;
        }

        self.living_hurt_server(world, source, amount)
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

    fn sound_volume(&self) -> f32 {
        SOUND_VOLUME
    }

    fn hurt_sound(&self, _source: &DamageSource) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_GHAST_HURT)
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_GHAST_DEATH)
    }
}

impl Mob for GhastEntity {
    fn mob_base(&self) -> &MobBase {
        &self.mob_base
    }

    fn tick_goal_selectors(&self) {
        PathfinderMob::tick_pathfinder_goal_selectors(self);
    }

    fn tick_path_navigation(&self) {
        PathfinderMob::tick_pathfinder_path_navigation(self);
    }

    /// Vanilla parity: `Ghast` installs a `GhastMoveControl`.
    fn tick_move_control(&self) {
        let float_duration = *self.float_duration.lock();
        *self.float_duration.lock() = GhastMoveControl::new(float_duration).tick(self);
    }

    /// Vanilla parity: `Ghast` installs no look control of its own; the
    /// `GhastLookGoal` turns it instead. Ticking the base look control here
    /// would fight the goal for the same rotation every tick.
    fn tick_look_control(&self) {}

    fn ambient_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_GHAST_AMBIENT)
    }

    fn mob_flags(&self) -> i8 {
        *self.entity_data.lock().mob().mob_flags.get()
    }

    fn set_mob_flags(&self, flags: i8) {
        self.entity_data.lock().mob_mut().mob_flags.set(flags);
    }

    /// Vanilla parity: `Ghast.leashSnapDistance`.
    fn leash_snap_distance(&self) -> f64 {
        LEASH_SNAP_DISTANCE
    }

    /// Vanilla parity: `Ghast.leashElasticDistance`.
    fn leash_elastic_distance(&self) -> f64 {
        LEASH_ELASTIC_DISTANCE
    }

    /// Returns whether this mob accepts where the spawner put it.
    ///
    /// Vanilla parity: `Ghast.checkGhastSpawnRules`, which throws away
    /// nineteen attempts in twenty on top of the usual monster rules.
    fn check_spawn_rules(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        pos: BlockPos,
    ) -> bool {
        world.difficulty() != Difficulty::Peaceful
            && rand::random_range(0..SPAWN_ROLL) == 0
            && check_monster_spawn_rules(world, spawn_reason, pos)
    }
}

impl PathfinderMob for GhastEntity {}

impl Enemy for GhastEntity {}

#[cfg(test)]
mod tests;
