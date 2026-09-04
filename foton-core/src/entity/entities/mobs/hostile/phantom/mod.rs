//! Phantom entity.
//!
//! Vanilla parity: `Phantom` and its four goals. A phantom never paths: it
//! circles an anchor point high above whatever it is hunting, and every eight
//! to twelve seconds it drops the circle and swoops straight through its
//! target. Everything it does is written into one `moveTargetPoint` that
//! `PhantomMoveControl` then flies at.

use std::cmp::Ordering;
use std::f32::consts::TAU;
use std::sync::{Arc, Weak};

use foton_macros::entity_behavior;
use foton_protocol::packets::game::SoundSource;
use foton_registry::blocks::block_state_ext::BlockStateExt as _;
use foton_registry::entity_data::EntityPose;
use foton_registry::entity_type::{EntityDimensions, EntityTypeRef};
use foton_registry::sound_event::SoundEventRef;
use foton_registry::vanilla_entity_data::PhantomEntityData;
use foton_registry::{level_events, sound_events, vanilla_attributes};
use foton_utils::locks::SyncMutex;
use foton_utils::{BlockPos, BlockStateId, Downcast as _, DowncastType, DowncastTypeKey};
use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::{NbtCompound, NbtTag};

use crate::chunk::heightmap::HeightmapType;
use crate::entity::Enemy;
use crate::entity::EntitySpawnReason;
use crate::entity::LivingEntitySyncedData;
use crate::entity::ai::control::{PHANTOM_INITIAL_SPEED, PhantomMoveControl};
use crate::entity::ai::goal::{Goal, GoalControls, reduced_tick_delay};
use crate::entity::ai::targeting::TargetingConditions;
use crate::entity::damage::DamageSource;
use crate::entity::entities::CatEntity;
use crate::entity::{
    Entity, EntityBase, EntityBaseLoad, EntitySyncedData, LivingEntity, LivingEntityBase, Mob,
    MobBase, PathfinderMob, SharedEntity, SpawnGroupData,
};
use crate::physics::MoveResult;
use crate::world::{LevelAccessor as _, World};

/// Experience a phantom drops.
///
/// Vanilla parity: the `this.xpReward = 5` of the constructor.
const XP_REWARD: i32 = 5;

/// Ticks between two wingbeats.
///
/// Vanilla parity: `Phantom.TICKS_PER_FLAP`, `Mth.ceil(24.166098F)`.
const TICKS_PER_FLAP: i32 = 25;

/// How far apart two phantoms' wingbeats are, in ticks per entity id.
///
/// Vanilla parity: `Phantom.getUniqueFlapTickOffset`.
const FLAP_TICK_OFFSET_PER_ID: i32 = 3;

/// Largest size a phantom may be set to.
///
/// Vanilla parity: the `Mth.clamp(size, 0, 64)` of `setPhantomSize`.
const MAX_SIZE: i32 = 64;

/// Attack damage of a phantom of size zero.
///
/// Vanilla parity: the `6 + this.getPhantomSize()` of `updatePhantomSizeInfo`.
const BASE_ATTACK_DAMAGE: f64 = 6.0;

/// How much bigger each size step makes a phantom.
///
/// Vanilla parity: the `1.0F + 0.15F * size` of `getDefaultDimensions`.
const SIZE_SCALE_STEP: f32 = 0.15;

/// How hard a phantom pushes against the air.
///
/// Vanilla parity: the `travelFlying(input, 0.2F)` of `Phantom.travel`.
const AIR_TRAVEL_SPEED: f32 = 0.2;

/// How far a phantom looks for someone to hunt.
///
/// Vanilla parity: the `TargetingConditions.forCombat().range(64.0)` of
/// `PhantomAttackPlayerTargetGoal`.
const TARGET_RANGE: f64 = 64.0;

/// Horizontal reach of the target scan, in blocks.
///
/// Vanilla parity: the `inflate(16.0, 64.0, 16.0)` of the same goal.
const TARGET_SCAN_HORIZONTAL: f64 = 16.0;

/// Vertical reach of the target scan, in blocks.
const TARGET_SCAN_VERTICAL: f64 = 64.0;

/// Ticks before the first target scan.
///
/// Vanilla parity: the `reducedTickDelay(20)` the goal starts at.
const FIRST_SCAN_DELAY_TICKS: i32 = 20;

/// Ticks between two target scans.
const SCAN_INTERVAL_TICKS: i32 = 60;

/// Ticks a phantom circles before its first swoop.
///
/// Vanilla parity: the `adjustedTickDelay(10)` of
/// `PhantomAttackStrategyGoal.start`.
const FIRST_SWEEP_DELAY_TICKS: i32 = 10;

/// Shortest gap between two swoops, in seconds.
///
/// Vanilla parity: the `(8 + random.nextInt(4)) * 20` of the same goal.
const SWEEP_INTERVAL_MIN_SECONDS: i32 = 8;

/// Span of the random part of that gap, in seconds.
const SWEEP_INTERVAL_SPAN_SECONDS: i32 = 4;

/// Ticks in a second.
const TICKS_PER_SECOND: i32 = 20;

/// How high above its target a phantom anchors before a swoop.
///
/// Vanilla parity: the `above(20 + random.nextInt(20))` of
/// `setAnchorAboveTarget`.
const ANCHOR_ABOVE_TARGET_MIN: i32 = 20;

/// Span of the random part of that height.
const ANCHOR_ABOVE_TARGET_SPAN: i32 = 20;

/// How high above the ground a phantom re-anchors once it loses its target.
///
/// Vanilla parity: the `above(10 + random.nextInt(20))` of
/// `PhantomAttackStrategyGoal.stop`.
const ANCHOR_ABOVE_GROUND_MIN: i32 = 10;

/// Span of the random part of that height.
const ANCHOR_ABOVE_GROUND_SPAN: i32 = 20;

/// How high above itself a freshly spawned phantom anchors.
///
/// Vanilla parity: the `this.blockPosition().above(5)` of
/// `Phantom.finalizeSpawn`.
const SPAWN_ANCHOR_HEIGHT: i32 = 5;

/// Smallest radius a phantom circles at, in blocks.
///
/// Vanilla parity: the `5.0F + random.nextFloat() * 10.0F` of
/// `PhantomCircleAroundAnchorGoal.start`.
const CIRCLE_RADIUS_MIN: f32 = 5.0;

/// Span of the random part of that radius.
const CIRCLE_RADIUS_SPAN: f32 = 10.0;

/// Radius at which a phantom's widening circle snaps back in.
///
/// Vanilla parity: the `distance > 15.0F` of the goal's tick.
const CIRCLE_RADIUS_MAX: f32 = 15.0;

/// Lowest a phantom will circle relative to its anchor, in blocks.
///
/// Vanilla parity: the `-4.0F + random.nextFloat() * 9.0F` height roll, on top
/// of the fixed `-4.0F` the position itself carries.
const CIRCLE_HEIGHT_MIN: f32 = -4.0;

/// Span of the random part of that height.
const CIRCLE_HEIGHT_SPAN: f32 = 9.0;

/// Degrees the phantom advances around its circle each time it picks a point.
///
/// Vanilla parity: the `this.clockwise * 15.0F * (PI / 180)` of `selectNext`.
const CIRCLE_STEP_DEGREES: f32 = 15.0;

/// One chance in this many ticks that a circling phantom changes height.
const CIRCLE_HEIGHT_REROLL_TICKS: i32 = 350;

/// One chance in this many ticks that it widens its circle.
const CIRCLE_RADIUS_REROLL_TICKS: i32 = 250;

/// One chance in this many ticks that it jumps to a fresh angle.
const CIRCLE_ANGLE_REROLL_TICKS: i32 = 450;

/// Squared distance at which a phantom counts as having reached its point.
///
/// Vanilla parity: the `distanceToSqr(...) < 4.0` of `touchingTarget`.
const TOUCHING_TARGET_SQR: f64 = 4.0;

/// Ticks between two cat scans while a phantom is swooping.
///
/// Vanilla parity: `PhantomSweepAttackGoal.CAT_SEARCH_TICK_DELAY`.
const CAT_SEARCH_TICK_DELAY: i32 = 20;

/// How far a swooping phantom looks for a cat, in blocks.
///
/// Vanilla parity: the `inflate(16.0)` of the cat scan.
const CAT_SEARCH_RANGE: f64 = 16.0;

/// How far a swooping phantom's hitbox is inflated when it checks for a hit.
///
/// Vanilla parity: the `inflate(0.2F)` of `PhantomSweepAttackGoal.tick`.
const SWEEP_HIT_INFLATE: f64 = 0.2;

/// Which half of its cycle a phantom is in.
///
/// Vanilla parity: `Phantom.AttackPhase`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AttackPhase {
    /// Wheeling around the anchor point.
    Circle,
    /// Diving through the target.
    Swoop,
}

/// A phantom.
#[entity_behavior(class = "Phantom")]
pub struct PhantomEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    entity_data: SyncMutex<PhantomEntityData>,
    /// The point the move control is flying at (vanilla `moveTargetPoint`).
    move_target_point: SyncMutex<DVec3>,
    /// The point the circle goal wheels around (vanilla `anchorPoint`).
    anchor_point: SyncMutex<Option<BlockPos>>,
    /// Which half of the cycle the phantom is in.
    attack_phase: SyncMutex<AttackPhase>,
    /// The move control's own speed, which vanilla keeps on the control object.
    flight_speed: SyncMutex<f32>,
}

// SAFETY: This key is owned by Foton and uniquely identifies `PhantomEntity`.
unsafe impl DowncastType for PhantomEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:entity/phantom");
}

impl PhantomEntity {
    /// Creates a phantom at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Creates a phantom from saved base data.
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
        let mut entity_data = PhantomEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);
        mob_base.set_xp_reward(XP_REWARD);

        {
            // Keep vanilla Phantom goal priorities in the same order.
            let mut goals = mob_base.goal_selector().lock();
            goals.add_goal(1, PhantomAttackStrategyGoal::new());
            goals.add_goal(2, PhantomSweepAttackGoal::new());
            goals.add_goal(3, PhantomCircleAroundAnchorGoal::new());
        }

        {
            let mut targets = mob_base.target_selector().lock();
            targets.add_goal(1, PhantomAttackPlayerTargetGoal::new());
        }

        Self {
            base,
            entity_type,
            living_base,
            mob_base,
            entity_data: SyncMutex::new(entity_data),
            move_target_point: SyncMutex::new(DVec3::ZERO),
            anchor_point: SyncMutex::new(None),
            attack_phase: SyncMutex::new(AttackPhase::Circle),
            flight_speed: SyncMutex::new(PHANTOM_INITIAL_SPEED),
        }
    }

    /// Returns how big this phantom is, from zero to sixty-four.
    ///
    /// Vanilla parity: `Phantom.getPhantomSize`.
    #[must_use]
    pub fn phantom_size(&self) -> i32 {
        *self.entity_data.lock().id_size.get()
    }

    /// Vanilla parity: `Phantom.setPhantomSize`, together with the
    /// `updatePhantomSizeInfo` that `onSyncedDataUpdated` runs straight after.
    pub fn set_phantom_size(&self, size: i32) {
        self.entity_data.lock().id_size.set(size.clamp(0, MAX_SIZE));
        self.update_phantom_size_info();
    }

    /// Vanilla parity: `Phantom.updatePhantomSizeInfo`. A bigger phantom is
    /// both wider and harder-hitting.
    fn update_phantom_size_info(&self) {
        self.refresh_dimensions();
        self.attributes().lock().set_base_value(
            vanilla_attributes::ATTACK_DAMAGE,
            BASE_ATTACK_DAMAGE + f64::from(self.phantom_size()),
        );
    }

    /// Returns the point the circle goal wheels around.
    #[must_use]
    pub fn anchor_point(&self) -> Option<BlockPos> {
        *self.anchor_point.lock()
    }

    /// Returns the point the move control is flying at.
    #[must_use]
    pub fn move_target_point(&self) -> DVec3 {
        *self.move_target_point.lock()
    }
}

/// Recovers the phantom a goal is running on.
fn phantom_of(mob: &dyn PathfinderMob) -> Option<&PhantomEntity> {
    mob.downcast_ref::<PhantomEntity>()
}

/// Hunts the highest player in range.
///
/// Vanilla parity: `Phantom.PhantomAttackPlayerTargetGoal`. Sorting by height
/// is what makes a phantom pick the player on the roof rather than the one in
/// the cellar.
struct PhantomAttackPlayerTargetGoal {
    targeting: TargetingConditions,
    next_scan_tick: i32,
}

impl PhantomAttackPlayerTargetGoal {
    const fn new() -> Self {
        Self {
            targeting: TargetingConditions::for_combat().range(TARGET_RANGE),
            next_scan_tick: reduced_tick_delay(FIRST_SCAN_DELAY_TICKS),
        }
    }
}

impl Goal for PhantomAttackPlayerTargetGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::TARGET
    }

    /// Vanilla parity: `PhantomAttackPlayerTargetGoal.canUse`.
    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        if self.next_scan_tick > 0 {
            self.next_scan_tick -= 1;
            return false;
        }
        self.next_scan_tick = reduced_tick_delay(SCAN_INTERVAL_TICKS);

        let Some(world) = mob.level() else {
            return false;
        };
        let search_box = mob.bounding_box().inflate_xyz(
            TARGET_SCAN_HORIZONTAL,
            TARGET_SCAN_VERTICAL,
            TARGET_SCAN_HORIZONTAL,
        );
        let level = world.as_ref();
        let mut players: Vec<SharedEntity> =
            world.get_entities_in_aabb_matching(&search_box, |entity| {
                entity.as_player().is_some()
                    && entity
                        .as_living_entity()
                        .is_some_and(|player| self.targeting.test(level, Some(mob), player))
            });
        if players.is_empty() {
            return false;
        }

        // Vanilla sorts by descending Y and takes the first player that passes
        // the default targeting conditions.
        players.sort_by(|left, right| {
            right
                .position()
                .y
                .partial_cmp(&left.position().y)
                .unwrap_or(Ordering::Equal)
        });

        let default_conditions = TargetingConditions::default();
        for player in players {
            let passes = player
                .as_living_entity()
                .is_some_and(|living| default_conditions.test(level, Some(mob), living));
            if passes {
                mob.set_target(Some(&player));
                return true;
            }
        }

        false
    }

    /// Vanilla parity: `PhantomAttackPlayerTargetGoal.canContinueToUse`.
    fn can_continue_to_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        let (Some(world), Some(target)) = (mob.level(), mob.target()) else {
            return false;
        };
        let conditions = TargetingConditions::default();
        target
            .as_living_entity()
            .is_some_and(|living| conditions.test(world.as_ref(), Some(mob), living))
    }
}

/// Alternates between circling and swooping.
///
/// Vanilla parity: `Phantom.PhantomAttackStrategyGoal`. It holds no control at
/// all -- its whole job is to flip the attack phase and move the anchor.
struct PhantomAttackStrategyGoal {
    next_sweep_tick: i32,
}

impl PhantomAttackStrategyGoal {
    const fn new() -> Self {
        Self { next_sweep_tick: 0 }
    }

    /// Moves the anchor high above whatever the phantom is hunting.
    ///
    /// Vanilla parity: `PhantomAttackStrategyGoal.setAnchorAboveTarget`, which
    /// only moves an anchor that already exists.
    fn set_anchor_above_target(mob: &dyn PathfinderMob) {
        let (Some(phantom), Some(target), Some(world)) =
            (phantom_of(mob), mob.target(), mob.level())
        else {
            return;
        };
        let mut anchor = phantom.anchor_point.lock();
        if anchor.is_none() {
            return;
        }

        let above = target
            .block_position()
            .above_n(ANCHOR_ABOVE_TARGET_MIN + rand::random_range(0..ANCHOR_ABOVE_TARGET_SPAN));
        let sea_level = world.sea_level;
        *anchor = Some(if above.y() < sea_level {
            BlockPos::new(above.x(), sea_level + 1, above.z())
        } else {
            above
        });
    }
}

impl Goal for PhantomAttackStrategyGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::EMPTY
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        let (Some(world), Some(target)) = (mob.level(), mob.target()) else {
            return false;
        };
        let conditions = TargetingConditions::default();
        target
            .as_living_entity()
            .is_some_and(|living| conditions.test(world.as_ref(), Some(mob), living))
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        self.next_sweep_tick = FIRST_SWEEP_DELAY_TICKS;
        if let Some(phantom) = phantom_of(mob) {
            *phantom.attack_phase.lock() = AttackPhase::Circle;
        }
        Self::set_anchor_above_target(mob);
    }

    /// Vanilla parity: `PhantomAttackStrategyGoal.stop`, which parks the anchor
    /// well above the terrain so the phantom keeps circling after it gives up.
    fn stop(&mut self, mob: &dyn PathfinderMob) {
        let (Some(phantom), Some(world)) = (phantom_of(mob), mob.level()) else {
            return;
        };
        let mut anchor = phantom.anchor_point.lock();
        let Some(current) = *anchor else {
            return;
        };

        let ground_y = world.heightmap_at(HeightmapType::MotionBlocking, current.x(), current.z());
        *anchor = Some(
            BlockPos::new(current.x(), ground_y, current.z())
                .above_n(ANCHOR_ABOVE_GROUND_MIN + rand::random_range(0..ANCHOR_ABOVE_GROUND_SPAN)),
        );
    }

    /// Vanilla parity: `PhantomAttackStrategyGoal.tick`.
    fn tick(&mut self, mob: &dyn PathfinderMob) {
        let Some(phantom) = phantom_of(mob) else {
            return;
        };
        if *phantom.attack_phase.lock() != AttackPhase::Circle {
            return;
        }

        self.next_sweep_tick -= 1;
        if self.next_sweep_tick > 0 {
            return;
        }

        *phantom.attack_phase.lock() = AttackPhase::Swoop;
        Self::set_anchor_above_target(mob);
        self.next_sweep_tick = (SWEEP_INTERVAL_MIN_SECONDS
            + rand::random_range(0..SWEEP_INTERVAL_SPAN_SECONDS))
            * TICKS_PER_SECOND;
        mob.play_sound(
            &sound_events::ENTITY_PHANTOM_SWOOP,
            10.0,
            0.1f32.mul_add(rand::random::<f32>(), 0.95),
        );
    }
}

/// Wheels around the anchor point.
///
/// Vanilla parity: `Phantom.PhantomCircleAroundAnchorGoal`.
struct PhantomCircleAroundAnchorGoal {
    angle: f32,
    distance: f32,
    height: f32,
    clockwise: f32,
}

impl PhantomCircleAroundAnchorGoal {
    const fn new() -> Self {
        Self {
            angle: 0.0,
            distance: 0.0,
            height: 0.0,
            clockwise: 1.0,
        }
    }

    /// Advances a step around the circle and writes the new point.
    ///
    /// Vanilla parity: `PhantomCircleAroundAnchorGoal.selectNext`.
    fn select_next(&mut self, mob: &dyn PathfinderMob) {
        let Some(phantom) = phantom_of(mob) else {
            return;
        };

        let anchor = {
            let mut anchor = phantom.anchor_point.lock();
            *anchor.get_or_insert_with(|| mob.block_position())
        };

        self.angle += self.clockwise * CIRCLE_STEP_DEGREES.to_radians();
        *phantom.move_target_point.lock() = DVec3::new(
            f64::from(anchor.x()) + f64::from(self.distance * self.angle.cos()),
            f64::from(anchor.y()) + f64::from(CIRCLE_HEIGHT_MIN + self.height),
            f64::from(anchor.z()) + f64::from(self.distance * self.angle.sin()),
        );
    }
}

impl Goal for PhantomCircleAroundAnchorGoal {
    /// Vanilla parity: `PhantomMoveTargetGoal` sets `Goal.Flag.MOVE`.
    fn controls(&self) -> GoalControls {
        GoalControls::MOVE
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        mob.target().is_none()
            || phantom_of(mob)
                .is_some_and(|phantom| *phantom.attack_phase.lock() == AttackPhase::Circle)
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        self.distance = rand::random::<f32>().mul_add(CIRCLE_RADIUS_SPAN, CIRCLE_RADIUS_MIN);
        self.height = rand::random::<f32>().mul_add(CIRCLE_HEIGHT_SPAN, CIRCLE_HEIGHT_MIN);
        self.clockwise = if rand::random::<bool>() { 1.0 } else { -1.0 };
        self.select_next(mob);
    }

    /// Vanilla parity: `PhantomCircleAroundAnchorGoal.tick`.
    fn tick(&mut self, mob: &dyn PathfinderMob) {
        if rand::random_range(0..CIRCLE_HEIGHT_REROLL_TICKS) == 0 {
            self.height = rand::random::<f32>().mul_add(CIRCLE_HEIGHT_SPAN, CIRCLE_HEIGHT_MIN);
        }

        if rand::random_range(0..CIRCLE_RADIUS_REROLL_TICKS) == 0 {
            self.distance += 1.0;
            if self.distance > CIRCLE_RADIUS_MAX {
                self.distance = CIRCLE_RADIUS_MIN;
                self.clockwise = -self.clockwise;
            }
        }

        if rand::random_range(0..CIRCLE_ANGLE_REROLL_TICKS) == 0 {
            self.angle = rand::random::<f32>() * TAU;
            self.select_next(mob);
        }

        let Some(phantom) = phantom_of(mob) else {
            return;
        };
        let Some(world) = mob.level() else {
            return;
        };

        let position = mob.position();
        let target_point = phantom.move_target_point();
        if target_point.distance_squared(position) < TOUCHING_TARGET_SQR {
            self.select_next(mob);
        }

        // Vanilla parity: the phantom refuses to aim into the ground or the
        // ceiling, and flips the height offset the moment it would.
        let block_position = mob.block_position();
        let target_point = phantom.move_target_point();
        if target_point.y < position.y && !world.get_block_state(block_position.below()).is_air() {
            self.height = self.height.max(1.0);
            self.select_next(mob);
        }

        let target_point = phantom.move_target_point();
        if target_point.y > position.y && !world.get_block_state(block_position.above()).is_air() {
            self.height = self.height.min(-1.0);
            self.select_next(mob);
        }
    }
}

/// Dives straight through the target.
///
/// Vanilla parity: `Phantom.PhantomSweepAttackGoal`.
struct PhantomSweepAttackGoal {
    /// Whether a cat has scared the phantom off this swoop.
    is_scared_of_cat: bool,
    /// The tick the next cat scan is due on.
    cat_search_tick: i32,
}

impl PhantomSweepAttackGoal {
    const fn new() -> Self {
        Self {
            is_scared_of_cat: false,
            cat_search_tick: 0,
        }
    }

    /// Makes every living cat within sixteen blocks hiss, and reports whether
    /// there was one.
    ///
    /// Vanilla parity: the cat scan of `PhantomSweepAttackGoal.canContinueToUse`.
    fn hiss_at_nearby_cats(mob: &dyn PathfinderMob) -> bool {
        let Some(world) = mob.level() else {
            return false;
        };

        let search_box = mob.bounding_box().inflate(CAT_SEARCH_RANGE);
        let cats = world.get_entities_in_aabb_matching(&search_box, |entity| {
            entity.is_alive() && entity.downcast_ref::<CatEntity>().is_some()
        });
        for entity in &cats {
            if let Some(cat) = entity.downcast_ref::<CatEntity>() {
                cat.hiss();
            }
        }

        !cats.is_empty()
    }
}

impl Goal for PhantomSweepAttackGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::MOVE
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        mob.target().is_some()
            && phantom_of(mob)
                .is_some_and(|phantom| *phantom.attack_phase.lock() == AttackPhase::Swoop)
    }

    /// Vanilla parity: `PhantomSweepAttackGoal.canContinueToUse`. A cat within
    /// sixteen blocks calls the swoop off, which is the whole reason a cat by
    /// the bed keeps phantoms away.
    fn can_continue_to_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        let Some(target) = mob.target() else {
            return false;
        };
        let alive = target
            .as_living_entity()
            .is_some_and(LivingEntity::is_alive);
        if !alive {
            return false;
        }
        if let Some(player) = target.as_player()
            && (player.is_spectator() || player.has_infinite_materials())
        {
            return false;
        }
        if !self.can_use(mob) {
            return false;
        }

        if mob.tick_count() > self.cat_search_tick {
            self.cat_search_tick = mob.tick_count() + CAT_SEARCH_TICK_DELAY;
            self.is_scared_of_cat = Self::hiss_at_nearby_cats(mob);
        }

        !self.is_scared_of_cat
    }

    fn stop(&mut self, mob: &dyn PathfinderMob) {
        mob.set_target(None);
        if let Some(phantom) = phantom_of(mob) {
            *phantom.attack_phase.lock() = AttackPhase::Circle;
        }
    }

    /// Vanilla parity: `PhantomSweepAttackGoal.tick`.
    fn tick(&mut self, mob: &dyn PathfinderMob) {
        let (Some(phantom), Some(target)) = (phantom_of(mob), mob.target()) else {
            return;
        };

        let target_position = target.position();
        *phantom.move_target_point.lock() = DVec3::new(
            target_position.x,
            target_position.y + target.bounding_box().height() * 0.5,
            target_position.z,
        );

        if mob
            .bounding_box()
            .inflate(SWEEP_HIT_INFLATE)
            .intersects(target.bounding_box())
        {
            if let Some(world) = mob.level() {
                let _ = mob.do_hurt_target(world.as_ref(), &target);
                *phantom.attack_phase.lock() = AttackPhase::Circle;
                if !mob.is_silent() {
                    world.level_event(
                        level_events::SOUND_PHANTOM_BITE,
                        mob.block_position(),
                        0,
                        None,
                    );
                }
            }
            return;
        }

        // Vanilla also breaks the swoop off on `hurtTime > 0`, the red-flash
        // timer. Foton models `invulnerableTime` but not `hurtTime`, so only
        // the wall half of the check is here.
        if mob.horizontal_collision() {
            *phantom.attack_phase.lock() = AttackPhase::Circle;
        }
    }
}

impl Entity for PhantomEntity {
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
        SoundSource::Hostile
    }

    /// Vanilla parity: `Phantom.isFlapping`.
    fn is_flapping(&self) -> bool {
        (self.id() * FLAP_TICK_OFFSET_PER_ID + self.tick_count()) % TICKS_PER_FLAP == 0
    }

    /// Vanilla parity: `Phantom.getDefaultDimensions`, which widens the phantom
    /// by fifteen percent for every size step.
    fn dimensions_for_pose(&self, _pose: EntityPose) -> EntityDimensions {
        self.entity_type
            .dimensions
            .scale(SIZE_SCALE_STEP.mul_add(self.phantom_size() as f32, 1.0))
    }

    /// Vanilla parity: `Phantom.checkFallDamage` is empty.
    fn check_fall_damage(
        &self,
        _vertical_movement: f64,
        _on_ground: bool,
        _on_state: BlockStateId,
        _pos: BlockPos,
        _world: &Arc<World>,
    ) {
    }

    /// Vanilla parity: `Phantom.onClimbable`.
    fn on_climbable(&self) -> bool {
        false
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_mob(nbt);
        if let Some(anchor) = self.anchor_point() {
            nbt.insert(
                "anchor_pos",
                NbtTag::IntArray(vec![anchor.x(), anchor.y(), anchor.z()]),
            );
        }
        nbt.insert("size", self.phantom_size());
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_mob(nbt);
        *self.anchor_point.lock() = nbt
            .int_array("anchor_pos")
            .and_then(|coords| match *coords {
                [x, y, z] => Some(BlockPos::new(x, y, z)),
                _ => None,
            });
        self.set_phantom_size(nbt.int("size").unwrap_or(0));
    }
}

impl LivingEntity for PhantomEntity {
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

    /// Vanilla parity: `Phantom.travel`.
    fn travel(&self, input: DVec3) -> Option<MoveResult> {
        self.travel_flying(input, AIR_TRAVEL_SPEED)
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
        Some(&sound_events::ENTITY_PHANTOM_HURT)
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_PHANTOM_DEATH)
    }
}

impl Mob for PhantomEntity {
    fn mob_base(&self) -> &MobBase {
        &self.mob_base
    }

    fn tick_goal_selectors(&self) {
        PathfinderMob::tick_pathfinder_goal_selectors(self);
    }

    fn tick_path_navigation(&self) {
        PathfinderMob::tick_pathfinder_path_navigation(self);
    }

    /// Vanilla parity: `Phantom` installs a `PhantomMoveControl`.
    fn tick_move_control(&self) {
        let speed = *self.flight_speed.lock();
        let updated = PhantomMoveControl::new(self.move_target_point(), speed).tick(self);
        *self.flight_speed.lock() = updated;
    }

    /// Vanilla parity: `Phantom.PhantomLookControl.tick` is empty; the move
    /// control does all of a phantom's turning.
    fn tick_look_control(&self) {}

    /// Vanilla parity: `Phantom.PhantomBodyRotationControl.clientTick`, which
    /// pins both rotations to the yaw the move control set rather than easing
    /// the body around after the head.
    fn tick_body_rotation_control(&self) {
        self.set_y_head_rot(self.y_body_rot());
        self.set_y_body_rot(self.rotation().0);
    }

    fn ambient_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_PHANTOM_AMBIENT)
    }

    fn mob_flags(&self) -> i8 {
        *self.entity_data.lock().mob().mob_flags.get()
    }

    fn set_mob_flags(&self, flags: i8) {
        self.entity_data.lock().mob_mut().mob_flags.set(flags);
    }

    /// Vanilla parity: `Phantom.finalizeSpawn`.
    fn finalize_spawn(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        group_data: Option<SpawnGroupData>,
    ) -> Option<SpawnGroupData> {
        *self.anchor_point.lock() = Some(self.block_position().above_n(SPAWN_ANCHOR_HEIGHT));
        self.set_phantom_size(0);
        self.finalize_spawn_mob_base(world, spawn_reason, group_data)
    }
}

impl PathfinderMob for PhantomEntity {}

impl Enemy for PhantomEntity {}

#[cfg(test)]
mod tests;
