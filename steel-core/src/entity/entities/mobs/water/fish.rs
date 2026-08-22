//! Behavior every fish shares.
//!
//! Vanilla parity: `AbstractFish` and `WaterAnimal`. Vanilla puts this in a
//! superclass; Steel builds each entity as its own struct, so the parts with
//! real logic live here and each fish delegates to them.

use glam::DVec3;
use steel_registry::sound_event::SoundEventRef;
use steel_registry::{vanilla_attributes, vanilla_damage_types};

use crate::entity::ai::control::MoveControlOperation;
use crate::entity::damage::DamageSource;
use crate::entity::mob::rotlerp;
use crate::entity::{Entity, LivingEntity, Mob};
use crate::physics::{MoveResult, MoverType};
use crate::world::World;

/// Air a fish holds, in ticks, and what it is refilled to underwater.
///
/// Vanilla parity: the `300` of `WaterAnimal.handleAirSupply`.
pub(super) const FISH_AIR_SUPPLY: i32 = 300;

/// Damage a suffocating fish takes each tick once its air runs out.
///
/// Vanilla parity: the `2.0F` of `WaterAnimal.handleAirSupply`.
pub(super) const SUFFOCATION_DAMAGE: f32 = 2.0;

/// Ticks between two idle sounds.
///
/// Vanilla parity: `WaterAnimal.AMBIENT_SOUND_INTERVAL`.
pub(super) const AMBIENT_SOUND_INTERVAL: i32 = 120;

/// Speed multiplier while fleeing.
///
/// Vanilla parity: `new PanicGoal(this, 1.25)`.
pub(super) const PANIC_SPEED_MODIFIER: f64 = 1.25;

/// Distance at which a fish notices a player it wants away from.
///
/// Vanilla parity: the `8.0F` of `AbstractFish.registerGoals`.
pub(super) const AVOID_PLAYER_RANGE: f32 = 8.0;

/// Speed a fleeing fish walks at.
///
/// Vanilla parity: the `1.6` of the same call.
pub(super) const AVOID_WALK_SPEED: f64 = 1.6;

/// Speed a fleeing fish sprints at once the player is close.
///
/// Vanilla parity: the `1.4` of the same call. Vanilla really does sprint
/// slower than it walks here.
pub(super) const AVOID_SPRINT_SPEED: f64 = 1.4;

/// Speed multiplier while wandering.
///
/// Vanilla parity: `FishSwimGoal`'s `super(fish, 1.0, 40)`.
pub(super) const SWIM_SPEED_MODIFIER: f64 = 1.0;

/// Ticks between two attempts to pick a new place to swim to.
///
/// Vanilla parity: the `40` of the same call.
pub(super) const SWIM_INTERVAL_TICKS: i32 = 40;

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

/// Drains air out of water and refills it in.
///
/// Vanilla parity: `WaterAnimal.handleAirSupply`, the mirror image of a land
/// mob drowning.
pub(super) fn handle_air_supply<M: LivingEntity + ?Sized>(
    fish: &M,
    world: &World,
    air_before_tick: i32,
) {
    if Entity::is_alive(fish) && !fish.is_in_water() {
        fish.set_air_supply(air_before_tick - 1);
        if fish.should_take_drowning_damage() {
            fish.set_air_supply(0);
            fish.hurt_server(
                world,
                &DamageSource::environment(&vanilla_damage_types::DROWN),
                SUFFOCATION_DAMAGE,
            );
        }
    } else {
        fish.set_air_supply(FISH_AIR_SUPPLY);
    }
}

/// Throws the fish about when it is stranded on land.
///
/// Vanilla parity: the flop branch of `AbstractFish.aiStep`.
pub(super) fn flop<M: LivingEntity + ?Sized>(fish: &M, flop_sound: SoundEventRef) {
    if fish.is_in_water() || !fish.on_ground() {
        return;
    }

    let scatter = || (rand::random::<f64>() * 2.0 - 1.0) * FLOP_SCATTER;
    fish.set_velocity(fish.velocity() + DVec3::new(scatter(), FLOP_LIFT, scatter()));
    fish.set_on_ground(false);
    fish.play_sound(flop_sound, 1.0, 1.0);
}

/// Swims instead of wading.
///
/// Vanilla parity: `AbstractFish.travelInWater`, which replaces the shared
/// water physics outright: a fish pushes itself along, keeps nine tenths of its
/// speed, and drifts down when it has nothing to chase.
pub(super) fn travel_in_water<M: Mob + ?Sized>(fish: &M, input: DVec3) -> Option<MoveResult> {
    fish.move_relative(SWIM_ACCELERATION, input);
    let result = fish.move_entity(MoverType::SelfMovement, fish.velocity());
    fish.set_velocity(fish.velocity() * SWIM_DRAG);
    if fish.target().is_none() {
        fish.set_velocity(fish.velocity() + DVec3::new(0.0, IDLE_SINK, 0.0));
    }
    result
}

/// Steers in three dimensions rather than along the ground.
///
/// Vanilla parity: `AbstractFish.FishMoveControl.tick`.
pub(super) fn tick_move_control<M: Mob + ?Sized>(fish: &M) {
    if fish.is_eye_in_water() {
        fish.set_velocity(fish.velocity() + DVec3::new(0.0, SUBMERGED_LIFT, 0.0));
    }

    let (operation, wanted_position, speed_modifier) = {
        let controls = fish.mob_base().controls().lock();
        let move_control = controls.move_control;
        (
            move_control.operation(),
            move_control.wanted_position(),
            move_control.speed_modifier(),
        )
    };

    let navigating = matches!(operation, MoveControlOperation::MoveTo)
        && !fish.mob_base().navigation().lock().is_done();
    if !navigating {
        fish.set_mob_speed(0.0);
        return;
    }

    let movement_speed = fish
        .attributes()
        .lock()
        .required_value(vanilla_attributes::MOVEMENT_SPEED);
    let target_speed = (speed_modifier * movement_speed) as f32;
    let current_speed = fish.get_speed();
    let speed = SPEED_LERP.mul_add(target_speed - current_speed, current_speed);
    fish.set_mob_speed(speed);

    let delta = wanted_position - fish.position();
    if delta.y != 0.0 {
        let distance = delta.length();
        if distance > 0.0 {
            let lift = f64::from(speed) * (delta.y / distance) * VERTICAL_STEER;
            fish.set_velocity(fish.velocity() + DVec3::new(0.0, lift, 0.0));
        }
    }

    if delta.x != 0.0 || delta.z != 0.0 {
        let wanted_yaw = (delta.z.atan2(delta.x).to_degrees() as f32) - 90.0;
        let (yaw, pitch) = fish.rotation();
        let turned = rotlerp(yaw, wanted_yaw, TURN_RATE);
        fish.set_rotation((turned, pitch));
        fish.set_y_body_rot(turned);
    }
}

/// Returns whether a fish should flee this entity.
///
/// Vanilla parity: the `Player.class` filter of the fish `AvoidEntityGoal`,
/// which is why a fish darts from a swimmer but ignores a squid.
pub(super) fn is_player_to_flee(target: &dyn LivingEntity) -> bool {
    target
        .as_player()
        .is_some_and(|player| !target.is_spectator() && !player.has_infinite_materials())
}
