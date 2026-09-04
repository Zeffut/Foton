//! Breeze entity.
//!
//! Vanilla parity: `net.minecraft.world.entity.monster.breeze.Breeze`. The
//! trial-chamber mob: it will not close on a player, it circles at a distance,
//! throws itself over the arena in a long arc and fires wind charges that shove
//! rather than wound. Everything about it is brain-driven -- it registers no
//! goals at all.
//!
//! **What is deliberately not here.** A large part of `Breeze` is client-side
//! and has no server work behind it:
//!
//! * `emitGroundParticles`, `emitJumpTrailParticles` and the six
//!   `AnimationState` fields are `Level.addParticle` and animation bookkeeping.
//! * `playWhirlSound` and `playAmbientSound` both go through
//!   `Level.playLocalSound(Entity, ...)`, whose body on `Level` is empty --
//!   only `ClientLevel` overrides it. A breeze on a vanilla server is silent
//!   between hurt and death; the client plays its own idle and whirl. So
//!   [`Mob::play_ambient_sound`] is overridden to do nothing, and the pose the
//!   behaviors set is what a client turns into an animation.
//!
//! One genuine gap: `Breeze.getHeadRotSpeed` returns 25 where the default is
//! 75. Foton's look control has no per-mob head rotation speed yet -- the same
//! foundation the frog is missing -- so only `getMaxHeadYRot` is honored.

mod behaviors;
mod breeze_ai;
mod breeze_util;

#[cfg(test)]
mod tests;

use std::sync::{Arc, Weak};

use foton_macros::entity_behavior;
use foton_protocol::packets::game::SoundSource;
use foton_registry::entity_type::EntityTypeRef;
use foton_registry::sound_event::SoundEventRef;
use foton_registry::vanilla_entity_data::BreezeEntityData;
use foton_registry::{sound_events, vanilla_entities};
use foton_utils::BlockPos;
use foton_utils::locks::SyncMutex;
use foton_utils::{DowncastType, DowncastTypeKey};
use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;

use crate::entity::LivingEntitySyncedData;
use crate::entity::ai::brain::Brain;
use crate::entity::ai::path::PathType;
use crate::entity::base::EntityMovementEmission;
use crate::entity::damage::DamageSource;
use crate::entity::spawn_rules::check_any_light_monster_spawn_rules;
use crate::entity::{
    Enemy, Entity, EntityBase, EntityBaseLoad, EntitySpawnReason, EntitySyncedData, LivingEntity,
    LivingEntityBase, Mob, MobBase, PathfinderMob, Projectile, ProjectileDeflection, SharedEntity,
};
use crate::world::World;

/// Experience this mob drops.
///
/// Vanilla parity: the `this.xpReward = 10` of the `Breeze` constructor, which
/// is twice what an ordinary monster is worth.
const XP_REWARD: i32 = 10;

/// Vanilla parity: `Breeze.getMaxHeadYRot`.
const MAX_HEAD_Y_ROT: f32 = 30.0;

/// Vanilla parity: `Breeze.FALL_DISTANCE_SOUND_TRIGGER_THRESHOLD`.
const FALL_DISTANCE_SOUND_TRIGGER_THRESHOLD: f64 = 3.0;

/// A breeze.
#[entity_behavior(class = "Breeze")]
pub struct BreezeEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    entity_data: SyncMutex<BreezeEntityData>,
    brain: Brain,
}

// SAFETY: This key is owned by Foton and uniquely identifies `BreezeEntity`.
unsafe impl DowncastType for BreezeEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:entity/breeze");
}

impl BreezeEntity {
    /// Creates a breeze at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Creates a breeze from saved base data.
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
        mob_base.set_xp_reward(XP_REWARD);
        let mut entity_data = BreezeEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);

        let breeze = Self {
            base,
            entity_type,
            living_base,
            mob_base,
            entity_data: SyncMutex::new(entity_data),
            brain: breeze_ai::make_brain(),
        };

        // Vanilla parity: the two `setPathfindingMalus` calls of the `Breeze`
        // constructor. A breeze refuses to path over a trapdoor or through
        // fire, which is what keeps it circling the open floor of a trial
        // chamber instead of walking into its own hazards.
        breeze.set_pathfinding_malus(PathType::OnTopOfTrapdoor, -1.0);
        breeze.set_pathfinding_malus(PathType::Fire, -1.0);

        breeze
    }

    /// Returns the height a wind charge leaves this breeze at.
    ///
    /// Vanilla parity: `Breeze.getFiringYPosition`.
    #[must_use]
    pub fn firing_y_position(&self) -> f64 {
        behaviors::firing_y_position(self)
    }

    /// Returns whether `source` came from another breeze.
    ///
    /// Vanilla parity: the `source.getEntity() instanceof Breeze` of
    /// `Breeze.isInvulnerableTo`, which is what stops two breezes in one trial
    /// chamber killing each other with their own gusts.
    fn hurt_by_another_breeze(&self, source: &DamageSource) -> bool {
        let Some(world) = self.level() else {
            return false;
        };
        source
            .causing_entity_id
            .and_then(|id| world.get_entity_by_id(id))
            .is_some_and(|entity| entity.entity_type() == &vanilla_entities::BREEZE)
    }
}

impl Entity for BreezeEntity {
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

    /// Vanilla parity: `Breeze.getMovementEmission`, which drops the movement
    /// *sounds* and keeps the game events -- a sliding breeze is heard by a
    /// sculk sensor but makes no step sound of its own.
    fn movement_emission(&self) -> EntityMovementEmission {
        EntityMovementEmission::Events
    }

    /// Vanilla parity: `Breeze.getFluidJumpThreshold`, which is the eye height
    /// rather than the usual `0.4`. A breeze therefore lets water reach its
    /// eyes before it starts swimming up, which is what makes
    /// `Swim.shouldSwim` -- and so `LongJump.canRun` -- read shallow water as
    /// standable.
    fn get_fluid_jump_threshold(&self) -> f64 {
        self.get_eye_height()
    }

    /// Vanilla parity: `Breeze.deflection`.
    ///
    /// A wind charge of either kind passes straight through: a breeze cannot
    /// bat away its own ammunition, and two breezes cannot rally one charge
    /// between them. Anything else is deflected if the breeze's entity type is
    /// in `#deflects_projectiles`, which is where the base implementation
    /// already looks.
    ///
    /// Two notes. Vanilla plays `BREEZE_DEFLECT` from inside the
    /// `ProjectileDeflection` lambda, which runs when the deflection is
    /// *applied*; Foton's `ProjectileDeflection` is a plain enum with no sound
    /// hook, so the sound is played here, when the deflection is *decided*.
    /// The two differ only for a projectile that hits the same breeze twice
    /// running, which `Projectile::hit_target_or_deflect_self` refuses to
    /// deflect a second time. And none of it is reachable yet: no Foton mob is
    /// `Entity::is_pickable`, so a projectile never scores an entity hit on a
    /// breeze at all.
    fn deflection(&self, projectile: &dyn Projectile) -> ProjectileDeflection {
        let projectile_type = projectile.entity_type();
        if projectile_type == &vanilla_entities::BREEZE_WIND_CHARGE
            || projectile_type == &vanilla_entities::WIND_CHARGE
        {
            return ProjectileDeflection::None;
        }

        let deflection = self.default_deflection(projectile);
        if deflection != ProjectileDeflection::None {
            self.play_sound(&sound_events::ENTITY_BREEZE_DEFLECT, 1.0, 1.0);
        }
        deflection
    }

    /// Vanilla parity: `Breeze.causeFallDamage`, which lands loudly and then
    /// takes the damage like anything else.
    fn cause_fall_damage(
        &self,
        fall_distance: f64,
        damage_modifier: f32,
        source: &DamageSource,
    ) -> bool {
        if fall_distance > FALL_DISTANCE_SOUND_TRIGGER_THRESHOLD {
            self.play_sound(&sound_events::ENTITY_BREEZE_LAND, 1.0, 1.0);
        }
        self.cause_living_fall_damage(fall_distance, damage_modifier, source)
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_mob(nbt);
        self.brain.save(nbt);
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_mob(nbt);
        self.brain.load(nbt);
    }
}

impl LivingEntity for BreezeEntity {
    /// Returns synchronized data declared by vanilla `LivingEntity`.
    fn living_synced_data(&self) -> Option<&dyn LivingEntitySyncedData> {
        Some(&self.entity_data)
    }

    fn living_base(&self) -> &LivingEntityBase {
        &self.living_base
    }

    /// Vanilla parity: the `Mob.serverAiStep` a breeze inherits, which is the
    /// only path to [`Mob::custom_server_ai_step`] and so to the brain.
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

    /// Vanilla parity: `Breeze.isInvulnerableTo`.
    fn is_invulnerable_to(&self, world: &World, source: &DamageSource) -> bool {
        self.hurt_by_another_breeze(source) || self.living_is_invulnerable_to(world, source)
    }

    fn hurt_sound(&self, _source: &DamageSource) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_BREEZE_HURT)
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_BREEZE_DEATH)
    }
}

impl Mob for BreezeEntity {
    /// Vanilla parity: `Breeze` derives from `Monster`.
    fn is_monster(&self) -> bool {
        true
    }

    fn mob_base(&self) -> &MobBase {
        &self.mob_base
    }

    fn brain(&self) -> Option<&Brain> {
        Some(&self.brain)
    }

    /// Vanilla parity: `Breeze.getTarget`.
    fn target(&self) -> Option<SharedEntity> {
        self.target_from_brain()
    }

    fn tick_goal_selectors(&self) {
        PathfinderMob::tick_pathfinder_goal_selectors(self);
    }

    fn tick_path_navigation(&self) {
        PathfinderMob::tick_pathfinder_path_navigation(self);
    }

    /// Vanilla parity: `Breeze.customServerAiStep`.
    fn custom_server_ai_step(&self) {
        let Some(world) = self.level() else {
            return;
        };
        self.brain.tick(&world, self);
        breeze_ai::update_activity(&self.brain);
    }

    /// Vanilla parity: `Breeze.canAttack`, which is the whole reason a breeze
    /// ignores the mobs it shares a trial chamber with.
    fn can_attack(&self, target: &dyn LivingEntity) -> bool {
        let entity_type = target.entity_type();
        (entity_type == &vanilla_entities::PLAYER || entity_type == &vanilla_entities::IRON_GOLEM)
            && self.mob_can_attack(target)
    }

    /// Vanilla parity: `Breeze.getMaxHeadYRot`.
    fn max_head_y_rot(&self) -> f32 {
        MAX_HEAD_Y_ROT
    }

    /// Vanilla parity: `Breeze.getAmbientSound`.
    ///
    /// Nothing on a server reads this -- see [`Self::play_ambient_sound`] --
    /// but a wrong answer here would be a silent trap for whatever does one
    /// day.
    fn ambient_sound(&self) -> Option<SoundEventRef> {
        Some(if self.on_ground() {
            &sound_events::ENTITY_BREEZE_IDLE_GROUND
        } else {
            &sound_events::ENTITY_BREEZE_IDLE_AIR
        })
    }

    /// Vanilla parity: `Breeze.playAmbientSound`, which routes through
    /// `Level.playLocalSound` -- an empty method on the server. The breeze's
    /// idle noise is played by each client for itself, so there is nothing to
    /// broadcast.
    fn play_ambient_sound(&self) {}

    /// Vanilla parity: the `Monster::checkAnyLightMonsterSpawnRules` a breeze
    /// is registered with in `SpawnPlacements` -- unlike most monsters it does
    /// not need the dark, though nothing spawns it naturally either.
    fn check_spawn_rules(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        pos: BlockPos,
    ) -> bool {
        check_any_light_monster_spawn_rules(world, spawn_reason, pos)
    }

    fn mob_flags(&self) -> i8 {
        *self.entity_data.lock().mob().mob_flags.get()
    }

    fn set_mob_flags(&self, flags: i8) {
        self.entity_data.lock().mob_mut().mob_flags.set(flags);
    }
}

impl PathfinderMob for BreezeEntity {}

impl Enemy for BreezeEntity {}
