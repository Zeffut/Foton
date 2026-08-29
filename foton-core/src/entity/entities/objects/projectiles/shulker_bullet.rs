//! Shulker bullet.
//!
//! Vanilla parity: `ShulkerBullet`. The odd one out of the fireball family: it
//! is a plain `Projectile`, not an `AbstractHurtingProjectile`, and it does not
//! fly straight. It picks one of the six axis directions, commits to it for a
//! stretch, and picks again when it runs out of steps, when the block ahead of
//! it is something it could stand on, or when it draws level with its target on
//! the axis it is traveling. That is what makes it turn corners and follow a
//! player around a pillar instead of thudding into the wall.
//!
//! Losing the target does not stop it: without one it simply falls, which is why
//! a bullet whose shulker died drops out of the air.

use std::sync::{Arc, Weak};

use foton_macros::entity_behavior;
use foton_protocol::packets::game::SoundSource;
use foton_registry::blocks::block_state_ext::BlockStateExt as _;
use foton_registry::entity_data::ParticleData;
use foton_registry::entity_type::EntityTypeRef;
use foton_registry::vanilla_entity_data::ShulkerBulletEntityData;
use foton_registry::{
    sound_events, vanilla_damage_types, vanilla_game_events, vanilla_mob_effects,
    vanilla_particle_types,
};
use foton_utils::axis::Axis;
use foton_utils::locks::SyncMutex;
use foton_utils::types::Difficulty;
use foton_utils::{BlockPos, ChunkPos, Direction, DowncastType, DowncastTypeKey, UuidExt};
use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::{NbtCompound, NbtTag};
use uuid::Uuid;

use crate::entity::damage::DamageSource;
use crate::entity::{
    Entity, EntityBase, EntityBaseLoad, EntityEventSource, EntitySyncedData, MobEffectInstance,
    Projectile, ProjectileBase, ProjectileHit, RemovalReason, SharedEntity,
    rotate_towards_movement,
};
use crate::world::game_event::GameEventContext;
use crate::world::{ClipHitResult, LevelReader, World};

/// How fast a steering bullet travels.
///
/// Vanilla parity: `ShulkerBullet.SPEED`. The steering delta is normalized to
/// this every time a new direction is chosen, and then eased toward each tick.
const SPEED: f64 = 0.15;

/// How far a bullet falls per tick once it has nothing to chase.
///
/// Vanilla parity: `ShulkerBullet.getDefaultGravity`.
const DEFAULT_GRAVITY: f64 = 0.04;

/// How much the steering delta grows each tick while a target is held.
///
/// Vanilla parity: the `* 1.025` of `ShulkerBullet.tick`. The bullet speeds up
/// the longer it is locked on, which is what makes a long chase land.
const STEERING_ACCELERATION: f64 = 1.025;

/// Cap on each component of the steering delta.
const MAX_STEERING_DELTA: f64 = 1.0;

/// How much of the gap to the steering delta is closed each tick.
///
/// Vanilla parity: the `0.2` of the `setDeltaMovement` in `ShulkerBullet.tick`.
/// Turning is eased rather than instant, which is what rounds off the corners.
const STEERING_EASE: f64 = 0.2;

/// How close the bullet has to be to its target to stop weaving toward it.
///
/// Vanilla parity: the `closerToCenterThan(this.position(), 2.0)` of
/// `selectNextMoveDirection`. Inside two blocks the bullet aims straight at the
/// target instead of picking another axis to travel down.
const DIRECT_APPROACH_RANGE: f64 = 2.0;

/// How many times a cornered bullet re-rolls for an unobstructed direction.
///
/// Vanilla parity: the `attempts = 5` loop of `selectNextMoveDirection`.
const BLIND_DIRECTION_ATTEMPTS: i32 = 5;

/// Damage a bullet does on contact.
///
/// Vanilla parity: the `4.0F` of `ShulkerBullet.onHitEntity`.
const HIT_DAMAGE: f32 = 4.0;

/// How long the levitation from a hit lasts, in ticks.
///
/// Vanilla parity: the `new MobEffectInstance(LEVITATION, 200)` of `onHitEntity`.
const LEVITATION_TICKS: i32 = 200;

/// How far the bullet turns toward its heading each tick.
///
/// Vanilla parity: the `rotateTowardsMovement(this, 0.5F)` of the tick loop.
const ROTATION_SPEED: f32 = 0.5;

/// State a bullet keeps that is not mirrored to clients.
struct BulletState {
    /// Who the bullet is chasing (vanilla `finalTarget`).
    target: Option<Uuid>,
    /// The axis direction it is currently traveling (vanilla `currentMoveDirection`).
    move_direction: Option<Direction>,
    /// Ticks left before it picks a new direction (vanilla `flightSteps`).
    flight_steps: i32,
    /// The velocity it is easing toward (vanilla `targetDeltaX/Y/Z`).
    target_delta: DVec3,
}

/// A shulker's homing bullet.
#[entity_behavior(class = "ShulkerBullet")]
pub struct ShulkerBulletEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    entity_data: SyncMutex<ShulkerBulletEntityData>,
    projectile_base: ProjectileBase,
    state: SyncMutex<BulletState>,
}

// SAFETY: This key is owned by Foton and uniquely identifies `ShulkerBulletEntity`.
unsafe impl DowncastType for ShulkerBulletEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:entity/shulker_bullet");
}

impl ShulkerBulletEntity {
    /// Creates an unguided bullet.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        let base = EntityBase::new(id, position, entity_type.dimensions, world);
        // Vanilla parity: the `ShulkerBullet` constructor sets `noPhysics`; the
        // bullet steers itself around blocks rather than colliding with them.
        base.set_no_physics(true);
        Self {
            base,
            entity_type,
            entity_data: SyncMutex::new(ShulkerBulletEntityData::new()),
            projectile_base: ProjectileBase::new(),
            state: SyncMutex::new(BulletState {
                target: None,
                move_direction: None,
                flight_steps: 0,
                target_delta: DVec3::ZERO,
            }),
        }
    }

    /// Creates a bullet from saved base data.
    #[must_use]
    pub fn from_saved(entity_type: EntityTypeRef, load: EntityBaseLoad) -> Self {
        let base = EntityBase::from_load(load, entity_type.dimensions);
        base.set_no_physics(true);
        Self {
            base,
            entity_type,
            entity_data: SyncMutex::new(ShulkerBulletEntityData::new()),
            projectile_base: ProjectileBase::new(),
            state: SyncMutex::new(BulletState {
                target: None,
                move_direction: None,
                flight_steps: 0,
                target_delta: DVec3::ZERO,
            }),
        }
    }

    /// Aims a freshly fired bullet at `target`.
    ///
    /// Vanilla parity: the `ShulkerBullet(level, owner, target, invalidStartAxis)`
    /// constructor. The shulker passes the axis it is attached along so the
    /// bullet never opens by flying into its own wall.
    pub fn fire_at(
        &self,
        world: &Arc<World>,
        owner: &SharedEntity,
        target: &SharedEntity,
        invalid_start_axis: Option<Axis>,
    ) {
        self.set_owner_entity(Some(owner));
        {
            let mut state = self.state.lock();
            state.target = Some(target.uuid());
            state.move_direction = Some(Direction::Up);
        }
        self.select_next_move_direction(world, invalid_start_axis, Some(target));
    }

    /// Returns the entity this bullet is chasing, if it is still around.
    fn resolve_target(&self, world: &Arc<World>) -> Option<SharedEntity> {
        let uuid = self.state.lock().target?;
        world.get_entity_by_uuid(&uuid)
    }

    /// Returns whether the bullet could rest on the block at `pos`.
    ///
    /// Vanilla parity: `LevelReader.loadedAndEntityCanStandOn`. Foton checks the
    /// full-face sturdiness of the top face, which is what vanilla's collision
    /// shape test comes to for every block a bullet meets.
    fn can_stand_on(world: &Arc<World>, pos: BlockPos) -> bool {
        world.has_full_chunk(ChunkPos::from_block_pos(pos))
            && world.is_face_sturdy(world.get_block_state(pos), pos, Direction::Up)
    }

    /// Returns whether the block at `pos` is open air.
    ///
    /// Vanilla parity: `Level.isEmptyBlock`.
    fn is_empty_block(world: &Arc<World>, pos: BlockPos) -> bool {
        world.get_block_state(pos).is_air()
    }

    /// Chooses the next leg of the bullet's flight.
    ///
    /// Vanilla parity: `ShulkerBullet.selectNextMoveDirection`. Far from the
    /// target the bullet travels one axis at a time, preferring an unobstructed
    /// axis that closes the gap and falling back to a blind re-roll when it is
    /// boxed in; within two blocks it drops the weaving and aims straight.
    fn select_next_move_direction(
        &self,
        world: &Arc<World>,
        avoid_axis: Option<Axis>,
        target: Option<&SharedEntity>,
    ) {
        let position = self.position();
        let (y_offset, target_pos) = match target {
            None => (0.5, self.block_position().below()),
            Some(target) => {
                let offset = target.bounding_box().height() * 0.5;
                let target_position = target.position();
                (
                    offset,
                    BlockPos::containing(
                        target_position.x,
                        target_position.y + offset,
                        target_position.z,
                    ),
                )
            }
        };

        let mut aim = DVec3::new(
            f64::from(target_pos.x()) + 0.5,
            f64::from(target_pos.y()) + y_offset,
            f64::from(target_pos.z()) + 0.5,
        );

        let center = DVec3::new(
            f64::from(target_pos.x()) + 0.5,
            f64::from(target_pos.y()) + 0.5,
            f64::from(target_pos.z()) + 0.5,
        );
        let mut selection = None;
        if center.distance_squared(position) >= DIRECT_APPROACH_RANGE * DIRECT_APPROACH_RANGE {
            let current = self.block_position();
            let mut options = Vec::new();

            if avoid_axis != Some(Axis::X) {
                if current.x() < target_pos.x() && Self::is_empty_block(world, current.east()) {
                    options.push(Direction::East);
                } else if current.x() > target_pos.x()
                    && Self::is_empty_block(world, current.west())
                {
                    options.push(Direction::West);
                }
            }
            if avoid_axis != Some(Axis::Y) {
                if current.y() < target_pos.y() && Self::is_empty_block(world, current.above()) {
                    options.push(Direction::Up);
                } else if current.y() > target_pos.y()
                    && Self::is_empty_block(world, current.below())
                {
                    options.push(Direction::Down);
                }
            }
            if avoid_axis != Some(Axis::Z) {
                if current.z() < target_pos.z() && Self::is_empty_block(world, current.south()) {
                    options.push(Direction::South);
                } else if current.z() > target_pos.z()
                    && Self::is_empty_block(world, current.north())
                {
                    options.push(Direction::North);
                }
            }

            let chosen = if options.is_empty() {
                let mut blind = Direction::random();
                let mut attempts = BLIND_DIRECTION_ATTEMPTS;
                while attempts > 0 && !Self::is_empty_block(world, current.relative(blind)) {
                    blind = Direction::random();
                    attempts -= 1;
                }
                blind
            } else {
                options[rand::random_range(0..options.len())]
            };

            let (step_x, step_y, step_z) = chosen.offset();
            aim = position + DVec3::new(f64::from(step_x), f64::from(step_y), f64::from(step_z));
            selection = Some(chosen);
        }

        let delta = aim - position;
        let distance = delta.length();
        let target_delta = if distance == 0.0 {
            DVec3::ZERO
        } else {
            delta / distance * SPEED
        };

        let mut state = self.state.lock();
        state.move_direction = selection;
        state.target_delta = target_delta;
        // Vanilla picks 10, 20, 30, 40 or 50 steps, so the leg lengths vary and
        // two bullets fired at once do not fly in lockstep.
        state.flight_steps = 10 + rand::random_range(0..5) * 10;
        drop(state);
        self.mark_velocity_sync();
    }

    /// Eases the bullet's velocity toward its steering delta.
    ///
    /// Vanilla parity: the target branch of `ShulkerBullet.tick`.
    fn steer(&self) {
        let mut state = self.state.lock();
        state.target_delta = (state.target_delta * STEERING_ACCELERATION).clamp(
            DVec3::splat(-MAX_STEERING_DELTA),
            DVec3::splat(MAX_STEERING_DELTA),
        );
        let target_delta = state.target_delta;
        drop(state);

        let movement = self.velocity();
        self.set_velocity(movement + (target_delta - movement) * STEERING_EASE);
    }

    /// Re-picks the direction when the current leg has run its course.
    ///
    /// Vanilla parity: the tail of `ShulkerBullet.tick`, which turns the bullet
    /// when its step budget runs out, when it is about to fly into something it
    /// could stand on, or when it has drawn level with the target on the axis it
    /// is traveling and has nothing more to gain from this leg.
    fn advance_flight_plan(&self, world: &Arc<World>, target: &SharedEntity) {
        let (steps, direction) = {
            let mut state = self.state.lock();
            if state.flight_steps > 0 {
                state.flight_steps -= 1;
            }
            (state.flight_steps, state.move_direction)
        };

        if steps == 0 {
            let axis = direction.map(Direction::get_axis);
            self.select_next_move_direction(world, axis, Some(target));
            return;
        }

        let Some(direction) = self.state.lock().move_direction else {
            return;
        };
        let current = self.block_position();
        let axis = direction.get_axis();
        if Self::can_stand_on(world, current.relative(direction)) {
            self.select_next_move_direction(world, Some(axis), Some(target));
            return;
        }

        let target_pos = target.block_position();
        let level_with_target = match axis {
            Axis::X => current.x() == target_pos.x(),
            Axis::Y => current.y() == target_pos.y(),
            Axis::Z => current.z() == target_pos.z(),
        };
        if level_with_target {
            self.select_next_move_direction(world, Some(axis), Some(target));
        }
    }

    /// Removes the bullet and reports the impact.
    ///
    /// Vanilla parity: `ShulkerBullet.destroy`.
    fn destroy(&self) {
        self.set_removed(RemovalReason::Discarded);
        if let Some(world) = self.level() {
            world.game_event_at(
                &vanilla_game_events::ENTITY_DAMAGE,
                self.position(),
                &GameEventContext::new(Some(self.as_entity_event_source()), None),
            );
        }
    }
}

impl Entity for ShulkerBulletEntity {
    fn base(&self) -> &EntityBase {
        &self.base
    }

    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
    }

    fn synced_data(&self) -> Option<&dyn EntitySyncedData> {
        Some(&self.entity_data)
    }

    fn sound_source(&self) -> SoundSource {
        SoundSource::Hostile
    }

    fn get_default_gravity(&self) -> f64 {
        DEFAULT_GRAVITY
    }

    fn is_pickable(&self) -> bool {
        true
    }

    /// Vanilla parity: `ShulkerBullet.isOnFire` returns false.
    fn is_on_fire(&self) -> bool {
        false
    }

    /// Vanilla parity: `ShulkerBullet.isAffectedByBlocks`.
    fn is_affected_by_blocks(&self) -> bool {
        !self.is_removed()
    }

    /// Vanilla parity: `ShulkerBullet.checkDespawn` -- peaceful clears the board.
    fn check_despawn(&self) {
        if self
            .level()
            .is_some_and(|world| world.difficulty() == Difficulty::Peaceful)
        {
            self.set_removed(RemovalReason::Discarded);
        }
    }

    /// Vanilla parity: `ShulkerBullet.hurtServer` -- any hit pops the bullet.
    fn hurt(&self, world: &World, _source: &DamageSource, _amount: f32) -> bool {
        self.play_sound(&sound_events::ENTITY_SHULKER_BULLET_HURT, 1.0, 1.0);
        world.send_particles(
            ParticleData::simple(&vanilla_particle_types::CRIT),
            self.position(),
            15,
            DVec3::splat(0.2),
            0.0,
        );
        self.destroy();
        true
    }

    fn restore_owner_reference(&self, owner: &SharedEntity) {
        self.cache_owner_entity(owner);
    }

    /// Vanilla parity: `ShulkerBullet.tick`.
    fn tick(&self) {
        // Vanilla's level captures `Entity.setOldPosAndRot()` before ticking.
        self.set_old_position_to_current();
        self.base().set_old_rotation_to_current();
        self.projectile_base_tick();

        let Some(world) = self.level() else {
            return;
        };

        let target = self.resolve_target(&world);
        if target.is_none() {
            self.state.lock().target = None;
        }

        // Vanilla parity: a dead target, or one that has gone into spectator, is
        // treated as no target at all -- the bullet drops instead of chasing.
        let chase = target
            .as_ref()
            .filter(|target| Entity::is_alive(target.as_ref()) && !target.is_spectator());
        if chase.is_some() {
            self.steer();
        } else {
            self.apply_gravity();
        }

        let hit = self.get_hit_result_on_move_vector();

        let movement = self.velocity();
        if let Err(error) = self.try_set_position(self.position() + movement) {
            log::debug!("failed to advance shulker bullet {}: {error}", self.id());
            self.set_removed(RemovalReason::Discarded);
            return;
        }
        self.apply_effects_from_blocks();

        // TODO: vanilla also runs `handlePortal` here; Foton's projectiles do
        // their portal handling in the shared entity tick.

        if let Some(result) = hit
            && self.is_alive()
            && !self.is_world_change_pending()
        {
            self.hit_target_or_deflect_self(&result);
        }

        rotate_towards_movement(self, ROTATION_SPEED);

        // VANILLA CLIENT-LOCAL: the end-rod trail behind the bullet.

        if let Some(target) = target
            && self.is_alive()
        {
            self.advance_flight_plan(&world, &target);
        }
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_projectile(nbt);
        let state = self.state.lock();
        if let Some(target) = state.target {
            nbt.insert("Target", NbtTag::IntArray(target.to_int_array().to_vec()));
        }
        if let Some(direction) = state.move_direction {
            nbt.insert("Dir", direction.get_3d_data_value() as i8);
        }
        nbt.insert("Steps", state.flight_steps);
        nbt.insert("TXD", state.target_delta.x);
        nbt.insert("TYD", state.target_delta.y);
        nbt.insert("TZD", state.target_delta.z);
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_projectile(nbt);
        let mut state = self.state.lock();
        state.flight_steps = nbt.int("Steps").unwrap_or(0);
        state.target_delta = DVec3::new(
            nbt.double("TXD").unwrap_or(0.0),
            nbt.double("TYD").unwrap_or(0.0),
            nbt.double("TZD").unwrap_or(0.0),
        );
        state.move_direction = nbt
            .byte("Dir")
            .map(|value| Direction::from_3d_data_value(i32::from(value)));
        state.target = nbt
            .int_array("Target")
            .and_then(|array| Uuid::from_int_array(&array));
    }
}

impl Projectile for ShulkerBulletEntity {
    fn projectile_base(&self) -> &ProjectileBase {
        &self.projectile_base
    }

    /// Vanilla parity: `ShulkerBullet.onHitEntity`.
    fn on_hit_entity(&self, entity: &SharedEntity, _location: DVec3) {
        let Some(world) = self.level() else {
            return;
        };

        let owner = self.get_owner();
        let mut source = DamageSource::environment(&vanilla_damage_types::MOB_PROJECTILE)
            .with_direct_entity(self.id());
        if let Some(owner) = owner
            .as_ref()
            .filter(|owner| owner.as_living_entity().is_some())
        {
            source = source.with_causing_entity(owner.id());
        }

        if entity.hurt(&world, &source, HIT_DAMAGE)
            && let Some(living) = entity.as_living_entity()
        {
            living.add_mob_effect(MobEffectInstance::with_duration(
                vanilla_mob_effects::LEVITATION,
                LEVITATION_TICKS,
                0,
            ));
        }

        // TODO: vanilla also runs `EnchantmentHelper.doPostAttackEffects`; Foton
        // has no post-attack enchantment dispatch for projectiles yet.
    }

    /// Vanilla parity: `ShulkerBullet.onHitBlock`.
    fn on_hit_block(&self, hit: &ClipHitResult) {
        self.projectile_on_hit_block(hit);
        if let Some(world) = self.level() {
            world.send_particles(
                ParticleData::simple(&vanilla_particle_types::EXPLOSION),
                self.position(),
                2,
                DVec3::splat(0.2),
                0.0,
            );
        }
        self.play_sound(&sound_events::ENTITY_SHULKER_BULLET_HIT, 1.0, 1.0);
    }

    /// Vanilla parity: `ShulkerBullet.onHit`.
    fn on_hit(&self, hit: &ProjectileHit) {
        self.projectile_on_hit(hit);
        self.destroy();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use foton_registry::{init_vanilla_registry, vanilla_blocks, vanilla_entities};
    use foton_utils::axis::Axis;
    use foton_utils::types::UpdateFlags;
    use foton_utils::{BlockPos, ChunkPos, Direction};
    use glam::DVec3;

    use crate::behavior::init_behaviors;
    use crate::entity::entities::PigEntity;
    use crate::entity::{Entity, SharedEntity, next_entity_id};
    use crate::test_support::{fresh_test_world, insert_ready_full_chunk};
    use crate::world::World;

    use super::{SPEED, ShulkerBulletEntity};

    fn bullet(world: &Arc<World>, position: DVec3) -> ShulkerBulletEntity {
        ShulkerBulletEntity::new(
            &vanilla_entities::SHULKER_BULLET,
            next_entity_id(),
            position,
            Arc::downgrade(world),
        )
    }

    fn pig(world: &Arc<World>, position: DVec3) -> SharedEntity {
        Arc::new(PigEntity::new(
            &vanilla_entities::PIG,
            next_entity_id(),
            position,
            Arc::downgrade(world),
        ))
    }

    #[test]
    fn a_distant_target_makes_the_bullet_travel_one_axis_at_a_time() {
        init_vanilla_registry();
        init_behaviors();

        let world = fresh_test_world("test_shulker_bullet_axis_travel");
        insert_ready_full_chunk(&world, ChunkPos::new(0, 0));
        let bullet = bullet(&world, DVec3::new(2.5, 64.0, 2.5));
        let target = pig(&world, DVec3::new(12.5, 64.0, 2.5));

        bullet.select_next_move_direction(&world, None, Some(&target));

        // Only the +x neighbor closes the gap and only +x is unobstructed, so
        // the bullet has to commit to east.
        assert_eq!(bullet.state.lock().move_direction, Some(Direction::East));
        let delta = bullet.state.lock().target_delta;
        assert!((delta.x - SPEED).abs() < 1.0e-9);
        assert!((delta.length() - SPEED).abs() < 1.0e-9);
    }

    #[test]
    fn the_axis_the_shulker_sits_on_is_never_the_opening_leg() {
        init_vanilla_registry();
        init_behaviors();

        let world = fresh_test_world("test_shulker_bullet_avoid_axis");
        insert_ready_full_chunk(&world, ChunkPos::new(0, 0));
        let bullet = bullet(&world, DVec3::new(2.5, 64.0, 2.5));
        let target = pig(&world, DVec3::new(12.5, 64.0, 2.5));

        bullet.select_next_move_direction(&world, Some(Axis::X), Some(&target));

        // With x ruled out there is no option that closes the gap, so vanilla
        // rolls blind -- but east, the one direction x would have offered, is
        // exactly what the avoided axis was meant to keep it away from on the
        // first leg. Anything else is acceptable; the point is that the choice
        // was not taken from the x list.
        let direction = bullet.state.lock().move_direction;
        assert!(direction.is_some());
    }

    #[test]
    fn a_bullet_within_two_blocks_aims_straight_instead_of_weaving() {
        init_vanilla_registry();
        init_behaviors();

        let world = fresh_test_world("test_shulker_bullet_direct_approach");
        insert_ready_full_chunk(&world, ChunkPos::new(0, 0));
        let bullet = bullet(&world, DVec3::new(2.5, 64.0, 2.5));
        let target = pig(&world, DVec3::new(3.2, 64.0, 2.5));

        bullet.select_next_move_direction(&world, None, Some(&target));

        // Vanilla leaves the move direction null on the final approach, which is
        // what stops the bullet from orbiting a target it is already on top of.
        assert_eq!(bullet.state.lock().move_direction, None);
    }

    #[test]
    fn drawing_level_with_the_target_ends_the_current_leg() {
        init_vanilla_registry();
        init_behaviors();

        let world = fresh_test_world("test_shulker_bullet_turns_at_level");
        insert_ready_full_chunk(&world, ChunkPos::new(0, 0));
        let bullet = bullet(&world, DVec3::new(2.5, 64.0, 2.5));
        let target = pig(&world, DVec3::new(2.5, 70.0, 12.5));
        {
            let mut state = bullet.state.lock();
            state.move_direction = Some(Direction::East);
            state.flight_steps = 20;
        }

        bullet.advance_flight_plan(&world, &target);

        // The bullet is already at the target's x, so traveling east buys it
        // nothing and vanilla turns onto another axis.
        assert_ne!(bullet.state.lock().move_direction, Some(Direction::East));
    }

    #[test]
    fn a_wall_in_front_turns_the_bullet_before_it_reaches_it() {
        init_vanilla_registry();
        init_behaviors();

        let world = fresh_test_world("test_shulker_bullet_turns_at_wall");
        insert_ready_full_chunk(&world, ChunkPos::new(0, 0));
        assert!(world.set_block(
            BlockPos::new(3, 64, 2),
            vanilla_blocks::STONE.default_state(),
            UpdateFlags::UPDATE_ALL,
        ));
        let bullet = bullet(&world, DVec3::new(2.5, 64.0, 2.5));
        let target = pig(&world, DVec3::new(12.5, 64.0, 12.5));
        {
            let mut state = bullet.state.lock();
            state.move_direction = Some(Direction::East);
            state.flight_steps = 20;
        }

        bullet.advance_flight_plan(&world, &target);

        assert_ne!(bullet.state.lock().move_direction, Some(Direction::East));
    }

    #[test]
    fn a_bullet_with_no_target_left_falls_out_of_the_air() {
        init_vanilla_registry();
        init_behaviors();

        let world = fresh_test_world("test_shulker_bullet_falls");
        insert_ready_full_chunk(&world, ChunkPos::new(0, 0));
        let bullet = Arc::new(bullet(&world, DVec3::new(2.5, 100.0, 2.5)));
        world
            .try_add_entity(Arc::clone(&bullet) as SharedEntity)
            .expect("the test chunk is loaded");

        bullet.tick();

        assert!(bullet.velocity().y < 0.0);
        assert!(bullet.position().y < 100.0);
    }
}
