//! Magma cube entity.
//!
//! Vanilla parity: `MagmaCube`, which is `AbstractCubeMob` with the Nether's
//! temperament. Nearly everything is [`super::cube_common`]; what is genuinely
//! its own is that it is dangerous at every size, armored in proportion to
//! that size, and jumps higher and less often than a slime -- a big magma cube
//! crosses ground in long arcs rather than a steady patter.

use std::sync::{Arc, Weak};

use glam::DVec3;
use steel_macros::entity_behavior;
use steel_protocol::packets::game::SoundSource;
use steel_registry::entity_data::EntityPose;
use steel_registry::entity_type::{EntityDimensions, EntityTypeRef};
use steel_registry::sound_event::SoundEventRef;
use steel_registry::vanilla_entity_data::MagmaCubeEntityData;
use steel_registry::vanilla_fluid_tags::FluidTag;
use steel_registry::{sound_events, vanilla_attributes};
use steel_utils::locks::SyncMutex;
use steel_utils::types::Difficulty;
use steel_utils::{BlockPos, DowncastType, DowncastTypeKey, Identifier};

use super::cube_common::{
    self, CubeAttackGoal, CubeFloatGoal, CubeKeepOnJumpingGoal, CubeLike, CubeRandomDirectionGoal,
    CubeState,
};
use crate::entity::Enemy;
use crate::entity::SharedEntity;
use crate::entity::ai::goal::{HurtByTargetGoal, NearestAttackableTargetGoal};
use crate::entity::damage::DamageSource;
use crate::entity::{
    Entity, EntityBase, EntityBaseLoad, EntitySpawnReason, EntitySyncedData, LivingEntity,
    LivingEntityBase, Mob, MobBase, PathfinderMob, SpawnGroupData, next_entity_id,
};
use crate::player::Player;
use crate::world::World;

/// Armor each size step is worth.
///
/// Vanilla parity: the `setBaseValue(size * 3)` of `MagmaCube.setSize`. A large
/// magma cube shrugs off a stone sword in a way a slime never does.
const ARMOR_PER_SIZE: f64 = 3.0;

/// Extra damage a magma cube deals over a slime of the same size.
///
/// Vanilla parity: `MagmaCube.getAttackDamage`, `super + 2.0F`.
const ATTACK_DAMAGE_BONUS: f64 = 2.0;

/// How much longer a magma cube waits between hops.
///
/// Vanilla parity: `MagmaCube.getJumpDelay`, `super * 4`.
const JUMP_DELAY_MULTIPLIER: i32 = 4;

/// Extra jump height each size step is worth.
///
/// Vanilla parity: the `getSize() * 0.1F` of `MagmaCube.jumpFromGround`.
const JUMP_BOOST_PER_SIZE: f64 = 0.1;

/// Upward speed a magma cube gets from a hop in lava.
///
/// Vanilla parity: the `0.22F` of `MagmaCube.jumpInLiquid`.
const LAVA_JUMP_BASE: f64 = 0.22;

/// Extra lava-hop speed each size step is worth.
const LAVA_JUMP_PER_SIZE: f64 = 0.05;

/// A magma cube.
#[entity_behavior(class = "MagmaCube")]
pub struct MagmaCubeEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    entity_data: SyncMutex<MagmaCubeEntityData>,
    cube: SyncMutex<CubeState>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `MagmaCubeEntity`.
unsafe impl DowncastType for MagmaCubeEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/magma_cube");
}

impl MagmaCubeEntity {
    /// Returns vanilla `AbstractCubeMob.getSize`.
    ///
    /// Public for the same reason [`super::slime::SlimeEntity::cube_size`] is:
    /// `Frog.canEat` needs it, and the frog cannot reach the `pub(super)`
    /// `CubeLike` trait from the passive module.
    #[must_use]
    pub fn cube_size(&self) -> i32 {
        <Self as CubeLike>::size(self)
    }

    /// Sets vanilla `AbstractCubeMob.setSize`, and with it everything the size
    /// decides -- health, armor, bite and hitbox.
    ///
    /// Public because vanilla's `setSize` is: the `CubeLike` trait that carries
    /// it is `pub(super)` to the hostile module, so nothing outside can size a
    /// cube without this.
    pub fn set_cube_size(&self, size: i32, update_health: bool) {
        <Self as CubeLike>::set_size(self, size, update_health);
    }

    /// Creates a magma cube at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Creates a magma cube from saved base data.
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
        let mut entity_data = MagmaCubeEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);
        entity_data.abstract_cube_mob_mut().id_size.set(1);

        {
            // Vanilla parity: the goal order of `AbstractCubeMob.registerGoals`
            // with `MagmaCube.addBehaviourGoals` slotted in at two -- the same
            // order the slime uses.
            let mut goals = mob_base.goal_selector().lock();
            let hooks = cube_common::hooks_for::<Self>();
            goals.add_goal(1, CubeFloatGoal::new(hooks));
            goals.add_goal(2, CubeAttackGoal::new(hooks));
            goals.add_goal(4, CubeRandomDirectionGoal::new(hooks));
            goals.add_goal(5, CubeKeepOnJumpingGoal::new(hooks));
        }

        {
            let mut targets = mob_base.target_selector().lock();
            targets.add_goal(1, HurtByTargetGoal::new());
            // Vanilla parity: a cube only takes a player within four blocks of
            // its own height, so one in a ravine does not aggro the ledge.
            targets.add_goal(
                1,
                NearestAttackableTargetGoal::new_for_players(true, |cube, target, _| {
                    cube.is_some_and(|cube| (target.position().y - cube.position().y).abs() <= 4.0)
                }),
            );
            // TODO: vanilla also targets iron golems at priority 3; the golem is
            // not implemented.
        }

        Self {
            base,
            entity_type,
            living_base,
            mob_base,
            entity_data: SyncMutex::new(entity_data),
            cube: SyncMutex::new(CubeState::default()),
        }
    }
}

/// Returns whether a magma cube may appear at all.
///
/// Vanilla parity: `MagmaCube.checkMagmaCubeSpawnRules`, which asks only that
/// the game is not on Peaceful -- it takes a position and ignores it. No light
/// check and no floor check: a magma cube will fill a lit bastion the way
/// nothing else in the Nether does.
#[must_use]
fn check_magma_cube_spawn_rules(world: &Arc<World>) -> bool {
    world.difficulty() != Difficulty::Peaceful
}

impl Entity for MagmaCubeEntity {
    fn base(&self) -> &EntityBase {
        &self.base
    }

    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
    }

    fn synced_data(&self) -> Option<&dyn EntitySyncedData> {
        Some(&self.entity_data)
    }

    fn dimensions_for_pose(&self, _pose: EntityPose) -> EntityDimensions {
        cube_common::dimensions_for_size(self)
    }

    fn base_tick(&self) {
        Mob::base_tick_mob(self);
        cube_common::tick_landing(self);
    }

    fn sound_source(&self) -> SoundSource {
        SoundSource::Hostile
    }

    /// Vanilla parity: `MagmaCube.isOnFire`, which is always false. A magma
    /// cube swimming in lava is not burning, and the client should not draw it
    /// that way.
    fn is_on_fire(&self) -> bool {
        false
    }

    fn player_touch(self: Arc<Self>, player: &Arc<Player>) {
        let target: SharedEntity = player.clone();
        cube_common::player_touch(self.as_ref(), &target);
    }
}

impl LivingEntity for MagmaCubeEntity {
    fn cube_loot_size(&self) -> Option<i32> {
        Some(CubeLike::size(self))
    }

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
        Some(if self.is_tiny() {
            &sound_events::ENTITY_MAGMA_CUBE_HURT_SMALL
        } else {
            &sound_events::ENTITY_MAGMA_CUBE_HURT
        })
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(if self.is_tiny() {
            &sound_events::ENTITY_MAGMA_CUBE_DEATH_SMALL
        } else {
            &sound_events::ENTITY_MAGMA_CUBE_DEATH
        })
    }

    /// Jumps higher the bigger it is.
    ///
    /// Vanilla parity: `MagmaCube.jumpFromGround`.
    fn jump_from_ground(&self) {
        let movement = self.velocity();
        let boost = JUMP_BOOST_PER_SIZE * f64::from(self.size());
        self.set_velocity(DVec3::new(
            movement.x,
            f64::from(self.get_jump_power()) + boost,
            movement.z,
        ));
        self.mark_velocity_sync();
    }

    /// Swims up through lava under its own power.
    ///
    /// Vanilla parity: `MagmaCube.jumpInLiquid`. In water it falls back to the
    /// shared nudge; in lava it gets a real push, which is how a magma cube
    /// climbs out of a lava sea instead of bobbing in it.
    fn jump_in_liquid(&self, fluid_tag: &Identifier) {
        if *fluid_tag != FluidTag::LAVA {
            self.living_jump_in_liquid(fluid_tag);
            return;
        }

        let movement = self.velocity();
        let lift = LAVA_JUMP_PER_SIZE.mul_add(f64::from(self.size()), LAVA_JUMP_BASE);
        self.set_velocity(DVec3::new(movement.x, lift, movement.z));
        self.mark_velocity_sync();
    }

    /// Splits before dying.
    ///
    /// Vanilla parity: the `remove` override of `AbstractCubeMob`, which only
    /// splits a dying cube; one that despawns leaves nothing behind.
    fn die(&self, source: &DamageSource) {
        if self.is_removed() {
            return;
        }
        if let Some(world) = self.level() {
            cube_common::split_on_death(self, &world);
        }
        self.living_die(source);
    }
}

impl Mob for MagmaCubeEntity {
    fn mob_base(&self) -> &MobBase {
        &self.mob_base
    }

    fn tick_goal_selectors(&self) {
        PathfinderMob::tick_pathfinder_goal_selectors(self);
    }

    fn tick_path_navigation(&self) {
        PathfinderMob::tick_pathfinder_path_navigation(self);
    }

    fn tick_move_control(&self) {
        cube_common::tick_move_control(self);
    }

    fn ambient_sound(&self) -> Option<SoundEventRef> {
        None
    }

    fn check_spawn_rules(
        &self,
        world: &Arc<World>,
        _spawn_reason: EntitySpawnReason,
        _pos: BlockPos,
    ) -> bool {
        check_magma_cube_spawn_rules(world)
    }

    fn finalize_spawn(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        group_data: Option<SpawnGroupData>,
    ) -> Option<SpawnGroupData> {
        let result = self.finalize_spawn_mob_base(world, spawn_reason, group_data);
        cube_common::set_spawn_size(self, world);
        result
    }

    fn mob_flags(&self) -> i8 {
        *self.entity_data.lock().mob().mob_flags.get()
    }

    fn set_mob_flags(&self, flags: i8) {
        self.entity_data.lock().mob_mut().mob_flags.set(flags);
    }
}

impl CubeLike for MagmaCubeEntity {
    fn cube_state(&self) -> &SyncMutex<CubeState> {
        &self.cube
    }

    fn size(&self) -> i32 {
        *self.entity_data.lock().abstract_cube_mob().id_size.get()
    }

    fn store_size(&self, size: i32) {
        self.entity_data
            .lock()
            .abstract_cube_mob_mut()
            .id_size
            .set(size);
    }

    /// Adds the armor and the extra bite on top of the shared arithmetic.
    ///
    /// Vanilla parity: `MagmaCube.setSize`.
    fn set_size(&self, size: i32, update_health: bool) {
        cube_common::apply_size(self, size, update_health);

        let size = self.size();
        // Vanilla parity: `MagmaCube.setSize` ends `this.xpReward = actualSize`.
        // It sits here rather than in `apply_size` because `SulfurCube`
        // overrides `setSize` too and deliberately sets no reward.
        self.set_xp_reward(size);
        let mut attributes = self.attributes().lock();
        attributes.set_base_value(vanilla_attributes::ARMOR, ARMOR_PER_SIZE * f64::from(size));
        // Vanilla parity: `getAttackDamage` adds two on top of the attribute,
        // which Steel reads straight from the attribute, so the bonus is baked
        // in here instead.
        attributes.set_base_value(
            vanilla_attributes::ATTACK_DAMAGE,
            f64::from(size) + ATTACK_DAMAGE_BONUS,
        );
    }

    /// Dangerous at every size.
    ///
    /// Vanilla parity: `MagmaCube.isDealsDamage` drops the `!isTiny()` the base
    /// class has. This one line is why a nest of tiny magma cubes is a problem
    /// and a nest of tiny slimes is a nuisance.
    fn deals_damage(&self) -> bool {
        self.is_effective_ai()
    }

    /// Vanilla parity: `MagmaCube.getJumpDelay`, four times the base.
    fn jump_delay(&self) -> i32 {
        (rand::random_range(0..20) + 10) * JUMP_DELAY_MULTIPLIER
    }

    fn jump_sound(&self) -> SoundEventRef {
        &sound_events::ENTITY_MAGMA_CUBE_JUMP
    }

    fn squish_sound(&self) -> SoundEventRef {
        if self.is_tiny() {
            &sound_events::ENTITY_MAGMA_CUBE_SQUISH_SMALL
        } else {
            &sound_events::ENTITY_MAGMA_CUBE_SQUISH
        }
    }

    fn split_child(&self, position: DVec3, world: &Arc<World>) -> SharedEntity {
        let child = Arc::new(Self::new(
            self.entity_type,
            next_entity_id(),
            position,
            Arc::downgrade(world),
        ));
        child.set_size(self.size() / 2, true);
        child.set_rotation((rand::random::<f32>() * 360.0, 0.0));
        child
    }
}

impl PathfinderMob for MagmaCubeEntity {}

impl Enemy for MagmaCubeEntity {}

#[cfg(test)]
mod tests {
    use std::sync::Weak;

    use glam::DVec3;
    use steel_registry::{init_vanilla_registry, vanilla_entities};

    use super::*;
    use crate::entity::entities::SlimeEntity;

    /// Vanilla parity: `MagmaCube.setSize` ends `this.xpReward = actualSize`,
    /// so one of these is worth what it is big.
    #[test]
    fn a_magma_cube_is_worth_its_size() {
        init_vanilla_registry();
        let cube = MagmaCubeEntity::new(
            &vanilla_entities::MAGMA_CUBE,
            next_entity_id(),
            DVec3::ZERO,
            Weak::new(),
        );

        cube.set_size(4, true);
        assert_eq!(cube.xp_reward(), 4);
        cube.set_size(2, true);
        assert_eq!(cube.xp_reward(), 2, "a split cube kept the parent's reward");
    }

    fn magma_cube() -> MagmaCubeEntity {
        MagmaCubeEntity::new(
            &vanilla_entities::MAGMA_CUBE,
            1,
            DVec3::ZERO,
            Weak::<World>::new(),
        )
    }

    fn slime() -> SlimeEntity {
        SlimeEntity::new(
            &vanilla_entities::SLIME,
            2,
            DVec3::ZERO,
            Weak::<World>::new(),
        )
    }

    #[test]
    fn downcast_key_identifies_magma_cube() {
        assert_eq!(
            MagmaCubeEntity::TYPE_KEY,
            DowncastTypeKey::new("steel:entity/magma_cube")
        );
    }

    #[test]
    fn a_tiny_magma_cube_still_hurts_where_a_tiny_slime_does_not() {
        init_vanilla_registry();
        let cube = magma_cube();
        let slime = slime();
        cube.set_size(1, true);
        slime.set_size(1, true);

        assert!(cube.is_tiny() && slime.is_tiny());
        // `deals_damage` also asks `is_effective_ai`, which a world-less mob
        // answers for itself; the two are compared rather than asserted flat so
        // the test says what it means -- they must differ.
        assert_ne!(
            cube.deals_damage(),
            slime.deals_damage(),
            "the whole point of the magma cube is that a tiny one is dangerous"
        );
    }

    #[test]
    fn size_buys_armor_and_a_bigger_bite() {
        init_vanilla_registry();
        let cube = magma_cube();

        cube.set_size(4, true);
        let (armor, damage) = {
            let attributes = cube.attributes().lock();
            (
                attributes.required_value(vanilla_attributes::ARMOR),
                attributes.required_value(vanilla_attributes::ATTACK_DAMAGE),
            )
        };

        assert!((armor - 12.0).abs() < 1e-9, "armor was {armor}");
        assert!((damage - 6.0).abs() < 1e-9, "attack damage was {damage}");
    }

    #[test]
    fn a_slime_of_the_same_size_wears_no_armor() {
        init_vanilla_registry();
        let slime = slime();
        slime.set_size(4, true);

        let armor = slime
            .attributes()
            .lock()
            .required_value(vanilla_attributes::ARMOR);
        assert!(
            armor.abs() < 1e-9,
            "a slime should have no armor, got {armor}"
        );
    }

    #[test]
    fn magma_cubes_hop_four_times_less_often() {
        init_vanilla_registry();
        let cube = magma_cube();

        // The delay is random, so the bound is what can be asserted: vanilla's
        // base is 10..30, so four times that can never fall below 40.
        for _ in 0..50 {
            assert!(cube.jump_delay() >= 40, "{}", cube.jump_delay());
            assert!(cube.jump_delay() <= 120);
        }
    }

    #[test]
    fn size_still_drives_health_and_hitbox_through_the_shared_base() {
        init_vanilla_registry();
        let cube = magma_cube();

        cube.set_size(4, true);
        assert!((cube.get_max_health() - 16.0).abs() < f32::EPSILON);

        let big = cube.dimensions_for_pose(EntityPose::Standing).width;
        cube.set_size(1, true);
        let small = cube.dimensions_for_pose(EntityPose::Standing).width;
        assert!(big > small);
    }

    #[test]
    fn a_magma_cube_in_lava_is_never_drawn_on_fire() {
        init_vanilla_registry();
        assert!(!Entity::is_on_fire(&magma_cube()));
    }
}
