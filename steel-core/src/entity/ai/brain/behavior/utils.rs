//! Shared helpers behaviors reach for.
//!
//! Vanilla parity: `net.minecraft.world.entity.ai.behavior.BehaviorUtils`.

use std::ptr;
use std::sync::Arc;

use glam::DVec3;
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::item_stack::ItemStack;
use steel_utils::BlockPos;
use steel_utils::types::InteractionHand;
use uuid::Uuid;

use crate::behavior::ITEM_BEHAVIORS;
use crate::entity::ai::brain::Brain;
use crate::entity::ai::brain::memory::{
    EntityMemory, MemoryModuleType, WalkTarget, memory_module_types,
};
use crate::entity::ai::brain::position_tracker::PositionTracker;
use crate::entity::{Entity, LivingEntity, Mob, SharedEntity};
use crate::world::World;

/// Vanilla parity: the `Vec3(0.3F, 0.3F, 0.3F)` of the two-argument `throwItem`.
const DEFAULT_THROW_VELOCITY: DVec3 = DVec3::new(0.3, 0.3, 0.3);
/// Vanilla parity: the `0.3F` hand offset below the eye of the two-argument `throwItem`.
const DEFAULT_HAND_Y_DISTANCE_FROM_EYE: f64 = 0.3;

/// Points the body at `target` and sends it walking there.
///
/// Vanilla parity: `BehaviorUtils.setWalkAndLookTargetMemories`.
pub(crate) fn set_walk_and_look_target_memories(
    brain: &Brain,
    target: PositionTracker,
    speed_modifier: f64,
    close_enough_dist: i32,
) {
    brain.set_memory(
        memory_module_types::WALK_TARGET,
        WalkTarget::new(target.clone(), speed_modifier, close_enough_dist),
    );
    brain.set_memory(memory_module_types::LOOK_TARGET, target);
}

/// Makes two bodies stare at each other and walk together.
///
/// Vanilla parity: `BehaviorUtils.lockGazeAndWalkToEachOther`.
pub(crate) fn lock_gaze_and_walk_to_each_other(
    first: &SharedEntity,
    second: &SharedEntity,
    speed_modifier: f64,
    close_enough_dist: i32,
) {
    walk_and_look_at(first, second, speed_modifier, close_enough_dist);
    walk_and_look_at(second, first, speed_modifier, close_enough_dist);
}

fn walk_and_look_at(
    walker: &SharedEntity,
    target: &SharedEntity,
    speed_modifier: f64,
    close_enough_dist: i32,
) {
    let Some(brain) = walker.as_mob().and_then(Mob::brain) else {
        return;
    };
    set_walk_and_look_target_memories(
        brain,
        PositionTracker::of_entity(target, true),
        speed_modifier,
        close_enough_dist,
    );
}

/// Whether the body currently sees `target`.
///
/// Vanilla parity: `BehaviorUtils.canSee`, which asks the brain rather than
/// recomputing the ray, so a behavior agrees with the sensor that fed it.
pub(crate) fn can_see(brain: &Brain, target: &dyn Entity) -> bool {
    brain
        .get_memory(memory_module_types::NEAREST_VISIBLE_LIVING_ENTITIES)
        .is_some_and(|visible| visible.contains_entity(target.id()))
}

/// Whether an entity memory still names a live, visible mob of one type.
///
/// Vanilla parity: `BehaviorUtils.targetIsValid(brain, memory, EntityType)`.
pub(crate) fn target_is_valid(
    brain: &Brain,
    memory: MemoryModuleType<EntityMemory>,
    entity_type: EntityTypeRef,
) -> bool {
    let Some(target) = brain.get_memory(memory).and_then(|memory| memory.get()) else {
        return false;
    };
    let Some(living) = target.as_living_entity() else {
        return false;
    };
    is_of_type(target.as_ref(), entity_type)
        && LivingEntity::is_alive(living)
        && can_see(brain, target.as_ref())
}

/// Returns whichever of the two candidates is nearer the body.
///
/// Vanilla parity: `BehaviorUtils.getNearestTarget`.
pub(crate) fn nearest_target(
    body: &dyn Entity,
    current: Option<SharedEntity>,
    candidate: SharedEntity,
) -> SharedEntity {
    let Some(current) = current else {
        return candidate;
    };
    let body_position = body.position();
    if body_position.distance_squared(current.position())
        < body_position.distance_squared(candidate.position())
    {
        current
    } else {
        candidate
    }
}

/// Whether `other` is much further off than the target the body already has.
///
/// Vanilla parity: `BehaviorUtils.isOtherTargetMuchFurtherAwayThanCurrentAttackTarget`.
pub(crate) fn is_other_target_much_further_away_than_current_attack_target(
    brain: &Brain,
    body: &dyn Entity,
    other: &dyn Entity,
    how_much_further_away: f64,
) -> bool {
    let Some(current) = brain
        .get_memory(memory_module_types::ATTACK_TARGET)
        .and_then(|memory| memory.get())
    else {
        return false;
    };
    let body_position = body.position();
    let dist_to_current = body_position.distance_squared(current.position());
    let dist_to_other = body_position.distance_squared(other.position());
    dist_to_other > dist_to_current + how_much_further_away * how_much_further_away
}

/// Resolves a memory that stores only a UUID back into a living entity.
///
/// Vanilla parity: `BehaviorUtils.getLivingEntityFromUUIDMemory`.
pub(crate) fn living_entity_from_uuid_memory(
    world: &World,
    brain: &Brain,
    memory: MemoryModuleType<Uuid>,
) -> Option<SharedEntity> {
    let uuid = brain.get_memory(memory)?;
    let entity = world.get_entity_by_uuid(&uuid)?;
    entity.as_living_entity()?;
    Some(entity)
}

/// Tosses `item` out of the thrower's hand toward `target_pos`.
///
/// Vanilla parity: `BehaviorUtils.throwItem`.
pub(crate) fn throw_item(thrower: &dyn LivingEntity, item: ItemStack, target_pos: DVec3) {
    throw_item_with_velocity(
        thrower,
        item,
        target_pos,
        DEFAULT_THROW_VELOCITY,
        DEFAULT_HAND_Y_DISTANCE_FROM_EYE,
    );
}

/// Vanilla parity: the five-argument `BehaviorUtils.throwItem`.
pub(crate) fn throw_item_with_velocity(
    thrower: &dyn LivingEntity,
    item: ItemStack,
    target_pos: DVec3,
    throw_velocity: DVec3,
    hand_y_distance_from_eye: f64,
) {
    if item.is_empty() {
        return;
    }
    let Some(world) = thrower.level() else {
        return;
    };
    let position = thrower.position();
    let hand_position = DVec3::new(
        position.x,
        thrower.get_eye_y() - hand_y_distance_from_eye,
        position.z,
    );
    let direction = (target_pos - position).normalize_or_zero() * throw_velocity;
    let Some(item_entity) = world.spawn_item_with_velocity(hand_position, item, direction) else {
        return;
    };
    item_entity.set_thrower(thrower.uuid());
}

/// The hand the body is holding `matches` in, preferring the main hand.
///
/// Vanilla parity: `ProjectileUtil.getWeaponHoldingHand`.
pub(crate) fn weapon_holding_hand(
    body: &dyn LivingEntity,
    mut matches: impl FnMut(&ItemStack) -> bool,
) -> InteractionHand {
    if matches(&body.get_item_in_hand(InteractionHand::MainHand)) {
        InteractionHand::MainHand
    } else {
        InteractionHand::OffHand
    }
}

/// Remembers `entity` in the shape the brain stores entity memories in.
pub(crate) fn remember(entity: &SharedEntity) -> EntityMemory {
    EntityMemory::new(entity)
}

/// Whether `first` and `second` are in the same world.
pub(crate) fn in_same_world(first: &dyn Entity, second: &Arc<World>) -> bool {
    first
        .level()
        .is_some_and(|level| Arc::ptr_eq(&level, second))
}

/// Whether `body` is close enough to attack `target` with what it holds.
///
/// Vanilla parity: `BehaviorUtils.isWithinAttackRange`. A mob holding a
/// projectile weapon it can use judges by that weapon's range; anything else
/// judges by melee reach, which is why a piglin with a sword closes and a
/// piglin with a crossbow does not.
pub(crate) fn is_within_attack_range(
    body: &dyn Mob,
    target: &dyn LivingEntity,
    projectile_attack_range_margin: i32,
) -> bool {
    let main_hand = body.get_item_in_hand(InteractionHand::MainHand);
    let projectile_range = ITEM_BEHAVIORS
        .get_behavior(main_hand.item())
        .default_projectile_range()
        .filter(|_| body.can_use_non_melee_weapon(&main_hand));

    let Some(range) = projectile_range else {
        return body.is_within_melee_attack_range(target);
    };
    let max_allowed = f64::from(range - projectile_attack_range_margin);
    body.position().distance_squared(target.position()) < max_allowed * max_allowed
}

/// Vanilla parity: `Entity.is(EntityType)`, which compares registry identity.
pub(crate) fn is_of_type(entity: &dyn Entity, entity_type: EntityTypeRef) -> bool {
    ptr::eq(entity.entity_type(), entity_type)
}

/// Vanilla parity: `Vec3i.distSqr(Vec3i)`, on raw block coordinates.
pub(crate) fn block_distance_squared(from: BlockPos, to: BlockPos) -> f64 {
    let dx = f64::from(from.x() - to.x());
    let dy = f64::from(from.y() - to.y());
    let dz = f64::from(from.z() - to.z());
    dz.mul_add(dz, dx.mul_add(dx, dy * dy))
}

/// Vanilla parity: `Vec3i.closerThan(Vec3i, double)`.
pub(crate) fn block_closer_than(from: BlockPos, to: BlockPos, distance: f64) -> bool {
    block_distance_squared(from, to) < distance * distance
}

/// Vanilla parity: `Vec3i.closerToCenterThan(Position, double)`, which measures
/// from the block's center rather than its corner.
pub(crate) fn block_closer_to_center_than(pos: BlockPos, position: DVec3, distance: f64) -> bool {
    let (x, y, z) = pos.get_center();
    DVec3::new(x, y, z).distance_squared(position) < distance * distance
}
