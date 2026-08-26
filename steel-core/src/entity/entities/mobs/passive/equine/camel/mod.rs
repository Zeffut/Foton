//! The camel.
//!
//! Vanilla parity: `net.minecraft.world.entity.animal.camel.Camel`. A camel is
//! an `AbstractHorse` that is tame from birth, carries two riders, sits down of
//! its own accord and has to be stood up before it will move, and dashes -- a
//! long forward leap on a fifty-five tick cooldown that is the only way a
//! player crosses a ravine on one.

use std::sync::{Arc, Weak};

use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;
use steel_macros::entity_behavior;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::entity_type::{EntityDimensions, EntityTypeRef};
use steel_registry::item_stack::ItemStack;
use steel_registry::sound_event::SoundEventRef;
use steel_registry::vanilla_block_tags::BlockTag;
use steel_registry::vanilla_entity_data::CamelEntityData;
use steel_registry::vanilla_item_tags::ItemTag;
use steel_registry::{
    REGISTRY, TaggedRegistryExt as _, sound_events, vanilla_attributes, vanilla_game_events,
    vanilla_items,
};
use steel_utils::locks::SyncMutex;
use steel_utils::types::InteractionHand;
use steel_utils::{BlockPos, BlockStateId, DowncastType, DowncastTypeKey};

use crate::behavior::InteractionResult;
use crate::entity::ai::brain::Brain;
use crate::entity::damage::DamageSource;
use crate::entity::{
    AbstractHorse, AbstractHorseBase, AgeableMob, AgeableMobBase, Animal, AnimalBase, Entity,
    EntityBase, EntityBaseLoad, EntityEventSource as _, EntityPose, EntitySpawnReason,
    EntitySyncedData, LivingEntity, LivingEntityBase, LivingEntitySyncedData, Mob, MobBase,
    MoveResult, PathfinderMob, SpawnGroupData,
};
use crate::physics::WorldCollisionProvider;
use crate::physics::collision::CollisionWorld as _;
use crate::player::Player;
use crate::world::game_event::GameEventContext;
use crate::world::{LevelReader as _, World};

mod camel_ai;

/// Vanilla parity: `Camel.BABY_SCALE`.
const BABY_SCALE: f32 = 0.6;
/// Vanilla parity: `Camel.DASH_COOLDOWN_TICKS`.
pub const DASH_COOLDOWN_TICKS: i32 = 55;
/// Vanilla parity: `Camel.MAX_HEAD_Y_ROT`.
const MAX_HEAD_Y_ROT: f32 = 30.0;
/// Vanilla parity: `Camel.RUNNING_SPEED_BONUS`.
const RUNNING_SPEED_BONUS: f32 = 0.1;
/// Vanilla parity: `Camel.DASH_VERTICAL_MOMENTUM`.
const DASH_VERTICAL_MOMENTUM: f64 = 1.428_5;
/// Vanilla parity: `Camel.DASH_HORIZONTAL_MOMENTUM`.
const DASH_HORIZONTAL_MOMENTUM: f64 = 22.222_2;
/// Vanilla parity: `Camel.SITDOWN_DURATION_TICKS`.
pub const SITDOWN_DURATION_TICKS: i64 = 40;
/// Vanilla parity: `Camel.STANDUP_DURATION_TICKS`.
pub const STANDUP_DURATION_TICKS: i64 = 52;
/// Vanilla parity: `Camel.SITTING_HEIGHT_DIFFERENCE`.
const SITTING_HEIGHT_DIFFERENCE: f32 = 1.43;
/// Vanilla parity: the `dashCooldown < 50` of `Camel.tick`, which is what makes
/// the dash last at least five ticks before the ground can end it.
const DASH_MINIMUM_DURATION_TICKS: i32 = 5;
/// Vanilla parity: the `getPassengers().size() < 2` of `Camel.mobInteract`.
const MAX_RIDERS: usize = 2;

/// Vanilla parity: `Camel.BABY_STANDING_DIMENSIONS`.
const BABY_STANDING_DIMENSIONS: EntityDimensions = EntityDimensions::new(0.95, 1.4, 1.38);
/// Vanilla parity: `Camel.BABY_SITTING_DIMENSIONS`.
const BABY_SITTING_DIMENSIONS: EntityDimensions = EntityDimensions::new(0.95, 0.425, 0.41);
/// Vanilla parity: the `withEyeHeight(0.845F)` of `ADULT_SITTING_DIMENSIONS`.
const ADULT_SITTING_EYE_HEIGHT: f32 = 0.845;

/// A camel.
#[entity_behavior(class = "Camel")]
pub struct CamelEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    ageable_base: AgeableMobBase,
    animal_base: AnimalBase,
    abstract_horse_base: AbstractHorseBase,
    brain: Brain,
    /// Vanilla parity: `Camel.dashCooldown`.
    dash_cooldown: SyncMutex<i32>,
    entity_data: SyncMutex<CamelEntityData>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `CamelEntity`.
unsafe impl DowncastType for CamelEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/camel");
}

impl CamelEntity {
    /// Creates a camel at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Creates a camel from saved base data.
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
        {
            // Vanilla parity: the two navigation calls of the `Camel`
            // constructor -- a camel is tall enough to step over a fence.
            let mut navigation = mob_base.navigation().lock();
            navigation.set_can_float(true);
            navigation.set_can_walk_over_fences(true);
        }
        let mut entity_data = CamelEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);

        Self {
            base,
            entity_type,
            living_base,
            mob_base,
            ageable_base,
            animal_base,
            abstract_horse_base: AbstractHorseBase::new(0),
            brain: camel_ai::make_brain(),
            dash_cooldown: SyncMutex::new(0),
            entity_data: SyncMutex::new(entity_data),
        }
    }

    /// Returns vanilla `Camel.isDashing`.
    #[must_use]
    pub fn is_dashing(&self) -> bool {
        *self.entity_data.lock().dash.get()
    }

    /// Sets vanilla `Camel.setDashing`.
    ///
    /// Vanilla's `onSyncedDataUpdated` starts the cooldown from the flag going
    /// up; Steel does it here, which is the only place the flag changes.
    pub fn set_dashing(&self, dashing: bool) {
        let changed = self.is_dashing() != dashing;
        self.entity_data.lock().dash.set(dashing);
        if changed && dashing && self.dash_cooldown() == 0 {
            *self.dash_cooldown.lock() = DASH_COOLDOWN_TICKS;
        }
    }

    /// Returns vanilla `Camel.getJumpCooldown`, which is the dash cooldown.
    #[must_use]
    pub fn dash_cooldown(&self) -> i32 {
        *self.dash_cooldown.lock()
    }

    fn last_pose_change_tick(&self) -> i64 {
        *self.entity_data.lock().last_pose_change_tick.get()
    }

    /// Vanilla parity: `Camel.resetLastPoseChangeTick`.
    pub fn reset_last_pose_change_tick(&self, synced_pose_tick_time: i64) {
        self.entity_data
            .lock()
            .last_pose_change_tick
            .set(synced_pose_tick_time);
    }

    /// Vanilla parity: `Camel.resetLastPoseChangeTickToFullStand`.
    fn reset_last_pose_change_tick_to_full_stand(&self, current_time: i64) {
        self.reset_last_pose_change_tick((current_time - STANDUP_DURATION_TICKS - 1).max(0));
    }

    /// Returns vanilla `Camel.getPoseTime`.
    ///
    /// The sign of the stored tick is the pose: negative means sitting, and the
    /// absolute value is when the change started.
    #[must_use]
    pub fn pose_time(&self) -> i64 {
        let Some(world) = self.level() else {
            return 0;
        };
        world.game_time() - self.last_pose_change_tick().abs()
    }

    /// Returns vanilla `Camel.isCamelSitting`.
    #[must_use]
    pub fn is_camel_sitting(&self) -> bool {
        self.last_pose_change_tick() < 0
    }

    /// Returns vanilla `Camel.isInPoseTransition`.
    #[must_use]
    pub fn is_in_pose_transition(&self) -> bool {
        let duration = if self.is_camel_sitting() {
            SITDOWN_DURATION_TICKS
        } else {
            STANDUP_DURATION_TICKS
        };
        self.pose_time() < duration
    }

    /// Returns vanilla `Camel.refuseToMove`, which is the whole reason a camel
    /// has to be stood up before it will go anywhere.
    #[must_use]
    pub fn refuse_to_move(&self) -> bool {
        self.is_camel_sitting() || self.is_in_pose_transition()
    }

    /// Returns vanilla `Camel.canCamelChangePose`.
    #[must_use]
    pub fn can_camel_change_pose(&self) -> bool {
        let target = if self.is_camel_sitting() {
            EntityPose::Standing
        } else {
            EntityPose::Sitting
        };
        self.would_not_suffocate_at_target_pose(target)
    }

    /// Vanilla parity: `LivingEntity.wouldNotSuffocateAtTargetPose`, which is
    /// what stops a camel sitting down under a two-block ceiling and then
    /// finding it cannot stand back up.
    fn would_not_suffocate_at_target_pose(&self, pose: EntityPose) -> bool {
        let Some(world) = self.level() else {
            return false;
        };
        let dimensions = self.dimensions_for_pose(pose);
        let position = self.position();
        let target_box = steel_utils::WorldAabb::entity_box(
            position.x,
            position.y,
            position.z,
            f64::from(dimensions.half_width()),
            f64::from(dimensions.height),
        );
        !WorldCollisionProvider::new(&world).has_block_collision(&target_box)
    }

    /// Vanilla parity: `Camel.sitDown`.
    pub fn sit_down(&self) {
        if self.is_camel_sitting() {
            return;
        }
        let Some(world) = self.level() else {
            return;
        };
        self.make_sound(Some(&sound_events::ENTITY_CAMEL_SIT));
        self.set_pose(EntityPose::Sitting);
        self.camel_game_event();
        self.reset_last_pose_change_tick(-world.game_time());
    }

    /// Vanilla parity: `Camel.standUp`.
    pub fn stand_up(&self) {
        if !self.is_camel_sitting() {
            return;
        }
        let Some(world) = self.level() else {
            return;
        };
        self.make_sound(Some(&sound_events::ENTITY_CAMEL_STAND));
        self.set_pose(EntityPose::Standing);
        self.camel_game_event();
        self.reset_last_pose_change_tick(world.game_time());
    }

    /// Vanilla parity: `Camel.standUpInstantly`, which is what damage and water
    /// do -- no animation, straight to standing.
    pub fn stand_up_instantly(&self) {
        let Some(world) = self.level() else {
            return;
        };
        self.set_pose(EntityPose::Standing);
        self.camel_game_event();
        self.reset_last_pose_change_tick_to_full_stand(world.game_time());
    }

    fn camel_game_event(&self) {
        let Some(world) = self.level() else {
            return;
        };
        world.game_event(
            &vanilla_game_events::ENTITY_ACTION,
            self.block_position(),
            &GameEventContext::new(Some(self.as_entity_event_source()), None),
        );
    }

    /// Vanilla parity: `Camel.executeRidersJump`, the dash.
    ///
    /// It is not a jump: the whole impulse is forward along the look vector,
    /// scaled by the camel's speed and the block it is standing on, with only a
    /// small lift on top.
    fn execute_camel_dash(&self, amount: f32) {
        let jump_momentum = self.get_jump_power();
        let speed = self
            .attributes()
            .lock()
            .get_value(vanilla_attributes::MOVEMENT_SPEED)
            .unwrap_or_default();
        let forward = (self.look_angle() * DVec3::new(1.0, 0.0, 1.0)).normalize_or_zero()
            * (DASH_HORIZONTAL_MOMENTUM
                * f64::from(amount)
                * speed
                * f64::from(self.block_speed_factor()));

        self.set_velocity(
            self.velocity()
                + forward
                + DVec3::new(
                    0.0,
                    DASH_VERTICAL_MOMENTUM * f64::from(amount) * f64::from(jump_momentum),
                    0.0,
                ),
        );
        self.mark_velocity_sync();
        *self.dash_cooldown.lock() = DASH_COOLDOWN_TICKS;
        self.set_dashing(true);
    }

    /// Vanilla parity: the dash half of `Camel.tick`.
    fn tick_dash(&self) {
        if self.is_dashing()
            && self.dash_cooldown() < DASH_COOLDOWN_TICKS - DASH_MINIMUM_DURATION_TICKS
            && (self.on_ground() || self.is_in_water() || self.is_in_lava() || self.is_passenger())
        {
            self.set_dashing(false);
        }

        let cooldown = self.dash_cooldown();
        if cooldown <= 0 {
            return;
        }
        *self.dash_cooldown.lock() = cooldown - 1;
        if cooldown - 1 == 0 {
            self.play_sound(&sound_events::ENTITY_CAMEL_DASH_READY, 1.0, 1.0);
        }
    }

    /// Returns whether the stack is vanilla camel food.
    #[must_use]
    pub fn is_camel_food(item_stack: &ItemStack) -> bool {
        REGISTRY
            .items
            .is_in_tag(item_stack.item(), &ItemTag::CAMEL_FOOD)
    }

    /// Vanilla parity: `Camel.checkCamelSpawnRules`.
    #[must_use]
    pub fn check_camel_spawn_rules(world: &Arc<World>, pos: BlockPos) -> bool {
        world
            .get_block_state(pos.below())
            .get_block()
            .has_tag(&BlockTag::CAMELS_SPAWNABLE_ON)
            && <Self as Animal>::is_bright_enough_to_spawn(world.as_ref(), pos)
    }
}

impl Entity for CamelEntity {
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

    /// Vanilla parity: `Camel.tick`.
    fn tick(&self) {
        self.default_tick();
        self.tick_dash();

        if self.refuse_to_move() {
            self.set_y_head_rot(self.y_body_rot());
        }
        if self.is_camel_sitting() && self.is_in_water() {
            self.stand_up_instantly();
        }
    }

    /// Vanilla parity: `Camel.getDefaultDimensions`, which is what makes a
    /// sitting camel low enough to walk over.
    fn dimensions_for_pose(&self, pose: EntityPose) -> EntityDimensions {
        let baby = AgeableMob::is_baby(self);
        match (pose, baby) {
            (EntityPose::Sitting, true) => BABY_SITTING_DIMENSIONS,
            (EntityPose::Sitting, false) => {
                let standing = self.entity_type().dimensions;
                EntityDimensions::new(
                    standing.width,
                    standing.height - SITTING_HEIGHT_DIFFERENCE,
                    ADULT_SITTING_EYE_HEIGHT,
                )
            }
            (_, true) => BABY_STANDING_DIMENSIONS,
            (_, false) => self.entity_type().dimensions,
        }
    }

    /// Vanilla parity: `Camel.playStepSound`, which is the only step sound in
    /// the game that reads a block tag.
    fn play_step_sound(&self, _pos: BlockPos, block_state: BlockStateId) {
        if block_state
            .get_block()
            .has_tag(&BlockTag::CAMEL_SAND_STEP_SOUND_BLOCKS)
        {
            self.play_sound(&sound_events::ENTITY_CAMEL_STEP_SAND, 1.0, 1.0);
        } else {
            self.play_sound(&sound_events::ENTITY_CAMEL_STEP, 1.0, 1.0);
        }
    }

    /// Vanilla parity: `Camel.canAddPassenger`, which is what makes a camel the
    /// only two-seat mount in the game.
    fn can_add_passenger(&self, _passenger: &dyn Entity) -> bool {
        self.passengers().len() < MAX_RIDERS
    }

    /// Vanilla parity: `Camel.handleStartJump`, which is where the dash sound
    /// and the flag come from.
    fn handle_start_jump(&self, _jump_scale: i32) {
        self.make_sound(Some(&sound_events::ENTITY_CAMEL_DASH));
        self.camel_game_event();
        self.set_dashing(true);
    }

    /// Vanilla parity: `Camel.canJump`, which refuses while it is sitting.
    fn can_jump_while_ridden(&self) -> bool {
        !self.refuse_to_move() && Mob::is_saddled(self)
    }

    /// Vanilla parity: `Camel.openCustomInventoryScreen`.
    fn open_custom_inventory_screen(&self, player: &Player) {
        self.open_horse_inventory_screen(player);
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_mob(nbt);
        self.save_ageable_mob(nbt);
        self.save_animal(nbt);
        self.save_abstract_horse(nbt);
        nbt.insert("LastPoseTick", self.last_pose_change_tick());
        self.brain.save(nbt);
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_mob(nbt);
        self.load_ageable_mob(nbt);
        self.load_animal(nbt);
        self.load_abstract_horse(nbt);
        let pose_tick = nbt.long("LastPoseTick").unwrap_or(0);
        if pose_tick < 0 {
            self.set_pose(EntityPose::Sitting);
        }
        self.reset_last_pose_change_tick(pose_tick);
        self.brain.load(nbt);
    }
}

impl LivingEntity for CamelEntity {
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
        Some(&sound_events::ENTITY_CAMEL_HURT)
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_CAMEL_DEATH)
    }

    fn get_age_scale(&self) -> f32 {
        if AgeableMob::is_baby(self) {
            BABY_SCALE
        } else {
            1.0
        }
    }

    /// Vanilla parity: `Camel.actuallyHurt`, which stands a sitting camel up
    /// before the damage lands.
    fn actually_hurt(&self, world: &World, source: &DamageSource, amount: f32) {
        self.stand_up_instantly();
        self.living_actually_hurt(world, source, amount);
    }

    fn server_ai_step(&self) {
        Mob::mob_server_ai_step(self);
    }

    /// Vanilla parity: `Camel.travel`, which pins a sitting camel in place.
    fn travel(&self, input: DVec3) -> Option<MoveResult> {
        if self.refuse_to_move() && self.on_ground() {
            self.set_velocity(self.velocity() * DVec3::new(0.0, 1.0, 0.0));
            return self.default_travel(input * DVec3::new(0.0, 1.0, 0.0));
        }
        self.default_travel(input)
    }

    /// Vanilla parity: `Camel.tickRidden`, whose one addition is that pushing
    /// forward on a sitting camel stands it up.
    fn tick_ridden(&self, controller: &Player, ridden_input: DVec3) {
        self.tick_ridden_abstract_horse(controller, ridden_input);
        if ridden_input.z > 0.0 && self.is_camel_sitting() && !self.is_in_pose_transition() {
            self.stand_up();
        }
    }

    /// Vanilla parity: `Camel.getRiddenInput`.
    fn ridden_input(&self, controller: &Player, _self_input: DVec3) -> DVec3 {
        if self.refuse_to_move() {
            return DVec3::ZERO;
        }
        self.abstract_horse_ridden_input(controller)
    }

    /// Vanilla parity: `Camel.getRiddenSpeed`, whose sprint bonus is only paid
    /// out while the dash is off cooldown.
    fn ridden_speed(&self, controller: &Player) -> f32 {
        let base = self
            .attributes()
            .lock()
            .get_value(vanilla_attributes::MOVEMENT_SPEED)
            .unwrap_or_default() as f32;
        if controller.is_sprinting() && self.dash_cooldown() == 0 {
            base + RUNNING_SPEED_BONUS
        } else {
            base
        }
    }

    fn drop_custom_death_loot(&self, _source: &DamageSource, _killed_by_player: bool) {
        self.drop_abstract_horse_inventory();
    }

    fn ai_step(&self) -> Option<MoveResult> {
        self.ai_step_abstract_horse();
        let result = self.default_ai_step();
        AgeableMob::tick_ageable_mob(self);
        Animal::tick_animal_love(self);
        self.server_ai_step_abstract_horse();
        result
    }
}

impl AgeableMob for CamelEntity {
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

impl Animal for CamelEntity {
    fn animal_base(&self) -> &AnimalBase {
        &self.animal_base
    }

    fn is_food(&self, item_stack: &ItemStack) -> bool {
        Self::is_camel_food(item_stack)
    }
}

impl AbstractHorse for CamelEntity {
    fn abstract_horse_base(&self) -> &AbstractHorseBase {
        &self.abstract_horse_base
    }

    fn horse_flags(&self) -> i8 {
        *self.entity_data.lock().abstract_horse().id_flags.get()
    }

    fn set_horse_flags(&self, flags: i8) {
        self.entity_data
            .lock()
            .abstract_horse_mut()
            .id_flags
            .set(flags);
    }

    /// Vanilla parity: `Camel.onPlayerJump`, which refuses unless the camel is
    /// saddled, off cooldown and on the ground -- there is no dashing in mid-air.
    fn on_player_jump(&self, jump_amount: i32) {
        if !Mob::is_saddled(self) || self.dash_cooldown() > 0 || !self.on_ground() {
            return;
        }
        self.abstract_horse_on_player_jump(jump_amount);
    }

    /// Vanilla parity: `Camel.executeRidersJump`, which is the dash rather than
    /// the horse's vertical hop.
    fn execute_riders_jump(&self, amount: f32, _input: DVec3) {
        self.execute_camel_dash(amount);
    }

    /// Vanilla parity: `Camel.isTamed`, a flat `true` -- a camel needs no
    /// breaking in, only a saddle.
    fn is_tamed(&self) -> bool {
        true
    }

    /// Vanilla parity: `Camel.canPerformRearing`, which is `false`. A camel
    /// dashes rather than rears.
    fn can_perform_rearing(&self) -> bool {
        false
    }

    /// Vanilla parity: `Camel.handleEating`, which is a fixed two hearts and
    /// ten seconds of growth rather than the horse's per-item table.
    fn handle_eating(&self, player: &Player, item_stack: &ItemStack) -> bool {
        if !Animal::is_food(self, item_stack) {
            return false;
        }

        let could_heal = self.get_health() < self.get_max_health();
        if could_heal {
            self.heal(2.0);
        }

        let could_set_in_love =
            self.is_tamed() && self.get_age() == 0 && Animal::can_fall_in_love(self);
        if could_set_in_love {
            self.set_in_love(Some(player));
        }

        let could_age_up = self.can_age_up();
        if could_age_up {
            self.age_up(10, false);
        }

        if !could_heal && !could_set_in_love && !could_age_up {
            return false;
        }

        if !self.is_silent() {
            self.play_sound(
                &sound_events::ENTITY_CAMEL_EAT,
                1.0,
                1.0 + (rand::random::<f32>() - rand::random::<f32>()) * 0.2,
            );
        }
        if let Some(world) = self.level() {
            world.game_event(
                &vanilla_game_events::EAT,
                self.block_position(),
                &GameEventContext::new(Some(self.as_entity_event_source()), None),
            );
        }
        true
    }
}

impl Mob for CamelEntity {
    fn mob_base(&self) -> &MobBase {
        &self.mob_base
    }

    fn brain(&self) -> Option<&Brain> {
        Some(&self.brain)
    }

    fn mob_flags(&self) -> i8 {
        *self.entity_data.lock().mob().mob_flags.get()
    }

    fn set_mob_flags(&self, flags: i8) {
        self.entity_data.lock().mob_mut().mob_flags.set(flags);
    }

    fn tick_path_navigation(&self) {
        PathfinderMob::tick_pathfinder_path_navigation(self);
    }

    /// Vanilla parity: `Camel.customServerAiStep`.
    fn custom_server_ai_step(&self) {
        let Some(world) = self.level() else {
            return;
        };
        self.brain.tick(&world, self);
        camel_ai::update_activity(&self.brain);
        Animal::custom_server_ai_step_animal(self);
    }

    /// Vanilla parity: `Camel.CamelMoveControl.tick`, which is what stands a
    /// wandering camel up when its brain gives it somewhere to be.
    fn tick_move_control(&self) {
        if !self.is_leashed()
            && self.is_camel_sitting()
            && !self.is_in_pose_transition()
            && self.can_camel_change_pose()
        {
            self.stand_up();
        }
        self.default_tick_move_control();
    }

    /// Vanilla parity: `Camel.CamelLookControl.tick`, which hands the head to
    /// the rider.
    fn tick_look_control(&self) {
        if self.has_controlling_passenger() {
            return;
        }
        self.default_tick_look_control();
    }

    /// Vanilla parity: `Camel.getMaxHeadYRot`.
    fn max_head_y_rot(&self) -> f32 {
        MAX_HEAD_Y_ROT
    }

    fn ambient_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_CAMEL_AMBIENT)
    }

    /// Vanilla parity: `Camel.onPlayerJump`, which refuses unless the camel is
    /// saddled, off cooldown and on the ground.
    fn check_spawn_rules(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        pos: BlockPos,
    ) -> bool {
        let _ = spawn_reason;
        Self::check_camel_spawn_rules(world, pos)
    }

    /// Vanilla parity: `Camel.finalizeSpawn`, which starts every camel fully
    /// stood up rather than mid-animation.
    fn finalize_spawn(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        group_data: Option<SpawnGroupData>,
    ) -> Option<SpawnGroupData> {
        self.reset_last_pose_change_tick_to_full_stand(world.game_time());
        self.finalize_spawn_ageable_mob(world, spawn_reason, group_data)
    }

    /// Vanilla parity: `Camel.mobInteract`.
    fn mob_interact(&self, player: &Player, hand: InteractionHand) -> InteractionResult {
        if player.is_secondary_use_active() && !AgeableMob::is_baby(self) {
            self.open_custom_inventory_screen(player);
            return InteractionResult::Success;
        }

        let held = {
            let inventory = player.inventory.lock();
            let held = inventory.get_item_in_hand(hand);
            held.copy_with_count(held.count())
        };
        if !held.is_empty() {
            let result = LivingEntity::interact_living_entity_with_equippable(self, player, hand);
            if result.consumes_action() {
                return result;
            }
        }

        if Animal::is_food(self, &held) {
            return self.fed_food(player, hand);
        }

        if self.passengers().len() < MAX_RIDERS && !AgeableMob::is_baby(self) {
            self.do_player_ride(player);
        }

        if AgeableMob::is_baby(self) && held.is(&vanilla_items::GOLDEN_DANDELION) {
            return Animal::mob_interact_animal(self, player, hand);
        }
        InteractionResult::Consume
    }
}

impl PathfinderMob for CamelEntity {}

#[cfg(test)]
mod tests;
