//! What every boat and raft shares.
//!
//! Vanilla parity: `AbstractBoat`. A boat is not a mob with a speed: it is a
//! box that floats, and everything interesting about it comes from where the
//! water surface is relative to its own hull. The five states below are that
//! question answered, and the float step is the answer applied.
//!
//! A ridden boat is driven by the client -- vanilla runs `controlBoat` there
//! and sends the result as a vehicle move, which Foton already accepts. What
//! the server owns is the boat nobody is riding: it still has to float, drift
//! and fall, or a boat left on a lake would sink through it.

use std::sync::Arc;

use foton_registry::blocks::block_state_ext::BlockStateExt as _;
use foton_registry::entity_type::EntityDimensions;
use foton_registry::vanilla_blocks;
use foton_utils::BlockPos;
use foton_utils::locks::SyncMutex;
use glam::DVec3;

use crate::behavior::InteractionResult;
use crate::entity::Entity;
use crate::fluid::{FluidStateExt as _, get_height};
use crate::physics::MoverType;
use crate::player::Player;
use crate::world::{LevelReader as _, World};

/// How long a submerged boat carries its passengers before throwing them out.
///
/// Vanilla parity: `AbstractBoat.TIME_TO_EJECT`.
const TIME_TO_EJECT_TICKS: f32 = 60.0;

/// Vanilla parity: `AbstractBoat.getDefaultGravity`.
pub(super) const BOAT_GRAVITY: f64 = 0.04;

/// How high a rider sits in a boat, as a share of its height.
///
/// Vanilla parity: `Boat.rideHeight`.
pub(super) const BOAT_RIDE_HEIGHT: f64 = 1.0 / 3.0;

/// How high a rider sits on a raft.
///
/// Vanilla parity: `Raft.rideHeight`. A raft has no hull, so the rider sits
/// almost on top of it.
pub(super) const RAFT_RIDE_HEIGHT: f64 = 0.888_888_9;

/// How much of its speed a boat keeps each tick in water.
const WATER_DRAG: f64 = 0.9;

/// How much it keeps while fully submerged, where it is being pushed up.
const UNDER_WATER_DRAG: f64 = 0.45;

/// Upward push a submerged boat gets.
const UNDER_WATER_BUOYANCY: f64 = 0.01;

/// The tiny downward drift of a boat under flowing water.
const UNDER_FLOWING_WATER_FALL: f64 = -7.0e-4;

/// How much of the buoyancy survives each tick.
///
/// Vanilla parity: the `* 0.75` of `floatBoat`.
const BUOYANCY_DAMPING: f64 = 0.75;

/// Divisor turning the water gap into an upward push.
///
/// Vanilla parity: the `getDefaultGravity() / 0.65` of `floatBoat`.
const BUOYANCY_SCALE: f64 = 0.65;

/// How many riders a boat carries.
///
/// Vanilla parity: `AbstractBoat.getMaxPassengers`.
pub(super) const MAX_PASSENGERS: usize = 2;

/// Where a boat sits relative to the water.
///
/// Vanilla parity: `AbstractBoat.Status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum BoatStatus {
    /// Floating with its deck above the surface.
    InWater,
    /// Fully submerged in still water.
    UnderWater,
    /// Fully submerged in flowing water.
    UnderFlowingWater,
    /// Resting on a block.
    OnLand,
    /// Falling.
    #[default]
    InAir,
}

impl BoatStatus {
    /// Returns whether the boat is fully under.
    ///
    /// Vanilla parity: `AbstractBoat.isUnderWater`.
    pub(super) const fn is_under_water(self) -> bool {
        matches!(self, Self::UnderWater | Self::UnderFlowingWater)
    }
}

/// The floating state vanilla keeps on the boat itself.
#[derive(Debug, Default)]
pub(super) struct BoatState {
    /// Where the boat sits right now.
    pub status: BoatStatus,
    /// Where it sat last tick, which is how a splashdown is detected.
    pub old_status: BoatStatus,
    /// Height of the water the hull is resting in.
    pub water_level: f64,
    /// Friction of whatever it is sitting on.
    pub land_friction: f32,
    /// Ticks it has spent submerged.
    pub out_of_control_ticks: f32,
    /// Vertical speed last tick, used by the fall check.
    pub last_yd: f64,
}

/// What a concrete boat exposes to the shared code.
pub(super) trait BoatLike: Entity {
    /// Returns the floating state.
    fn boat_state(&self) -> &SyncMutex<BoatState>;

    /// Returns how high a rider sits.
    ///
    /// Vanilla parity: `AbstractBoat.rideHeight`, the one thing a raft changes.
    fn ride_height(&self, dimensions: EntityDimensions) -> f64;
}

/// Boards a player, or refuses to.
///
/// Vanilla parity: `AbstractBoat.interact`. Sneaking passes rather than
/// boarding, which is what leaves room for a chest boat to be opened instead,
/// and a boat that has spent three seconds upside down will not take a rider
/// at all -- otherwise a player could climb into a boat that is already
/// throwing its passengers out.
pub(super) fn interact_boat<B: BoatLike>(boat: &B, player: &Player) -> InteractionResult {
    if player.is_secondary_use_active() {
        return InteractionResult::Pass;
    }
    if boat.boat_state().lock().out_of_control_ticks >= TIME_TO_EJECT_TICKS {
        return InteractionResult::Pass;
    }
    let Some(world) = boat.level() else {
        return InteractionResult::Pass;
    };
    let Some(vehicle) = world.get_entity_by_id(boat.id()) else {
        return InteractionResult::Pass;
    };
    if player.start_riding(&vehicle) {
        InteractionResult::Success
    } else {
        InteractionResult::Pass
    }
}

/// Runs one tick of a boat's own physics.
///
/// Vanilla parity: the server half of `AbstractBoat.tick`. The client half --
/// reading the rider's keys and steering -- belongs to the client, which sends
/// the result back as a vehicle move.
pub(super) fn tick_boat<B: BoatLike>(boat: &B) {
    let Some(world) = boat.level() else {
        return;
    };

    let status = compute_status(boat, &world);
    let submerged = {
        let mut state = boat.boat_state().lock();
        state.old_status = state.status;
        state.status = status;
        if status.is_under_water() {
            state.out_of_control_ticks += 1.0;
        } else {
            state.out_of_control_ticks = 0.0;
        }
        state.out_of_control_ticks >= TIME_TO_EJECT_TICKS
    };

    // Vanilla parity: a boat held under for three seconds throws everyone out,
    // which is what stops a player drowning inside one.
    if submerged {
        for passenger in boat.passengers() {
            passenger.stop_riding();
        }
    }

    float_boat(boat, &world);

    // A ridden boat is moved by its rider's client; an empty one drifts here.
    if boat.controlling_passenger().is_none() {
        boat.move_entity(MoverType::SelfMovement, boat.velocity());
    }
}

/// Applies buoyancy, drag and gravity for this tick.
///
/// Vanilla parity: `AbstractBoat.floatBoat`.
fn float_boat<B: BoatLike>(boat: &B, world: &Arc<World>) {
    let (status, old_status, water_level, land_friction) = {
        let state = boat.boat_state().lock();
        (
            state.status,
            state.old_status,
            state.water_level,
            state.land_friction,
        )
    };

    // Vanilla parity: the splashdown branch, which drops a falling boat onto
    // the surface rather than letting it plunge through.
    if old_status == BoatStatus::InAir
        && status != BoatStatus::InAir
        && status != BoatStatus::OnLand
    {
        let target_y =
            f64::from(water_level_above(boat, world)) - boat.bounding_box().height() + 0.101;
        let position = boat.position();
        let _ = boat.try_set_position(DVec3::new(position.x, target_y, position.z));
        boat.set_velocity(boat.velocity() * DVec3::new(1.0, 0.0, 1.0));

        let mut state = boat.boat_state().lock();
        state.last_yd = 0.0;
        state.status = BoatStatus::InWater;
        return;
    }

    let mut fall_speed = -BOAT_GRAVITY;
    let mut buoyancy = 0.0;
    let drag = match status {
        BoatStatus::InWater => {
            buoyancy = (water_level - boat.position().y) / boat.bounding_box().height();
            WATER_DRAG
        }
        BoatStatus::UnderFlowingWater => {
            fall_speed = UNDER_FLOWING_WATER_FALL;
            WATER_DRAG
        }
        BoatStatus::UnderWater => {
            buoyancy = UNDER_WATER_BUOYANCY;
            UNDER_WATER_DRAG
        }
        BoatStatus::InAir => WATER_DRAG,
        BoatStatus::OnLand => f64::from(land_friction),
    };

    let movement = boat.velocity();
    let mut next = DVec3::new(
        movement.x * drag,
        movement.y + fall_speed,
        movement.z * drag,
    );

    if buoyancy > 0.0 {
        next.y = (next.y + buoyancy * (BOAT_GRAVITY / BUOYANCY_SCALE)) * BUOYANCY_DAMPING;
    }

    boat.set_velocity(next);
}

/// Works out where the boat sits, and records the water level it found.
///
/// Vanilla parity: `AbstractBoat.getStatus`.
fn compute_status<B: BoatLike>(boat: &B, world: &Arc<World>) -> BoatStatus {
    if let Some(submerged) = submerged_status(boat, world) {
        boat.boat_state().lock().water_level = boat.bounding_box().max_y();
        return submerged;
    }

    if check_in_water(boat, world) {
        return BoatStatus::InWater;
    }

    let friction = ground_friction(boat, world);
    if friction > 0.0 {
        boat.boat_state().lock().land_friction = friction;
        return BoatStatus::OnLand;
    }

    BoatStatus::InAir
}

/// Returns the submerged status, if the boat's deck is under water.
///
/// Vanilla parity: `AbstractBoat.isUnderwater`.
fn submerged_status<B: BoatLike>(boat: &B, world: &Arc<World>) -> Option<BoatStatus> {
    let aabb = boat.bounding_box();
    let deck = aabb.max_y() + 0.001;
    let mut still = false;

    for x in floor(aabb.min_x())..ceil(aabb.max_x()) {
        for y in floor(aabb.max_y())..ceil(deck) {
            for z in floor(aabb.min_z())..ceil(aabb.max_z()) {
                let pos = BlockPos::new(x, y, z);
                let fluid = world.get_block_state(pos).get_fluid_state();
                if !fluid.is_water() {
                    continue;
                }
                let surface = f64::from(y) + f64::from(get_height(world, pos, fluid));
                if deck >= surface {
                    continue;
                }
                if !fluid.is_source() {
                    return Some(BoatStatus::UnderFlowingWater);
                }
                still = true;
            }
        }
    }

    still.then_some(BoatStatus::UnderWater)
}

/// Returns whether the hull is touching water, recording the surface height.
///
/// Vanilla parity: `AbstractBoat.checkInWater`.
fn check_in_water<B: BoatLike>(boat: &B, world: &Arc<World>) -> bool {
    let aabb = boat.bounding_box();
    let mut in_water = false;
    let mut water_level = f64::MIN;

    for x in floor(aabb.min_x())..ceil(aabb.max_x()) {
        for y in floor(aabb.min_y())..ceil(aabb.min_y() + 0.001) {
            for z in floor(aabb.min_z())..ceil(aabb.max_z()) {
                let pos = BlockPos::new(x, y, z);
                let fluid = world.get_block_state(pos).get_fluid_state();
                if !fluid.is_water() {
                    continue;
                }
                let surface = f64::from(y) + f64::from(get_height(world, pos, fluid));
                water_level = water_level.max(surface);
                in_water |= aabb.min_y() < surface;
            }
        }
    }

    boat.boat_state().lock().water_level = water_level;
    in_water
}

/// Returns the water surface just above the boat.
///
/// Vanilla parity: `AbstractBoat.getWaterLevelAbove`, which is what a falling
/// boat lands on.
fn water_level_above<B: BoatLike>(boat: &B, world: &Arc<World>) -> f32 {
    let aabb = boat.bounding_box();
    let last_yd = boat.boat_state().lock().last_yd;
    let top = ceil(aabb.max_y() - last_yd);

    for y in floor(aabb.max_y())..top {
        let mut height = 0.0_f32;
        for x in floor(aabb.min_x())..ceil(aabb.max_x()) {
            for z in floor(aabb.min_z())..ceil(aabb.max_z()) {
                let pos = BlockPos::new(x, y, z);
                let fluid = world.get_block_state(pos).get_fluid_state();
                if fluid.is_water() {
                    height = height.max(get_height(world, pos, fluid));
                }
            }
        }
        if height < 1.0 {
            #[expect(
                clippy::cast_precision_loss,
                reason = "a build height, well inside f32"
            )]
            return y as f32 + height;
        }
    }

    #[expect(clippy::cast_precision_loss, reason = "a build height")]
    let above = top as f32 + 1.0;
    above
}

/// Returns the average friction of the blocks under the hull.
///
/// Vanilla parity: `AbstractBoat.getGroundFriction`, which is how a boat slides
/// forever on ice and stops dead on grass.
fn ground_friction<B: BoatLike>(boat: &B, world: &Arc<World>) -> f32 {
    let aabb = boat.bounding_box();
    let x0 = floor(aabb.min_x()) - 1;
    let x1 = ceil(aabb.max_x()) + 1;
    let y0 = floor(aabb.min_y() - 0.001) - 1;
    let y1 = ceil(aabb.min_y()) + 1;
    let z0 = floor(aabb.min_z()) - 1;
    let z1 = ceil(aabb.max_z()) + 1;

    let mut total = 0.0;
    let mut count = 0;

    for x in x0..x1 {
        for z in z0..z1 {
            let edges = i32::from(x == x0 || x == x1 - 1) + i32::from(z == z0 || z == z1 - 1);
            if edges == 2 {
                continue;
            }
            for y in y0..y1 {
                if edges > 0 && (y == y0 || y == y1 - 1) {
                    continue;
                }
                let pos = BlockPos::new(x, y, z);
                let state = world.get_block_state(pos);
                // Vanilla parity: a lily pad is not something a boat rests on,
                // which is why a boat runs one over instead of grinding to a
                // halt on it.
                if state.get_block() == &vanilla_blocks::LILY_PAD || state.is_air() {
                    continue;
                }
                total += state.get_block().config.friction;
                count += 1;
            }
        }
    }

    if count == 0 {
        0.0
    } else {
        #[expect(clippy::cast_precision_loss, reason = "a small block count")]
        let average = total / count as f32;
        average
    }
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "world coordinates are well inside i32"
)]
const fn floor(value: f64) -> i32 {
    value.floor() as i32
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "world coordinates are well inside i32"
)]
const fn ceil(value: f64) -> i32 {
    value.ceil() as i32
}
