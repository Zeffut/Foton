//! Cod entity.
//!
//! Vanilla parity: `Cod`, `AbstractFish` and `WaterAnimal`. The first mob in
//! Steel that swims: it navigates in three dimensions through water, drowns in
//! air the way a land mob drowns in water, and flops when it lands on a bank.

use std::sync::Weak;

use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;
use steel_macros::entity_behavior;
use steel_protocol::packets::game::SoundSource;
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::sound_event::SoundEventRef;
use steel_registry::vanilla_entity_data::CodEntityData;
use steel_registry::{sound_events, vanilla_attributes, vanilla_damage_types};
use steel_utils::locks::SyncMutex;
use steel_utils::{BlockPos, BlockStateId, DowncastType, DowncastTypeKey};

use crate::entity::ai::control::MoveControlOperation;
use crate::entity::ai::goal::{PanicGoal, RandomSwimmingGoal};
use crate::entity::ai::path::PathType;
use crate::entity::damage::DamageSource;
use crate::entity::mob::{NavigationKind, rotlerp};
use crate::entity::{
    Entity, EntityBase, EntityBaseLoad, EntitySyncedData, LivingEntity, LivingEntityBase, Mob,
    MobBase, PathfinderMob,
};
use crate::physics::{MoveResult, MoverType};
use crate::world::World;

/// Air a fish holds, in ticks, and what it is refilled to underwater.
///
/// Vanilla parity: the `300` of `WaterAnimal.handleAirSupply`.
const FISH_AIR_SUPPLY: i32 = 300;

/// Damage a suffocating fish takes each tick once its air runs out.
///
/// Vanilla parity: the `2.0F` of `WaterAnimal.handleAirSupply`.
const SUFFOCATION_DAMAGE: f32 = 2.0;

/// Ticks between two idle sounds.
///
/// Vanilla parity: `WaterAnimal.AMBIENT_SOUND_INTERVAL`.
const AMBIENT_SOUND_INTERVAL: i32 = 120;

/// Speed multiplier while fleeing.
///
/// Vanilla parity: `new PanicGoal(this, 1.25)`.
const PANIC_SPEED_MODIFIER: f64 = 1.25;

/// Speed multiplier while wandering.
///
/// Vanilla parity: `FishSwimGoal`'s `super(fish, 1.0, 40)`.
const SWIM_SPEED_MODIFIER: f64 = 1.0;

/// Ticks between two attempts to pick a new place to swim to.
///
/// Vanilla parity: the `40` of the same call.
const SWIM_INTERVAL_TICKS: i32 = 40;

/// Fraction of speed a swimming fish keeps each tick.
///
/// Vanilla parity: the `scale(0.9)` of `AbstractFish.travelInWater`.
const SWIM_DRAG: f64 = 0.9;

/// How hard a fish pushes itself through the water.
///
/// Vanilla parity: the `0.01F` of `moveRelative` in the same method.
const SWIM_ACCELERATION: f32 = 0.01;

/// Downward drift a fish with nothing to chase settles into.
///
/// Vanilla parity: the `-0.005` of `AbstractFish.travelInWater`, which is why
/// an idle fish sinks slowly instead of hanging still.
const IDLE_SINK: f64 = -0.005;

/// Upward nudge a submerged fish gets every tick.
///
/// Vanilla parity: the `0.005` of `AbstractFish.FishMoveControl.tick`, which
/// offsets the sink above so a swimming fish holds its depth.
const SUBMERGED_LIFT: f64 = 0.005;

/// How fast the fish converges on its wanted speed.
///
/// Vanilla parity: the `Mth.lerp(0.125F, ...)` of the same method.
const SPEED_LERP: f32 = 0.125;

/// Share of its speed a fish converts into vertical movement.
///
/// Vanilla parity: the `0.1` factor applied to the vertical component.
const VERTICAL_STEER: f64 = 0.1;

/// Degrees a fish may turn toward its heading in one tick.
///
/// Vanilla parity: the `90.0F` passed to `rotlerp`.
const TURN_RATE: f32 = 90.0;

/// Upward kick of a flop.
///
/// Vanilla parity: the `0.4F` of `AbstractFish.aiStep`.
const FLOP_LIFT: f64 = 0.4;

/// Sideways scatter of a flop.
///
/// Vanilla parity: the `0.05F` of the same line.
const FLOP_SCATTER: f64 = 0.05;

/// A cod.
#[entity_behavior(class = "Cod")]
pub struct CodEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    entity_data: SyncMutex<CodEntityData>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `CodEntity`.
unsafe impl DowncastType for CodEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/cod");
}

impl CodEntity {
    /// Creates a cod at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Creates a cod from saved base data.
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
        // Vanilla parity: `WaterAnimal` clears the water malus, so open water is
        // free to path through rather than merely allowed.
        mob_base
            .pathfinding_malus()
            .lock()
            .set(PathType::Water, 0.0);
        let mut entity_data = CodEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);

        {
            // Keep vanilla AbstractFish goal priorities in the same order.
            let mut goals = mob_base.goal_selector().lock();
            goals.add_goal(0, PanicGoal::new(PANIC_SPEED_MODIFIER));
            goals.add_goal(
                4,
                RandomSwimmingGoal::new(SWIM_SPEED_MODIFIER, SWIM_INTERVAL_TICKS),
            );
            // TODO: vanilla also flees players within eight blocks at priority 2
            // via AvoidEntityGoal, which the goal module does not export yet.
        }

        Self {
            base,
            entity_type,
            living_base,
            mob_base,
            entity_data: SyncMutex::new(entity_data),
        }
    }

    /// Returns whether this cod came out of a bucket.
    ///
    /// Vanilla parity: `AbstractFish.fromBucket`, whose name this keeps even
    /// though it reads state rather than converting anything.
    #[must_use]
    #[expect(
        clippy::wrong_self_convention,
        reason = "the name mirrors the vanilla accessor"
    )]
    pub fn from_bucket(&self) -> bool {
        *self.entity_data.lock().abstract_fish.from_bucket.get()
    }

    /// Marks this cod as having come out of a bucket.
    pub fn set_from_bucket(&self, from_bucket: bool) {
        self.entity_data
            .lock()
            .abstract_fish_mut()
            .from_bucket
            .set(from_bucket);
    }

    /// Drains air out of water and refills it in.
    ///
    /// Vanilla parity: `WaterAnimal.handleAirSupply`, the mirror image of a land
    /// mob drowning.
    fn handle_air_supply(&self, world: &World, air_before_tick: i32) {
        if Entity::is_alive(self) && !self.is_in_water() {
            self.set_air_supply(air_before_tick - 1);
            if self.should_take_drowning_damage() {
                self.set_air_supply(0);
                self.hurt_server(
                    world,
                    &DamageSource::environment(&vanilla_damage_types::DROWN),
                    SUFFOCATION_DAMAGE,
                );
            }
        } else {
            self.set_air_supply(FISH_AIR_SUPPLY);
        }
    }

    /// Throws the fish about when it is stranded on land.
    ///
    /// Vanilla parity: the flop branch of `AbstractFish.aiStep`.
    fn flop(&self) {
        if self.is_in_water() || !self.on_ground() {
            return;
        }

        let scatter = |value: f64| (value * 2.0 - 1.0) * FLOP_SCATTER;
        self.set_velocity(
            self.velocity()
                + DVec3::new(
                    scatter(rand::random::<f64>()),
                    FLOP_LIFT,
                    scatter(rand::random::<f64>()),
                ),
        );
        self.set_on_ground(false);
        self.play_sound(&sound_events::ENTITY_COD_FLOP, 1.0, 1.0);
    }
}

impl Entity for CodEntity {
    fn base(&self) -> &EntityBase {
        &self.base
    }

    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
    }

    fn synced_data(&self) -> Option<&dyn EntitySyncedData> {
        Some(&self.entity_data)
    }

    /// Vanilla parity: `WaterAnimal.baseTick`, which reads the air left before
    /// the shared tick spends it.
    fn base_tick(&self) {
        let air_before_tick = self.air_supply();
        Mob::base_tick_mob(self);
        if let Some(world) = self.level() {
            self.handle_air_supply(&world, air_before_tick);
        }
    }

    fn sound_source(&self) -> SoundSource {
        SoundSource::Neutral
    }

    /// Vanilla parity: `AbstractFish.playStepSound` is empty; a fish has no feet.
    fn play_step_sound(&self, _pos: BlockPos, _block_state: BlockStateId) {}

    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_mob(nbt);
        nbt.insert("FromBucket", self.from_bucket());
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_mob(nbt);
        self.set_from_bucket(nbt.byte("FromBucket").is_some_and(|flag| flag != 0));
    }
}

impl LivingEntity for CodEntity {
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
        Some(&sound_events::ENTITY_COD_HURT)
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_COD_DEATH)
    }

    fn server_ai_step(&self) {
        Mob::mob_server_ai_step(self);
    }

    fn ai_step(&self) -> Option<MoveResult> {
        self.flop();
        self.default_ai_step()
    }

    /// Swims instead of wading.
    ///
    /// Vanilla parity: `AbstractFish.travelInWater`, which replaces the shared
    /// water physics outright: a fish pushes itself along, keeps nine tenths of
    /// its speed, and drifts down when it has nothing to chase.
    fn travel_in_water(
        &self,
        input: DVec3,
        _base_gravity: f64,
        _is_falling: bool,
        _old_y: f64,
    ) -> Option<MoveResult> {
        self.move_relative(SWIM_ACCELERATION, input);
        let result = self.move_entity(MoverType::SelfMovement, self.velocity());
        self.set_velocity(self.velocity() * SWIM_DRAG);
        if Mob::target(self).is_none() {
            self.set_velocity(self.velocity() + DVec3::new(0.0, IDLE_SINK, 0.0));
        }
        result
    }
}

impl Mob for CodEntity {
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
        Some(&sound_events::ENTITY_COD_AMBIENT)
    }

    /// Vanilla parity: `WaterAnimal.getAmbientSoundInterval`.
    fn ambient_sound_interval(&self) -> i32 {
        AMBIENT_SOUND_INTERVAL
    }

    /// Vanilla parity: `AbstractFish.removeWhenFarAway`. A bucketed fish someone
    /// released stays where it was put.
    fn remove_when_far_away(&self, _dist_sqr: f64) -> bool {
        !self.from_bucket()
    }

    /// Steers in three dimensions rather than along the ground.
    ///
    /// Vanilla parity: `AbstractFish.FishMoveControl.tick`.
    fn tick_move_control(&self) {
        if self.is_eye_in_water() {
            self.set_velocity(self.velocity() + DVec3::new(0.0, SUBMERGED_LIFT, 0.0));
        }

        let (operation, wanted_position, speed_modifier) = {
            let controls = self.mob_base().controls().lock();
            let move_control = controls.move_control;
            (
                move_control.operation(),
                move_control.wanted_position(),
                move_control.speed_modifier(),
            )
        };

        let navigating = matches!(operation, MoveControlOperation::MoveTo)
            && !self.mob_base().navigation().lock().is_done();
        if !navigating {
            self.set_mob_speed(0.0);
            return;
        }

        let movement_speed = self
            .attributes()
            .lock()
            .required_value(vanilla_attributes::MOVEMENT_SPEED);
        let target_speed = (speed_modifier * movement_speed) as f32;
        let current_speed = self.get_speed();
        let speed = SPEED_LERP.mul_add(target_speed - current_speed, current_speed);
        self.set_mob_speed(speed);

        let position = self.position();
        let delta = wanted_position - position;
        if delta.y != 0.0 {
            let distance = delta.length();
            if distance > 0.0 {
                let lift = f64::from(speed) * (delta.y / distance) * VERTICAL_STEER;
                self.set_velocity(self.velocity() + DVec3::new(0.0, lift, 0.0));
            }
        }

        if delta.x != 0.0 || delta.z != 0.0 {
            let wanted_yaw = (delta.z.atan2(delta.x).to_degrees() as f32) - 90.0;
            let (yaw, pitch) = self.rotation();
            let turned = rotlerp(yaw, wanted_yaw, TURN_RATE);
            self.set_rotation((turned, pitch));
            self.set_y_body_rot(turned);
        }
    }

    fn mob_flags(&self) -> i8 {
        *self.entity_data.lock().mob().mob_flags.get()
    }

    fn set_mob_flags(&self, flags: i8) {
        self.entity_data.lock().mob_mut().mob_flags.set(flags);
    }
}

impl PathfinderMob for CodEntity {
    /// Vanilla parity: `AbstractFish.createNavigation` returns a
    /// `WaterBoundPathNavigation`; a cod never breaches.
    fn navigation_kind(&self) -> NavigationKind {
        NavigationKind::WaterBound {
            allow_breaching: false,
        }
    }
}
