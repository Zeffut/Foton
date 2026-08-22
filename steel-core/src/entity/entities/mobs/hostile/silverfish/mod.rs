//! Silverfish entity.
//!
//! Vanilla parity: `Silverfish`, `Silverfish.SilverfishWakeUpFriendsGoal`, and
//! `Silverfish.SilverfishMergeWithStoneGoal`. A silverfish wanders quietly,
//! attacks in melee, and — in vanilla — hides inside stone-like blocks and
//! calls nearby infested blocks for help when it is hurt.

use std::sync::Weak;

use glam::DVec3;
use steel_macros::entity_behavior;
use steel_protocol::packets::game::SoundSource;
use steel_registry::blocks::BlockRef;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::sound_event::SoundEventRef;
use steel_registry::sound_events;
use steel_registry::vanilla_blocks;
use steel_registry::vanilla_damage_type_tags;
use steel_registry::vanilla_entity_data::SilverfishEntityData;
use steel_utils::locks::SyncMutex;
use steel_utils::{BlockPos, BlockStateId, Downcast as _, DowncastType, DowncastTypeKey};

use crate::entity::ai::goal::{
    ClimbOnTopOfPowderSnowGoal, FloatGoal, Goal, GoalControls, HurtByTargetGoal, MeleeAttackGoal,
    NearestAttackableTargetGoal, RandomStrollGoal,
};
use crate::entity::damage::DamageSource;
use crate::entity::{
    Entity, EntityBase, EntityBaseLoad, EntityMovementEmission, EntitySyncedData, LivingEntity,
    LivingEntityBase, Mob, MobBase, PathfinderMob,
};
use crate::world::World;

/// Speed multiplier while chasing.
///
/// Vanilla parity: `new MeleeAttackGoal(this, 1.0, false)`.
const ATTACK_SPEED_MODIFIER: f64 = 1.0;

/// Speed multiplier while wandering.
///
/// Vanilla parity: `SilverfishMergeWithStoneGoal`'s
/// `super(silverfish, 1.0, 10)` call into `RandomStrollGoal`. Silverfish
/// wander noticeably faster than most mobs, which use `0.8`.
const MERGE_STROLL_SPEED_MODIFIER: f64 = 1.0;

/// Ticks between two attempts to pick a new wander target.
///
/// Vanilla parity: the `10` of `super(silverfish, 1.0, 10)`, twelve times more
/// eager than the `120`-tick default, which is why a silverfish never stands
/// still for long.
const MERGE_STROLL_INTERVAL_TICKS: i32 = 10;

/// Ticks a silverfish waits after being hurt before it looks for a nearby
/// infested block to wake up.
///
/// Vanilla parity: `SilverfishWakeUpFriendsGoal.notifyHurt`'s
/// `adjustedTickDelay(20)`.
const WAKE_UP_FRIENDS_DELAY_TICKS: i32 = 20;

/// Walk-target value returned for a position with a compatible host block
/// directly below it, pulling wandering silverfish toward stone they could
/// (in vanilla) merge into.
///
/// Vanilla parity: the `10.0F` literal in `Silverfish.getWalkTargetValue`.
const WALK_TARGET_HOST_BLOCK_VALUE: f32 = 10.0;

/// Volume of a silverfish's footstep.
///
/// Vanilla parity: the `0.15F` literal in `Silverfish.playStepSound`.
const STEP_SOUND_VOLUME: f32 = 0.15;

/// Pitch of a silverfish's footstep.
///
/// Vanilla parity: the `1.0F` literal in `Silverfish.playStepSound`.
const STEP_SOUND_PITCH: f32 = 1.0;

/// Returns whether `block` is a vanilla `InfestedBlock` host — an ordinary
/// block a silverfish can burrow into and later wake back up from.
///
/// Vanilla parity: `InfestedBlock.isCompatibleHostBlock`, which asks a map
/// every `InfestedBlock` registers itself into. Steel has no `InfestedBlock`
/// behavior yet, so the fixed host set is spelled out here; `classes.json`
/// already carries the same pairing under `host_block`, which is where this
/// should read from once that behavior exists.
fn is_compatible_host_block(block: BlockRef) -> bool {
    block == &vanilla_blocks::STONE
        || block == &vanilla_blocks::COBBLESTONE
        || block == &vanilla_blocks::STONE_BRICKS
        || block == &vanilla_blocks::MOSSY_STONE_BRICKS
        || block == &vanilla_blocks::CRACKED_STONE_BRICKS
        || block == &vanilla_blocks::CHISELED_STONE_BRICKS
        || block == &vanilla_blocks::DEEPSLATE
}

/// A silverfish.
///
/// TODO: vanilla spawns silverfish through `Silverfish.checkSilverfishSpawnRules`
/// (`checkAnyLightMonsterSpawnRules`, then "no player within 5 blocks" unless
/// spawner-spawned), registered externally as this entity type's natural-spawn
/// placement predicate. Steel's natural spawner
/// (`steel-core/src/world/natural_spawn.rs`) has no per-entity-type
/// spawn-rule hook to register that against yet, so it is not implemented
/// here.
#[entity_behavior(class = "Silverfish")]
pub struct SilverfishEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    entity_data: SyncMutex<SilverfishEntityData>,
    /// Ticks left before the wake-up-friends goal acts, or `0` when idle.
    ///
    /// Vanilla parity: `SilverfishWakeUpFriendsGoal.lookForFriends`.
    look_for_friends: SyncMutex<i32>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `SilverfishEntity`.
unsafe impl DowncastType for SilverfishEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/silverfish");
}

impl SilverfishEntity {
    /// Creates a silverfish at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Creates a silverfish from saved base data.
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
        let mut entity_data = SilverfishEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);

        {
            // Keep vanilla Silverfish goal priorities in the same order.
            let mut goals = mob_base.goal_selector().lock();
            goals.add_goal(1, FloatGoal::new(&mob_base));
            goals.add_goal(1, ClimbOnTopOfPowderSnowGoal::new());
            goals.add_goal(3, SilverfishWakeUpFriendsGoal);
            goals.add_goal(4, MeleeAttackGoal::new(ATTACK_SPEED_MODIFIER, false));
            goals.add_goal(
                5,
                SilverfishMergeWithStoneGoal::new(MERGE_STROLL_SPEED_MODIFIER),
            );
        }

        {
            let mut targets = mob_base.target_selector().lock();
            targets.add_goal(1, HurtByTargetGoal::new().set_alert_others([]));
            targets.add_goal(
                2,
                NearestAttackableTargetGoal::new_for_players(true, |_, _| true),
            );
        }

        Self {
            base,
            entity_type,
            living_base,
            mob_base,
            entity_data: SyncMutex::new(entity_data),
            look_for_friends: SyncMutex::new(0),
        }
    }

    /// Arms the wake-up-friends countdown if it is not already running.
    ///
    /// Vanilla parity: `SilverfishWakeUpFriendsGoal.notifyHurt`, called from
    /// `Silverfish.hurtServer` whenever a real attacker (or a damage type
    /// tagged `always_triggers_silverfish`, e.g. potions) hurts this
    /// silverfish. See [`LivingEntity::before_actually_hurt`] below for where
    /// Steel calls this.
    fn notify_friends_hurt(&self) {
        let mut counter = self.look_for_friends.lock();
        if *counter == 0 {
            *counter = WAKE_UP_FRIENDS_DELAY_TICKS;
        }
    }
}

/// Wanders like vanilla `Silverfish.SilverfishMergeWithStoneGoal`, minus its
/// burrow-into-stone branch.
///
/// Vanilla parity: `SilverfishMergeWithStoneGoal` extends `RandomStrollGoal`
/// and, before wandering, rolls a `1`-in-`10` chance (gated on `mobGriefing`)
/// to check a random neighboring block; if that block is a compatible
/// `InfestedBlock` host the silverfish burrows into it (replacing the block
/// and discarding itself) instead of wandering.
///
/// TODO: the burrow-into-stone branch is unimplemented — Steel has no
/// `InfestedBlock` behavior, so there is no host-to-infested `BlockState`
/// conversion to write and no infested block for the silverfish to disappear
/// into. A silverfish running this goal only ever wanders.
struct SilverfishMergeWithStoneGoal {
    stroll: RandomStrollGoal,
}

impl SilverfishMergeWithStoneGoal {
    const fn new(speed_modifier: f64) -> Self {
        Self {
            stroll: RandomStrollGoal::with_interval(speed_modifier, MERGE_STROLL_INTERVAL_TICKS),
        }
    }
}

impl Goal for SilverfishMergeWithStoneGoal {
    fn controls(&self) -> GoalControls {
        self.stroll.controls()
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        self.stroll.can_use(mob)
    }

    fn can_continue_to_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        self.stroll.can_continue_to_use(mob)
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        self.stroll.start(mob);
    }

    fn stop(&mut self, mob: &dyn PathfinderMob) {
        self.stroll.stop(mob);
    }
}

/// Counts down after the silverfish is hurt, then (in vanilla) wakes a nearby
/// infested block.
///
/// Vanilla parity: `Silverfish.SilverfishWakeUpFriendsGoal`. The countdown
/// itself, primed by [`SilverfishEntity::notify_friends_hurt`], behaves like
/// vanilla; the search-and-convert step at the end does not run yet.
///
/// TODO: once the countdown reaches zero, vanilla scans roughly an
/// `11 (Y) x 21 x 21` box centered on the silverfish for `InfestedBlock`s and
/// either destroys one (spawning a fresh silverfish, if `mobGriefing` is on)
/// or turns it back into its host block. Steel has no `InfestedBlock`
/// equivalent (no host<->infested `BlockState` conversion) to perform that
/// with, so this is a no-op once the countdown elapses.
struct SilverfishWakeUpFriendsGoal;

impl Goal for SilverfishWakeUpFriendsGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::EMPTY
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        mob.downcast_ref::<SilverfishEntity>()
            .is_some_and(|silverfish| *silverfish.look_for_friends.lock() > 0)
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        let Some(silverfish) = mob.downcast_ref::<SilverfishEntity>() else {
            return;
        };
        let mut counter = silverfish.look_for_friends.lock();
        *counter -= 1;
        if *counter <= 0 {
            *counter = 0;
            // TODO: see the struct doc comment above — the neighbor scan and
            // block conversion vanilla performs here are unimplemented.
        }
    }
}

impl Entity for SilverfishEntity {
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
        // Vanilla parity: `Silverfish.tick` forces the body to always face
        // the same way as the head every tick, since silverfish have no
        // independent body-turning animation.
        self.set_y_body_rot(self.rotation().0);
        Mob::base_tick_mob(self);
    }

    fn sound_source(&self) -> SoundSource {
        SoundSource::Hostile
    }

    /// Vanilla parity: `Silverfish.getMovementEmission`. Silverfish emit
    /// movement game events (for sculk sensors etc.) but no natural footstep
    /// sound as they move; [`Self::play_step_sound`] plays vanilla's fixed
    /// step sound explicitly instead.
    fn movement_emission(&self) -> EntityMovementEmission {
        EntityMovementEmission::Events
    }

    /// Vanilla parity: `Silverfish.playStepSound`, which ignores the block
    /// stepped on and always plays the same fixed sound.
    fn play_step_sound(&self, _pos: BlockPos, _block_state: BlockStateId) {
        self.play_sound(
            &sound_events::ENTITY_SILVERFISH_STEP,
            STEP_SOUND_VOLUME,
            STEP_SOUND_PITCH,
        );
    }
}

impl LivingEntity for SilverfishEntity {
    fn living_base(&self) -> &LivingEntityBase {
        &self.living_base
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
        Some(&sound_events::ENTITY_SILVERFISH_HURT)
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_SILVERFISH_DEATH)
    }

    /// Vanilla parity: the friend-notify half of `Silverfish.hurtServer`.
    ///
    /// TODO: vanilla calls `SilverfishWakeUpFriendsGoal.notifyHurt` from
    /// `hurtServer` itself, immediately after the `isInvulnerableTo` check
    /// and before the invulnerability-frame/cooldown gate inside
    /// `LivingEntity.hurtServer`. Steel's `LivingEntity::hurt_server` default
    /// has no equivalent pre-cooldown extension point (unlike
    /// `is_invulnerable_to`/`default_is_invulnerable_to`), and overriding
    /// `hurt_server` here would mean duplicating its entire damage pipeline.
    /// `before_actually_hurt` is the closest available hook; it runs after
    /// the cooldown gate, so a silverfish hit again during its brief
    /// invulnerability-frame window will not re-arm the countdown the way
    /// vanilla does.
    fn before_actually_hurt(&self, source: &DamageSource, _amount: f32) {
        if source.causing_entity_id.is_some()
            || source.is(&vanilla_damage_type_tags::DamageTypeTag::ALWAYS_TRIGGERS_SILVERFISH)
        {
            self.notify_friends_hurt();
        }
    }

    /// Vanilla parity: `Silverfish.setYBodyRot`, which snaps the head to
    /// match whenever the body rotation is forced (e.g. by AI look control),
    /// instead of turning independently.
    fn set_y_body_rot(&self, y_body_rot: f32) {
        let (_, pitch) = self.rotation();
        self.set_rotation((y_body_rot, pitch));
        self.living_base().set_y_body_rot(y_body_rot);
    }
}

impl Mob for SilverfishEntity {
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
        Some(&sound_events::ENTITY_SILVERFISH_AMBIENT)
    }

    fn mob_flags(&self) -> i8 {
        *self.entity_data.lock().mob().mob_flags.get()
    }

    fn set_mob_flags(&self, flags: i8) {
        self.entity_data.lock().mob_mut().mob_flags.set(flags);
    }
}

impl PathfinderMob for SilverfishEntity {
    /// Vanilla parity: `Silverfish.getWalkTargetValue`. Pulls wandering
    /// silverfish toward a compatible host block so they linger near stone
    /// they could (in vanilla) merge into.
    ///
    /// TODO: vanilla falls back to `super.getWalkTargetValue` (`Mob`'s
    /// darkness-based formula) when the block below is not a host block.
    /// Steel's `PathfinderMob::get_walk_target_value` default only
    /// implements that formula for `Animal`s (see `animal_walk_target_value`)
    /// and returns `0.0` for every other mob, so this falls back to `0.0`
    /// too instead of vanilla's formula.
    fn get_walk_target_value(&self, pos: BlockPos) -> f32 {
        let Some(world) = self.level() else {
            return 0.0;
        };

        if is_compatible_host_block(world.get_block_state(pos.below()).get_block()) {
            WALK_TARGET_HOST_BLOCK_VALUE
        } else {
            0.0
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Weak};

    use glam::DVec3;
    use steel_registry::{init_vanilla_registry, vanilla_damage_types, vanilla_entities};
    use steel_utils::ChunkPos;
    use steel_utils::types::UpdateFlags;

    use super::*;
    use crate::test_support::{fresh_test_world, insert_ready_full_chunk};

    fn silverfish() -> SilverfishEntity {
        SilverfishEntity::new(
            &vanilla_entities::SILVERFISH,
            1,
            DVec3::ZERO,
            Weak::<World>::new(),
        )
    }

    #[test]
    fn downcast_key_identifies_silverfish() {
        assert_eq!(
            SilverfishEntity::TYPE_KEY,
            DowncastTypeKey::new("steel:entity/silverfish")
        );
    }

    #[test]
    fn movement_emission_is_events_not_all() {
        init_vanilla_registry();
        let mob = silverfish();
        assert_eq!(mob.movement_emission(), EntityMovementEmission::Events);
    }

    #[test]
    fn sound_hooks_match_vanilla_silverfish_sounds() {
        init_vanilla_registry();
        let mob = silverfish();
        let source = DamageSource::environment(&vanilla_damage_types::GENERIC);

        assert_eq!(
            mob.hurt_sound(&source).expect("hurt sound").key,
            sound_events::ENTITY_SILVERFISH_HURT.key
        );
        assert_eq!(
            mob.death_sound().expect("death sound").key,
            sound_events::ENTITY_SILVERFISH_DEATH.key
        );
        assert_eq!(
            mob.ambient_sound().expect("ambient sound").key,
            sound_events::ENTITY_SILVERFISH_AMBIENT.key
        );
    }

    #[test]
    fn is_compatible_host_block_matches_vanilla_host_set() {
        init_vanilla_registry();
        assert!(is_compatible_host_block(&vanilla_blocks::STONE));
        assert!(is_compatible_host_block(&vanilla_blocks::DEEPSLATE));
        assert!(!is_compatible_host_block(&vanilla_blocks::DIRT));
    }

    #[test]
    fn walk_target_value_without_world_returns_zero() {
        init_vanilla_registry();
        let mob = silverfish();
        assert!(mob.get_walk_target_value(BlockPos::new(0, 64, 0)).abs() < f32::EPSILON);
    }

    #[test]
    fn walk_target_value_prefers_compatible_host_block() {
        init_vanilla_registry();
        let world = fresh_test_world("silverfish_walk_target_value");
        insert_ready_full_chunk(&world, ChunkPos::new(0, 0));
        let pos = BlockPos::new(8, 65, 8);
        let _ = world.set_block(
            pos.below(),
            vanilla_blocks::STONE.default_state(),
            UpdateFlags::UPDATE_CLIENTS,
        );

        let mob = SilverfishEntity::new(
            &vanilla_entities::SILVERFISH,
            1,
            DVec3::new(8.5, 65.0, 8.5),
            Arc::downgrade(&world),
        );

        assert!(
            (mob.get_walk_target_value(pos) - WALK_TARGET_HOST_BLOCK_VALUE).abs() < f32::EPSILON
        );
    }

    #[test]
    fn notify_friends_hurt_arms_the_wake_up_friends_goal() {
        init_vanilla_registry();
        let mob = silverfish();
        let mut goal = SilverfishWakeUpFriendsGoal;
        assert!(!goal.can_use(&mob));

        mob.notify_friends_hurt();

        assert!(goal.can_use(&mob));
        assert_eq!(*mob.look_for_friends.lock(), WAKE_UP_FRIENDS_DELAY_TICKS);
    }

    /// Vanilla parity: `SilverfishWakeUpFriendsGoal.notifyHurt` only arms a
    /// fresh countdown when the previous one has fully elapsed.
    #[test]
    fn notify_friends_hurt_does_not_restart_an_active_countdown() {
        init_vanilla_registry();
        let mob = silverfish();
        mob.notify_friends_hurt();
        *mob.look_for_friends.lock() = 5;

        mob.notify_friends_hurt();

        assert_eq!(*mob.look_for_friends.lock(), 5);
    }

    #[test]
    fn wake_up_friends_goal_counts_down_to_zero() {
        init_vanilla_registry();
        let mob = silverfish();
        let mut goal = SilverfishWakeUpFriendsGoal;
        mob.notify_friends_hurt();

        for _ in 0..WAKE_UP_FRIENDS_DELAY_TICKS {
            assert!(goal.can_use(&mob));
            goal.tick(&mob);
        }

        assert!(!goal.can_use(&mob));
        assert_eq!(*mob.look_for_friends.lock(), 0);
    }

    #[test]
    fn wake_up_friends_goal_has_no_controls() {
        let goal = SilverfishWakeUpFriendsGoal;
        assert_eq!(goal.controls(), GoalControls::EMPTY);
    }

    #[test]
    fn merge_with_stone_goal_uses_move_control() {
        let goal = SilverfishMergeWithStoneGoal::new(MERGE_STROLL_SPEED_MODIFIER);
        assert_eq!(goal.controls(), GoalControls::MOVE);
    }

    #[test]
    fn before_actually_hurt_arms_countdown_for_a_real_attacker() {
        init_vanilla_registry();
        let mob = silverfish();
        let source =
            DamageSource::environment(&vanilla_damage_types::MOB_ATTACK).with_causing_entity(99);

        mob.before_actually_hurt(&source, 1.0);

        assert_eq!(*mob.look_for_friends.lock(), WAKE_UP_FRIENDS_DELAY_TICKS);
    }

    /// Vanilla parity: `Silverfish.hurtServer` also notifies friends for
    /// damage types tagged `always_triggers_silverfish` (e.g. `magic`) even
    /// with no direct attacker entity.
    #[test]
    fn before_actually_hurt_triggers_for_always_triggers_silverfish_damage() {
        init_vanilla_registry();
        let mob = silverfish();
        let source = DamageSource::environment(&vanilla_damage_types::MAGIC);

        mob.before_actually_hurt(&source, 1.0);

        assert_eq!(*mob.look_for_friends.lock(), WAKE_UP_FRIENDS_DELAY_TICKS);
    }

    #[test]
    fn before_actually_hurt_ignores_untagged_environmental_damage() {
        init_vanilla_registry();
        let mob = silverfish();
        let source = DamageSource::environment(&vanilla_damage_types::GENERIC);

        mob.before_actually_hurt(&source, 1.0);

        assert_eq!(*mob.look_for_friends.lock(), 0);
    }

    #[test]
    fn set_y_body_rot_snaps_yaw_to_match() {
        init_vanilla_registry();
        let mob = silverfish();

        mob.set_y_body_rot(123.0);

        assert!((mob.rotation().0 - 123.0).abs() < f32::EPSILON);
        assert!((mob.y_body_rot() - 123.0).abs() < f32::EPSILON);
    }
}
