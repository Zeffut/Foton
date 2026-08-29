//! The five behaviors only a breeze runs.
//!
//! Vanilla splits these across `Shoot`, `ShootWhenStuck`, `Slide`, `LongJump`
//! and the `BreezeAi.SlideToTargetSink` inner class, all in the breeze package.
//! They are one fight -- circle, shoot, jump away, shoot again -- and only make
//! sense read together, so they are one file here.

use std::sync::Arc;

use foton_registry::blocks::block_state_ext::BlockStateExt as _;
use foton_registry::vanilla_fluid_tags::FluidTag;
use foton_registry::{
    sound_events, vanilla_attributes, vanilla_blocks, vanilla_entities, vanilla_mob_effects,
};
use foton_utils::BlockPos;
use glam::DVec3;

use super::breeze_util;
use crate::entity::ai::brain::behavior::{
    BrainContext, MemoryModuleId, MemoryStatus, MoveToTargetSink, Swim, TimedBehavior,
    calculate_jump_vector_for_angle,
};
use crate::entity::ai::brain::memory::{Unit, WalkTarget, memory_module_types};
use crate::entity::ai::goal::default_random_pos_away;
use crate::entity::dismount_helper::is_block_dangerous;
use crate::entity::entities::BreezeWindChargeEntity;
use crate::entity::{
    Entity, EntityAnchor, EntityPose, PathfinderMob, Projectile, SharedEntity, next_entity_id,
};
use crate::world::{ClipBlockShape, ClipFluid};

/// Vanilla parity: `BreezeAi.SPEED_MULTIPLIER_WHEN_SLIDING`.
pub(super) const SPEED_MULTIPLIER_WHEN_SLIDING: f64 = 0.6;

/// Vanilla parity: `BreezeAi.JUMP_CIRCLE_INNER_RADIUS`.
const JUMP_CIRCLE_INNER_RADIUS: f64 = 4.0;
/// Vanilla parity: `BreezeAi.JUMP_CIRCLE_MIDDLE_RADIUS`.
const JUMP_CIRCLE_MIDDLE_RADIUS: f64 = 8.0;
/// Vanilla parity: `Breeze.JUMP_CIRCLE_DISTANCE_Y`, the vertical half of the
/// inner-circle test.
const JUMP_CIRCLE_DISTANCE_Y: f64 = 10.0;

/// Vanilla parity: `Shoot.ATTACK_RANGE_MAX_SQRT`.
const ATTACK_RANGE_MAX_SQR: f64 = 256.0;
/// Vanilla parity: `Shoot.UNCERTAINTY_BASE`.
const UNCERTAINTY_BASE: i32 = 5;
/// Vanilla parity: `Shoot.UNCERTAINTY_MULTIPLIER`.
const UNCERTAINTY_MULTIPLIER: i32 = 4;
/// Vanilla parity: `Shoot.PROJECTILE_MOVEMENT_SCALE`.
const PROJECTILE_MOVEMENT_SCALE: f32 = 0.7;
/// Vanilla parity: `Shoot.SHOOT_INITIAL_DELAY_TICKS`.
const SHOOT_INITIAL_DELAY_TICKS: i32 = 15;
/// Vanilla parity: `Shoot.SHOOT_RECOVER_DELAY_TICKS`.
const SHOOT_RECOVER_DELAY_TICKS: i32 = 4;
/// Vanilla parity: `Shoot.SHOOT_COOLDOWN_TICKS`.
const SHOOT_COOLDOWN_TICKS: i64 = 10;
/// Vanilla parity: the `0.3` and `0.8` height fractions `Shoot.tick` aims at.
const AIM_HEIGHT_FRACTION: f64 = 0.3;
const AIM_HEIGHT_FRACTION_RIDDEN: f64 = 0.8;

/// Vanilla parity: the `60L` life of the `BREEZE_SHOOT` memory `ShootWhenStuck`
/// and `SlideToTargetSink` both set.
const SHOOT_MEMORY_TICKS: i64 = 60;
/// Vanilla parity: the `100L` life `LongJump` gives the same memory on landing.
const SHOOT_AFTER_JUMP_TICKS: i64 = 100;

/// Vanilla parity: `LongJump.REQUIRED_AIR_BLOCKS_ABOVE`.
const REQUIRED_AIR_BLOCKS_ABOVE: i32 = 4;
/// Vanilla parity: `LongJump.JUMP_COOLDOWN_TICKS`.
const JUMP_COOLDOWN_TICKS: i64 = 10;
/// Vanilla parity: `LongJump.JUMP_COOLDOWN_WHEN_HURT_TICKS`.
const JUMP_COOLDOWN_WHEN_HURT_TICKS: i64 = 2;
/// Vanilla parity: `LongJump.INHALING_DURATION_TICKS`.
const INHALING_DURATION_TICKS: i64 = 10;
/// Vanilla parity: `LongJump.MAX_JUMP_VELOCITY_MULTIPLIER`, which turns the
/// breeze's follow range into a launch-speed ceiling.
const MAX_JUMP_VELOCITY_MULTIPLIER: f64 = 0.058_333_334;
/// Vanilla parity: `LongJump.ALLOWED_ANGLES`, in degrees.
const ALLOWED_ANGLES: [i32; 5] = [40, 55, 60, 75, 80];
/// Vanilla parity: the `200` duration of the `LongJump` behavior.
const LONG_JUMP_DURATION: i32 = 200;
/// Vanilla parity: the `4.0F` of `LongJump.tooCloseForJump`.
const TOO_CLOSE_FOR_JUMP: f64 = 4.0;
/// Vanilla parity: the `10.0` reach of `LongJump.snapToSurface`.
const SURFACE_SNAP_REACH: f64 = 10.0;

/// Vanilla parity: `BreezeAi.SlideToTargetSink`'s `(20, 40)` timeout.
const SLIDE_SINK_MIN_TIMEOUT: i32 = 20;
const SLIDE_SINK_MAX_TIMEOUT: i32 = 40;

/// Vanilla parity: `Breeze.getFiringYPosition`, the height a wind charge leaves
/// the breeze at.
pub(super) fn firing_y_position(breeze: &dyn Entity) -> f64 {
    breeze.position().y + breeze.bounding_box().height() * 0.5 + 0.3
}

/// Vanilla parity: `Breeze.withinInnerCircleRange`, a cylinder four blocks
/// across and twenty tall centered on the breeze's own block.
fn within_inner_circle_range(breeze: &dyn PathfinderMob, target: DVec3) -> bool {
    let (x, y, z) = breeze.block_position().get_center();
    let dx = target.x - x;
    let dy = target.y - y;
    let dz = target.z - z;
    dz.mul_add(dz, dx * dx) < JUMP_CIRCLE_INNER_RADIUS * JUMP_CIRCLE_INNER_RADIUS
        && dy.abs() < JUMP_CIRCLE_DISTANCE_Y
}

/// Reads the attack target out of the brain as a living entity.
fn attack_target(ctx: &BrainContext<'_>) -> Option<SharedEntity> {
    let target = ctx
        .brain()
        .get_memory(memory_module_types::ATTACK_TARGET)?
        .get()?;
    target.as_living_entity()?;
    Some(target)
}

/// Inhales, then fires a wind charge.
///
/// Vanilla parity: `net.minecraft.world.entity.monster.breeze.Shoot`.
pub(super) struct Shoot {
    entry_condition: [(MemoryModuleId, MemoryStatus); 7],
}

impl Shoot {
    /// Creates the shoot behavior.
    pub(super) const fn new() -> Self {
        Self {
            entry_condition: [
                (
                    memory_module_types::ATTACK_TARGET.id(),
                    MemoryStatus::ValuePresent,
                ),
                (
                    memory_module_types::BREEZE_SHOOT_COOLDOWN.id(),
                    MemoryStatus::ValueAbsent,
                ),
                (
                    memory_module_types::BREEZE_SHOOT_CHARGING.id(),
                    MemoryStatus::ValueAbsent,
                ),
                (
                    memory_module_types::BREEZE_SHOOT_RECOVERING.id(),
                    MemoryStatus::ValueAbsent,
                ),
                (
                    memory_module_types::BREEZE_SHOOT.id(),
                    MemoryStatus::ValuePresent,
                ),
                (
                    memory_module_types::WALK_TARGET.id(),
                    MemoryStatus::ValueAbsent,
                ),
                (
                    memory_module_types::BREEZE_JUMP_TARGET.id(),
                    MemoryStatus::ValueAbsent,
                ),
            ],
        }
    }

    /// Vanilla parity: the private `Shoot.isTargetWithinRange`.
    fn is_target_within_range(breeze: &dyn PathfinderMob, target: &SharedEntity) -> bool {
        breeze.position().distance_squared(target.position()) < ATTACK_RANGE_MAX_SQR
    }

    /// Vanilla parity: the `Projectile.spawnProjectileUsingShoot` call of
    /// `Shoot.tick`.
    fn fire(ctx: &BrainContext<'_>, target: &SharedEntity) {
        let breeze = ctx.mob();
        let world = ctx.world();
        let position = breeze.position();
        let target_position = target.position();

        let aim_fraction = if target.is_passenger() {
            AIM_HEIGHT_FRACTION_RIDDEN
        } else {
            AIM_HEIGHT_FRACTION
        };
        let direction = DVec3::new(
            target_position.x - position.x,
            target
                .bounding_box()
                .height()
                .mul_add(aim_fraction, target_position.y)
                - firing_y_position(breeze),
            target_position.z - position.z,
        );

        let charge = Arc::new(BreezeWindChargeEntity::new(
            &vanilla_entities::BREEZE_WIND_CHARGE,
            next_entity_id(),
            DVec3::new(position.x, firing_y_position(breeze), position.z),
            Arc::downgrade(world),
        ));
        if let Some(owner) = world.get_entity_by_id(breeze.id()) {
            charge.set_owner_entity(Some(&owner));
        }

        // Vanilla parity: `5 - level.getDifficulty().getId() * 4`, which is the
        // only difficulty scaling a breeze has -- its aim tightens as the
        // difficulty rises, and on hard the inaccuracy is negative, which
        // vanilla's symmetric triangle roll treats the same as its magnitude.
        let uncertainty =
            UNCERTAINTY_BASE - i32::from(world.difficulty() as u8) * UNCERTAINTY_MULTIPLIER;
        #[expect(
            clippy::cast_precision_loss,
            reason = "a small whole-number inaccuracy handed straight to `shoot`"
        )]
        let uncertainty = uncertainty as f32;
        charge.shoot(direction, PROJECTILE_MOVEMENT_SCALE, uncertainty);

        let entity: SharedEntity = charge;
        if let Err(error) = world.try_add_entity(entity) {
            log::debug!("breeze failed to fire a wind charge: {error}");
            return;
        }
        breeze.play_sound(&sound_events::ENTITY_BREEZE_SHOOT, 1.5, 1.0);
    }
}

impl TimedBehavior for Shoot {
    fn entry_condition(&self) -> &[(MemoryModuleId, MemoryStatus)] {
        &self.entry_condition
    }

    fn duration(&self) -> (i32, i32) {
        let duration = SHOOT_INITIAL_DELAY_TICKS + 1 + SHOOT_RECOVER_DELAY_TICKS;
        (duration, duration)
    }

    /// Vanilla parity: `Shoot.checkExtraStartConditions`, which forgets the
    /// reason to shoot when the target has walked out of range.
    fn check_extra_start_conditions(&mut self, ctx: &BrainContext<'_>) -> bool {
        if ctx.mob().pose() != EntityPose::Standing {
            return false;
        }
        let Some(target) = attack_target(ctx) else {
            return false;
        };
        if Self::is_target_within_range(ctx.mob(), &target) {
            return true;
        }
        ctx.brain()
            .erase_memory(memory_module_types::BREEZE_SHOOT.id());
        false
    }

    fn can_still_use(&mut self, ctx: &BrainContext<'_>) -> bool {
        let brain = ctx.brain();
        brain.has_memory_value(memory_module_types::ATTACK_TARGET.id())
            && brain.has_memory_value(memory_module_types::BREEZE_SHOOT.id())
    }

    /// Vanilla parity: `Shoot.start`.
    fn start(&mut self, ctx: &BrainContext<'_>) {
        if attack_target(ctx).is_some() {
            ctx.mob().set_pose(EntityPose::Shooting);
        }
        ctx.brain().set_memory_with_expiry(
            memory_module_types::BREEZE_SHOOT_CHARGING,
            Unit,
            i64::from(SHOOT_INITIAL_DELAY_TICKS),
        );
        ctx.mob()
            .play_sound(&sound_events::ENTITY_BREEZE_INHALE, 1.0, 1.0);
    }

    /// Vanilla parity: `Shoot.stop`.
    fn stop(&mut self, ctx: &BrainContext<'_>) {
        if ctx.mob().pose() == EntityPose::Shooting {
            ctx.mob().set_pose(EntityPose::Standing);
        }
        ctx.brain().set_memory_with_expiry(
            memory_module_types::BREEZE_SHOOT_COOLDOWN,
            Unit,
            SHOOT_COOLDOWN_TICKS,
        );
        ctx.brain()
            .erase_memory(memory_module_types::BREEZE_SHOOT.id());
    }

    /// Vanilla parity: `Shoot.tick`, which fires on the one tick the charge is
    /// neither still building nor already recovering.
    fn tick(&mut self, ctx: &BrainContext<'_>) {
        let Some(target) = attack_target(ctx) else {
            return;
        };
        Entity::look_at(ctx.mob(), EntityAnchor::Eyes, target.position());

        let brain = ctx.brain();
        if brain.has_memory_value(memory_module_types::BREEZE_SHOOT_CHARGING.id())
            || brain.has_memory_value(memory_module_types::BREEZE_SHOOT_RECOVERING.id())
        {
            return;
        }

        brain.set_memory_with_expiry(
            memory_module_types::BREEZE_SHOOT_RECOVERING,
            Unit,
            i64::from(SHOOT_RECOVER_DELAY_TICKS),
        );
        Self::fire(ctx, &target);
    }

    fn debug_name(&self) -> &'static str {
        "Shoot"
    }
}

/// Gives a pinned breeze a reason to fire.
///
/// Vanilla parity: `net.minecraft.world.entity.monster.breeze.ShootWhenStuck`.
/// A breeze that is riding something, standing in water or drifting on
/// levitation cannot jump, so this hands it the shoot memory instead of letting
/// it do nothing at all.
pub(super) struct ShootWhenStuck {
    entry_condition: [(MemoryModuleId, MemoryStatus); 5],
}

impl ShootWhenStuck {
    /// Creates the behavior.
    pub(super) const fn new() -> Self {
        Self {
            entry_condition: [
                (
                    memory_module_types::ATTACK_TARGET.id(),
                    MemoryStatus::ValuePresent,
                ),
                (
                    memory_module_types::BREEZE_JUMP_INHALING.id(),
                    MemoryStatus::ValueAbsent,
                ),
                (
                    memory_module_types::BREEZE_JUMP_TARGET.id(),
                    MemoryStatus::ValueAbsent,
                ),
                (
                    memory_module_types::WALK_TARGET.id(),
                    MemoryStatus::ValueAbsent,
                ),
                (
                    memory_module_types::BREEZE_SHOOT.id(),
                    MemoryStatus::ValueAbsent,
                ),
            ],
        }
    }
}

impl TimedBehavior for ShootWhenStuck {
    fn entry_condition(&self) -> &[(MemoryModuleId, MemoryStatus)] {
        &self.entry_condition
    }

    fn check_extra_start_conditions(&mut self, ctx: &BrainContext<'_>) -> bool {
        let breeze = ctx.mob();
        breeze.is_passenger()
            || breeze.is_in_water()
            || breeze.has_mob_effect(vanilla_mob_effects::LEVITATION)
    }

    /// Vanilla parity: `ShootWhenStuck.canStillUse`, a flat `false` -- it hands
    /// over the memory and stops on the same tick.
    fn can_still_use(&mut self, _ctx: &BrainContext<'_>) -> bool {
        false
    }

    fn start(&mut self, ctx: &BrainContext<'_>) {
        ctx.brain().set_memory_with_expiry(
            memory_module_types::BREEZE_SHOOT,
            Unit,
            SHOOT_MEMORY_TICKS,
        );
    }

    fn debug_name(&self) -> &'static str {
        "ShootWhenStuck"
    }
}

/// Picks somewhere to slide to.
///
/// Vanilla parity: `net.minecraft.world.entity.monster.breeze.Slide`. Standing
/// too close, the breeze backs off; otherwise it circles, either to behind the
/// player or to the middle ring of its jump circle.
pub(super) struct Slide {
    entry_condition: [(MemoryModuleId, MemoryStatus); 4],
}

impl Slide {
    /// Creates the behavior.
    pub(super) const fn new() -> Self {
        Self {
            entry_condition: [
                (
                    memory_module_types::ATTACK_TARGET.id(),
                    MemoryStatus::ValuePresent,
                ),
                (
                    memory_module_types::WALK_TARGET.id(),
                    MemoryStatus::ValueAbsent,
                ),
                (
                    memory_module_types::BREEZE_JUMP_COOLDOWN.id(),
                    MemoryStatus::ValueAbsent,
                ),
                (
                    memory_module_types::BREEZE_SHOOT.id(),
                    MemoryStatus::ValueAbsent,
                ),
            ],
        }
    }

    /// Vanilla parity: the private `Slide.randomPointInMiddleCircle`, which
    /// pulls the breeze in to somewhere between four and eight blocks out.
    fn random_point_in_middle_circle(breeze: &dyn PathfinderMob, enemy: &SharedEntity) -> DVec3 {
        let direction = enemy.position() - breeze.position();
        // Vanilla parity: `Mth.lerp(nextDouble(), 8.0, 4.0)`, which runs from
        // the outer radius down to the inner one as the roll rises.
        let ring = (JUMP_CIRCLE_INNER_RADIUS - JUMP_CIRCLE_MIDDLE_RADIUS)
            .mul_add(rand::random::<f64>(), JUMP_CIRCLE_MIDDLE_RADIUS);
        let distance = direction.length() - ring;
        breeze.position() + direction.normalize_or_zero() * distance
    }
}

impl TimedBehavior for Slide {
    fn entry_condition(&self) -> &[(MemoryModuleId, MemoryStatus)] {
        &self.entry_condition
    }

    fn check_extra_start_conditions(&mut self, ctx: &BrainContext<'_>) -> bool {
        let breeze = ctx.mob();
        breeze.on_ground() && !breeze.is_in_water() && breeze.pose() == EntityPose::Standing
    }

    /// Vanilla parity: `Slide.start`.
    fn start(&mut self, ctx: &BrainContext<'_>) {
        let Some(enemy) = attack_target(ctx) else {
            return;
        };
        let breeze = ctx.mob();
        let enemy_position = enemy.position();

        let mut destination = None;
        if within_inner_circle_range(breeze, enemy_position) {
            // Vanilla parity: the `DefaultRandomPos.getPosAway(breeze, 5, 5, ...)`
            // retreat, taken only when it is visible and genuinely further from
            // the enemy than the breeze already is.
            if let Some(away) = default_random_pos_away(breeze, 5, 5, enemy_position)
                && breeze_util::has_line_of_sight(breeze, away)
                && enemy_position.distance_squared(away)
                    > enemy_position.distance_squared(breeze.position())
            {
                destination = Some(away);
            }
        }

        let destination = destination.unwrap_or_else(|| {
            if rand::random::<bool>() {
                enemy.as_living_entity().map_or_else(
                    || breeze.position(),
                    breeze_util::random_point_behind_target,
                )
            } else {
                Self::random_point_in_middle_circle(breeze, &enemy)
            }
        });

        ctx.brain().set_memory(
            memory_module_types::WALK_TARGET,
            WalkTarget::of_block(
                BlockPos::containing(destination.x, destination.y, destination.z),
                SPEED_MULTIPLIER_WHEN_SLIDING,
                1,
            ),
        );
    }

    fn debug_name(&self) -> &'static str {
        "Slide"
    }
}

/// Crouches, then throws the breeze in an arc onto a spot behind its target.
///
/// Vanilla parity: `net.minecraft.world.entity.monster.breeze.LongJump`.
pub(super) struct LongJump {
    entry_condition: [(MemoryModuleId, MemoryStatus); 7],
}

impl LongJump {
    /// Creates the behavior.
    pub(super) const fn new() -> Self {
        Self {
            entry_condition: [
                (
                    memory_module_types::ATTACK_TARGET.id(),
                    MemoryStatus::ValuePresent,
                ),
                (
                    memory_module_types::BREEZE_JUMP_COOLDOWN.id(),
                    MemoryStatus::ValueAbsent,
                ),
                (
                    memory_module_types::BREEZE_JUMP_INHALING.id(),
                    MemoryStatus::Registered,
                ),
                (
                    memory_module_types::BREEZE_JUMP_TARGET.id(),
                    MemoryStatus::Registered,
                ),
                (
                    memory_module_types::BREEZE_SHOOT.id(),
                    MemoryStatus::ValueAbsent,
                ),
                (
                    memory_module_types::WALK_TARGET.id(),
                    MemoryStatus::ValueAbsent,
                ),
                (
                    memory_module_types::BREEZE_LEAVING_WATER.id(),
                    MemoryStatus::Registered,
                ),
            ],
        }
    }

    /// Vanilla parity: the public `LongJump.canRun`, which both starts the
    /// behavior and, as a side effect, chooses the landing block.
    pub(super) fn can_run(ctx: &BrainContext<'_>) -> bool {
        let breeze = ctx.mob();
        if !breeze.on_ground() && !breeze.is_in_water() {
            return false;
        }
        if Swim::should_swim(breeze) {
            return false;
        }
        let brain = ctx.brain();
        if brain.check_memory(
            memory_module_types::BREEZE_JUMP_TARGET.id(),
            MemoryStatus::ValuePresent,
        ) {
            return true;
        }

        let Some(target) = attack_target(ctx) else {
            return false;
        };
        if Self::out_of_aggro_range(breeze, &target) {
            brain.erase_memory(memory_module_types::ATTACK_TARGET.id());
            return false;
        }
        if Self::too_close_for_jump(breeze, &target) {
            return false;
        }
        if !Self::can_jump_from_current_position(ctx) {
            return false;
        }

        let Some(living) = target.as_living_entity() else {
            return false;
        };
        let Some(target_pos) =
            Self::snap_to_surface(ctx, breeze_util::random_point_behind_target(living))
        else {
            return false;
        };

        let below = ctx.world().get_block_state(target_pos.below());
        if is_block_dangerous(breeze, below) {
            return false;
        }

        let (x, y, z) = target_pos.get_center();
        let (ax, ay, az) = target_pos.above_n(REQUIRED_AIR_BLOCKS_ABOVE).get_center();
        if !breeze_util::has_line_of_sight(breeze, DVec3::new(x, y, z))
            && !breeze_util::has_line_of_sight(breeze, DVec3::new(ax, ay, az))
        {
            return false;
        }

        brain.set_memory(memory_module_types::BREEZE_JUMP_TARGET, target_pos);
        true
    }

    /// Vanilla parity: the private `LongJump.outOfAggroRange`.
    fn out_of_aggro_range(breeze: &dyn PathfinderMob, target: &SharedEntity) -> bool {
        let follow_range = breeze
            .attributes()
            .lock()
            .get_value(vanilla_attributes::FOLLOW_RANGE)
            .unwrap_or(0.0);
        breeze.position().distance_squared(target.position()) > follow_range * follow_range
    }

    /// Vanilla parity: the private `LongJump.tooCloseForJump`.
    fn too_close_for_jump(breeze: &dyn PathfinderMob, target: &SharedEntity) -> bool {
        breeze.position().distance(target.position()) - TOO_CLOSE_FOR_JUMP <= 0.0
    }

    /// Vanilla parity: the private `LongJump.canJumpFromCurrentPosition`, which
    /// wants four blocks of headroom and refuses to launch off honey.
    fn can_jump_from_current_position(ctx: &BrainContext<'_>) -> bool {
        let world = ctx.world();
        let current = ctx.mob().block_position();
        if world.get_block_state(current).get_block() == &vanilla_blocks::HONEY_BLOCK {
            return false;
        }

        (1..=REQUIRED_AIR_BLOCKS_ABOVE).all(|offset| {
            let above = current.above_n(offset);
            let state = world.get_block_state(above);
            state.is_air() || state.get_fluid_state().fluid_id.has_tag(&FluidTag::WATER)
        })
    }

    /// Vanilla parity: the private `LongJump.snapToSurface`, which drops a ray
    /// ten blocks down and, failing that, ten blocks up.
    fn snap_to_surface(ctx: &BrainContext<'_>, target: DVec3) -> Option<BlockPos> {
        let world = ctx.world();
        for reach in [-SURFACE_SNAP_REACH, SURFACE_SNAP_REACH] {
            let end = target + DVec3::new(0.0, reach, 0.0);
            let hit = world.clip(target, end, ClipBlockShape::Collider, ClipFluid::None);
            if !hit.is_miss() {
                let location = hit.location;
                return Some(BlockPos::containing(location.x, location.y, location.z).above());
            }
        }
        None
    }

    /// Vanilla parity: the private `LongJump.isFinishedInhaling`.
    fn is_finished_inhaling(ctx: &BrainContext<'_>) -> bool {
        !ctx.brain()
            .has_memory_value(memory_module_types::BREEZE_JUMP_INHALING.id())
            && ctx.mob().pose() == EntityPose::Inhaling
    }

    /// Vanilla parity: the private `LongJump.isFinishedJumping`.
    fn is_finished_jumping(ctx: &BrainContext<'_>) -> bool {
        let breeze = ctx.mob();
        if breeze.pose() != EntityPose::LongJumping {
            return false;
        }
        let landed_in_water = breeze.is_in_water()
            && ctx.brain().check_memory(
                memory_module_types::BREEZE_LEAVING_WATER.id(),
                MemoryStatus::ValueAbsent,
            );
        breeze.on_ground() || landed_in_water
    }

    /// Vanilla parity: the private `LongJump.calculateOptimalJumpVector`, which
    /// tries the five allowed angles in a shuffled order and takes the first
    /// that reaches.
    fn calculate_optimal_jump_vector(
        breeze: &dyn PathfinderMob,
        target_pos: DVec3,
    ) -> Option<DVec3> {
        let follow_range = breeze
            .attributes()
            .lock()
            .get_value(vanilla_attributes::FOLLOW_RANGE)
            .unwrap_or(0.0);
        let max_jump_velocity = MAX_JUMP_VELOCITY_MULTIPLIER * follow_range;

        let mut angles = ALLOWED_ANGLES;
        for index in (1..angles.len()).rev() {
            angles.swap(index, rand::random_range(0..=index));
        }

        for angle in angles {
            let Some(velocity) = calculate_jump_vector_for_angle(
                breeze,
                target_pos,
                max_jump_velocity,
                angle,
                false,
            ) else {
                continue;
            };
            if !breeze.has_mob_effect(vanilla_mob_effects::JUMP_BOOST) {
                return Some(velocity);
            }
            let lift = velocity.normalize_or_zero().y * f64::from(breeze.get_jump_boost_power());
            return Some(velocity + DVec3::new(0.0, lift, 0.0));
        }
        None
    }
}

impl TimedBehavior for LongJump {
    fn entry_condition(&self) -> &[(MemoryModuleId, MemoryStatus)] {
        &self.entry_condition
    }

    fn duration(&self) -> (i32, i32) {
        (LONG_JUMP_DURATION, LONG_JUMP_DURATION)
    }

    fn check_extra_start_conditions(&mut self, ctx: &BrainContext<'_>) -> bool {
        Self::can_run(ctx)
    }

    fn can_still_use(&mut self, ctx: &BrainContext<'_>) -> bool {
        ctx.mob().pose() != EntityPose::Standing
            && !ctx
                .brain()
                .has_memory_value(memory_module_types::BREEZE_JUMP_COOLDOWN.id())
    }

    /// Vanilla parity: `LongJump.start`.
    fn start(&mut self, ctx: &BrainContext<'_>) {
        let brain = ctx.brain();
        if brain.check_memory(
            memory_module_types::BREEZE_JUMP_INHALING.id(),
            MemoryStatus::ValueAbsent,
        ) {
            brain.set_memory_with_expiry(
                memory_module_types::BREEZE_JUMP_INHALING,
                Unit,
                INHALING_DURATION_TICKS,
            );
        }

        let breeze = ctx.mob();
        breeze.set_pose(EntityPose::Inhaling);
        breeze.play_sound(&sound_events::ENTITY_BREEZE_CHARGE, 1.0, 1.0);
        if let Some(target_pos) = brain.get_memory(memory_module_types::BREEZE_JUMP_TARGET) {
            let (x, y, z) = target_pos.get_center();
            Entity::look_at(breeze, EntityAnchor::Eyes, DVec3::new(x, y, z));
        }
    }

    /// Vanilla parity: `LongJump.tick`, which launches once the inhale is over
    /// and lands the breeze once the arc ends.
    fn tick(&mut self, ctx: &BrainContext<'_>) {
        let breeze = ctx.mob();
        let brain = ctx.brain();
        let in_water = breeze.is_in_water();
        if !in_water
            && brain.check_memory(
                memory_module_types::BREEZE_LEAVING_WATER.id(),
                MemoryStatus::ValuePresent,
            )
        {
            brain.erase_memory(memory_module_types::BREEZE_LEAVING_WATER.id());
        }

        if Self::is_finished_inhaling(ctx) {
            let velocity = brain
                .get_memory(memory_module_types::BREEZE_JUMP_TARGET)
                .and_then(|target_pos| {
                    let (x, y, z) = target_pos.get_bottom_center();
                    Self::calculate_optimal_jump_vector(breeze, DVec3::new(x, y, z))
                });
            let Some(velocity) = velocity else {
                breeze.set_pose(EntityPose::Standing);
                return;
            };

            if in_water {
                brain.set_memory(memory_module_types::BREEZE_LEAVING_WATER, Unit);
            }

            breeze.play_sound(&sound_events::ENTITY_BREEZE_JUMP, 1.0, 1.0);
            breeze.set_pose(EntityPose::LongJumping);
            let (_, pitch) = breeze.rotation();
            breeze.set_rotation((breeze.y_body_rot(), pitch));
            breeze.set_discard_friction(true);
            breeze.set_velocity(velocity);
            breeze.mark_velocity_sync();
            return;
        }

        if !Self::is_finished_jumping(ctx) {
            return;
        }

        breeze.play_sound(&sound_events::ENTITY_BREEZE_LAND, 1.0, 1.0);
        breeze.set_pose(EntityPose::Standing);
        breeze.set_discard_friction(false);
        let was_hurt = brain.has_memory_value(memory_module_types::HURT_BY.id());
        brain.set_memory_with_expiry(
            memory_module_types::BREEZE_JUMP_COOLDOWN,
            Unit,
            if was_hurt {
                JUMP_COOLDOWN_WHEN_HURT_TICKS
            } else {
                JUMP_COOLDOWN_TICKS
            },
        );
        brain.set_memory_with_expiry(
            memory_module_types::BREEZE_SHOOT,
            Unit,
            SHOOT_AFTER_JUMP_TICKS,
        );
    }

    /// Vanilla parity: `LongJump.stop`.
    fn stop(&mut self, ctx: &BrainContext<'_>) {
        let breeze = ctx.mob();
        let pose = breeze.pose();
        if pose == EntityPose::LongJumping || pose == EntityPose::Inhaling {
            breeze.set_pose(EntityPose::Standing);
        }
        let brain = ctx.brain();
        brain.erase_memory(memory_module_types::BREEZE_JUMP_TARGET.id());
        brain.erase_memory(memory_module_types::BREEZE_JUMP_INHALING.id());
        brain.erase_memory(memory_module_types::BREEZE_LEAVING_WATER.id());
    }

    fn debug_name(&self) -> &'static str {
        "LongJump"
    }
}

/// Walks the breeze to its walk target in the sliding pose.
///
/// Vanilla parity: `BreezeAi.SlideToTargetSink`, a `MoveToTargetSink` that
/// switches the pose on the way in and, on the way out, hands the breeze a
/// reason to shoot at whatever it is still fighting.
pub(super) struct SlideToTargetSink {
    inner: MoveToTargetSink,
}

impl SlideToTargetSink {
    /// Creates the sink.
    pub(super) const fn new() -> Self {
        Self {
            inner: MoveToTargetSink::with_timeout(SLIDE_SINK_MIN_TIMEOUT, SLIDE_SINK_MAX_TIMEOUT),
        }
    }
}

impl TimedBehavior for SlideToTargetSink {
    fn entry_condition(&self) -> &[(MemoryModuleId, MemoryStatus)] {
        self.inner.entry_condition()
    }

    fn duration(&self) -> (i32, i32) {
        self.inner.duration()
    }

    fn times_out(&self) -> bool {
        self.inner.times_out()
    }

    fn check_extra_start_conditions(&mut self, ctx: &BrainContext<'_>) -> bool {
        self.inner.check_extra_start_conditions(ctx)
    }

    fn can_still_use(&mut self, ctx: &BrainContext<'_>) -> bool {
        self.inner.can_still_use(ctx)
    }

    fn start(&mut self, ctx: &BrainContext<'_>) {
        self.inner.start(ctx);
        ctx.mob()
            .make_sound(Some(&sound_events::ENTITY_BREEZE_SLIDE));
        ctx.mob().set_pose(EntityPose::Sliding);
    }

    fn stop_with_timeout(&mut self, ctx: &BrainContext<'_>, timed_out: bool) {
        self.inner.stop_with_timeout(ctx, timed_out);
        ctx.mob().set_pose(EntityPose::Standing);
        if ctx
            .brain()
            .has_memory_value(memory_module_types::ATTACK_TARGET.id())
        {
            ctx.brain().set_memory_with_expiry(
                memory_module_types::BREEZE_SHOOT,
                Unit,
                SHOOT_MEMORY_TICKS,
            );
        }
    }

    fn debug_name(&self) -> &'static str {
        "SlideToTargetSink"
    }
}
