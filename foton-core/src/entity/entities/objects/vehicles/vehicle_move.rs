//! Telling plugins that a boat or minecart moved.

use glam::DVec3;

use crate::entity::Entity;
use crate::event::VehicleMoveEvent;

/// Where a vehicle is and which way it faces.
pub(super) type Placement = (DVec3, (f32, f32));

/// Returns where `vehicle` is now.
pub(super) fn placement_of(vehicle: &dyn Entity) -> Placement {
    (vehicle.position(), vehicle.rotation())
}

/// Fires [`VehicleMoveEvent`] when a vehicle's tick ended elsewhere.
///
/// Paper parity: the event of `AbstractBoat.tick` and `AbstractMinecart.tick`.
/// A horse or any other mount is not a vehicle to Paper, so it has none.
pub(super) fn fire_if_moved(vehicle: &dyn Entity, from: Placement) {
    let to = placement_of(vehicle);
    if from == to {
        return;
    }
    let Some(world) = vehicle.level() else {
        return;
    };
    let mut event = VehicleMoveEvent::new(vehicle.uuid(), world.key.to_string(), from, to);
    world.fire_event(&mut event);
}
