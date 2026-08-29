//! Happy ghast entity.
//!
//! Vanilla parity: `net.minecraft.world.entity.animal.happyghast.HappyGhast`.
//! A happy ghast is two mobs in one body. The ghastling is a brain mob that
//! drifts after whoever it has taken to; the adult is a goal mob, a four-seat
//! flying vehicle, and the second holder in the game a leashable can hang four
//! ropes off. Growing up swaps the whole control set over.
//!
//! **Gaps**: `getDismountLocationForPassenger` puts a rider who gets off on top
//! of the ghast, and Foton has no dismount-location hook to hang that on -- the
//! `DismountHelper` foundation is landed but wired to nothing, so a rider
//! leaves from where it was sitting. `getMaxSpawnClusterSize` caps a natural
//! spawn at one, which Foton's spawner has no per-entity hook for; that is the
//! same gap the ghast carries. `BabyFlyingPathNavigation.setRequiredPathLength`
//! has no equivalent, so a ghastling's paths are as long as any flier's. And
//! `setServerStillTimeout` pushes a position packet the instant the ghast is
//! told to hold still: Foton has no path from an entity to its tracker, so the
//! full position `requires_precise_position` forces arrives on the next tracker
//! pass instead of inside the same tick. `getWalkTargetValue` is written here
//! against vanilla, but nothing in Foton reads `PathfinderMob::get_walk_target_value`
//! yet -- the hook is landed and unwired for every mob that overrides it, this
//! one included.
//!
//! Two overrides are missing here because there is nothing under them yet.
//! `shouldStayCloseToLeashHolder` returns false on a happy ghast, and Foton has
//! no `PathfinderMob.closeRangeLeashBehaviour` for it to gate -- no leashed mob
//! in Foton walks back toward its holder, so the answer is already false for
//! everybody. `PathfinderMob.whenLeashedTo` sets a home at the holder, which is
//! what `checkRestriction` here is skipping around while leashed; Foton's
//! `when_leashed_to` only notifies the holder, so a leashed happy ghast keeps
//! whatever home it had.

mod happy_ghast_ai;

use std::sync::{Arc, Weak};

use foton_macros::entity_behavior;
use foton_protocol::packets::game::SoundSource;
use foton_registry::blocks::block_state_ext::BlockStateExt as _;
use foton_registry::entity_type::{EntityDimensions, EntityTypeRef};
use foton_registry::equipment::EquipmentSlot;
use foton_registry::item_stack::ItemStack;
use foton_registry::sound_event::SoundEventRef;
use foton_registry::vanilla_entity_data::HappyGhastEntityData;
use foton_registry::vanilla_item_tags::ItemTag;
use foton_registry::{REGISTRY, TaggedRegistryExt as _, sound_events, vanilla_attributes};
use foton_utils::locks::SyncMutex;
use foton_utils::types::InteractionHand;
use foton_utils::{
    BlockPos, BlockStateId, Downcast as _, DowncastType, DowncastTypeKey, WorldAabb,
};
use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;

use crate::behavior::InteractionResult;
use crate::entity::ai::brain::Brain;
use crate::entity::ai::control::GhastMoveControl;
use crate::entity::ai::goal::{
    FloatGoal, Goal, GoalControls, RandomFloatAroundGoal, TemptGoal, TemptNavigation,
    face_movement_direction,
};
use crate::entity::damage::DamageSource;
use crate::entity::mob::{MoveControlKind, NavigationKind, wrap_degrees, wrap_degrees_90};
use crate::entity::{
    AgeableMob, AgeableMobBase, Animal, AnimalBase, Entity, EntityBase, EntityBaseLoad, EntityPose,
    EntitySpawnReason, EntitySyncedData, LivingEntity, LivingEntityBase, LivingEntitySyncedData,
    Mob, MobBase, MoveResult, PathfinderMob, SharedEntity,
};
use crate::player::Player;
use crate::world::{LevelReader as _, Precipitation, World};

/// Vanilla parity: `HappyGhast.BABY_SCALE`.
const BABY_SCALE: f32 = 0.2375;
/// Vanilla parity: the `withEyeHeight(0.46875F)` of `HappyGhast.BABY_DIMENSIONS`.
const BABY_EYE_HEIGHT: f32 = 0.468_75;
/// Vanilla parity: `HappyGhast.WANDER_GROUND_DISTANCE`, the drift's preference
/// for staying within sight of something solid.
const WANDER_GROUND_DISTANCE: i32 = 16;
/// Vanilla parity: `HappyGhast.SMALL_RESTRICTION_RADIUS`.
const SMALL_RESTRICTION_RADIUS: i32 = 32;
/// Vanilla parity: `HappyGhast.LARGE_RESTRICTION_RADIUS`.
const LARGE_RESTRICTION_RADIUS: i32 = 64;
/// Vanilla parity: `HappyGhast.RESTRICTION_RADIUS_BUFFER`.
const RESTRICTION_RADIUS_BUFFER: i32 = 16;
/// Vanilla parity: `HappyGhast.FAST_HEALING_TICKS`, the rate in cloud or rain.
const FAST_HEALING_TICKS: i32 = 20;
/// Vanilla parity: `HappyGhast.SLOW_HEALING_TICKS`.
const SLOW_HEALING_TICKS: i32 = 600;
/// How many riders a happy ghast carries.
///
/// Vanilla parity: the four-seat cap of `HappyGhast.canAddPassenger`.
const MAX_PASSENGERS: usize = 4;
/// Vanilla parity: `HappyGhast.STILL_TIMEOUT_ON_LOAD_GRACE_PERIOD`, which is
/// why a ghast loaded holding still keeps holding still for three seconds.
const STILL_TIMEOUT_ON_LOAD_GRACE_PERIOD: i32 = 60;
/// Vanilla parity: `HappyGhast.MAX_STILL_TIMEOUT`.
const MAX_STILL_TIMEOUT: i32 = 10;
/// Vanilla parity: `HappyGhast.MAX_SCALE`.
const MAX_SCALE: f32 = 1.0;
/// Vanilla parity: the `this.leashHolderTime = 5` of `notifyLeashHolder`.
const LEASH_HOLDER_TIME: i32 = 5;
/// Vanilla parity: `HappyGhast.leashElasticDistance`.
const LEASH_ELASTIC_DISTANCE: f64 = 10.0;
/// Vanilla parity: `HappyGhast.leashSnapDistance`.
const LEASH_SNAP_DISTANCE: f64 = 16.0;
/// Vanilla parity: the `* 5.0F / 3.0F` of `HappyGhast.travel`.
const TRAVEL_SPEED_SCALE: f32 = 5.0 / 3.0;
/// Vanilla parity: the `scale(3.9F * FLYING_SPEED)` of `getRiddenInput`.
const RIDDEN_INPUT_SCALE: f64 = 3.9;
/// Vanilla parity: the `up += 0.5F` a rider's jump adds.
const RIDDEN_JUMP_CLIMB: f32 = 0.5;
/// Vanilla parity: the `*= -0.5F` a rider's reverse scales the look vector by.
const RIDDEN_REVERSE_SCALE: f32 = -0.5;
/// Vanilla parity: the `turnSpeed = 0.08F` of `tickRidden`, which is what makes
/// a ridden happy ghast swing round to the rider's heading rather than snap.
const RIDDEN_TURN_SPEED: f32 = 0.08;
/// Vanilla parity: the `controller.getXRot() * 0.5F` of `getRiddenRotation`.
const RIDDEN_PITCH_SCALE: f32 = 0.5;
/// Vanilla parity: the `interval * 6` of `getAmbientSoundInterval`.
const RIDDEN_AMBIENT_SOUND_FACTOR: i32 = 6;
/// Vanilla parity: `HappyGhast.getSoundVolume`.
const ADULT_SOUND_VOLUME: f32 = 4.0;
const BABY_SOUND_VOLUME: f32 = 1.0;
/// Vanilla parity: `HappyGhast.getVoicePitch`, which is flat -- a happy ghast
/// does not get the random pitch every other mob does.
const VOICE_PITCH: f32 = 1.0;
/// Vanilla parity: the `1.0` the player-above scan widens its box by.
const PLAYER_SCAN_MARGIN: f64 = 1.0;
/// Vanilla parity: the `1.0E-5F` the scan box starts below the ghast's top.
const PLAYER_SCAN_TOP_EPSILON: f64 = 1.0e-5;
/// Vanilla parity: the `1.0` speed modifier of the adult's tempt goal.
const TEMPT_SPEED_MODIFIER: f64 = 1.0;
/// Vanilla parity: the `7.0` stop distance of the adult's tempt goal.
const TEMPT_STOP_DISTANCE: f64 = 7.0;
/// Vanilla parity: the `10.0F` of `getWalkTargetValue`, which is what makes a
/// happy ghast prefer to hang one block above a drop.
const WALK_TARGET_VALUE_OVERHANG: f32 = 10.0;
/// Vanilla parity: the `5.0F` of `getWalkTargetValue`.
const WALK_TARGET_VALUE_OPEN: f32 = 5.0;
/// Vanilla parity: the `180` of the ghastling's `FlyingMoveControl`.
const BABY_MOVE_CONTROL_MAX_TURN: f32 = 180.0;

/// Returns whether two block positions are within `distance` of each other.
///
/// Vanilla parity: `Vec3i.closerThan(Vec3i, double)`.
fn block_pos_closer_than(from: BlockPos, to: BlockPos, distance: i32) -> bool {
    let dx = f64::from(from.x() - to.x());
    let dy = f64::from(from.y() - to.y());
    let dz = f64::from(from.z() - to.z());
    let limit = f64::from(distance);
    dx.mul_add(dx, dy.mul_add(dy, dz * dz)) < limit * limit
}

/// A happy ghast.
#[entity_behavior(class = "HappyGhast")]
pub struct HappyGhastEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    ageable_base: AgeableMobBase,
    animal_base: AnimalBase,
    brain: Brain,
    entity_data: SyncMutex<HappyGhastEntityData>,
    /// Ticks left on the "four ropes are on me" flag the client draws from.
    ///
    /// Vanilla parity: `HappyGhast.leashHolderTime`.
    leash_holder_time: SyncMutex<i32>,
    /// Ticks left of the hold-still the server is keeping it in.
    ///
    /// Vanilla parity: `HappyGhast.serverStillTimeout`.
    server_still_timeout: SyncMutex<i32>,
    /// Ticks left before the move control's next shove.
    ///
    /// Vanilla parity: `GhastMoveControl.floatDuration`, which vanilla keeps on
    /// the control object. Foton recreates its controls each tick, so the state
    /// they carry lives on the mob.
    float_duration: SyncMutex<i32>,
}

// SAFETY: This key is owned by Foton and uniquely identifies `HappyGhastEntity`.
unsafe impl DowncastType for HappyGhastEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:entity/happy_ghast");
}

impl HappyGhastEntity {
    /// Creates a happy ghast at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Creates a happy ghast from saved base data.
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
        let mut entity_data = HappyGhastEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);

        let ghast = Self {
            base,
            entity_type,
            living_base,
            mob_base,
            ageable_base,
            animal_base,
            brain: happy_ghast_ai::make_brain(),
            entity_data: SyncMutex::new(entity_data),
            leash_holder_time: SyncMutex::new(0),
            server_still_timeout: SyncMutex::new(0),
            float_duration: SyncMutex::new(0),
        };
        // Vanilla parity: `Mob`'s constructor calls `registerGoals`, and a happy
        // ghast is born adult. `age_boundary_changed` takes the set away again
        // the moment a saved age says otherwise.
        ghast.register_goals();
        ghast
    }

    /// Vanilla parity: `HappyGhast.registerGoals`.
    fn register_goals(&self) {
        let mut goals = self.mob_base.goal_selector().lock();
        goals.add_goal(3, HappyGhastFloatGoal::new(&self.mob_base));
        goals.add_goal(
            4,
            TemptGoal::mob_aware(
                TEMPT_SPEED_MODIFIER,
                |mob, item_stack| {
                    let tag =
                        mob.downcast_ref::<Self>()
                            .map_or(ItemTag::HAPPY_GHAST_FOOD, |ghast| {
                                if ghast.is_wearing_body_armor() || AgeableMob::is_baby(ghast) {
                                    ItemTag::HAPPY_GHAST_FOOD
                                } else {
                                    ItemTag::HAPPY_GHAST_TEMPT_ITEMS
                                }
                            });
                    REGISTRY.items.is_in_tag(item_stack.item(), &tag)
                },
                false,
                TEMPT_STOP_DISTANCE,
            )
            .with_navigation(HappyGhastTemptNavigation, GoalControls::MOVE),
        );
        goals.add_goal(
            5,
            RandomFloatAroundGoal::with_distance_to_blocks(WANDER_GROUND_DISTANCE),
        );
    }

    /// Vanilla parity: `HappyGhast.adultGhastSetup`.
    fn adult_ghast_setup(&self) {
        self.mob_base.goal_selector().lock().remove_all_goals(self);
        self.register_goals();
        if let Some(world) = self.level() {
            self.brain.stop_all(&world, self);
        }
        self.brain.clear_memories();
    }

    /// Vanilla parity: `HappyGhast.babyGhastSetup`.
    fn baby_ghast_setup(&self) {
        self.set_server_still_timeout(0);
        self.mob_base.goal_selector().lock().remove_all_goals(self);
    }

    /// Vanilla parity: `HappyGhast.BABY_DIMENSIONS`.
    fn baby_dimensions(entity_type: EntityTypeRef) -> EntityDimensions {
        EntityDimensions {
            eye_height: BABY_EYE_HEIGHT,
            ..entity_type.dimensions.scale(BABY_SCALE)
        }
    }

    /// Vanilla parity: `HappyGhast.setServerStillTimeout`.
    ///
    /// Vanilla also resets the position codec and pushes a position packet the
    /// moment the timeout starts. Foton has no path from an entity to its
    /// tracker, so `requires_precise_position` carries the same intent to the
    /// next tracker pass instead; see this module's gap note.
    fn set_server_still_timeout(&self, server_still_timeout: i32) {
        *self.server_still_timeout.lock() = server_still_timeout;
        self.sync_stay_still_flag();
    }

    /// Vanilla parity: `HappyGhast.syncStayStillFlag`.
    fn sync_stay_still_flag(&self) {
        let stays_still = *self.server_still_timeout.lock() > 0;
        self.entity_data.lock().stays_still.set(stays_still);
    }

    /// Vanilla parity: `HappyGhast.staysStill`.
    #[must_use]
    pub fn stays_still(&self) -> bool {
        *self.entity_data.lock().stays_still.get()
    }

    /// Returns whether this ghast is holding position for its riders.
    ///
    /// Vanilla parity: `HappyGhast.isOnStillTimeout`.
    #[must_use]
    pub fn is_on_still_timeout(&self) -> bool {
        self.stays_still() || *self.server_still_timeout.lock() > 0
    }

    /// Vanilla parity: `HappyGhast.isLeashHolder`, the flag that tells the
    /// client to draw four ropes rather than none.
    #[must_use]
    pub fn is_leash_holder(&self) -> bool {
        *self.entity_data.lock().is_leash_holder.get()
    }

    /// Vanilla parity: `HappyGhast.setLeashHolder`.
    fn set_leash_holder(&self, is_leash_holder: bool) {
        self.entity_data.lock().is_leash_holder.set(is_leash_holder);
    }

    /// Vanilla parity: `HappyGhast.getHappyGhastRestrictionRadius`.
    fn restriction_radius(&self) -> i32 {
        if !AgeableMob::is_baby(self) && !self.is_wearing_body_armor() {
            LARGE_RESTRICTION_RADIUS
        } else {
            SMALL_RESTRICTION_RADIUS
        }
    }

    /// Vanilla parity: `HappyGhast.checkRestriction`, which is what keeps a
    /// loose happy ghast circling where it was found.
    fn check_restriction(&self) {
        if self.is_leashed() || self.is_vehicle() {
            return;
        }

        let radius = self.restriction_radius();
        let position = self.block_position();
        let home_is_close = self.has_home()
            && block_pos_closer_than(
                self.home_position(),
                position,
                radius + RESTRICTION_RADIUS_BUFFER,
            );
        if !home_is_close || radius != self.home_radius() {
            self.set_home_to(position, radius);
        }
    }

    /// Vanilla parity: `HappyGhast.continuousHeal`, the slow mend that is
    /// twenty times faster inside a cloud or under rain.
    fn continuous_heal(&self) {
        let Some(world) = self.level() else {
            return;
        };
        if !Entity::is_alive(self)
            || self.living_base().death_time() != 0
            || (self.get_max_health() - self.get_health()).abs() < f32::EPSILON
        {
            return;
        }

        let fast = self.is_in_clouds()
            || world.precipitation_at(self.block_position()) != Precipitation::None;
        let interval = if fast {
            FAST_HEALING_TICKS
        } else {
            SLOW_HEALING_TICKS
        };
        if self.tick_count() % interval == 0 {
            self.heal(1.0);
        }
    }

    /// Vanilla parity: `HappyGhast.scanPlayerAboveGhast`, which is what makes a
    /// happy ghast freeze the moment somebody steps onto its back.
    fn scan_player_above_ghast(&self) -> bool {
        let Some(world) = self.level() else {
            return false;
        };
        let bounding_box = self.bounding_box();
        let detection_box = WorldAabb::new(
            bounding_box.min_x() - PLAYER_SCAN_MARGIN,
            bounding_box.max_y() - PLAYER_SCAN_TOP_EPSILON,
            bounding_box.min_z() - PLAYER_SCAN_MARGIN,
            bounding_box.max_x() + PLAYER_SCAN_MARGIN,
            bounding_box.max_y() + bounding_box.height() / 2.0,
            bounding_box.max_z() + PLAYER_SCAN_MARGIN,
        );

        let mut found = false;
        world.players.iter_players(|_, player| {
            if player.is_spectator() {
                return true;
            }
            let (root_position, root_is_happy_ghast) = player.root_vehicle().map_or_else(
                || (player.position(), false),
                |root| (root.position(), root.downcast_ref::<Self>().is_some()),
            );
            if !root_is_happy_ghast && detection_box.contains(root_position) {
                found = true;
                return false;
            }
            true
        });
        found
    }

    /// Vanilla parity: `HappyGhast.doPlayerRide`.
    fn do_player_ride(&self, player: &Player) {
        let Some(world) = self.level() else {
            return;
        };
        let Some(vehicle) = world.get_entity_by_id(self.id()) else {
            return;
        };
        player.start_riding(&vehicle);
    }

    /// Vanilla parity: `HappyGhast.getRiddenRotation`, which halves the rider's
    /// pitch so a ghast never stands on its nose.
    fn ridden_rotation(controller: &Player) -> (f32, f32) {
        let (yaw, pitch) = controller.rotation();
        (yaw, pitch * RIDDEN_PITCH_SCALE)
    }

    /// Vanilla parity: the `travelFlying` speed of `HappyGhast.travel`.
    fn travel_speed(&self) -> f32 {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "vanilla casts the same attribute to a float"
        )]
        let flying_speed = self
            .attributes()
            .lock()
            .required_value(vanilla_attributes::FLYING_SPEED) as f32;
        flying_speed * TRAVEL_SPEED_SCALE
    }
}

/// Bobs to the surface unless the ghast has been told to hold still.
///
/// Vanilla parity: `HappyGhast.HappyGhastFloatGoal`.
struct HappyGhastFloatGoal(FloatGoal);

impl HappyGhastFloatGoal {
    fn new(mob_base: &MobBase) -> Self {
        Self(FloatGoal::new(mob_base))
    }
}

impl Goal for HappyGhastFloatGoal {
    fn controls(&self) -> GoalControls {
        self.0.controls()
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        let held_still = mob
            .downcast_ref::<HappyGhastEntity>()
            .is_some_and(HappyGhastEntity::is_on_still_timeout);
        !held_still && self.0.can_use(mob)
    }

    fn requires_update_every_tick(&self) -> bool {
        self.0.requires_update_every_tick()
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        self.0.tick(mob);
    }
}

/// Steers a tempted happy ghast, which has a move control and no path.
///
/// Vanilla parity: `TemptGoal.ForNonPathfinders`.
struct HappyGhastTemptNavigation;

impl TemptNavigation for HappyGhastTemptNavigation {
    fn stop_navigation(&self, mob: &dyn PathfinderMob) {
        mob.mob_base().controls().lock().move_control.set_wait();
    }

    fn navigate_towards(&self, mob: &dyn PathfinderMob, player: &Arc<Player>, speed_modifier: f64) {
        let position = mob.position();
        let player_position = player.position();
        let player_eyes = DVec3::new(player_position.x, player.get_eye_y(), player_position.z);
        let toward_eyes = player_eyes - position;
        let target = toward_eyes * rand::random::<f64>() + position;
        mob.set_wanted_position(target, speed_modifier);
    }
}

impl Entity for HappyGhastEntity {
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
        SoundSource::Neutral
    }

    /// Vanilla parity: `HappyGhast.supportQuadLeashAsHolder`. This is what the
    /// whole mob is for: four leads, four corners, one holder.
    fn support_quad_leash_as_holder(&self) -> bool {
        true
    }

    /// Vanilla parity: `HappyGhast.notifyLeashHolder`, which lights the flag
    /// the client draws four ropes from and lets it go out five ticks later.
    fn notify_leash_holder(&self, leashable: &dyn Entity) {
        if leashable.as_mob().is_some_and(Mob::support_quad_leash) {
            *self.leash_holder_time.lock() = LEASH_HOLDER_TIME;
        }
    }

    /// Vanilla parity: `HappyGhast.getDefaultDimensions`.
    fn dimensions_for_pose(&self, _pose: EntityPose) -> EntityDimensions {
        if AgeableMob::is_baby(self) {
            return Self::baby_dimensions(self.entity_type);
        }
        if self.entity_type.fixed {
            self.entity_type.dimensions
        } else {
            self.entity_type
                .dimensions
                .scale(LivingEntity::get_scale(self))
        }
    }

    /// Vanilla parity: `HappyGhast.checkFallDamage` is empty.
    fn check_fall_damage(
        &self,
        _vertical_movement: f64,
        _on_ground: bool,
        _on_state: BlockStateId,
        _pos: BlockPos,
        _world: &Arc<World>,
    ) {
    }

    /// Vanilla parity: `HappyGhast.onClimbable`.
    fn on_climbable(&self) -> bool {
        false
    }

    /// Vanilla parity: `HappyGhast.playStepSound` is empty.
    fn play_step_sound(&self, _pos: BlockPos, _block_state: BlockStateId) {}

    /// Vanilla parity: `HappyGhast.canAddPassenger`.
    fn can_add_passenger(&self, _passenger: &dyn Entity) -> bool {
        self.passengers().len() < MAX_PASSENGERS
    }

    /// Vanilla parity: `HappyGhast.getControllingPassenger`.
    fn controlling_passenger(&self) -> Option<SharedEntity> {
        if self.is_wearing_body_armor()
            && !self.is_on_still_timeout()
            && let Some(first) = self.first_passenger()
            && first.as_player().is_some()
        {
            return Some(first);
        }
        self.controlling_passenger_mob()
    }

    /// Vanilla parity: `HappyGhast.addPassenger`.
    fn on_passenger_added(&self, _passenger: &dyn Entity) {
        if self.passengers().len() == 1 {
            self.play_sound(
                &sound_events::ENTITY_HAPPY_GHAST_HARNESS_GOGGLES_DOWN,
                1.0,
                1.0,
            );
        }

        if self.scan_player_above_ghast() {
            if *self.server_still_timeout.lock() > MAX_STILL_TIMEOUT {
                self.set_server_still_timeout(MAX_STILL_TIMEOUT);
            }
        } else {
            self.set_server_still_timeout(0);
        }
    }

    /// Vanilla parity: `HappyGhast.removePassenger`.
    fn on_passenger_removed(&self, _passenger: &dyn Entity) {
        self.set_server_still_timeout(MAX_STILL_TIMEOUT);
        if !self.is_vehicle() {
            self.clear_home();
            self.play_sound(
                &sound_events::ENTITY_HAPPY_GHAST_HARNESS_GOGGLES_UP,
                1.0,
                1.0,
            );
        }
    }

    /// Vanilla parity: `HappyGhast.canBeCollidedWith`. An adult holding still
    /// is solid enough to stand on, which is how a player boards one.
    fn can_be_collided_with(&self, other: Option<&dyn Entity>) -> bool {
        if AgeableMob::is_baby(self) || !Entity::is_alive(self) {
            return false;
        }
        if self.is_vehicle() && other.is_some_and(|other| other.downcast_ref::<Self>().is_some()) {
            return true;
        }
        self.is_on_still_timeout()
    }

    /// Vanilla parity: `HappyGhast.isFlyingVehicle`, which is what stops the
    /// server kicking a rider for floating.
    fn is_flying_vehicle(&self) -> bool {
        !AgeableMob::is_baby(self)
    }

    /// Vanilla parity: `HappyGhast.tick`.
    fn tick(&self) {
        LivingEntity::tick_living_entity(self);

        {
            let mut leash_holder_time = self.leash_holder_time.lock();
            if *leash_holder_time > 0 {
                *leash_holder_time -= 1;
            }
        }
        self.set_leash_holder(*self.leash_holder_time.lock() > 0);

        let still_timeout = *self.server_still_timeout.lock();
        if still_timeout > 0 {
            let ticked = if self.tick_count() > STILL_TIMEOUT_ON_LOAD_GRACE_PERIOD {
                still_timeout - 1
            } else {
                still_timeout
            };
            self.set_server_still_timeout(ticked);
        }

        if self.scan_player_above_ghast() {
            self.set_server_still_timeout(MAX_STILL_TIMEOUT);
        }
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_mob(nbt);
        self.save_ageable_mob(nbt);
        self.save_animal(nbt);
        self.brain.save(nbt);
        nbt.insert("still_timeout", *self.server_still_timeout.lock());
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_mob(nbt);
        self.load_ageable_mob(nbt);
        self.load_animal(nbt);
        self.brain.load(nbt);
        self.set_server_still_timeout(nbt.int("still_timeout").unwrap_or(0));
    }
}

impl LivingEntity for HappyGhastEntity {
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

    /// Vanilla parity: `HappyGhast.sanitizeScale`, which is why no command can
    /// grow a happy ghast past the harness its riders sit on.
    fn sanitize_scale(&self, scale: f32) -> f32 {
        scale.min(MAX_SCALE)
    }

    /// Vanilla parity: `HappyGhast.getAgeScale`.
    fn get_age_scale(&self) -> f32 {
        if AgeableMob::is_baby(self) {
            BABY_SCALE
        } else {
            1.0
        }
    }

    /// Vanilla parity: `HappyGhast.canBreatheUnderwater`, which only a ghastling
    /// can.
    fn can_breathe_underwater(&self) -> bool {
        AgeableMob::is_baby(self) || self.entity_type().flags.can_breathe_underwater
    }

    fn hurt_sound(&self, _source: &DamageSource) -> Option<SoundEventRef> {
        Some(if AgeableMob::is_baby(self) {
            &sound_events::ENTITY_GHASTLING_HURT
        } else {
            &sound_events::ENTITY_HAPPY_GHAST_HURT
        })
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(if AgeableMob::is_baby(self) {
            &sound_events::ENTITY_GHASTLING_DEATH
        } else {
            &sound_events::ENTITY_HAPPY_GHAST_DEATH
        })
    }

    /// Vanilla parity: `HappyGhast.getSoundVolume`.
    fn sound_volume(&self) -> f32 {
        if AgeableMob::is_baby(self) {
            BABY_SOUND_VOLUME
        } else {
            ADULT_SOUND_VOLUME
        }
    }

    /// Vanilla parity: `HappyGhast.getVoicePitch`.
    fn voice_pitch(&self) -> f32 {
        VOICE_PITCH
    }

    /// Vanilla parity: `HappyGhast.canUseSlot`, which opens the harness slot on
    /// a living adult only.
    fn can_use_slot(&self, slot: EquipmentSlot) -> bool {
        slot != EquipmentSlot::Body || (Entity::is_alive(self) && !AgeableMob::is_baby(self))
    }

    /// Vanilla parity: `HappyGhast.canDispenserEquipIntoSlot`.
    fn can_dispenser_equip_into_slot(&self, slot: EquipmentSlot) -> bool {
        slot == EquipmentSlot::Body
    }

    fn server_ai_step(&self) {
        Mob::mob_server_ai_step(self);
    }

    /// Vanilla parity: `HappyGhast.aiStep`.
    fn ai_step(&self) -> Option<MoveResult> {
        self.set_requires_precise_position(self.is_on_still_timeout());
        let result = self.default_ai_step();
        AgeableMob::tick_ageable_mob(self);
        Animal::tick_animal_love(self);
        self.continuous_heal();
        result
    }

    /// Vanilla parity: `HappyGhast.travel`, the only `travelFlying` call in the
    /// game that pushes as hard through water and lava as it does through air.
    fn travel(&self, input: DVec3) -> Option<MoveResult> {
        let speed = self.travel_speed();
        self.travel_flying_in_fluids(input, speed, speed, speed)
    }

    /// Vanilla parity: `HappyGhast.getRiddenInput`, which is the whole of
    /// steering one: the rider's pitch decides how much of the push is climb.
    fn ridden_input(&self, controller: &Player, _self_input: DVec3) -> DVec3 {
        let input = controller.travel_input();
        let strafe = input.sideways();
        let mut forward = 0.0;
        let mut up = 0.0;
        if input.forward() != 0.0 {
            let pitch_radians = controller.rotation().1.to_radians();
            let mut forward_look = pitch_radians.cos();
            let mut up_look = -pitch_radians.sin();
            if input.forward() < 0.0 {
                forward_look *= RIDDEN_REVERSE_SCALE;
                up_look *= RIDDEN_REVERSE_SCALE;
            }
            up = up_look;
            forward = forward_look;
        }
        if controller.is_jumping() {
            up += RIDDEN_JUMP_CLIMB;
        }

        let flying_speed = self
            .attributes()
            .lock()
            .required_value(vanilla_attributes::FLYING_SPEED);
        DVec3::new(f64::from(strafe), f64::from(up), f64::from(forward))
            * (RIDDEN_INPUT_SCALE * flying_speed)
    }

    /// Vanilla parity: `HappyGhast.tickRidden`.
    fn tick_ridden(&self, controller: &Player, _ridden_input: DVec3) {
        let (wanted_yaw, wanted_pitch) = Self::ridden_rotation(controller);
        let yaw = self.rotation().0;
        let difference = wrap_degrees(wanted_yaw - yaw);
        let yaw = difference.mul_add(RIDDEN_TURN_SPEED, yaw);
        self.set_rotation((yaw, wanted_pitch));
        self.base.set_old_yaw_to_current();
        self.set_y_body_rot(yaw);
        self.set_y_head_rot(yaw);
    }
}

impl AgeableMob for HappyGhastEntity {
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

    /// Vanilla parity: `HappyGhast.ageBoundaryReached`, which is where the two
    /// halves of this mob change places.
    fn age_boundary_changed(&self, baby: bool) {
        if baby {
            self.baby_ghast_setup();
        } else {
            self.adult_ghast_setup();
        }
        self.refresh_dimensions();
    }
}

impl Animal for HappyGhastEntity {
    fn animal_base(&self) -> &AnimalBase {
        &self.animal_base
    }

    fn is_food(&self, item_stack: &ItemStack) -> bool {
        REGISTRY
            .items
            .is_in_tag(item_stack.item(), &ItemTag::HAPPY_GHAST_FOOD)
    }

    /// Vanilla parity: `HappyGhast.canFallInLove`. Happy ghasts do not breed;
    /// the next one comes out of a dried ghast block.
    fn can_fall_in_love(&self) -> bool {
        false
    }
}

impl Mob for HappyGhastEntity {
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

    fn tick_goal_selectors(&self) {
        PathfinderMob::tick_pathfinder_goal_selectors(self);
    }

    fn tick_path_navigation(&self) {
        PathfinderMob::tick_pathfinder_path_navigation(self);
    }

    /// Vanilla parity: the `GhastMoveControl<>(this, true, this::isOnStillTimeout)`
    /// an adult installs and the `FlyingMoveControl(this, 180, true)` a
    /// ghastling installs in its place.
    fn tick_move_control(&self) {
        if AgeableMob::is_baby(self) {
            self.default_tick_move_control();
            return;
        }

        let float_duration = *self.float_duration.lock();
        *self.float_duration.lock() = GhastMoveControl::new(float_duration)
            .careful()
            .stopped(self.is_on_still_timeout())
            .tick(self);
    }

    /// Vanilla parity: the `FlyingMoveControl(this, 180, true)` of
    /// `babyGhastSetup`; an adult drives its own control above.
    fn move_control_kind(&self) -> MoveControlKind {
        MoveControlKind::Flying {
            max_turn: BABY_MOVE_CONTROL_MAX_TURN,
            hovers_in_place: true,
        }
    }

    /// Vanilla parity: `HappyGhast.HappyGhastLookControl`.
    fn tick_look_control(&self) {
        if AgeableMob::is_baby(self) {
            self.default_tick_look_control();
            return;
        }

        if self.is_on_still_timeout() {
            // Vanilla parity: the `wrapDegrees90` snap, which is what squares a
            // held ghast up with the world so its riders face an axis.
            let yaw = self.rotation().0;
            let squared = yaw - wrap_degrees_90(yaw);
            self.set_rotation((squared, self.rotation().1));
            self.set_y_head_rot(squared);
            return;
        }

        let looking = {
            let mut controls = self.mob_base.controls().lock();
            controls
                .look_control
                .tick_cooldown()
                .then(|| controls.look_control.wanted_position())
        };
        let Some(wanted_position) = looking else {
            face_movement_direction(self);
            return;
        };

        let position = self.position();
        let yaw = -(wanted_position.x - position.x)
            .atan2(wanted_position.z - position.z)
            .to_degrees();
        #[expect(
            clippy::cast_possible_truncation,
            reason = "vanilla computes the same angle as a float"
        )]
        let yaw = yaw as f32;
        self.set_rotation((yaw, self.rotation().1));
        self.set_y_body_rot(yaw);
        self.set_y_head_rot(yaw);
    }

    /// Vanilla parity: `HappyGhast.HappyGhastBodyRotationControl`, which locks
    /// the head to the body while anybody is aboard.
    fn tick_body_rotation_control(&self) {
        if self.is_vehicle() {
            let yaw = self.rotation().0;
            self.set_y_head_rot(yaw);
            self.set_y_body_rot(yaw);
        }
        self.default_tick_body_rotation_control();
    }

    /// Vanilla parity: `HappyGhast.customServerAiStep`, whose brain half only
    /// runs while the ghast is a ghastling.
    fn custom_server_ai_step(&self) {
        if AgeableMob::is_baby(self)
            && let Some(world) = self.level()
        {
            self.brain.tick(&world, self);
            happy_ghast_ai::update_activity(&self.brain);
        }

        self.check_restriction();
        Animal::custom_server_ai_step_animal(self);
    }

    fn ambient_sound(&self) -> Option<SoundEventRef> {
        Some(if AgeableMob::is_baby(self) {
            &sound_events::ENTITY_GHASTLING_AMBIENT
        } else {
            &sound_events::ENTITY_HAPPY_GHAST_AMBIENT
        })
    }

    /// Vanilla parity: `HappyGhast.getAmbientSoundInterval`, which is six times
    /// as long once somebody is riding -- a ridden happy ghast is quieter.
    fn ambient_sound_interval(&self) -> i32 {
        let interval = Animal::ambient_sound_interval_animal(self);
        if self.is_vehicle() {
            interval * RIDDEN_AMBIENT_SOUND_FACTOR
        } else {
            interval
        }
    }

    /// Vanilla parity: `HappyGhast.leashElasticDistance`.
    fn leash_elastic_distance(&self) -> f64 {
        LEASH_ELASTIC_DISTANCE
    }

    /// Vanilla parity: `HappyGhast.leashSnapDistance`.
    fn leash_snap_distance(&self) -> f64 {
        LEASH_SNAP_DISTANCE
    }

    /// Vanilla parity: `HappyGhast.onElasticLeashPull`, which drops whatever the
    /// move control was aiming at so the pull is not fought.
    fn on_elastic_leash_pull(&self) {
        self.check_fall_distance_accumulation();
        self.mob_base.controls().lock().move_control.set_wait();
    }

    /// Vanilla parity: `HappyGhast.mobInteract`.
    ///
    /// Vanilla runs `ItemStack.interactLivingEntity` here before deciding to
    /// mount. Foton's interaction dispatch already runs the equippable branch
    /// after `mob_interact` passes, so a harness in hand still goes on the ghast
    /// rather than putting the player on it.
    fn mob_interact(&self, player: &Player, hand: InteractionHand) -> InteractionResult {
        if AgeableMob::is_baby(self) {
            return Animal::mob_interact_animal(self, player, hand);
        }

        if self.is_wearing_body_armor() && !player.is_secondary_use_active() {
            self.do_player_ride(player);
            return InteractionResult::SuccessServer;
        }

        Animal::mob_interact_animal(self, player, hand)
    }

    fn check_spawn_rules(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        pos: BlockPos,
    ) -> bool {
        <Self as Animal>::check_animal_spawn_rules(world.as_ref(), spawn_reason, pos)
    }
}

impl PathfinderMob for HappyGhastEntity {
    /// Vanilla parity: `HappyGhast.createNavigation` and the ghastling's
    /// `BabyFlyingPathNavigation`; both fly.
    fn navigation_kind(&self) -> NavigationKind {
        NavigationKind::Flying
    }

    /// Vanilla parity: `HappyGhast.getWalkTargetValue`, which is what keeps a
    /// drifting happy ghast hanging over an edge rather than over open ground.
    fn get_walk_target_value(&self, pos: BlockPos) -> f32 {
        let Some(world) = self.level() else {
            return 0.0;
        };
        if !world.get_block_state(pos).is_air() {
            return 0.0;
        }
        let one_below = world.get_block_state(pos.below()).is_air();
        let two_below = world.get_block_state(pos.below().below()).is_air();
        if one_below && !two_below {
            WALK_TARGET_VALUE_OVERHANG
        } else {
            WALK_TARGET_VALUE_OPEN
        }
    }
}

#[cfg(test)]
mod tests;
