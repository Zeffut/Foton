//! Shulker entity.
//!
//! Vanilla parity: `Shulker` and its three goals. A shulker is a block that
//! shoots: it clamps itself to a face, opens its lid to fire a homing bullet,
//! and teleports away rather than be dug out. Vanilla derives it from
//! `AbstractGolem`, not `Monster`.
//!
//! **Gaps** in this port, all of them in Foton's foundations rather than the
//! mob:
//!
//! - `Shulker.makeBoundingBox` grows the hitbox out of the attach face as the
//!   lid opens, and `onPeekAmountChange` shoves whatever the lid meets. Foton's
//!   `EntityBase` owns the bounding box and derives it from the dimensions, so
//!   only the downward-facing half of `getDefaultDimensions` is modeled here.
//!   [`progress_aabb`] is a faithful port of the same math and is what
//!   `can_stay_at` tests against, so where a shulker may sit is right even
//!   though what it currently fills is approximate.
//! - `Shulker.ShulkerDefenseAttackGoal` only runs for a shulker on a scoreboard
//!   team, and Foton has no teams, so it is left out rather than written as a
//!   goal that can never fire.
//! - `Shulker.move` turns a `MoverType.SHULKER_BOX` push into a teleport.
//!   Nothing in Foton pushes with that mover type yet, and `Entity::move_entity`
//!   has no callable base body to fall through to, so the override is left out
//!   until the shulker box block needs it.
//! - `getInterpolation`, `sanitizeScale` and the `SHULKER_COLOR` data component
//!   have no Foton equivalent yet.

use std::sync::{Arc, Weak};

use foton_macros::entity_behavior;
use foton_protocol::packets::game::SoundSource;
use foton_registry::blocks::block_state_ext::BlockStateExt as _;
use foton_registry::entity_data::EntityPose;
use foton_registry::entity_type::{EntityDimensions, EntityTypeRef};
use foton_registry::sound_event::SoundEventRef;
use foton_registry::vanilla_damage_type_tags::DamageTypeTag;
use foton_registry::vanilla_entity_data::ShulkerEntityData;
use foton_registry::{
    DyeColor, sound_events, vanilla_attributes, vanilla_blocks, vanilla_entities,
    vanilla_game_events,
};
use foton_utils::locks::SyncMutex;
use foton_utils::types::Difficulty;
use foton_utils::{
    BlockPos, ChunkPos, Direction, Downcast as _, DowncastType, DowncastTypeKey, Identifier,
    WorldAabb, axis::Axis,
};
use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;

use crate::entity::LivingEntitySyncedData;
use crate::entity::ai::control::ShulkerLookControl;
use crate::entity::ai::goal::{
    Goal, GoalControls, HurtByTargetGoal, LookAtPlayerGoal, NearestAttackableTargetGoal,
    RandomLookAroundGoal, reduced_tick_delay,
};
use crate::entity::attribute::{AttributeModifier, AttributeModifierOperation};
use crate::entity::damage::DamageSource;
use crate::entity::entities::ShulkerBulletEntity;
use crate::entity::{
    Enemy, Entity, EntityBase, EntityBaseLoad, EntityEventSource as _, EntityMovementEmission,
    EntitySpawnReason, EntitySyncedData, LivingEntity, LivingEntityBase, Mob, MobBase,
    PathfinderMob, SharedEntity, SpawnGroupData, next_entity_id,
};
use crate::physics::WorldCollisionProvider;
use crate::world::{LevelReader as _, World};

/// Experience a shulker drops.
///
/// Vanilla parity: the `this.xpReward = 5` of the constructor.
const XP_REWARD: i32 = 5;

/// The id of the armor bonus a closed shulker carries.
///
/// Vanilla parity: `Shulker.COVERED_ARMOR_MODIFIER_ID`.
const COVERED_ARMOR_MODIFIER_ID: Identifier = Identifier::vanilla_static("covered");

/// How much armor a closed shulker gains.
///
/// Vanilla parity: `Shulker.COVERED_ARMOR_MODIFIER`, which is what makes a
/// closed shulker nearly immune to melee.
const COVERED_ARMOR_BONUS: f64 = 20.0;

/// The synchronized value that means "no dye color".
///
/// Vanilla parity: `Shulker.NO_COLOR`.
const NO_COLOR: i8 = 16;

/// How far a shulker will teleport, in blocks.
///
/// Vanilla parity: `Shulker.MAX_TELEPORT_DISTANCE`.
const MAX_TELEPORT_DISTANCE: i32 = 8;

/// How many spots a teleporting shulker tries.
///
/// Vanilla parity: the `for (int attempt = 0; attempt < 5; attempt++)` of
/// `teleportSomewhere`.
const TELEPORT_ATTEMPTS: i32 = 5;

/// How far a bullet-struck shulker looks for company before it breeds.
///
/// Vanilla parity: the `oldAabb.inflate(8.0)` of `hitByShulkerBullet`.
const OTHER_SHULKER_SCAN_RADIUS: f64 = 8.0;

/// How many nearby shulkers make a split certain to fail.
///
/// Vanilla parity: the `(shulkerCount - 1) / 5.0F` failure chance, so six
/// shulkers in range never make a seventh.
const OTHER_SHULKER_LIMIT: f32 = 5.0;

/// The peek value a shulker opens to while it is fighting.
///
/// Vanilla parity: the `setRawPeekAmount(100)` of `ShulkerAttackGoal.start`.
const ATTACK_PEEK: i32 = 100;

/// The peek value a shulker opens to while it is idle.
///
/// Vanilla parity: the `setRawPeekAmount(30)` of `ShulkerPeekGoal.start`.
const IDLE_PEEK: i32 = 30;

/// Distance at which a shulker watches a player.
///
/// Vanilla parity: `new LookAtPlayerGoal(this, Player.class, 8.0F, 0.02F, true)`.
const LOOK_AT_PLAYER_RANGE: f64 = 8.0;

/// How often a shulker bothers to look at a player.
const LOOK_AT_PLAYER_PROBABILITY: f32 = 0.02;

/// One chance in this many ticks that an idle shulker opens its lid.
///
/// Vanilla parity: the `reducedTickDelay(40)` of `ShulkerPeekGoal.canUse`.
const PEEK_ATTEMPT_INTERVAL_TICKS: i32 = 40;

/// How long an idle peek lasts, in twenty-tick steps.
///
/// Vanilla parity: the `20 * (1 + random.nextInt(3))` of `ShulkerPeekGoal.start`.
const PEEK_DURATION_STEP_TICKS: i32 = 20;

/// How many twenty-tick steps a peek may last.
const PEEK_DURATION_STEPS: i32 = 3;

/// Ticks between the first two bullets.
///
/// Vanilla parity: the `this.attackTime = 20` of `ShulkerAttackGoal.start`.
const FIRST_SHOT_DELAY_TICKS: i32 = 20;

/// Base gap between two bullets, in ticks.
///
/// Vanilla parity: the `20 + random.nextInt(10) * 20 / 2` of the attack goal.
const SHOT_INTERVAL_BASE_TICKS: i32 = 20;

/// Span of the random part of that gap, in ten-tick steps.
const SHOT_INTERVAL_SPAN: i32 = 10;

/// Squared distance beyond which a shulker gives its target up.
///
/// Vanilla parity: the `distance < 400.0` of `ShulkerAttackGoal.tick`.
const SHOT_RANGE_SQR: f64 = 400.0;

/// How far a shulker's head turns each tick while it is firing.
const ATTACK_LOOK_TURN_RATE: f32 = 180.0;

/// How far a shulker's head may turn at all.
///
/// Vanilla parity: `Shulker.getMaxHeadXRot` and `getMaxHeadYRot`.
const MAX_HEAD_ROT: f32 = 180.0;

/// How much of a shulker's height the open lid adds.
///
/// Vanilla parity: the `1.0F + this.currentPeekAmount` of
/// `Shulker.getDefaultDimensions`.
const PEEK_HEIGHT_SCALE: f32 = 1.0;

/// How fast the visible lid follows the synchronized peek value.
///
/// Vanilla parity: `Shulker.PEEK_PER_TICK`.
const PEEK_PER_TICK: f32 = 0.05;

/// Scales the synchronized peek byte into the zero-to-one lid opening.
///
/// Vanilla parity: the `getRawPeekAmount() * 0.01F` of `updatePeekAmount`.
const PEEK_SCALE: f32 = 0.01;

/// Squared distance a shulker's collision boxes are deflated by before a
/// clearance test, so touching faces do not count as a collision.
///
/// Vanilla parity: the `deflate(1.0E-6)` of `canStayAt` and `teleportSomewhere`.
const CLEARANCE_EPSILON: f64 = 1.0e-6;

/// Builds the box a shulker of `size` fills when its lid is `progress_to` open
/// along `direction`.
///
/// Vanilla parity: `Shulker.getProgressAabb`, which is
/// `getProgressDeltaAabb(size, direction, -1.0F, progressTo, position)`.
#[must_use]
pub fn progress_aabb(
    size: f32,
    direction: Direction,
    progress_to: f32,
    position: DVec3,
) -> WorldAabb {
    progress_delta_aabb(size, direction, -1.0, progress_to, position)
}

/// Builds the box a shulker's lid sweeps between two openings.
///
/// Vanilla parity: `Shulker.getProgressDeltaAabb`.
#[must_use]
pub fn progress_delta_aabb(
    size: f32,
    direction: Direction,
    progress_from: f32,
    progress_to: f32,
    position: DVec3,
) -> WorldAabb {
    let size = f64::from(size);
    let bounds_at_bottom_center =
        WorldAabb::new(-size * 0.5, 0.0, -size * 0.5, size * 0.5, size, size * 0.5);
    let max_movement = f64::from(progress_from.max(progress_to));
    let min_movement = f64::from(progress_from.min(progress_to));
    let (step_x, step_y, step_z) = direction.offset();
    let step = DVec3::new(f64::from(step_x), f64::from(step_y), f64::from(step_z));

    let expanded = bounds_at_bottom_center.expand_towards(step * max_movement * size);
    contract(expanded, -step * (1.0 + min_movement) * size).translate(position)
}

/// Shrinks a box on the side each component points at.
///
/// Vanilla parity: `AABB.contract`, which Foton's uniform `deflate` cannot
/// express: a negative component moves the minimum face, a positive one moves
/// the maximum face.
fn contract(aabb: WorldAabb, amount: DVec3) -> WorldAabb {
    let shift = |min: f64, max: f64, delta: f64| {
        if delta < 0.0 {
            (min - delta, max)
        } else if delta > 0.0 {
            (min, max - delta)
        } else {
            (min, max)
        }
    };

    let (min_x, max_x) = shift(aabb.min_x(), aabb.max_x(), amount.x);
    let (min_y, max_y) = shift(aabb.min_y(), aabb.max_y(), amount.y);
    let (min_z, max_z) = shift(aabb.min_z(), aabb.max_z(), amount.z);

    WorldAabb::new(min_x, min_y, min_z, max_x, max_y, max_z)
}

/// A shulker.
#[entity_behavior(class = "Shulker")]
pub struct ShulkerEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    entity_data: SyncMutex<ShulkerEntityData>,
    /// How far the lid has actually swung, between zero and one.
    ///
    /// Vanilla parity: `Shulker.currentPeekAmount`, which chases the
    /// synchronized byte a twentieth at a time.
    current_peek_amount: SyncMutex<f32>,
}

// SAFETY: This key is owned by Foton and uniquely identifies `ShulkerEntity`.
unsafe impl DowncastType for ShulkerEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:entity/shulker");
}

impl ShulkerEntity {
    /// Creates a shulker at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Creates a shulker from saved base data.
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
        let mut entity_data = ShulkerEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);
        mob_base.set_xp_reward(XP_REWARD);

        {
            // Keep vanilla Shulker goal priorities in the same order.
            let mut goals = mob_base.goal_selector().lock();
            goals.add_goal(
                1,
                LookAtPlayerGoal::new_with_probability_and_horizontal(
                    LOOK_AT_PLAYER_RANGE,
                    LOOK_AT_PLAYER_PROBABILITY,
                    true,
                ),
            );
            goals.add_goal(4, ShulkerAttackGoal::new());
            goals.add_goal(7, ShulkerPeekGoal::new());
            goals.add_goal(8, RandomLookAroundGoal::new());
        }

        {
            let mut targets = mob_base.target_selector().lock();
            // Vanilla parity: `new HurtByTargetGoal(this, this.getClass()).setAlertOthers()`.
            // A shulker never turns on the shulker whose bullet hit it, but it
            // does wake the rest of the nest.
            targets.add_goal(
                1,
                HurtByTargetGoal::new()
                    .with_ignored_damage_types([Self::TYPE_KEY])
                    .set_alert_others([]),
            );
            targets.add_goal(
                2,
                NearestAttackableTargetGoal::new_for_players(true, |_, _, world| {
                    // Vanilla parity: `ShulkerNearestAttackGoal.canUse` refuses
                    // outright on peaceful.
                    world.difficulty() != Difficulty::Peaceful
                }),
            );
        }

        Self {
            base,
            entity_type,
            living_base,
            mob_base,
            entity_data: SyncMutex::new(entity_data),
            current_peek_amount: SyncMutex::new(0.0),
        }
    }

    /// Returns the face this shulker is clamped to.
    ///
    /// Vanilla parity: `Shulker.getAttachFace`.
    #[must_use]
    pub fn attach_face(&self) -> Direction {
        *self.entity_data.lock().attach_face.get()
    }

    /// Vanilla parity: `Shulker.setAttachFace`.
    pub fn set_attach_face(&self, attach_face: Direction) {
        self.entity_data.lock().attach_face.set(attach_face);
    }

    /// Returns how far open the lid is told to be, from zero to a hundred.
    ///
    /// Vanilla parity: `Shulker.getRawPeekAmount`.
    #[must_use]
    pub fn raw_peek_amount(&self) -> i32 {
        i32::from(*self.entity_data.lock().peek.get())
    }

    /// Returns whether the lid is shut.
    ///
    /// Vanilla parity: `Shulker.isClosed`.
    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.raw_peek_amount() == 0
    }

    /// Opens or shuts the lid, with everything that goes with it.
    ///
    /// Vanilla parity: `Shulker.setRawPeekAmount`. Closing is what gives a
    /// shulker its twenty points of armor, which is why hitting a closed one
    /// with a sword does almost nothing.
    pub fn set_raw_peek_amount(&self, amount: i32) {
        self.attributes()
            .lock()
            .remove_modifier(vanilla_attributes::ARMOR, &COVERED_ARMOR_MODIFIER_ID);

        if amount == 0 {
            self.attributes().lock().set_modifier(
                vanilla_attributes::ARMOR,
                AttributeModifier {
                    id: COVERED_ARMOR_MODIFIER_ID,
                    amount: COVERED_ARMOR_BONUS,
                    operation: AttributeModifierOperation::AddValue,
                },
                true,
            );
            self.play_sound(&sound_events::ENTITY_SHULKER_CLOSE, 1.0, 1.0);
            self.game_event(&vanilla_game_events::CONTAINER_CLOSE);
        } else {
            self.play_sound(&sound_events::ENTITY_SHULKER_OPEN, 1.0, 1.0);
            self.game_event(&vanilla_game_events::CONTAINER_OPEN);
        }

        #[expect(
            clippy::cast_possible_truncation,
            reason = "vanilla stores the peek amount as a byte"
        )]
        self.entity_data.lock().peek.set(amount as i8);
    }

    /// Returns this shulker's dye color, if it has one.
    ///
    /// Vanilla parity: `Shulker.getColor`.
    #[must_use]
    pub fn color(&self) -> Option<DyeColor> {
        let color = *self.entity_data.lock().color.get();
        (color != NO_COLOR && color <= 15).then(|| DyeColor::by_id(i32::from(color)))
    }

    /// Vanilla parity: `Shulker.setVariant`.
    pub fn set_color(&self, color: Option<DyeColor>) {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "every dye color id fits in a byte"
        )]
        let value = color.map_or(NO_COLOR, |color| color.id() as i8);
        self.entity_data.lock().color.set(value);
    }

    /// Advances the visible lid toward the synchronized peek value.
    ///
    /// Vanilla parity: `Shulker.updatePeekAmount`. Returns whether it moved.
    fn update_peek_amount(&self) -> bool {
        #[expect(clippy::cast_precision_loss, reason = "the raw peek amount is a byte")]
        let target = self.raw_peek_amount() as f32 * PEEK_SCALE;
        let mut current = self.current_peek_amount.lock();
        if (*current - target).abs() < f32::EPSILON {
            return false;
        }

        *current = if *current > target {
            (*current - PEEK_PER_TICK).clamp(target, 1.0)
        } else {
            (*current + PEEK_PER_TICK).clamp(0.0, target)
        };

        true
    }

    /// Returns whether this shulker could sit at `target` clamped to `face`.
    ///
    /// Vanilla parity: `Shulker.canStayAt`.
    fn can_stay_at(&self, target: BlockPos, face: Direction) -> bool {
        let Some(world) = self.level() else {
            return false;
        };
        if self.is_position_blocked(target) {
            return false;
        }

        let opposite = face.opposite();
        let support = target.relative(face);
        let loaded = world.has_full_chunk(ChunkPos::from_block_pos(support));
        if !loaded || !world.is_face_sturdy(world.get_block_state(support), support, opposite) {
            return false;
        }

        let (x, _, z) = target.get_center();
        let bottom_center = DVec3::new(x, f64::from(target.y()), z);
        let fully_opened = progress_aabb(self.get_scale(), opposite, 1.0, bottom_center)
            .deflate(CLEARANCE_EPSILON);

        !self.has_collision(&world, fully_opened)
    }

    /// Vanilla parity: `Shulker.isPositionBlocked`.
    fn is_position_blocked(&self, target: BlockPos) -> bool {
        let Some(world) = self.level() else {
            return true;
        };
        let state = world.get_block_state(target);
        if state.is_air() {
            return false;
        }

        // Vanilla lets a shulker stand in the moving-piston block it is being
        // pushed out of, and nothing else.
        let moving_piston_here =
            state.get_block() == &vanilla_blocks::MOVING_PISTON && target == self.block_position();
        !moving_piston_here
    }

    /// Vanilla parity: `Level.noCollision(this, aabb)`, negated.
    fn has_collision(&self, world: &Arc<World>, aabb: WorldAabb) -> bool {
        WorldCollisionProvider::for_entity(world, self.as_entity_event_source())
            .has_entity_context_collision(aabb, self.position().y, self.is_descending())
    }

    /// Returns a face at `target` this shulker could clamp to.
    ///
    /// Vanilla parity: `Shulker.findAttachableSurface`.
    fn find_attachable_surface(&self, target: BlockPos) -> Option<Direction> {
        Direction::ALL
            .into_iter()
            .find(|direction| self.can_stay_at(target, *direction))
    }

    /// Re-clamps a shulker whose wall has gone, or moves it if there is none.
    ///
    /// Vanilla parity: `Shulker.findNewAttachment`.
    fn find_new_attachment(&self) {
        if let Some(direction) = self.find_attachable_surface(self.block_position()) {
            self.set_attach_face(direction);
        } else {
            self.teleport_somewhere();
        }
    }

    /// Moves the shulker up to eight blocks to a spot it can clamp to.
    ///
    /// Vanilla parity: `Shulker.teleportSomewhere`.
    pub fn teleport_somewhere(&self) -> bool {
        if self.is_no_ai() || !LivingEntity::is_alive(self) {
            return false;
        }
        let Some(world) = self.level() else {
            return false;
        };

        let current = self.block_position();
        for _ in 0..TELEPORT_ATTEMPTS {
            let target = current.offset(
                rand::random_range(-MAX_TELEPORT_DISTANCE..=MAX_TELEPORT_DISTANCE),
                rand::random_range(-MAX_TELEPORT_DISTANCE..=MAX_TELEPORT_DISTANCE),
                rand::random_range(-MAX_TELEPORT_DISTANCE..=MAX_TELEPORT_DISTANCE),
            );
            if target.y() <= world.min_y()
                || !world.get_block_state(target).is_air()
                || !world.is_block_within_world_border(target)
            {
                continue;
            }

            let block_box = WorldAabb::new(
                f64::from(target.x()),
                f64::from(target.y()),
                f64::from(target.z()),
                f64::from(target.x() + 1),
                f64::from(target.y() + 1),
                f64::from(target.z() + 1),
            )
            .deflate(CLEARANCE_EPSILON);
            if self.has_collision(&world, block_box) {
                continue;
            }

            let Some(direction) = self.find_attachable_surface(target) else {
                continue;
            };

            self.stop_riding();
            self.set_attach_face(direction);
            self.play_sound(&sound_events::ENTITY_SHULKER_TELEPORT, 1.0, 1.0);
            let (x, _, z) = target.get_center();
            if let Err(error) = self.try_set_position(DVec3::new(x, f64::from(target.y()), z)) {
                log::debug!("shulker could not teleport: {error}");
                return false;
            }
            self.game_event(&vanilla_game_events::TELEPORT);
            self.entity_data.lock().peek.set(0);
            self.set_target(None);
            return true;
        }

        false
    }

    /// Splits into a second shulker after taking a bullet.
    ///
    /// Vanilla parity: `Shulker.hitByShulkerBullet`. It only works while the
    /// lid is open, and it fails more often the more shulkers are already
    /// crowded around, which is what stops a nest growing without bound.
    fn hit_by_shulker_bullet(&self) {
        if self.is_closed() {
            return;
        }
        let old_position = self.position();
        let old_box = self.bounding_box();
        if !self.teleport_somewhere() {
            return;
        }
        let Some(world) = self.level() else {
            return;
        };

        let search_box = old_box.inflate(OTHER_SHULKER_SCAN_RADIUS);
        let shulker_count = world
            .get_entities_in_aabb_matching(&search_box, |entity| {
                entity.is_alive() && entity.entity_type() == &vanilla_entities::SHULKER
            })
            .len();
        #[expect(
            clippy::cast_precision_loss,
            reason = "a shulker count in one small box is tiny"
        )]
        let failure_chance = (shulker_count as f32 - 1.0) / OTHER_SHULKER_LIMIT;
        if rand::random::<f32>() < failure_chance {
            return;
        }

        let baby = Arc::new(Self::new(
            &vanilla_entities::SHULKER,
            next_entity_id(),
            old_position,
            Arc::downgrade(&world),
        ));
        baby.set_color(self.color());

        let entity: SharedEntity = baby;
        if let Err(error) = world.try_add_entity(entity) {
            log::debug!("shulker failed to split: {error}");
        }
    }
}

/// Recovers the shulker a goal is running on.
fn shulker_of(mob: &dyn PathfinderMob) -> Option<&ShulkerEntity> {
    mob.downcast_ref::<ShulkerEntity>()
}

/// Opens the lid and fires bullets at whatever it can see.
///
/// Vanilla parity: `Shulker.ShulkerAttackGoal`.
struct ShulkerAttackGoal {
    attack_time: i32,
}

impl ShulkerAttackGoal {
    const fn new() -> Self {
        Self { attack_time: 0 }
    }

    /// Fires one bullet at `target`.
    fn shoot(shulker: &ShulkerEntity, mob: &dyn PathfinderMob, target: &SharedEntity) {
        let Some(world) = mob.level() else {
            return;
        };
        let Some(owner) = world.get_entity_by_id(mob.id()) else {
            return;
        };

        let bullet = Arc::new(ShulkerBulletEntity::new(
            &vanilla_entities::SHULKER_BULLET,
            next_entity_id(),
            mob.position(),
            Arc::downgrade(&world),
        ));
        // Vanilla hands the bullet the axis the shulker is stuck along, so it
        // never opens by flying into its own wall.
        bullet.fire_at(&world, &owner, target, Some(axis_of(shulker.attach_face())));

        let entity: SharedEntity = bullet;
        if let Err(error) = world.try_add_entity(entity) {
            log::debug!("shulker failed to fire a bullet: {error}");
            return;
        }

        shulker.play_sound(
            &sound_events::ENTITY_SHULKER_SHOOT,
            2.0,
            (rand::random::<f32>() - rand::random::<f32>()).mul_add(0.2, 1.0),
        );
    }
}

/// Returns the axis a face lies along.
const fn axis_of(direction: Direction) -> Axis {
    match direction {
        Direction::East | Direction::West => Axis::X,
        Direction::Up | Direction::Down => Axis::Y,
        Direction::North | Direction::South => Axis::Z,
    }
}

impl Goal for ShulkerAttackGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::MOVE | GoalControls::LOOK
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        let Some(world) = mob.level() else {
            return false;
        };
        if world.difficulty() == Difficulty::Peaceful {
            return false;
        }

        mob.target().is_some_and(|target| {
            target
                .as_living_entity()
                .is_some_and(LivingEntity::is_alive)
        })
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        self.attack_time = FIRST_SHOT_DELAY_TICKS;
        if let Some(shulker) = shulker_of(mob) {
            shulker.set_raw_peek_amount(ATTACK_PEEK);
        }
    }

    fn stop(&mut self, mob: &dyn PathfinderMob) {
        if let Some(shulker) = shulker_of(mob) {
            shulker.set_raw_peek_amount(0);
        }
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    /// Vanilla parity: `ShulkerAttackGoal.tick`.
    fn tick(&mut self, mob: &dyn PathfinderMob) {
        let (Some(world), Some(shulker)) = (mob.level(), shulker_of(mob)) else {
            return;
        };
        if world.difficulty() == Difficulty::Peaceful {
            return;
        }

        self.attack_time -= 1;
        let Some(target) = mob.target() else {
            return;
        };

        let target_position = target.position();
        mob.mob_base().controls().lock().look_control.set_look_at(
            DVec3::new(target_position.x, target.get_eye_y(), target_position.z),
            ATTACK_LOOK_TURN_RATE,
            ATTACK_LOOK_TURN_RATE,
        );

        if mob.position().distance_squared(target_position) >= SHOT_RANGE_SQR {
            mob.set_target(None);
            return;
        }

        if self.attack_time <= 0 {
            self.attack_time = SHOT_INTERVAL_BASE_TICKS
                + rand::random_range(0..SHOT_INTERVAL_SPAN) * SHOT_INTERVAL_BASE_TICKS / 2;
            Self::shoot(shulker, mob, &target);
        }
    }
}

/// Cracks the lid open now and then when nothing is happening.
///
/// Vanilla parity: `Shulker.ShulkerPeekGoal`.
struct ShulkerPeekGoal {
    peek_time: i32,
}

impl ShulkerPeekGoal {
    const fn new() -> Self {
        Self { peek_time: 0 }
    }
}

impl Goal for ShulkerPeekGoal {
    /// Vanilla parity: `ShulkerPeekGoal` never calls `setFlags`.
    fn controls(&self) -> GoalControls {
        GoalControls::EMPTY
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        if mob.target().is_some() {
            return false;
        }
        if rand::random_range(0..reduced_tick_delay(PEEK_ATTEMPT_INTERVAL_TICKS)) != 0 {
            return false;
        }

        shulker_of(mob)
            .is_some_and(|shulker| shulker.can_stay_at(mob.block_position(), shulker.attach_face()))
    }

    fn can_continue_to_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        mob.target().is_none() && self.peek_time > 0
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        self.peek_time =
            PEEK_DURATION_STEP_TICKS * (1 + rand::random_range(0..PEEK_DURATION_STEPS));
        if let Some(shulker) = shulker_of(mob) {
            shulker.set_raw_peek_amount(IDLE_PEEK);
        }
    }

    fn stop(&mut self, mob: &dyn PathfinderMob) {
        if mob.target().is_none()
            && let Some(shulker) = shulker_of(mob)
        {
            shulker.set_raw_peek_amount(0);
        }
    }

    fn tick(&mut self, _mob: &dyn PathfinderMob) {
        self.peek_time -= 1;
    }
}

impl Entity for ShulkerEntity {
    fn base(&self) -> &EntityBase {
        &self.base
    }

    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
    }

    fn synced_data(&self) -> Option<&dyn EntitySyncedData> {
        Some(&self.entity_data)
    }

    /// Vanilla parity: `Shulker.tick`, which re-clamps a shulker whose wall has
    /// gone and then swings the lid a twentieth of the way toward wherever the
    /// goals put it.
    fn tick(&self) {
        self.tick_living_entity();

        if !self.is_passenger() && !self.can_stay_at(self.block_position(), self.attach_face()) {
            self.find_new_attachment();
        }

        if self.update_peek_amount() {
            self.refresh_dimensions();
        }
    }

    fn base_tick(&self) {
        Mob::base_tick_mob(self);
    }

    fn sound_source(&self) -> SoundSource {
        SoundSource::Hostile
    }

    /// Vanilla parity: `Shulker.getMovementEmission`. A shulker makes no
    /// movement sound and fires no movement game events at all.
    fn movement_emission(&self) -> EntityMovementEmission {
        EntityMovementEmission::None
    }

    /// Vanilla parity: `Shulker.getDeltaMovement` always answers zero.
    fn velocity(&self) -> DVec3 {
        DVec3::ZERO
    }

    /// Vanilla parity: `Shulker.setDeltaMovement` is empty; nothing can push a
    /// shulker off its wall.
    fn set_velocity(&self, _velocity: DVec3) {}

    /// Vanilla parity: `Shulker.push` is empty.
    fn push_entity(&self, _entity: &dyn Entity) {}

    /// Vanilla parity: `Shulker.canBeCollidedWith`.
    fn can_be_collided_with(&self, _other: Option<&dyn Entity>) -> bool {
        LivingEntity::is_alive(self)
    }

    /// Vanilla parity: `Shulker.getDefaultDimensions`, the downward-facing half.
    ///
    /// A shulker attached to the floor grows upward as its lid opens; one stuck
    /// to a wall or ceiling grows into the block it is facing, which Foton's
    /// dimension-derived bounding box cannot express (see the module gaps).
    fn dimensions_for_pose(&self, _pose: EntityPose) -> EntityDimensions {
        let dimensions = self.entity_type.dimensions;
        let peek = *self.current_peek_amount.lock();
        if self.attach_face() != Direction::Down || peek <= 0.0 {
            return dimensions;
        }

        EntityDimensions {
            height: dimensions.height * (PEEK_HEIGHT_SCALE + peek),
            eye_height: dimensions.eye_height * (PEEK_HEIGHT_SCALE + peek),
            ..dimensions
        }
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_mob(nbt);
        #[expect(
            clippy::cast_possible_truncation,
            reason = "vanilla writes the attach face as a legacy byte id"
        )]
        nbt.insert("AttachFace", legacy_direction_id(self.attach_face()) as i8);
        nbt.insert("Peek", *self.entity_data.lock().peek.get());
        nbt.insert("Color", *self.entity_data.lock().color.get());
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_mob(nbt);
        if let Some(face) = nbt
            .byte("AttachFace")
            .and_then(|id| direction_from_legacy_id(i32::from(id)))
        {
            self.set_attach_face(face);
        }
        if let Some(peek) = nbt.byte("Peek") {
            self.entity_data.lock().peek.set(peek);
        }
        if let Some(color) = nbt.byte("Color") {
            self.entity_data.lock().color.set(color);
        }
    }
}

/// Returns the id vanilla writes a direction as in a shulker's save data.
///
/// Vanilla parity: `Direction.LEGACY_ID_CODEC`, which is `get3DDataValue`.
const fn legacy_direction_id(direction: Direction) -> i32 {
    match direction {
        Direction::Down => 0,
        Direction::Up => 1,
        Direction::North => 2,
        Direction::South => 3,
        Direction::West => 4,
        Direction::East => 5,
    }
}

/// Returns the direction a legacy save id names.
const fn direction_from_legacy_id(id: i32) -> Option<Direction> {
    match id {
        0 => Some(Direction::Down),
        1 => Some(Direction::Up),
        2 => Some(Direction::North),
        3 => Some(Direction::South),
        4 => Some(Direction::West),
        5 => Some(Direction::East),
        _ => None,
    }
}

impl LivingEntity for ShulkerEntity {
    /// Returns synchronized data declared by vanilla `LivingEntity`.
    fn living_synced_data(&self) -> Option<&dyn LivingEntitySyncedData> {
        Some(&self.entity_data)
    }

    fn living_base(&self) -> &LivingEntityBase {
        &self.living_base
    }

    /// Vanilla parity: `Mob.serverAiStep`, which is where a mob's goals run.
    fn server_ai_step(&self) {
        Mob::mob_server_ai_step(self);
    }

    /// Vanilla parity: `Shulker.hurtServer`. A closed shulker shrugs arrows
    /// off entirely, a wounded one teleports away, and one hit by another
    /// shulker's bullet splits in two.
    fn hurt_server(&self, world: &World, source: &DamageSource, amount: f32) -> bool {
        if self.is_closed()
            && source
                .direct_entity_id
                .and_then(|id| world.get_entity_by_id(id))
                .is_some_and(|direct| direct.is_abstract_arrow())
        {
            return false;
        }

        if !self.living_hurt_server(world, source, amount) {
            return false;
        }

        if self.get_health() < self.get_max_health() * 0.5 && rand::random_range(0..4) == 0 {
            self.teleport_somewhere();
        } else if source.is(&DamageTypeTag::IS_PROJECTILE)
            && source
                .direct_entity_id
                .and_then(|id| world.get_entity_by_id(id))
                .is_some_and(|direct| direct.entity_type() == &vanilla_entities::SHULKER_BULLET)
        {
            self.hit_by_shulker_bullet();
        }

        true
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

    /// Vanilla parity: `Shulker.getHurtSound`.
    fn hurt_sound(&self, _source: &DamageSource) -> Option<SoundEventRef> {
        Some(if self.is_closed() {
            &sound_events::ENTITY_SHULKER_HURT_CLOSED
        } else {
            &sound_events::ENTITY_SHULKER_HURT
        })
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_SHULKER_DEATH)
    }
}

impl Mob for ShulkerEntity {
    fn mob_base(&self) -> &MobBase {
        &self.mob_base
    }

    fn tick_goal_selectors(&self) {
        PathfinderMob::tick_pathfinder_goal_selectors(self);
    }

    fn tick_path_navigation(&self) {
        PathfinderMob::tick_pathfinder_path_navigation(self);
    }

    /// Vanilla parity: `Shulker` installs a `ShulkerLookControl`.
    fn tick_look_control(&self) {
        ShulkerLookControl::new(self.attach_face()).tick(self);
    }

    /// Vanilla parity: `Shulker.ShulkerBodyRotationControl.clientTick` is
    /// empty, so a shulker's body never turns on its own.
    fn tick_body_rotation_control(&self) {}

    /// Vanilla parity: `Shulker.getMaxHeadXRot`.
    fn max_head_x_rot(&self) -> f32 {
        MAX_HEAD_ROT
    }

    /// Vanilla parity: `Shulker.getMaxHeadYRot`.
    fn max_head_y_rot(&self) -> f32 {
        MAX_HEAD_ROT
    }

    fn ambient_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_SHULKER_AMBIENT)
    }

    /// Vanilla parity: `Shulker.playAmbientSound`. A shut shulker is silent.
    fn play_ambient_sound(&self) {
        if !self.is_closed() {
            self.make_sound(self.ambient_sound());
        }
    }

    fn mob_flags(&self) -> i8 {
        *self.entity_data.lock().mob().mob_flags.get()
    }

    fn set_mob_flags(&self, flags: i8) {
        self.entity_data.lock().mob_mut().mob_flags.set(flags);
    }

    /// Vanilla parity: `Shulker.finalizeSpawn`, which squares the shulker up
    /// with the world before anything else looks at it.
    fn finalize_spawn(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        group_data: Option<SpawnGroupData>,
    ) -> Option<SpawnGroupData> {
        let (_, pitch) = self.rotation();
        self.set_rotation((0.0, pitch));
        self.set_y_head_rot(0.0);
        self.base().set_old_position_to_current();
        self.base().set_old_rotation_to_current();

        self.finalize_spawn_mob_base(world, spawn_reason, group_data)
    }
}

impl PathfinderMob for ShulkerEntity {}

impl Enemy for ShulkerEntity {}

#[cfg(test)]
mod tests;
