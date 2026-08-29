//! Wither skeleton entity.
//!
//! Vanilla parity: `WitherSkeleton`. A taller, fire-immune `AbstractSkeleton`
//! that always fights in melee instead of switching to a bow: vanilla spawns
//! it holding a stone sword and never gives it a bow, so `reassessWeaponGoal`
//! would pick the melee goal every time anyway. Foton has no hostile-mob
//! equipment system yet, so the stone sword itself, the
//! `wither_skeleton_disliked_weapons` item tag (`WitherSkeleton.canHoldItem`),
//! and the burning arrows a bow-wielding `AbstractSkeleton` would fire are not
//! modeled; see the TODOs below for the goals that depend on mob types Foton
//! does not implement yet either.

use std::sync::{Arc, Weak};

use foton_macros::entity_behavior;
use foton_protocol::packets::game::SoundSource;
use foton_registry::entity_type::EntityTypeRef;
use foton_registry::sound_event::SoundEventRef;
use foton_registry::vanilla_entity_data::WitherSkeletonEntityData;
use foton_registry::{sound_events, vanilla_attributes, vanilla_mob_effects};
use foton_utils::locks::SyncMutex;
use foton_utils::{BlockPos, BlockStateId, DowncastType, DowncastTypeKey};
use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;

use crate::entity::Enemy;
use crate::entity::ai::goal::{
    FleeSunGoal, HurtByTargetGoal, LookAtPlayerGoal, MeleeAttackGoal, NearestAttackableTargetGoal,
    RandomLookAroundGoal, RestrictSunGoal, WaterAvoidingRandomStrollGoal,
};
use crate::entity::ai::path::PathType;
use crate::entity::damage::DamageSource;
use crate::entity::spawn_rules::check_monster_spawn_rules;
use crate::entity::{
    Entity, EntityBase, EntityBaseLoad, EntitySpawnReason, EntitySyncedData, LivingEntity,
    LivingEntityBase, Mob, MobBase, MobEffectInstance, PathfinderMob, SharedEntity, SpawnGroupData,
};
use crate::world::World;

/// Experience this mob drops.
///
/// Vanilla parity: the `this.xpReward = 5` of the `Monster` constructor, which
/// every monster inherits and this one does not override.
const XP_REWARD: i32 = 5;

/// Speed multiplier while chasing in melee.
///
/// Vanilla parity: `AbstractSkeleton`'s private `meleeGoal`, built with
/// `new MeleeAttackGoal(this, 1.2, false)`.
const ATTACK_SPEED_MODIFIER: f64 = 1.2;

/// Speed multiplier for aimless wandering.
///
/// Vanilla parity: `AbstractSkeleton.registerGoals`'s
/// `WaterAvoidingRandomStrollGoal(this, 1.0)`.
const STROLL_SPEED_MODIFIER: f64 = 1.0;

/// Distance at which a wither skeleton watches a player.
///
/// Vanilla parity: `AbstractSkeleton.registerGoals`'s
/// `LookAtPlayerGoal(this, Player.class, 8.0F)`.
const LOOK_AT_PLAYER_RANGE: f64 = 8.0;

/// Pathfinding malus applied to lava.
///
/// Vanilla parity: `WitherSkeleton`'s constructor call
/// `this.setPathfindingMalus(PathType.LAVA, 8.0F)`, which lets it path
/// straight across lava lakes in the Nether instead of routing around them
/// like most mobs.
const LAVA_PATHFINDING_MALUS: f32 = 8.0;

/// Attack damage a wither skeleton is boosted to on spawn.
///
/// Vanilla parity: `WitherSkeleton.finalizeSpawn`'s
/// `this.getAttribute(Attributes.ATTACK_DAMAGE).setBaseValue(4.0)`, above the
/// shared `AbstractSkeleton` default of 2.0.
const WITHER_SKELETON_ATTACK_DAMAGE: f64 = 4.0;

/// Duration of the Wither effect inflicted on a landed hit.
///
/// Vanilla parity: `WitherSkeleton.doHurtTarget`'s
/// `new MobEffectInstance(MobEffects.WITHER, 200)`.
const WITHER_EFFECT_DURATION_TICKS: i32 = 200;

/// Volume of the wither skeleton's step sound.
///
/// Vanilla parity: `AbstractSkeleton.playStepSound`'s
/// `this.playSound(this.getStepSound(), 0.15F, 1.0F)`.
const STEP_SOUND_VOLUME: f32 = 0.15;

/// A wither skeleton.
#[entity_behavior(class = "WitherSkeleton")]
pub struct WitherSkeletonEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    entity_data: SyncMutex<WitherSkeletonEntityData>,
}

// SAFETY: This key is owned by Foton and uniquely identifies `WitherSkeletonEntity`.
unsafe impl DowncastType for WitherSkeletonEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:entity/wither_skeleton");
}

impl WitherSkeletonEntity {
    /// Creates a wither skeleton at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Creates a wither skeleton from saved base data.
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
        // Vanilla parity: `WitherSkeleton`'s constructor raises the lava malus
        // so it paths straight through lava instead of routing around it.
        mob_base
            .pathfinding_malus()
            .lock()
            .set(PathType::Lava, LAVA_PATHFINDING_MALUS);
        let mut entity_data = WitherSkeletonEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);

        {
            // Keep vanilla AbstractSkeleton goal priorities, but skip straight to
            // melee: a wither skeleton always spawns holding a stone sword, so
            // vanilla's own `reassessWeaponGoal` would never install the bow goal
            // either.
            let mut goals = mob_base.goal_selector().lock();
            goals.add_goal(2, RestrictSunGoal::new());
            goals.add_goal(3, FleeSunGoal::new(1.0));
            goals.add_goal(4, MeleeAttackGoal::new(ATTACK_SPEED_MODIFIER, false));
            goals.add_goal(5, WaterAvoidingRandomStrollGoal::new(STROLL_SPEED_MODIFIER));
            goals.add_goal(6, LookAtPlayerGoal::new(LOOK_AT_PLAYER_RANGE));
            goals.add_goal(6, RandomLookAroundGoal::new());
            // TODO: vanilla also flees wolves at priority 3; the goal is
            // available, the wolf is not.
        }

        {
            // Vanilla parity: `WitherSkeleton.registerGoals` adds the piglin
            // target ahead of calling `super.registerGoals` for the shared
            // `AbstractSkeleton` set, all at priority 3.
            let mut targets = mob_base.target_selector().lock();
            targets.add_goal(1, HurtByTargetGoal::new());
            targets.add_goal(
                2,
                NearestAttackableTargetGoal::new_for_players(true, |_, _, _| true),
            );
            // TODO: vanilla also targets AbstractPiglin and IronGolem, neither of
            // which exists in Foton yet, and baby Turtle on land at priority 3
            // (`AbstractSkeleton.java`), which is now implementable.
        }

        Self {
            base,
            entity_type,
            living_base,
            mob_base,
            entity_data: SyncMutex::new(entity_data),
        }
    }
}

impl Entity for WitherSkeletonEntity {
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

    /// Vanilla parity: `AbstractSkeleton.playStepSound`, which always plays the
    /// class's own step sound instead of the walked-on block's sound.
    fn play_step_sound(&self, _pos: BlockPos, _block_state: BlockStateId) {
        self.play_sound(
            &sound_events::ENTITY_WITHER_SKELETON_STEP,
            STEP_SOUND_VOLUME,
            1.0,
        );
    }

    /// Vanilla parity: `WitherSkeleton` adds nothing to `AbstractSkeleton`,
    /// which adds nothing to `Mob`, so the shared half is the whole of it.
    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_mob(nbt);
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_mob(nbt);
    }
}

impl LivingEntity for WitherSkeletonEntity {
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

    fn hurt_sound(&self, _source: &DamageSource) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_WITHER_SKELETON_HURT)
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_WITHER_SKELETON_DEATH)
    }

    /// A wither skeleton cannot be withered itself.
    ///
    /// Vanilla parity: `WitherSkeleton.canBeAffected` rejects `MobEffects.WITHER`
    /// outright and otherwise defers to the shared immunity checks (which
    /// already reject Poison and Regeneration for every undead mob, wither
    /// skeletons included).
    fn can_be_affected(&self, effect: &MobEffectInstance) -> bool {
        if effect.effect() == vanilla_mob_effects::WITHER {
            return false;
        }
        self.default_can_be_affected(effect)
    }
}

impl Mob for WitherSkeletonEntity {
    /// Vanilla parity: `WitherSkeleton` derives from `Monster`.
    fn is_monster(&self) -> bool {
        true
    }

    /// Returns whether this mob accepts where the spawner put it.
    ///
    /// Vanilla parity: the `Monster::checkMonsterSpawnRules` this mob is
    /// registered with in `SpawnPlacements`. It only appears in the dark.
    fn check_spawn_rules(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        pos: BlockPos,
    ) -> bool {
        check_monster_spawn_rules(world, spawn_reason, pos)
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
        Some(&sound_events::ENTITY_WITHER_SKELETON_AMBIENT)
    }

    /// Boosts attack damage above the shared `AbstractSkeleton` baseline.
    ///
    /// Vanilla parity: `WitherSkeleton.finalizeSpawn`, which runs the shared
    /// `Mob.finalizeSpawn` logic and then raises `ATTACK_DAMAGE` to 4.0.
    fn finalize_spawn(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        group_data: Option<SpawnGroupData>,
    ) -> Option<SpawnGroupData> {
        let group_data = self.finalize_spawn_mob_base(world, spawn_reason, group_data);
        // Vanilla parity: the `setCanPickUpLoot` of
        // `AbstractSkeleton.finalizeSpawn`, which is what lets a skeleton pick
        // your bow up off the ground.
        self.roll_spawn_can_pick_up_loot(world);
        self.attributes().lock().set_base_value(
            vanilla_attributes::ATTACK_DAMAGE,
            WITHER_SKELETON_ATTACK_DAMAGE,
        );
        group_data
    }

    /// Inflicts Wither on the target after a landed hit.
    ///
    /// Vanilla parity: `WitherSkeleton.doHurtTarget`, which applies a fixed
    /// duration regardless of difficulty, unlike the husk's hunger or the cave
    /// spider's poison.
    fn do_hurt_target(&self, world: &World, target: &SharedEntity) -> bool {
        if !Mob::mob_do_hurt_target(self, world, target) {
            return false;
        }
        let Some(living) = target.as_living_entity() else {
            return true;
        };
        living.add_mob_effect(MobEffectInstance::with_duration(
            vanilla_mob_effects::WITHER,
            WITHER_EFFECT_DURATION_TICKS,
            0,
        ));
        true
    }

    fn mob_flags(&self) -> i8 {
        *self.entity_data.lock().mob().mob_flags.get()
    }

    fn set_mob_flags(&self, flags: i8) {
        self.entity_data.lock().mob_mut().mob_flags.set(flags);
    }
}

impl PathfinderMob for WitherSkeletonEntity {}

impl Enemy for WitherSkeletonEntity {}

#[cfg(test)]
mod tests {
    use std::sync::Weak;

    use foton_registry::{init_vanilla_registry, vanilla_damage_types, vanilla_entities};
    use glam::DVec3;

    use super::*;

    fn spawn_wither_skeleton() -> WitherSkeletonEntity {
        init_vanilla_registry();
        WitherSkeletonEntity::new(
            &vanilla_entities::WITHER_SKELETON,
            1,
            DVec3::ZERO,
            Weak::<World>::new(),
        )
    }

    /// Vanilla parity: `AbstractSkeleton.registerGoals`, minus the melee/bow
    /// swap which is hardcoded to melee (see the module doc comment).
    #[test]
    fn registers_vanilla_goal_priorities() {
        let skeleton = spawn_wither_skeleton();

        let selector = skeleton.mob_base().goal_selector().lock();
        assert_eq!(selector.available_goal_count(), 6);
        assert_eq!(selector.available_goal_priorities(), vec![2, 3, 4, 5, 6, 6]);
    }

    /// Vanilla parity: `AbstractSkeleton.registerGoals`'s target selector, minus
    /// the entries that need mob types Foton does not implement yet.
    #[test]
    fn registers_vanilla_target_priorities() {
        let skeleton = spawn_wither_skeleton();

        let selector = skeleton.mob_base().target_selector().lock();
        assert_eq!(selector.available_goal_count(), 2);
        assert_eq!(selector.available_goal_priorities(), vec![1, 2]);
    }

    #[test]
    fn sets_lava_pathfinding_malus() {
        let skeleton = spawn_wither_skeleton();

        assert!(
            (skeleton.get_pathfinding_malus(PathType::Lava) - LAVA_PATHFINDING_MALUS).abs()
                < f32::EPSILON
        );
    }

    /// Vanilla parity: `WitherSkeleton.canBeAffected` always rejects
    /// `MobEffects.WITHER`.
    #[test]
    fn is_immune_to_its_own_wither_effect() {
        let skeleton = spawn_wither_skeleton();
        let wither = MobEffectInstance::with_duration(
            vanilla_mob_effects::WITHER,
            WITHER_EFFECT_DURATION_TICKS,
            0,
        );

        assert!(!skeleton.can_be_affected(&wither));
    }

    /// A wither skeleton is undead, so it keeps the shared poison immunity that
    /// `default_can_be_affected` grants every mob in the
    /// `ignores_poison_and_regen` tag, even though `can_be_affected` is
    /// overridden here for the wither-specific check.
    #[test]
    fn inherits_shared_undead_poison_immunity() {
        let skeleton = spawn_wither_skeleton();
        let poison = MobEffectInstance::with_duration(vanilla_mob_effects::POISON, 100, 0);

        assert!(!skeleton.can_be_affected(&poison));
    }

    #[test]
    fn accepts_unrelated_effects() {
        let skeleton = spawn_wither_skeleton();
        let speed = MobEffectInstance::with_duration(vanilla_mob_effects::SPEED, 100, 0);

        assert!(skeleton.can_be_affected(&speed));
    }

    #[test]
    fn uses_expected_vanilla_sounds() {
        let skeleton = spawn_wither_skeleton();

        assert_eq!(
            skeleton.ambient_sound().expect("ambient sound").key,
            sound_events::ENTITY_WITHER_SKELETON_AMBIENT.key
        );
        assert_eq!(
            skeleton
                .hurt_sound(&DamageSource::environment(&vanilla_damage_types::GENERIC))
                .expect("hurt sound")
                .key,
            sound_events::ENTITY_WITHER_SKELETON_HURT.key
        );
        assert_eq!(
            skeleton.death_sound().expect("death sound").key,
            sound_events::ENTITY_WITHER_SKELETON_DEATH.key
        );
    }
}
