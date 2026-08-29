//! What every minecart shares.
//!
//! Vanilla parity: `AbstractMinecart` and `OldMinecartBehavior`. A minecart is
//! not steered: it is snapped onto the line between a rail's two exits every
//! tick, pushed along that line, and slowed down. Everything interesting comes
//! out of that one idea, including the way a cart takes a corner without ever
//! being told a corner is there.
//!
//! Vanilla has a second, newer set of physics behind the `minecart_improvements`
//! game rule. This is the old one, which is what a world runs with the rule off
//! -- the default.
//!
//! `pushAndPickupEntities` is here: a moving rideable cart scoops up the mob
//! that walks into it and shoves everything else, and a stationary one is
//! shoved by other carts. The note that used to sit here said Foton had no
//! entity push; it does, in `Entity::push_entity`, and every living entity had
//! been using it. Only the cart was missing.

use std::sync::Arc;

use foton_registry::blocks::block_state_ext::BlockStateExt as _;
use foton_registry::blocks::properties::{BlockStateProperties, BoolProperty, RailShape};
use foton_registry::{vanilla_blocks, vanilla_entities};
use foton_utils::locks::SyncMutex;
use foton_utils::{BlockPos, BlockStateId};
use glam::DVec3;

use crate::behavior::BLOCK_BEHAVIORS;
use crate::behavior::blocks::rail_shape_at;
use crate::entity::Entity;
use crate::physics::MoverType;
use crate::world::{LevelReader as _, World};

/// Vanilla parity: `AbstractMinecart.getAirDrag`.
const AIR_DRAG: f64 = 0.95;

/// How fast a cart may go, in blocks per tick.
///
/// Vanilla parity: `OldMinecartBehavior.MAX_SPEED_ON_LAND` and
/// `MAX_SPEED_IN_WATER`.
const MAX_SPEED_ON_LAND: f64 = 0.4;
const MAX_SPEED_IN_WATER: f64 = 0.2;

/// How much speed a cart keeps each tick.
///
/// Vanilla parity: `OldMinecartBehavior.getSlowdownFactor`. A ridden cart keeps
/// almost all of it, which is why an empty cart stops and a carried one does
/// not.
const SLOWDOWN_RIDDEN: f64 = 0.997;
const SLOWDOWN_EMPTY: f64 = 0.96;

/// Extra drag while under water, shared by every cart.
///
/// Vanilla parity: the `0.95F` of `AbstractMinecart.applyNaturalSlowdown`.
const WATER_SLOWDOWN: f64 = 0.95;

/// How hard a slope pulls a cart downhill each tick.
///
/// Vanilla parity: the `slideSpeed` of `OldMinecartBehavior.moveAlongTrack`.
const SLIDE_SPEED: f64 = 0.007_812_5;

/// What that becomes under water.
const SLIDE_SPEED_IN_WATER_FACTOR: f64 = 0.2;

/// How hard a powered rail pushes a moving cart.
const POWERED_RAIL_PUSH: f64 = 0.06;

/// How hard it nudges a stopped one off a solid block.
const POWERED_RAIL_KICK: f64 = 0.02;

/// Below this a cart on an unpowered powered rail is simply stopped.
const HALT_THRESHOLD: f64 = 0.03;

/// Above this a cart on a powered rail is pushed rather than kicked.
const PUSH_THRESHOLD: f64 = 0.01;

/// A cart carrying somebody moves at three quarters speed.
///
/// Vanilla parity: the `scale` of `OldMinecartBehavior.moveAlongTrack`.
const RIDDEN_SCALE: f64 = 0.75;

/// Vanilla parity: the `Math.min(2.0, ...)` that caps redirected speed.
const MAX_REDIRECTED_SPEED: f64 = 2.0;

/// How far a cart has to move before it turns to face its travel.
const ROTATION_THRESHOLD: f64 = 0.001;

/// Whether a powered rail is switched on.
const POWERED: &BoolProperty = &BlockStateProperties::POWERED;

/// What a minecart remembers between ticks.
#[derive(Debug, Default, Clone, Copy)]
pub(super) struct MinecartState {
    /// Whether the cart was on a rail last tick.
    ///
    /// Vanilla parity: `AbstractMinecart.onRails`.
    pub on_rails: bool,
    /// Whether the cart is pointing backwards.
    ///
    /// Vanilla parity: `AbstractMinecart.flipped`. A cart has no front, so
    /// rather than swing a half turn when it reverses, it remembers which way
    /// round it is and keeps its yaw.
    pub flipped: bool,
}

/// What a concrete minecart exposes to the shared code.
pub(super) trait MinecartLike: Entity {
    /// Returns the rail state.
    fn minecart_state(&self) -> &SyncMutex<MinecartState>;

    /// Called when the cart passes over an activator rail.
    ///
    /// Vanilla parity: `AbstractMinecart.activateMinecart`, which is a no-op on
    /// the base class and does something on four of the six carts.
    fn activate_minecart(&self, _world: &Arc<World>, _pos: BlockPos, _powered: bool) {}

    /// Whether a mob that walks into this cart climbs aboard.
    ///
    /// Vanilla parity: `AbstractMinecart.isRideable`, which is false on the
    /// base class and true only on the plain minecart -- a chest cart runs a
    /// pig over rather than carrying it.
    fn is_rideable(&self) -> bool {
        false
    }

    /// Scales how fast this cart may go.
    ///
    /// Vanilla parity: the `getMaxSpeed` override of `MinecartFurnace`, which
    /// is the only cart that is not an ordinary speed.
    fn max_speed_factor(&self) -> f64 {
        1.0
    }

    /// Applies the drag this cart feels every tick.
    ///
    /// Vanilla parity: `AbstractMinecart.applyNaturalSlowdown`. The vertical
    /// component is dropped entirely, which is why a cart on a rail never
    /// accumulates fall speed. A container cart overrides this: a fuller chest
    /// rolls less far.
    fn apply_natural_slowdown(&self, movement: DVec3) -> DVec3 {
        let slowdown = if self.is_vehicle() {
            SLOWDOWN_RIDDEN
        } else {
            SLOWDOWN_EMPTY
        };
        let mut slowed = DVec3::new(movement.x * slowdown, 0.0, movement.z * slowdown);
        if self.is_in_water() {
            slowed *= WATER_SLOWDOWN;
        }
        slowed
    }
}

/// Returns the two directions a rail of this shape leads out in.
///
/// Vanilla parity: `AbstractMinecart.EXITS`. A y of -1 means that end of the
/// rail is one block lower, which is how a slope is described without any
/// separate notion of height.
const fn exits(shape: RailShape) -> ([i32; 3], [i32; 3]) {
    const WEST: [i32; 3] = [-1, 0, 0];
    const EAST: [i32; 3] = [1, 0, 0];
    const NORTH: [i32; 3] = [0, 0, -1];
    const SOUTH: [i32; 3] = [0, 0, 1];
    const WEST_DOWN: [i32; 3] = [-1, -1, 0];
    const EAST_DOWN: [i32; 3] = [1, -1, 0];
    const NORTH_DOWN: [i32; 3] = [0, -1, -1];
    const SOUTH_DOWN: [i32; 3] = [0, -1, 1];

    match shape {
        RailShape::NorthSouth => (NORTH, SOUTH),
        RailShape::EastWest => (WEST, EAST),
        RailShape::AscendingEast => (WEST_DOWN, EAST),
        RailShape::AscendingWest => (WEST, EAST_DOWN),
        RailShape::AscendingNorth => (NORTH, SOUTH_DOWN),
        RailShape::AscendingSouth => (NORTH_DOWN, SOUTH),
        RailShape::SouthEast => (SOUTH, EAST),
        RailShape::SouthWest => (SOUTH, WEST),
        RailShape::NorthWest => (NORTH, WEST),
        RailShape::NorthEast => (NORTH, EAST),
    }
}

/// Returns the block a cart should be reading its rail from.
///
/// Vanilla parity: `AbstractMinecart.getCurrentBlockPosOrRailBelow`. A cart
/// riding a slope sits a little above the rail it is on, so the block it is
/// inside is empty and the rail is the one below.
fn current_block_pos_or_rail_below<M: MinecartLike>(cart: &M, world: &Arc<World>) -> BlockPos {
    let position = cart.position();
    let x = position.x.floor() as i32;
    let y = position.y.floor() as i32;
    let z = position.z.floor() as i32;

    let below = BlockPos::new(x, y - 1, z);
    if rail_shape_at(world.get_block_state(below)).is_some() {
        return below;
    }
    BlockPos::new(x, y, z)
}

/// Returns how fast this cart may go right now.
///
/// Vanilla parity: `OldMinecartBehavior.getMaxSpeed`.
fn max_speed<M: MinecartLike>(cart: &M) -> f64 {
    let base = if cart.is_in_water() {
        MAX_SPEED_IN_WATER
    } else {
        MAX_SPEED_ON_LAND
    };
    base * cart.max_speed_factor()
}

/// Returns whether the block at `pos` can carry redstone through it.
///
/// Vanilla parity: `AbstractMinecart.isRedstoneConductor`, which a powered rail
/// uses to decide which way to kick a stopped cart: away from the solid block.
fn is_redstone_conductor(world: &Arc<World>, pos: BlockPos) -> bool {
    let state = world.get_block_state(pos);
    BLOCK_BEHAVIORS
        .get_behavior(state.get_block())
        .is_redstone_conductor(state, world.as_ref(), pos)
}

/// Returns the exact point on the rail at `x, y, z`, or `None` off the rails.
///
/// Vanilla parity: `OldMinecartBehavior.getPos`. This is the line between the
/// two exits, sampled at the cart's progress along it, and it is what gives a
/// slope its height: the two ends differ in y, so a cart halfway up sits
/// halfway up.
fn rail_position(world: &Arc<World>, x: f64, y: f64, z: f64) -> Option<DVec3> {
    let xt = x.floor() as i32;
    let mut yt = y.floor() as i32;
    let zt = z.floor() as i32;

    if rail_shape_at(world.get_block_state(BlockPos::new(xt, yt - 1, zt))).is_some() {
        yt -= 1;
    }

    let shape = rail_shape_at(world.get_block_state(BlockPos::new(xt, yt, zt)))?;
    let (exit0, exit1) = exits(shape);

    let x0 = f64::from(xt) + 0.5 + f64::from(exit0[0]) * 0.5;
    let y0 = f64::from(yt) + 0.0625 + f64::from(exit0[1]) * 0.5;
    let z0 = f64::from(zt) + 0.5 + f64::from(exit0[2]) * 0.5;
    let x1 = f64::from(xt) + 0.5 + f64::from(exit1[0]) * 0.5;
    let y1 = f64::from(yt) + 0.0625 + f64::from(exit1[1]) * 0.5;
    let z1 = f64::from(zt) + 0.5 + f64::from(exit1[2]) * 0.5;

    let x_span = x1 - x0;
    let y_span = (y1 - y0) * 2.0;
    let z_span = z1 - z0;

    let progress = if x_span == 0.0 {
        z - f64::from(zt)
    } else if z_span == 0.0 {
        x - f64::from(xt)
    } else {
        ((x - x0) * x_span + (z - z0) * z_span) * 2.0
    };

    let mut position = DVec3::new(
        x_span.mul_add(progress, x0),
        y_span.mul_add(progress, y0),
        z_span.mul_add(progress, z0),
    );
    if y_span < 0.0 {
        position.y += 1.0;
    } else if y_span > 0.0 {
        position.y += 0.5;
    }
    Some(position)
}

/// Moves a cart that is not on a rail.
///
/// Vanilla parity: `AbstractMinecart.comeOffTrack`. It is still clamped to the
/// rail speed, which is why a cart shoved off the end of a line does not fly.
fn come_off_track<M: MinecartLike>(cart: &M, world: &Arc<World>) {
    let limit = max_speed(cart);
    let movement = cart.velocity();
    cart.set_velocity(DVec3::new(
        movement.x.clamp(-limit, limit),
        movement.y,
        movement.z.clamp(-limit, limit),
    ));

    if cart.on_ground() {
        cart.set_velocity(cart.velocity() * 0.5);
    }

    cart.move_entity(MoverType::SelfMovement, cart.velocity());

    if !cart.on_ground() {
        cart.set_velocity(cart.velocity() * AIR_DRAG);
    }

    let _ = world;
}

/// Runs one tick of a cart that is on a rail.
///
/// Vanilla parity: `OldMinecartBehavior.moveAlongTrack`.
#[expect(
    clippy::too_many_lines,
    reason = "one vanilla method, kept in one piece so it can be read against it"
)]
fn move_along_track<M: MinecartLike>(
    cart: &M,
    world: &Arc<World>,
    pos: BlockPos,
    state: BlockStateId,
    shape: RailShape,
) {
    cart.reset_fall_distance();

    let start = cart.position();
    let old_rail_position = rail_position(world, start.x, start.y, start.z);
    let mut x = start.x;
    let mut z = start.z;
    let mut y = f64::from(pos.y());

    // A powered rail either pushes or brakes, depending on whether it is on.
    let mut power_track = false;
    let mut halt_track = false;
    if state.get_block() == &vanilla_blocks::POWERED_RAIL {
        power_track = state.get_value(POWERED);
        halt_track = !power_track;
    }

    let slide_speed = if cart.is_in_water() {
        SLIDE_SPEED * SLIDE_SPEED_IN_WATER_FACTOR
    } else {
        SLIDE_SPEED
    };

    // Gravity along a slope. The rail is one block tall, so a cart on one sits
    // a block higher than the rail's own y.
    let movement = cart.velocity();
    match shape {
        RailShape::AscendingEast => {
            cart.set_velocity(movement + DVec3::new(-slide_speed, 0.0, 0.0));
            y += 1.0;
        }
        RailShape::AscendingWest => {
            cart.set_velocity(movement + DVec3::new(slide_speed, 0.0, 0.0));
            y += 1.0;
        }
        RailShape::AscendingNorth => {
            cart.set_velocity(movement + DVec3::new(0.0, 0.0, slide_speed));
            y += 1.0;
        }
        RailShape::AscendingSouth => {
            cart.set_velocity(movement + DVec3::new(0.0, 0.0, -slide_speed));
            y += 1.0;
        }
        _ => {}
    }

    // Redirect whatever speed the cart has onto the line the rail runs along.
    // This is the whole of cornering: nothing detects a corner, the speed is
    // simply reprojected onto the new line every tick.
    let movement = cart.velocity();
    let (exit0, exit1) = exits(shape);
    let mut x_span = f64::from(exit1[0] - exit0[0]);
    let mut z_span = f64::from(exit1[2] - exit0[2]);
    let length = x_span.hypot(z_span);
    if movement.x.mul_add(x_span, movement.z * z_span) < 0.0 {
        x_span = -x_span;
        z_span = -z_span;
    }

    let speed = movement
        .length_squared_xz()
        .sqrt()
        .min(MAX_REDIRECTED_SPEED);
    cart.set_velocity(DVec3::new(
        speed * x_span / length,
        movement.y,
        speed * z_span / length,
    ));

    // An unpowered powered rail is a brake.
    if halt_track {
        let movement = cart.velocity();
        if movement.length_squared_xz().sqrt() < HALT_THRESHOLD {
            cart.set_velocity(DVec3::ZERO);
        } else {
            cart.set_velocity(DVec3::new(movement.x * 0.5, 0.0, movement.z * 0.5));
        }
    }

    // Snap the cart onto the rail line before moving it, so it rides the
    // middle of the track however it arrived.
    let x0 = f64::from(pos.x()) + 0.5 + f64::from(exit0[0]) * 0.5;
    let z0 = f64::from(pos.z()) + 0.5 + f64::from(exit0[2]) * 0.5;
    let x1 = f64::from(pos.x()) + 0.5 + f64::from(exit1[0]) * 0.5;
    let z1 = f64::from(pos.z()) + 0.5 + f64::from(exit1[2]) * 0.5;
    let along_x = x1 - x0;
    let along_z = z1 - z0;

    let progress = if along_x == 0.0 {
        z - f64::from(pos.z())
    } else if along_z == 0.0 {
        x - f64::from(pos.x())
    } else {
        ((x - x0) * along_x + (z - z0) * along_z) * 2.0
    };

    x = along_x.mul_add(progress, x0);
    z = along_z.mul_add(progress, z0);
    let _ = cart.try_set_position(DVec3::new(x, y, z));

    let scale = if cart.is_vehicle() { RIDDEN_SCALE } else { 1.0 };
    let limit = max_speed(cart);
    let movement = cart.velocity();
    cart.move_entity(
        MoverType::SelfMovement,
        DVec3::new(
            (scale * movement.x).clamp(-limit, limit),
            0.0,
            (scale * movement.z).clamp(-limit, limit),
        ),
    );

    // Stepping onto the low end of a slope drops the cart a block.
    let moved = cart.position();
    let cell_x = moved.x.floor() as i32 - pos.x();
    let cell_z = moved.z.floor() as i32 - pos.z();
    if exit0[1] != 0 && cell_x == exit0[0] && cell_z == exit0[2] {
        let _ = cart.try_set_position(DVec3::new(moved.x, moved.y + f64::from(exit0[1]), moved.z));
    } else if exit1[1] != 0 && cell_x == exit1[0] && cell_z == exit1[2] {
        let _ = cart.try_set_position(DVec3::new(moved.x, moved.y + f64::from(exit1[1]), moved.z));
    }

    cart.set_velocity(cart.apply_natural_slowdown(cart.velocity()));

    // Height it actually gained or lost, converted back into speed: this is
    // what makes a cart run faster downhill and slower up.
    let moved = cart.position();
    if let (Some(old), Some(new)) = (
        old_rail_position,
        rail_position(world, moved.x, moved.y, moved.z),
    ) {
        let gained = (old.y - new.y) * 0.05;
        let movement = cart.velocity();
        let flat = movement.length_squared_xz().sqrt();
        if flat > 0.0 {
            let factor = (flat + gained) / flat;
            cart.set_velocity(DVec3::new(
                movement.x * factor,
                movement.y,
                movement.z * factor,
            ));
        }
        let here = cart.position();
        let _ = cart.try_set_position(DVec3::new(here.x, new.y, here.z));
    }

    // Crossing into the next block turns the cart's whole speed that way, so a
    // corner does not bleed speed into the direction it just left.
    let moved = cart.position();
    let cell_x = moved.x.floor() as i32;
    let cell_z = moved.z.floor() as i32;
    if cell_x != pos.x() || cell_z != pos.z() {
        let movement = cart.velocity();
        let flat = movement.length_squared_xz().sqrt();
        cart.set_velocity(DVec3::new(
            flat * f64::from(cell_x - pos.x()),
            movement.y,
            flat * f64::from(cell_z - pos.z()),
        ));
    }

    if power_track {
        let movement = cart.velocity();
        let flat = movement.length_squared_xz().sqrt();
        if flat > PUSH_THRESHOLD {
            cart.set_velocity(
                movement
                    + DVec3::new(
                        movement.x / flat * POWERED_RAIL_PUSH,
                        0.0,
                        movement.z / flat * POWERED_RAIL_PUSH,
                    ),
            );
        } else {
            // A stopped cart is kicked away from whatever solid block is on one
            // side, which is how a powered rail against a wall launches one.
            let mut kicked = movement;
            match shape {
                RailShape::EastWest => {
                    if is_redstone_conductor(world, pos.west()) {
                        kicked.x = POWERED_RAIL_KICK;
                    } else if is_redstone_conductor(world, pos.east()) {
                        kicked.x = -POWERED_RAIL_KICK;
                    }
                }
                RailShape::NorthSouth => {
                    if is_redstone_conductor(world, pos.north()) {
                        kicked.z = POWERED_RAIL_KICK;
                    } else if is_redstone_conductor(world, pos.south()) {
                        kicked.z = -POWERED_RAIL_KICK;
                    }
                }
                _ => return,
            }
            cart.set_velocity(kicked);
        }
    }
}

/// Runs one tick of a minecart.
///
/// Vanilla parity: `OldMinecartBehavior.tick`, the server half.
pub(super) fn tick_minecart<M: MinecartLike>(cart: &M) {
    let Some(world) = cart.level() else {
        return;
    };

    cart.apply_gravity();

    let pos = current_block_pos_or_rail_below(cart, &world);
    let state = world.get_block_state(pos);
    let shape = rail_shape_at(state);
    cart.minecart_state().lock().on_rails = shape.is_some();

    if let Some(shape) = shape {
        move_along_track(cart, &world, pos, state, shape);
        if state.get_block() == &vanilla_blocks::ACTIVATOR_RAIL {
            cart.activate_minecart(&world, pos, state.get_value(POWERED));
        }
    } else {
        come_off_track(cart, &world);
    }

    push_and_pickup_entities(cart, &world);
    cart.apply_effects_from_blocks();
    face_travel(cart);
}

/// Shoves what the cart runs into, and scoops up what can ride.
///
/// Vanilla parity: `OldMinecartBehavior.pushAndPickupEntities`. A rideable cart
/// with real horizontal speed takes on the first thing that is not a player, an
/// iron golem or another cart; everything else it simply shoves. A cart that is
/// stopped, or one nothing can ride, only gets shoved by other carts.
///
/// Vanilla filters with `EntitySelector.pushableBy`, which is `isPushable`
/// narrowed by team collision rules. Foton has no teams, so it reduces to
/// `is_pushable` and skipping spectators.
fn push_and_pickup_entities<M: MinecartLike>(cart: &M, world: &Arc<World>) {
    let hitbox = cart.bounding_box().inflate_xyz(0.2, 0.0, 0.2);
    let moving = cart.velocity().with_y(0.0).length_squared() >= 0.01;
    let riders_welcome = cart.is_rideable() && moving;

    for entity in world.get_entities_in_aabb(&hitbox) {
        if entity.id() == cart.id() {
            continue;
        }

        if riders_welcome {
            if entity.is_spectator() || !entity.is_pushable() {
                continue;
            }
            let boardable = entity.as_player().is_none()
                && entity.entity_type().key != vanilla_entities::IRON_GOLEM.key
                && !entity.is_minecart()
                && !cart.is_vehicle()
                && !entity.is_passenger();
            if boardable {
                if let Some(shared_cart) = world.get_entity_by_id(cart.id()) {
                    entity.start_riding(&shared_cart);
                }
            } else {
                entity.push_entity(cart);
            }
        } else if !cart.has_passenger(entity.as_ref())
            && entity.is_pushable()
            && entity.is_minecart()
        {
            entity.push_entity(cart);
        }
    }
}

/// Turns the cart to face the way it just moved.
///
/// Vanilla parity: the rotation block at the end of `OldMinecartBehavior.tick`.
/// A cart has no front, so reversing flips a remembered flag instead of
/// swinging the model through a half turn.
fn face_travel<M: MinecartLike>(cart: &M) {
    let (previous_yaw, _) = cart.rotation();
    let moved = cart.base().old_position() - cart.position();
    let mut yaw = previous_yaw;

    if moved.x.mul_add(moved.x, moved.z * moved.z) > ROTATION_THRESHOLD {
        yaw = moved.z.atan2(moved.x).to_degrees() as f32;
        if cart.minecart_state().lock().flipped {
            yaw += 180.0;
        }
    }

    let turned = wrap_degrees(f64::from(yaw - previous_yaw));
    if !(-170.0..170.0).contains(&turned) {
        yaw += 180.0;
        let mut state = cart.minecart_state().lock();
        state.flipped = !state.flipped;
    }

    cart.set_rotation((yaw % 360.0, 0.0));
}

/// Vanilla parity: `Mth.wrapDegrees`.
fn wrap_degrees(degrees: f64) -> f64 {
    let wrapped = degrees % 360.0;
    if wrapped >= 180.0 {
        wrapped - 360.0
    } else if wrapped < -180.0 {
        wrapped + 360.0
    } else {
        wrapped
    }
}

/// The horizontal part of a vector, squared.
trait HorizontalLength {
    fn length_squared_xz(self) -> f64;
}

impl HorizontalLength for DVec3 {
    fn length_squared_xz(self) -> f64 {
        self.x.mul_add(self.x, self.z * self.z)
    }
}

#[cfg(test)]
mod push_tests {
    use std::sync::Arc;

    use foton_registry::{init_vanilla_registry, vanilla_entities};
    use foton_utils::ChunkPos;
    use glam::DVec3;

    use super::push_and_pickup_entities;
    use crate::entity::entities::{ChestMinecartEntity, MinecartEntity, PigEntity};
    use crate::entity::{Entity, SharedEntity, next_entity_id};
    use crate::test_support::{fresh_test_world, insert_ready_full_chunk};
    use crate::world::World;

    /// The one spot the test world is solid ground rather than a column the
    /// fluid scan walks.
    const TRACK: DVec3 = DVec3::new(8.5, 64.0, 8.5);

    fn track_world(key: &'static str) -> Arc<World> {
        init_vanilla_registry();
        let world = fresh_test_world(key);
        insert_ready_full_chunk(&world, ChunkPos::new(0, 0));
        world
    }

    fn add(world: &Arc<World>, entity: SharedEntity) -> SharedEntity {
        world
            .try_add_entity(Arc::clone(&entity))
            .expect("the test chunk is loaded, so the entity should attach");
        entity
    }

    /// Vanilla parity: the `startRiding` branch of `pushAndPickupEntities`. A
    /// rolling minecart runs a pig over and the pig ends up in it.
    #[test]
    fn a_rolling_minecart_scoops_up_the_pig_it_runs_into() {
        let world = track_world("cart_scoops_pig");
        let cart = Arc::new(MinecartEntity::new(
            &vanilla_entities::MINECART,
            next_entity_id(),
            TRACK,
            Arc::downgrade(&world),
        ));
        add(&world, Arc::clone(&cart) as SharedEntity);
        let pig: SharedEntity = Arc::new(PigEntity::new(
            &vanilla_entities::PIG,
            next_entity_id(),
            TRACK,
            Arc::downgrade(&world),
        ));
        add(&world, Arc::clone(&pig));

        cart.set_velocity(DVec3::new(0.5, 0.0, 0.0));
        push_and_pickup_entities(cart.as_ref(), &world);

        assert!(pig.is_passenger(), "the pig did not board the cart");
        assert!(cart.is_vehicle(), "the cart is carrying nobody");
    }

    /// Vanilla parity: `AbstractMinecart.isRideable`, false on every cart but
    /// the plain one. A chest cart runs a pig over rather than carrying it.
    #[test]
    fn a_chest_minecart_carries_nobody() {
        let world = track_world("chest_cart_carries_nobody");
        let cart = Arc::new(ChestMinecartEntity::new(
            &vanilla_entities::CHEST_MINECART,
            next_entity_id(),
            TRACK,
            Arc::downgrade(&world),
        ));
        add(&world, Arc::clone(&cart) as SharedEntity);
        let pig: SharedEntity = Arc::new(PigEntity::new(
            &vanilla_entities::PIG,
            next_entity_id(),
            TRACK,
            Arc::downgrade(&world),
        ));
        add(&world, Arc::clone(&pig));

        cart.set_velocity(DVec3::new(0.5, 0.0, 0.0));
        push_and_pickup_entities(cart.as_ref(), &world);

        assert!(!pig.is_passenger(), "a chest cart took a passenger");
    }

    /// Vanilla parity: the speed gate on the same branch. A cart standing
    /// still is furniture, and a pig may share the square with it.
    #[test]
    fn a_stopped_minecart_scoops_up_nobody() {
        let world = track_world("stopped_cart_scoops_nobody");
        let cart = Arc::new(MinecartEntity::new(
            &vanilla_entities::MINECART,
            next_entity_id(),
            TRACK,
            Arc::downgrade(&world),
        ));
        add(&world, Arc::clone(&cart) as SharedEntity);
        let pig: SharedEntity = Arc::new(PigEntity::new(
            &vanilla_entities::PIG,
            next_entity_id(),
            TRACK,
            Arc::downgrade(&world),
        ));
        add(&world, Arc::clone(&pig));

        push_and_pickup_entities(cart.as_ref(), &world);

        assert!(!pig.is_passenger(), "a stopped cart took a passenger");
    }

    /// Vanilla parity: the `else` branch, which is the only thing a cart that
    /// carries nobody does -- carts shove each other rather than overlapping.
    #[test]
    fn a_cart_shoves_the_cart_it_is_sitting_in() {
        let world = track_world("cart_shoves_cart");
        let rolling = Arc::new(MinecartEntity::new(
            &vanilla_entities::MINECART,
            next_entity_id(),
            TRACK,
            Arc::downgrade(&world),
        ));
        add(&world, Arc::clone(&rolling) as SharedEntity);
        let parked: SharedEntity = Arc::new(MinecartEntity::new(
            &vanilla_entities::MINECART,
            next_entity_id(),
            TRACK + DVec3::new(0.3, 0.0, 0.0),
            Arc::downgrade(&world),
        ));
        add(&world, Arc::clone(&parked));

        push_and_pickup_entities(rolling.as_ref(), &world);

        assert!(
            parked.velocity().length_squared() > 0.0,
            "one cart passed straight through another"
        );
    }
}
