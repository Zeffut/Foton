//! What a camel and a camel husk share.
//!
//! Vanilla parity: `net.minecraft.world.entity.animal.camel.Camel`, which
//! `CamelHusk` extends without adding a single field -- it replaces sounds, its
//! food tag and its ability to breed, and nothing else. Duplicating the camel to
//! get that would have meant two copies of the pose clock, the dash and the
//! riding controls, so the class itself is [`CamelLike`] and the husk overrides
//! the handful of hooks vanilla's subclass overrides.
//!
//! The brain is shared too, and reaches its camel through [`CamelHooks`] for
//! the same reason [`super::super::hostile`]'s cube goals do: a behavior is
//! handed a `&dyn PathfinderMob` and has to recover the concrete type, so each
//! camel builds the hooks once with [`hooks_for`] and that call is the only
//! place the type is named.

use std::sync::Arc;

use foton_registry::blocks::block_state_ext::BlockStateExt as _;
use foton_registry::entity_type::EntityDimensions;
use foton_registry::item_stack::ItemStack;
use foton_registry::sound_event::SoundEventRef;
use foton_registry::vanilla_block_tags::BlockTag;
use foton_registry::vanilla_item_tags::ItemTag;
use foton_registry::{
    REGISTRY, TaggedRegistryExt as _, sound_events, vanilla_attributes, vanilla_game_events,
    vanilla_items,
};
use foton_utils::locks::SyncMutex;
use foton_utils::types::InteractionHand;
use foton_utils::{BlockPos, BlockStateId, Downcast as _, DowncastType};
use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;

use crate::behavior::InteractionResult;
use crate::entity::damage::DamageSource;
use crate::entity::{
    AbstractHorse, AgeableMob, Animal, EntityPose, LivingEntity, Mob, MoveResult, PathfinderMob,
};
use crate::physics::WorldCollisionProvider;
use crate::physics::collision::CollisionWorld as _;
use crate::player::Player;
use crate::world::World;
use crate::world::game_event::GameEventContext;

/// Vanilla parity: `Camel.BABY_SCALE`.
pub(super) const BABY_SCALE: f32 = 0.6;
/// Vanilla parity: `Camel.DASH_COOLDOWN_TICKS`.
pub(super) const DASH_COOLDOWN_TICKS: i32 = 55;
/// Vanilla parity: `Camel.MAX_HEAD_Y_ROT`.
pub(super) const MAX_HEAD_Y_ROT: f32 = 30.0;
/// Vanilla parity: `Camel.RUNNING_SPEED_BONUS`.
const RUNNING_SPEED_BONUS: f32 = 0.1;
/// Vanilla parity: `Camel.DASH_VERTICAL_MOMENTUM`.
const DASH_VERTICAL_MOMENTUM: f64 = 1.428_5;
/// Vanilla parity: `Camel.DASH_HORIZONTAL_MOMENTUM`.
const DASH_HORIZONTAL_MOMENTUM: f64 = 22.222_2;
/// Vanilla parity: `Camel.SITDOWN_DURATION_TICKS`.
pub(super) const SITDOWN_DURATION_TICKS: i64 = 40;
/// Vanilla parity: `Camel.STANDUP_DURATION_TICKS`.
pub(super) const STANDUP_DURATION_TICKS: i64 = 52;
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

/// Vanilla parity: the two hearts and ten seconds of growth `Camel.handleEating`
/// pays out.
const FEED_HEAL_AMOUNT: f32 = 2.0;
const FEED_AGE_UP_SECONDS: i32 = 10;

/// The one field a camel keeps that clients never see.
///
/// Vanilla parity: `Camel.dashCooldown`. The pose clock is synced and so lives
/// in the entity data instead.
pub(super) struct CamelBase {
    dash_cooldown: SyncMutex<i32>,
}

impl CamelBase {
    /// Creates a camel that is not mid-dash.
    #[must_use]
    pub(super) const fn new() -> Self {
        Self {
            dash_cooldown: SyncMutex::new(0),
        }
    }
}

/// A camel of either kind.
///
/// Vanilla parity: the `Camel` class. What a concrete camel supplies is the
/// synced-data seams and the hooks `CamelHusk` overrides; everything else is a
/// default method here.
pub(super) trait CamelLike: AbstractHorse {
    /// Returns the dash cooldown storage.
    fn camel_base(&self) -> &CamelBase;

    /// Returns the synced `Camel.DASH` flag.
    fn dash_flag(&self) -> bool;

    /// Writes the synced `Camel.DASH` flag, and nothing else.
    ///
    /// [`Self::set_dashing`] is what starts the cooldown; this is only the store.
    fn store_dash_flag(&self, dashing: bool);

    /// Returns the synced `Camel.LAST_POSE_CHANGE_TICK`.
    fn stored_last_pose_change_tick(&self) -> i64;

    /// Writes the synced `Camel.LAST_POSE_CHANGE_TICK`.
    fn store_last_pose_change_tick(&self, tick: i64);

    // The hooks `CamelHusk` overrides. Vanilla's defaults are the camel's.

    /// Vanilla parity: `Camel.getAmbientSound`.
    fn camel_ambient_sound(&self) -> SoundEventRef {
        &sound_events::ENTITY_CAMEL_AMBIENT
    }

    /// Vanilla parity: `Camel.getDeathSound`.
    fn camel_death_sound(&self) -> SoundEventRef {
        &sound_events::ENTITY_CAMEL_DEATH
    }

    /// Vanilla parity: `Camel.getHurtSound`.
    fn camel_hurt_sound(&self) -> SoundEventRef {
        &sound_events::ENTITY_CAMEL_HURT
    }

    /// Vanilla parity: the non-sand branch of `Camel.playStepSound`.
    fn camel_step_sound(&self) -> SoundEventRef {
        &sound_events::ENTITY_CAMEL_STEP
    }

    /// Vanilla parity: the sand branch of the same.
    fn camel_sand_step_sound(&self) -> SoundEventRef {
        &sound_events::ENTITY_CAMEL_STEP_SAND
    }

    /// Vanilla parity: `Camel.getDashingSound`.
    fn camel_dashing_sound(&self) -> SoundEventRef {
        &sound_events::ENTITY_CAMEL_DASH
    }

    /// Vanilla parity: `Camel.getDashReadySound`.
    fn camel_dash_ready_sound(&self) -> SoundEventRef {
        &sound_events::ENTITY_CAMEL_DASH_READY
    }

    /// Vanilla parity: `Camel.getEatingSound`.
    fn camel_eating_sound(&self) -> SoundEventRef {
        &sound_events::ENTITY_CAMEL_EAT
    }

    /// Vanilla parity: `Camel.getStandUpSound`.
    fn camel_stand_up_sound(&self) -> SoundEventRef {
        &sound_events::ENTITY_CAMEL_STAND
    }

    /// Vanilla parity: `Camel.getSitDownSound`.
    fn camel_sit_down_sound(&self) -> SoundEventRef {
        &sound_events::ENTITY_CAMEL_SIT
    }

    /// Vanilla parity: `Camel.isFood`, which the husk replaces with its own tag.
    fn is_camel_food(&self, item_stack: &ItemStack) -> bool {
        REGISTRY
            .items
            .is_in_tag(item_stack.item(), &ItemTag::CAMEL_FOOD)
    }

    // Everything below is the shared body of the class.

    /// Vanilla parity: `Camel.isDashing`.
    fn is_dashing(&self) -> bool {
        self.dash_flag()
    }

    /// Vanilla parity: `Camel.setDashing`.
    ///
    /// Vanilla's `onSyncedDataUpdated` starts the cooldown from the flag going
    /// up; Foton does it here, which is the only place the flag changes.
    fn set_dashing(&self, dashing: bool) {
        let changed = self.is_dashing() != dashing;
        self.store_dash_flag(dashing);
        if changed && dashing && self.dash_cooldown() == 0 {
            *self.camel_base().dash_cooldown.lock() = DASH_COOLDOWN_TICKS;
        }
    }

    /// Vanilla parity: `Camel.getJumpCooldown`, which is the dash cooldown.
    fn dash_cooldown(&self) -> i32 {
        *self.camel_base().dash_cooldown.lock()
    }

    /// Vanilla parity: `Camel.resetLastPoseChangeTick`.
    fn reset_last_pose_change_tick(&self, synced_pose_tick_time: i64) {
        self.store_last_pose_change_tick(synced_pose_tick_time);
    }

    /// Vanilla parity: `Camel.resetLastPoseChangeTickToFullStand`.
    fn reset_last_pose_change_tick_to_full_stand(&self, current_time: i64) {
        self.reset_last_pose_change_tick((current_time - STANDUP_DURATION_TICKS - 1).max(0));
    }

    /// Vanilla parity: `Camel.getPoseTime`.
    ///
    /// The sign of the stored tick is the pose: negative means sitting, and the
    /// absolute value is when the change started.
    fn pose_time(&self) -> i64 {
        let Some(world) = self.level() else {
            return 0;
        };
        world.game_time() - self.stored_last_pose_change_tick().abs()
    }

    /// Vanilla parity: `Camel.isCamelSitting`.
    fn is_camel_sitting(&self) -> bool {
        self.stored_last_pose_change_tick() < 0
    }

    /// Vanilla parity: `Camel.isInPoseTransition`.
    fn is_in_pose_transition(&self) -> bool {
        let duration = if self.is_camel_sitting() {
            SITDOWN_DURATION_TICKS
        } else {
            STANDUP_DURATION_TICKS
        };
        self.pose_time() < duration
    }

    /// Vanilla parity: `Camel.refuseToMove`, which is the whole reason a camel
    /// has to be stood up before it will go anywhere.
    fn refuse_to_move(&self) -> bool {
        self.is_camel_sitting() || self.is_in_pose_transition()
    }

    /// Vanilla parity: `Camel.canCamelChangePose`.
    fn can_camel_change_pose(&self) -> bool {
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
        let target_box = foton_utils::WorldAabb::entity_box(
            position.x,
            position.y,
            position.z,
            f64::from(dimensions.half_width()),
            f64::from(dimensions.height),
        );
        !WorldCollisionProvider::new(&world).has_block_collision(&target_box)
    }

    /// Vanilla parity: `Camel.sitDown`.
    fn sit_down(&self) {
        if self.is_camel_sitting() {
            return;
        }
        let Some(world) = self.level() else {
            return;
        };
        self.make_sound(Some(self.camel_sit_down_sound()));
        self.set_pose(EntityPose::Sitting);
        self.camel_game_event();
        self.reset_last_pose_change_tick(-world.game_time());
    }

    /// Vanilla parity: `Camel.standUp`.
    fn stand_up(&self) {
        if !self.is_camel_sitting() {
            return;
        }
        let Some(world) = self.level() else {
            return;
        };
        self.make_sound(Some(self.camel_stand_up_sound()));
        self.set_pose(EntityPose::Standing);
        self.camel_game_event();
        self.reset_last_pose_change_tick(world.game_time());
    }

    /// Vanilla parity: `Camel.standUpInstantly`, which is what damage and water
    /// do -- no animation, straight to standing.
    fn stand_up_instantly(&self) {
        let Some(world) = self.level() else {
            return;
        };
        self.set_pose(EntityPose::Standing);
        self.camel_game_event();
        self.reset_last_pose_change_tick_to_full_stand(world.game_time());
    }

    /// Vanilla parity: the `gameEvent(GameEvent.ENTITY_ACTION)` every pose
    /// change ends with.
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
        *self.camel_base().dash_cooldown.lock() = DASH_COOLDOWN_TICKS;
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
        *self.camel_base().dash_cooldown.lock() = cooldown - 1;
        if cooldown - 1 == 0 {
            self.play_sound(self.camel_dash_ready_sound(), 1.0, 1.0);
        }
    }

    /// Vanilla parity: `Camel.tick`.
    fn camel_tick(&self) {
        // `Entity::default_tick` is only vanilla's `Entity.baseTick`. A living
        // entity's `super.tick()` is `tick_living_entity`, and calling the
        // wrong one costs the mob its item use, its mob effects, its death
        // handling and its whole AI. Nine mobs had this exact bug.
        LivingEntity::tick_living_entity(self);
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
    fn camel_dimensions_for_pose(&self, pose: EntityPose) -> EntityDimensions {
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
    fn camel_play_step_sound(&self, block_state: BlockStateId) {
        if block_state
            .get_block()
            .has_tag(&BlockTag::CAMEL_SAND_STEP_SOUND_BLOCKS)
        {
            self.play_sound(self.camel_sand_step_sound(), 1.0, 1.0);
        } else {
            self.play_sound(self.camel_step_sound(), 1.0, 1.0);
        }
    }

    /// Vanilla parity: `Camel.canAddPassenger`, which is what makes a camel the
    /// only two-seat mount in the game.
    fn camel_can_add_passenger(&self) -> bool {
        self.passengers().len() < MAX_RIDERS
    }

    /// Vanilla parity: `Camel.handleStartJump`, which is where the dash sound
    /// and the flag come from.
    fn camel_handle_start_jump(&self) {
        self.make_sound(Some(self.camel_dashing_sound()));
        self.camel_game_event();
        self.set_dashing(true);
    }

    /// Vanilla parity: `Camel.canJump`, which refuses while it is sitting.
    fn camel_can_jump_while_ridden(&self) -> bool {
        !self.refuse_to_move() && Mob::is_saddled(self)
    }

    /// Vanilla parity: `Camel.travel`, which pins a sitting camel in place.
    fn camel_travel(&self, input: DVec3) -> Option<MoveResult> {
        if self.refuse_to_move() && self.on_ground() {
            self.set_velocity(self.velocity() * DVec3::new(0.0, 1.0, 0.0));
            return self.default_travel(input * DVec3::new(0.0, 1.0, 0.0));
        }
        self.default_travel(input)
    }

    /// Vanilla parity: `Camel.tickRidden`, whose one addition is that pushing
    /// forward on a sitting camel stands it up.
    fn camel_tick_ridden(&self, controller: &Player, ridden_input: DVec3) {
        self.tick_ridden_abstract_horse(controller, ridden_input);
        if ridden_input.z > 0.0 && self.is_camel_sitting() && !self.is_in_pose_transition() {
            self.stand_up();
        }
    }

    /// Vanilla parity: `Camel.getRiddenInput`.
    fn camel_ridden_input(&self, controller: &Player) -> DVec3 {
        if self.refuse_to_move() {
            return DVec3::ZERO;
        }
        self.abstract_horse_ridden_input(controller)
    }

    /// Vanilla parity: `Camel.getRiddenSpeed`, whose sprint bonus is only paid
    /// out while the dash is off cooldown.
    fn camel_ridden_speed(&self, controller: &Player) -> f32 {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "a movement-speed attribute, immediately used as a speed"
        )]
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

    /// Vanilla parity: `Camel.actuallyHurt`, which stands a sitting camel up
    /// before the damage lands.
    fn camel_actually_hurt(&self, world: &World, source: &DamageSource, amount: f32) {
        self.stand_up_instantly();
        self.living_actually_hurt(world, source, amount);
    }

    /// Vanilla parity: `Camel.onPlayerJump`, which refuses unless the camel is
    /// saddled, off cooldown and on the ground -- there is no dashing in mid-air.
    fn camel_on_player_jump(&self, jump_amount: i32) {
        if !Mob::is_saddled(self) || self.dash_cooldown() > 0 || !self.on_ground() {
            return;
        }
        self.abstract_horse_on_player_jump(jump_amount);
    }

    /// Vanilla parity: `Camel.handleEating`, which is a fixed two hearts and
    /// ten seconds of growth rather than the horse's per-item table.
    fn camel_handle_eating(&self, player: &Player, item_stack: &ItemStack) -> bool {
        if !Animal::is_food(self, item_stack) {
            return false;
        }

        let could_heal = self.get_health() < self.get_max_health();
        if could_heal {
            self.heal(FEED_HEAL_AMOUNT);
        }

        let could_set_in_love =
            self.is_tamed() && self.get_age() == 0 && Animal::can_fall_in_love(self);
        if could_set_in_love {
            self.set_in_love(Some(player));
        }

        let could_age_up = self.can_age_up();
        if could_age_up {
            self.age_up(FEED_AGE_UP_SECONDS, false);
        }

        if !could_heal && !could_set_in_love && !could_age_up {
            return false;
        }

        if !self.is_silent() {
            self.play_sound(
                self.camel_eating_sound(),
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

    /// Vanilla parity: `Camel.CamelMoveControl.tick`, which is what stands a
    /// wandering camel up when its brain gives it somewhere to be.
    fn camel_tick_move_control(&self) {
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
    fn camel_tick_look_control(&self) {
        if self.has_controlling_passenger() {
            return;
        }
        self.default_tick_look_control();
    }

    /// Vanilla parity: `Camel.mobInteract`.
    fn camel_mob_interact(&self, player: &Player, hand: InteractionHand) -> InteractionResult {
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

    /// Vanilla parity: the camel half of `Camel.addAdditionalSaveData`.
    fn save_camel(&self, nbt: &mut NbtCompound) {
        self.save_mob(nbt);
        self.save_ageable_mob(nbt);
        self.save_animal(nbt);
        self.save_abstract_horse(nbt);
        nbt.insert("LastPoseTick", self.stored_last_pose_change_tick());
    }

    /// Vanilla parity: the camel half of `Camel.readAdditionalSaveData`.
    fn load_camel(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_mob(nbt);
        self.load_ageable_mob(nbt);
        self.load_animal(nbt);
        self.load_abstract_horse(nbt);
        let pose_tick = nbt.long("LastPoseTick").unwrap_or(0);
        if pose_tick < 0 {
            self.set_pose(EntityPose::Sitting);
        }
        self.reset_last_pose_change_tick(pose_tick);
    }
}

/// How the shared brain reaches the camel it was handed.
///
/// A behavior receives a `&dyn PathfinderMob`, so the concrete type has to be
/// recovered by downcast. Each camel builds these once with [`hooks_for`], and
/// that call is the only place the type is named.
#[derive(Clone, Copy)]
pub(super) struct CamelHooks {
    /// Vanilla parity: `Camel.standUpInstantly`, which `CamelAi.CamelPanic`
    /// runs before it flees.
    pub stand_up_instantly: fn(&dyn PathfinderMob),
    /// Vanilla parity: `Camel.refuseToMove`.
    pub refuses_to_move: fn(&dyn PathfinderMob) -> bool,
    /// Vanilla parity: `AbstractHorse.isMobControlled`, which stops a camel a
    /// mob is steering from panicking out from under it.
    pub is_mob_controlled: fn(&dyn PathfinderMob) -> bool,
    /// Vanilla parity: the whole condition of `CamelAi.RandomSitting`.
    pub can_random_sit: fn(&dyn PathfinderMob, i64) -> bool,
    /// Vanilla parity: the body of the same behavior's `start`.
    pub random_sit: fn(&dyn PathfinderMob),
}

/// Builds the hooks for one concrete camel type.
pub(super) fn hooks_for<C>() -> CamelHooks
where
    C: CamelLike + PathfinderMob + DowncastType + 'static,
{
    CamelHooks {
        stand_up_instantly: |mob| {
            if let Some(camel) = mob.downcast_ref::<C>() {
                camel.stand_up_instantly();
            }
        },
        refuses_to_move: |mob| {
            mob.downcast_ref::<C>()
                .is_some_and(CamelLike::refuse_to_move)
        },
        is_mob_controlled: |mob| {
            mob.downcast_ref::<C>()
                .is_some_and(AbstractHorse::is_mob_controlled)
        },
        can_random_sit: |mob, minimal_pose_ticks| {
            let Some(camel) = mob.downcast_ref::<C>() else {
                return false;
            };
            !camel.is_in_water()
                && camel.pose_time() >= minimal_pose_ticks
                && !camel.is_leashed()
                && camel.on_ground()
                && !camel.has_controlling_passenger()
                && camel.can_camel_change_pose()
        },
        random_sit: |mob| {
            let Some(camel) = mob.downcast_ref::<C>() else {
                return;
            };
            if camel.is_camel_sitting() {
                camel.stand_up();
            } else if !camel.is_panicking() {
                camel.sit_down();
            }
        },
    }
}

/// Vanilla parity: `Camel.checkCamelSpawnRules`.
///
/// `Animal.isBrightEnoughToSpawn` is an associated function rather than a
/// method, so the concrete camel is named here only to reach it; the answer does
/// not depend on which camel is asking.
#[must_use]
pub(super) fn check_camel_spawn_rules<C: Animal + Sized>(
    world: &Arc<World>,
    pos: BlockPos,
) -> bool {
    use crate::world::LevelReader as _;

    world
        .get_block_state(pos.below())
        .get_block()
        .has_tag(&BlockTag::CAMELS_SPAWNABLE_ON)
        && <C as Animal>::is_bright_enough_to_spawn(world.as_ref(), pos)
}
