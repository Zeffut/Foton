//! Ocelot entity.
//!
//! Vanilla parity: `Ocelot`. The ocelot is the counter-example to the rest of
//! this batch: since 1.14 it cannot be tamed at all. Fish buys *trust*, which
//! only stops it running away -- it never follows, never sits, and never gets
//! an owner. It is an [`Animal`], not a `TamableAnimal`, and the trust flag is
//! one synced boolean.

mod tempt;

use std::sync::{Arc, Weak};

use foton_macros::entity_behavior;
use foton_registry::entity_data::EntityPose;
use foton_registry::entity_type::{
    EntityAttachmentPoint, EntityAttachments, EntityDimensions, EntityTypeRef,
};
use foton_registry::item_stack::ItemStack;
use foton_registry::sound_event::SoundEventRef;
use foton_registry::vanilla_entity_data::OcelotEntityData;
use foton_registry::vanilla_item_tags::ItemTag;
use foton_registry::{REGISTRY, TaggedRegistryExt, sound_events, vanilla_entities};
use foton_utils::entity_events::EntityStatus;
use foton_utils::locks::SyncMutex;
use foton_utils::types::InteractionHand;
use foton_utils::{BlockPos, Downcast as _, DowncastType, DowncastTypeKey};
use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;

use crate::behavior::InteractionResult;
use crate::entity::ai::control::MoveControlOperation;
use crate::entity::ai::goal::{
    AvoidEntityGoal, BreedGoal, FloatGoal, Goal, GoalControls, LeapAtTargetGoal, LookAtPlayerGoal,
    NearestAttackableTargetGoal, OcelotAttackGoal, WaterAvoidingRandomStrollGoal,
};
use crate::entity::damage::DamageSource;
use crate::entity::{
    AgeableMob, AgeableMobBase, AgeableMobGroupData, Animal, AnimalBase, Entity, EntityBase,
    EntityBaseLoad, EntitySpawnReason, EntitySyncedData, LivingEntity, LivingEntityBase,
    LivingEntitySyncedData, Mob, MobBase, PathfinderMob, SpawnGroupData,
};
use crate::physics::MoveResult;
use crate::player::Player;
use crate::world::World;

use tempt::new as ocelot_tempt_goal;

/// Speed an ocelot creeps at.
///
/// Vanilla parity: `Ocelot.CROUCH_SPEED_MOD`.
const CROUCH_SPEED_MOD: f64 = 0.6;

/// Speed an ocelot walks at.
///
/// Vanilla parity: `Ocelot.WALK_SPEED_MOD`.
const WALK_SPEED_MOD: f64 = 0.8;

/// Speed an ocelot sprints at.
///
/// Vanilla parity: `Ocelot.SPRINT_SPEED_MOD`.
const SPRINT_SPEED_MOD: f64 = 1.33;

/// The ocelot's baby hitbox.
///
/// Vanilla parity: `Ocelot.BABY_DIMENSIONS`.
const OCELOT_BABY_PASSENGER_ATTACHMENTS: [EntityAttachmentPoint; 1] =
    [EntityAttachmentPoint::new(0.0, 0.3125, 0.0)];
const OCELOT_BABY_DIMENSIONS: EntityDimensions = EntityDimensions::new_with_attachments(
    0.3,
    0.35,
    0.34375,
    EntityAttachments::new(&OCELOT_BABY_PASSENGER_ATTACHMENTS, &[], &[], &[]),
);

/// One chance in this many that a fish earns the ocelot's trust.
///
/// Vanilla parity: the `random.nextInt(3) == 0` of `Ocelot.mobInteract`.
const TRUST_CHANCE: i32 = 3;

/// How close a player must be for the fish to count.
///
/// Vanilla parity: the `player.distanceToSqr(this) < 9.0` of the same method.
const FEED_DISTANCE_SQR: f64 = 9.0;

/// How long an untrusting ocelot lives before it despawns.
///
/// Vanilla parity: the `tickCount > 2400` of `Ocelot.removeWhenFarAway`.
const UNTRUSTING_DESPAWN_AGE_TICKS: i32 = 2400;

/// How far an untrusting ocelot runs from a player.
///
/// Vanilla parity: the `16.0F` of `Ocelot.OcelotAvoidEntityGoal`.
const AVOID_PLAYER_DISTANCE: f32 = 16.0;

/// Probability the stroll goal aims at a tree rather than the ground.
///
/// Vanilla parity: the `1.0000001E-5F` of `Ocelot.registerGoals`, which is
/// effectively "never", and is what keeps an ocelot on the forest floor.
const STROLL_LAND_PROBABILITY: f32 = 1.000_000_1e-5;

/// Vanilla ocelot entity.
#[entity_behavior(class = "Ocelot")]
pub struct OcelotEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    ageable_base: AgeableMobBase,
    animal_base: AnimalBase,
    entity_data: SyncMutex<OcelotEntityData>,
}

// SAFETY: This key is owned by Foton and uniquely identifies `OcelotEntity`.
unsafe impl DowncastType for OcelotEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:entity/ocelot");
}

impl OcelotEntity {
    /// Creates a new ocelot entity.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Creates an ocelot entity from saved base data.
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
        let ageable_base = AgeableMobBase::new();
        let animal_base = AnimalBase::new();
        AnimalBase::initialize_pathfinding_malus(&mob_base);
        let mut entity_data = OcelotEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);

        let ocelot = Self {
            base,
            entity_type,
            living_base,
            mob_base,
            ageable_base,
            animal_base,
            entity_data: SyncMutex::new(entity_data),
        };
        ocelot.register_goals();
        ocelot
    }

    /// Vanilla parity: `Ocelot.registerGoals`.
    ///
    /// Vanilla's `reassessTrustingGoals` adds and removes the avoid-players
    /// goal as trust changes. Foton registers it once with the trust checks the
    /// vanilla subclass already carries in `canUse`/`canContinueToUse`, which
    /// leaves the same behavior.
    fn register_goals(&self) {
        {
            let mut goals = self.mob_base.goal_selector().lock();
            goals.add_goal(1, FloatGoal::new(&self.mob_base));
            goals.add_goal(3, ocelot_tempt_goal(CROUCH_SPEED_MOD));
            goals.add_goal(4, OcelotAvoidPlayersGoal::new());
            goals.add_goal(7, LeapAtTargetGoal::new(0.3));
            goals.add_goal(8, OcelotAttackGoal::new());
            goals.add_goal(9, BreedGoal::new(WALK_SPEED_MOD));
            goals.add_goal(
                10,
                WaterAvoidingRandomStrollGoal::with_probability(
                    WALK_SPEED_MOD,
                    STROLL_LAND_PROBABILITY,
                ),
            );
            goals.add_goal(11, LookAtPlayerGoal::new(10.0));
        }

        let mut targets = self.mob_base.target_selector().lock();
        targets.add_goal(
            1,
            NearestAttackableTargetGoal::new(false, |_, target, _| {
                target.entity_type() == &vanilla_entities::CHICKEN
            }),
        );
        // Vanilla parity gap: the second target goal hunts land-bound baby
        // turtles. Foton has no turtle yet.
    }

    /// Returns vanilla `Ocelot.isTrusting`.
    #[must_use]
    pub fn is_trusting(&self) -> bool {
        *self.entity_data.lock().trusting.get()
    }

    /// Sets vanilla `Ocelot.setTrusting`.
    pub fn set_trusting(&self, trusting: bool) {
        self.entity_data.lock().trusting.set(trusting);
    }

    /// Returns whether the stack is vanilla ocelot food.
    #[must_use]
    pub fn is_ocelot_food(item_stack: &ItemStack) -> bool {
        REGISTRY
            .items
            .is_in_tag(item_stack.item(), &ItemTag::OCELOT_FOOD)
    }

    fn update_dirty_mob_effect_entity_data(&self) {
        if !self.living_base.take_effects_dirty() {
            return;
        }

        let display = self.living_base.mob_effect_display_state();

        {
            let mut entity_data = self.entity_data.lock();
            let living = entity_data.living_entity_mut();
            living.effect_particles.set(display.particles);
            living.effect_ambience.set(display.ambient);
        }

        self.entity_data.set_base_invisible_flag(display.invisible);
        self.entity_data
            .set_base_glowing_flag(self.has_glowing_tag() || display.glowing);
    }
}

/// The goal that keeps an untrusting ocelot away from players.
///
/// Vanilla parity: `Ocelot.OcelotAvoidEntityGoal`.
struct OcelotAvoidPlayersGoal {
    avoid: AvoidEntityGoal,
}

impl OcelotAvoidPlayersGoal {
    fn new() -> Self {
        Self {
            avoid: AvoidEntityGoal::with_selector(
                AVOID_PLAYER_DISTANCE,
                WALK_SPEED_MOD,
                SPRINT_SPEED_MOD,
                |_, target, _| {
                    target.as_player().is_some_and(|player| {
                        !target.is_spectator() && !player.has_infinite_materials()
                    })
                },
            ),
        }
    }

    fn is_trusting(mob: &dyn PathfinderMob) -> bool {
        mob.downcast_ref::<OcelotEntity>()
            .is_some_and(OcelotEntity::is_trusting)
    }
}

impl Goal for OcelotAvoidPlayersGoal {
    fn controls(&self) -> GoalControls {
        self.avoid.controls()
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        !Self::is_trusting(mob) && self.avoid.can_use(mob)
    }

    fn can_continue_to_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        !Self::is_trusting(mob) && self.avoid.can_continue_to_use(mob)
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        self.avoid.start(mob);
    }

    fn stop(&mut self, mob: &dyn PathfinderMob) {
        self.avoid.stop(mob);
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        self.avoid.tick(mob);
    }
}

/// Returns whether an ocelot may appear at all.
///
/// Vanilla parity: `Ocelot.checkOcelotSpawnRules`, which is nothing but a
/// two-in-three roll on top of the ordinary animal rules.
#[must_use]
fn check_ocelot_spawn_rules() -> bool {
    rand::random_range(0..3) != 0
}

impl Entity for OcelotEntity {
    fn base(&self) -> &EntityBase {
        &self.base
    }

    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
    }

    fn base_tick(&self) {
        Mob::base_tick_mob(self);
    }

    fn dimensions_for_pose(&self, _pose: EntityPose) -> EntityDimensions {
        let scale = LivingEntity::get_scale(self);
        if AgeableMob::is_baby(self) {
            OCELOT_BABY_DIMENSIONS.scale(scale)
        } else if self.entity_type.fixed {
            self.entity_type.dimensions
        } else {
            self.entity_type.dimensions.scale(scale)
        }
    }

    fn synced_data(&self) -> Option<&dyn EntitySyncedData> {
        Some(&self.entity_data)
    }

    fn update_data_before_sync(&self) {
        self.update_dirty_mob_effect_entity_data();
    }

    /// Vanilla parity: `Ocelot.isSteppingCarefully`.
    fn is_stepping_carefully(&self) -> bool {
        Entity::is_crouching(self) || self.is_suppressing_bounce()
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_mob(nbt);
        self.save_ageable_mob(nbt);
        self.save_animal(nbt);
        nbt.insert("Trusting", i8::from(self.is_trusting()));
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_mob(nbt);
        self.load_ageable_mob(nbt);
        self.load_animal(nbt);
        self.set_trusting(nbt.byte("Trusting").is_some_and(|value| value != 0));
    }
}

impl LivingEntity for OcelotEntity {
    fn living_base(&self) -> &LivingEntityBase {
        &self.living_base
    }

    fn living_synced_data(&self) -> Option<&dyn LivingEntitySyncedData> {
        Some(&self.entity_data)
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
        Some(&sound_events::ENTITY_OCELOT_HURT)
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_OCELOT_DEATH)
    }

    fn server_ai_step(&self) {
        Mob::mob_server_ai_step(self);
    }

    fn ai_step(&self) -> Option<MoveResult> {
        let result = self.default_ai_step();
        AgeableMob::tick_ageable_mob(self);
        Animal::tick_animal_love(self);
        result
    }
}

impl AgeableMob for OcelotEntity {
    fn ageable_base(&self) -> &AgeableMobBase {
        &self.ageable_base
    }

    fn is_age_locked(&self) -> bool {
        *self.entity_data.lock().ageable_mob().age_locked.get()
    }

    fn set_age_locked(&self, age_locked: bool) {
        self.entity_data
            .lock()
            .ageable_mob_mut()
            .age_locked
            .set(age_locked);
    }

    fn set_synced_baby(&self, baby: bool) {
        self.entity_data.lock().ageable_mob_mut().baby.set(baby);
    }
}

impl Animal for OcelotEntity {
    fn animal_base(&self) -> &AnimalBase {
        &self.animal_base
    }

    fn is_food(&self, item_stack: &ItemStack) -> bool {
        Self::is_ocelot_food(item_stack)
    }
}

impl Mob for OcelotEntity {
    fn mob_base(&self) -> &MobBase {
        &self.mob_base
    }

    fn mob_flags(&self) -> i8 {
        *self.entity_data.lock().mob().mob_flags.get()
    }

    fn set_mob_flags(&self, flags: i8) {
        self.entity_data.lock().mob_mut().mob_flags.set(flags);
    }

    fn tick_goal_selectors(&self) {
        PathfinderMob::tick_pathfinder_goal_selectors(self);
    }

    fn tick_path_navigation(&self) {
        PathfinderMob::tick_pathfinder_path_navigation(self);
    }

    fn ambient_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_OCELOT_AMBIENT)
    }

    fn ambient_sound_interval(&self) -> i32 {
        900
    }

    /// Vanilla parity: `Ocelot.customServerAiStep`, the same speed-to-pose
    /// mapping the cat uses.
    fn custom_server_ai_step(&self) {
        Animal::custom_server_ai_step_animal(self);

        let wanted_speed = {
            let controls = self.mob_base.controls().lock();
            (controls.move_control.operation() == MoveControlOperation::MoveTo)
                .then(|| controls.move_control.speed_modifier())
        };

        let (pose, sprinting) = match wanted_speed {
            Some(speed) if (speed - CROUCH_SPEED_MOD).abs() < f64::EPSILON => {
                (EntityPose::Sneaking, false)
            }
            Some(speed) if (speed - SPRINT_SPEED_MOD).abs() < f64::EPSILON => {
                (EntityPose::Standing, true)
            }
            _ => (EntityPose::Standing, false),
        };

        self.set_pose(pose);
        self.set_sprinting(sprinting);
    }

    /// Vanilla parity: `Ocelot.removeWhenFarAway`.
    fn remove_when_far_away(&self, _dist_sqr: f64) -> bool {
        !self.is_trusting() && self.tick_count() > UNTRUSTING_DESPAWN_AGE_TICKS
    }

    fn check_spawn_rules(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        pos: BlockPos,
    ) -> bool {
        // Vanilla parity gap: `Ocelot.checkSpawnObstruction` also refuses any
        // position below sea level or off grass and leaves. Foton has no
        // `checkSpawnObstruction` hook on `Mob`, so only the spawn predicate
        // runs; the biome spawn lists already keep ocelots in the jungle.
        check_ocelot_spawn_rules()
            && <Self as Animal>::check_animal_spawn_rules(world.as_ref(), spawn_reason, pos)
    }

    /// Vanilla parity: `Ocelot.finalizeSpawn`, which always spawns a kitten
    /// alongside the adult by handing the group data a baby chance of one.
    fn finalize_spawn(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        group_data: Option<SpawnGroupData>,
    ) -> Option<SpawnGroupData> {
        let group_data = group_data.unwrap_or(SpawnGroupData::AgeableMob(
            AgeableMobGroupData::with_baby_spawn_chance(1.0),
        ));
        self.finalize_spawn_ageable_mob(world, spawn_reason, Some(group_data))
    }

    /// Vanilla parity: `Ocelot.mobInteract`. Trust is not taming: it stops the
    /// ocelot fleeing and nothing else.
    fn mob_interact(&self, player: &Player, hand: InteractionHand) -> InteractionResult {
        let item_stack = {
            let inventory = player.inventory.lock();
            let item_stack = inventory.get_item_in_hand(hand);
            item_stack.copy_with_count(item_stack.count())
        };

        let close_enough = player.position().distance_squared(self.position()) < FEED_DISTANCE_SQR;
        if !self.is_being_tempted()
            || self.is_trusting()
            || !Self::is_ocelot_food(&item_stack)
            || !close_enough
        {
            return Animal::mob_interact_animal(self, player, hand);
        }

        Mob::use_player_item(self, player, hand);
        if rand::random_range(0..TRUST_CHANCE) == 0 {
            self.set_trusting(true);
            self.broadcast_entity_event(EntityStatus::TrustingSucceeded);
        } else {
            self.broadcast_entity_event(EntityStatus::TrustingFailed);
        }

        InteractionResult::Success
    }
}

impl PathfinderMob for OcelotEntity {}

#[cfg(test)]
mod tests;
