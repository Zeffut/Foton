//! The long jump: `LongJumpUtil`, `LongJumpToRandomPos`,
//! `LongJumpToPreferredBlock` and `LongJumpMidJump`.
//!
//! Vanilla splits these across four files, but they are one mechanism -- a mob
//! picks a landing spot it cannot walk to, solves a ballistic arc that clears
//! everything between, crouches for two seconds and throws itself -- and the
//! three behaviors only make sense read together.

use std::f64::consts::PI;

use foton_registry::blocks::block_state_ext::BlockStateExt as _;
use foton_registry::entity_type::EntityDimensions;
use foton_registry::sound_event::SoundEventRef;
use foton_registry::vanilla_block_tags::BlockTag;
use foton_registry::{vanilla_attributes, vanilla_blocks};
use foton_utils::value_providers::UniformIntProvider;
use foton_utils::{BlockPos, Identifier};
use glam::DVec3;

use super::{BrainContext, MemoryModuleId, MemoryStatus, TimedBehavior};

use crate::entity::ai::brain::memory::memory_module_types;
use crate::entity::ai::brain::position_tracker::PositionTracker;
use crate::entity::ai::path::PathfindingContext;
use crate::entity::ai::walk::WalkPathEvaluator;
use crate::entity::{EntityPose, PathfinderMob};
use crate::physics::collision::{WorldCollisionProvider, has_collision};

/// Vanilla parity: `LongJumpToRandomPos.FIND_JUMP_TRIES`.
const FIND_JUMP_TRIES: i32 = 20;
/// Vanilla parity: `LongJumpToRandomPos.PREPARE_JUMP_DURATION`.
const PREPARE_JUMP_DURATION: i64 = 40;
/// Vanilla parity: `LongJumpToRandomPos.MIN_PATHFIND_DISTANCE_TO_VALID_JUMP`.
const MIN_PATHFIND_DISTANCE_TO_VALID_JUMP: i32 = 8;
/// Vanilla parity: `LongJumpToRandomPos.TIME_OUT_DURATION`.
const LONG_JUMP_TIME_OUT: i32 = 200;
/// Vanilla parity: `LongJumpMidJump.TIME_OUT_DURATION`.
const MID_JUMP_TIME_OUT: i32 = 100;
/// Vanilla parity: `LongJumpToRandomPos.ALLOWED_ANGLES`, in degrees.
const ALLOWED_ANGLES: [i32; 4] = [65, 70, 75, 80];
/// Vanilla parity: the `scale(0.95F)` that ends `calculateJumpVectorForAngle`.
const JUMP_VELOCITY_SCALE: f64 = 0.95;
/// Vanilla parity: the `0.5` the target is pulled back by so the mob lands on
/// the block rather than into its far face.
const TARGET_PULLBACK: f64 = 0.5;
/// Vanilla parity: the `minDimension * 0.9F` step of `isClearTransition`.
const TRANSITION_STEP_SCALE: f64 = 0.9;
/// Vanilla parity: the `multiply(0.1F, 1.0, 0.1F)` a landing mob is slowed by.
const LANDING_HORIZONTAL_DAMPING: f64 = 0.1;
/// Vanilla parity: the `2.0F` volume of the landing sound.
const LANDING_SOUND_VOLUME: f32 = 2.0;

/// A candidate landing block and how heavily it is weighted.
///
/// Vanilla parity: `LongJumpToRandomPos.PossibleJump`, whose weight is the
/// squared distance -- so a mob prefers the longest jump it can make.
#[derive(Debug, Clone, Copy)]
struct PossibleJump {
    target_pos: BlockPos,
    weight: i32,
}

/// Whether a mob will accept landing on a given block.
///
/// Vanilla parity: the `BiPredicate<E, BlockPos> acceptableLandingSpot`.
type AcceptableLandingSpot = Box<dyn Fn(&dyn PathfinderMob, BlockPos) -> bool + Send>;

/// Vanilla parity: `LongJumpToRandomPos.defaultAcceptableLandingSpot`.
#[must_use]
pub fn default_acceptable_landing_spot(body: &dyn PathfinderMob, target_pos: BlockPos) -> bool {
    let Some(world) = body.level() else {
        return false;
    };
    if !world.get_block_state(target_pos.below()).is_solid_render() {
        return false;
    }

    let mut context = PathfindingContext::new(world.as_ref(), body.block_position());
    let path_type = WalkPathEvaluator::path_type_static(&mut context, target_pos);
    body.get_pathfinding_malus(path_type) == 0.0
}

/// Crouches, solves an arc, and throws the mob along it.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.behavior.LongJumpToRandomPos`.
pub struct LongJumpToRandomPos {
    time_between_long_jumps: UniformIntProvider,
    max_long_jump_height: i32,
    max_long_jump_width: i32,
    max_jump_velocity_multiplier: f32,
    jump_sound: SoundEventRef,
    acceptable_landing_spot: AcceptableLandingSpot,
    /// Vanilla parity: `LongJumpToPreferredBlock.preferredBlockTag`, folded in
    /// rather than subclassed -- the two differ only in how a candidate is
    /// drawn, and Rust has no `super` to call from an override.
    preferred_block_tag: Option<Identifier>,
    preferred_blocks_chance: f32,

    jump_candidates: Vec<PossibleJump>,
    not_preferred_jump_candidates: Vec<PossibleJump>,
    currently_wanting_preferred_ones: bool,
    initial_position: Option<DVec3>,
    chosen_jump: Option<DVec3>,
    find_jump_tries: i32,
    prepare_jump_start: i64,
}

/// Vanilla parity: the `entryCondition` map of `LongJumpToRandomPos`.
const LONG_JUMP_ENTRY_CONDITION: &[(MemoryModuleId, MemoryStatus)] = &[
    (
        memory_module_types::LOOK_TARGET.id(),
        MemoryStatus::Registered,
    ),
    (
        memory_module_types::LONG_JUMP_COOLDOWN_TICKS.id(),
        MemoryStatus::ValueAbsent,
    ),
    (
        memory_module_types::LONG_JUMP_MID_JUMP.id(),
        MemoryStatus::ValueAbsent,
    ),
];

impl LongJumpToRandomPos {
    /// Jumps to any block it cannot walk to.
    ///
    /// Vanilla parity: the five-argument `LongJumpToRandomPos` constructor.
    #[must_use]
    pub fn new(
        time_between_long_jumps: UniformIntProvider,
        max_long_jump_height: i32,
        max_long_jump_width: i32,
        max_jump_velocity_multiplier: f32,
        jump_sound: SoundEventRef,
    ) -> Self {
        Self {
            time_between_long_jumps,
            max_long_jump_height,
            max_long_jump_width,
            max_jump_velocity_multiplier,
            jump_sound,
            acceptable_landing_spot: Box::new(default_acceptable_landing_spot),
            preferred_block_tag: None,
            preferred_blocks_chance: 0.0,
            jump_candidates: Vec::new(),
            not_preferred_jump_candidates: Vec::new(),
            currently_wanting_preferred_ones: false,
            initial_position: None,
            chosen_jump: None,
            find_jump_tries: 0,
            prepare_jump_start: 0,
        }
    }

    /// Replaces the test for a block worth landing on.
    ///
    /// Vanilla parity: the six-argument constructor's `acceptableLandingSpot`.
    #[must_use]
    pub fn with_acceptable_landing_spot(
        mut self,
        acceptable_landing_spot: impl Fn(&dyn PathfinderMob, BlockPos) -> bool + Send + 'static,
    ) -> Self {
        self.acceptable_landing_spot = Box::new(acceptable_landing_spot);
        self
    }

    /// Aims most jumps at one kind of block.
    ///
    /// Vanilla parity: `LongJumpToPreferredBlock`, which is how a frog ends up
    /// on the lily pads and big dripleaves rather than anywhere it can reach.
    #[must_use]
    pub fn preferring(mut self, preferred_block_tag: Identifier, chance: f32) -> Self {
        self.preferred_block_tag = Some(preferred_block_tag);
        self.preferred_blocks_chance = chance;
        self
    }

    /// Puts the cooldown on at half length, the way vanilla does whenever the
    /// jump cannot start or cannot continue.
    fn set_half_cooldown(&self, ctx: &BrainContext<'_>) {
        ctx.brain().set_memory(
            memory_module_types::LONG_JUMP_COOLDOWN_TICKS,
            sample(self.time_between_long_jumps) / 2,
        );
    }

    /// Vanilla parity: `LongJumpToRandomPos.getJumpCandidate` plus the
    /// `LongJumpToPreferredBlock` override.
    fn take_jump_candidate(&mut self, ctx: &BrainContext<'_>) -> Option<PossibleJump> {
        if !self.currently_wanting_preferred_ones {
            return take_weighted(&mut self.jump_candidates);
        }

        let Some(tag) = self.preferred_block_tag.clone() else {
            return take_weighted(&mut self.jump_candidates);
        };
        let world = ctx.world();

        while let Some(candidate) = take_weighted(&mut self.jump_candidates) {
            if world
                .get_block_state(candidate.target_pos.below())
                .get_block()
                .has_tag(&tag)
            {
                return Some(candidate);
            }
            self.not_preferred_jump_candidates.push(candidate);
        }

        if self.not_preferred_jump_candidates.is_empty() {
            None
        } else {
            Some(self.not_preferred_jump_candidates.remove(0))
        }
    }

    /// Vanilla parity: `LongJumpToRandomPos.pickCandidate`.
    fn pick_candidate(&mut self, ctx: &BrainContext<'_>, timestamp: i64) {
        while !self.jump_candidates.is_empty() || !self.not_preferred_jump_candidates.is_empty() {
            let Some(candidate) = self.take_jump_candidate(ctx) else {
                return;
            };
            let target_pos = candidate.target_pos;
            if !self.is_acceptable_landing_position(ctx, target_pos) {
                continue;
            }

            let (cx, cy, cz) = target_pos.get_center();
            let Some(jump_vector) = self.calculate_optimal_jump_vector(ctx, DVec3::new(cx, cy, cz))
            else {
                continue;
            };

            ctx.brain().set_memory(
                memory_module_types::LOOK_TARGET,
                PositionTracker::of_block(target_pos),
            );
            // Vanilla only jumps to somewhere it could not simply walk to.
            let walkable = ctx.mob().create_path_to(target_pos, 0).is_some_and(|path| {
                path.can_reach()
                    && path.node_count() <= MIN_PATHFIND_DISTANCE_TO_VALID_JUMP as usize
            });
            if !walkable {
                self.chosen_jump = Some(jump_vector);
                self.prepare_jump_start = timestamp;
                return;
            }
        }
    }

    /// Vanilla parity: `LongJumpToRandomPos.isAcceptableLandingPosition`, which
    /// refuses a block directly above or below the mob.
    fn is_acceptable_landing_position(&self, ctx: &BrainContext<'_>, target_pos: BlockPos) -> bool {
        let body_pos = ctx.mob().block_position();
        if body_pos.x() == target_pos.x() && body_pos.z() == target_pos.z() {
            return false;
        }
        (self.acceptable_landing_spot)(ctx.mob(), target_pos)
    }

    /// Vanilla parity: `LongJumpToRandomPos.calculateOptimalJumpVector`, which
    /// tries the four launch angles in a random order and takes the first that
    /// both reaches and clears.
    fn calculate_optimal_jump_vector(
        &self,
        ctx: &BrainContext<'_>,
        target_pos: DVec3,
    ) -> Option<DVec3> {
        let body = ctx.mob();
        let mut angles = ALLOWED_ANGLES;
        for index in (1..angles.len()).rev() {
            angles.swap(index, rand::random_range(0..=index));
        }

        let jump_strength = body
            .attributes()
            .lock()
            .required_value(vanilla_attributes::JUMP_STRENGTH);
        let max_jump_velocity = jump_strength * f64::from(self.max_jump_velocity_multiplier);

        angles.into_iter().find_map(|angle| {
            calculate_jump_vector_for_angle(body, target_pos, max_jump_velocity, angle, true)
        })
    }
}

impl TimedBehavior for LongJumpToRandomPos {
    fn entry_condition(&self) -> &[(MemoryModuleId, MemoryStatus)] {
        LONG_JUMP_ENTRY_CONDITION
    }

    fn duration(&self) -> (i32, i32) {
        (LONG_JUMP_TIME_OUT, LONG_JUMP_TIME_OUT)
    }

    fn check_extra_start_conditions(&mut self, ctx: &BrainContext<'_>) -> bool {
        let body = ctx.mob();
        let on_honey = ctx
            .world()
            .get_block_state(body.block_position())
            .get_block()
            == &vanilla_blocks::HONEY_BLOCK;
        let can_start = body.on_ground() && !body.is_in_water() && !body.is_in_lava() && !on_honey;
        if !can_start {
            self.set_half_cooldown(ctx);
        }
        can_start
    }

    fn can_still_use(&mut self, ctx: &BrainContext<'_>) -> bool {
        let body = ctx.mob();
        let is_valid = self.initial_position == Some(body.position())
            && self.find_jump_tries > 0
            && !body.is_in_water()
            && (self.chosen_jump.is_some() || !self.jump_candidates.is_empty());

        if !is_valid
            && !ctx
                .brain()
                .has_memory_value(memory_module_types::LONG_JUMP_MID_JUMP.id())
        {
            self.set_half_cooldown(ctx);
            ctx.brain()
                .erase_memory(memory_module_types::LOOK_TARGET.id());
        }

        is_valid
    }

    fn start(&mut self, ctx: &BrainContext<'_>) {
        let body = ctx.mob();
        self.chosen_jump = None;
        self.find_jump_tries = FIND_JUMP_TRIES;
        self.initial_position = Some(body.position());
        self.not_preferred_jump_candidates.clear();
        self.currently_wanting_preferred_ones =
            rand::random::<f32>() < self.preferred_blocks_chance;

        let mob_pos = body.block_position();
        self.jump_candidates.clear();
        for x in (mob_pos.x() - self.max_long_jump_width)..=(mob_pos.x() + self.max_long_jump_width)
        {
            for y in (mob_pos.y() - self.max_long_jump_height)
                ..=(mob_pos.y() + self.max_long_jump_height)
            {
                for z in (mob_pos.z() - self.max_long_jump_width)
                    ..=(mob_pos.z() + self.max_long_jump_width)
                {
                    let pos = BlockPos::new(x, y, z);
                    if pos == mob_pos {
                        continue;
                    }
                    let weight = distance_squared_ceil(mob_pos, pos);
                    self.jump_candidates.push(PossibleJump {
                        target_pos: pos,
                        weight,
                    });
                }
            }
        }
    }

    fn tick(&mut self, ctx: &BrainContext<'_>) {
        let timestamp = ctx.game_time();
        let Some(chosen_jump) = self.chosen_jump else {
            self.find_jump_tries -= 1;
            self.pick_candidate(ctx, timestamp);
            return;
        };

        if timestamp - self.prepare_jump_start < PREPARE_JUMP_DURATION {
            return;
        }

        let body = ctx.mob();
        let (_, pitch) = body.rotation();
        body.set_rotation((body.y_body_rot(), pitch));
        body.set_discard_friction(true);

        // Vanilla parity: the jump boost lengthens the arc without changing its
        // direction.
        let length = chosen_jump.length();
        let boosted = length + f64::from(body.get_jump_boost_power());
        body.set_velocity(chosen_jump * (boosted / length));

        ctx.brain()
            .set_memory(memory_module_types::LONG_JUMP_MID_JUMP, true);
        body.play_sound(self.jump_sound, 1.0, 1.0);
    }

    fn debug_name(&self) -> &'static str {
        "LongJumpToRandomPos"
    }
}

/// Holds the mob's pose while it is in the air and lands it.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.behavior.LongJumpMidJump`.
pub struct LongJumpMidJump {
    time_between_long_jumps: UniformIntProvider,
    landing_sound: SoundEventRef,
}

/// Vanilla parity: the `entryCondition` map of `LongJumpMidJump`.
const MID_JUMP_ENTRY_CONDITION: &[(MemoryModuleId, MemoryStatus)] = &[
    (
        memory_module_types::LOOK_TARGET.id(),
        MemoryStatus::Registered,
    ),
    (
        memory_module_types::LONG_JUMP_MID_JUMP.id(),
        MemoryStatus::ValuePresent,
    ),
];

impl LongJumpMidJump {
    /// Vanilla parity: `new LongJumpMidJump(UniformInt, SoundEvent)`.
    #[must_use]
    pub const fn new(
        time_between_long_jumps: UniformIntProvider,
        landing_sound: SoundEventRef,
    ) -> Self {
        Self {
            time_between_long_jumps,
            landing_sound,
        }
    }
}

impl TimedBehavior for LongJumpMidJump {
    fn entry_condition(&self) -> &[(MemoryModuleId, MemoryStatus)] {
        MID_JUMP_ENTRY_CONDITION
    }

    fn duration(&self) -> (i32, i32) {
        (MID_JUMP_TIME_OUT, MID_JUMP_TIME_OUT)
    }

    fn can_still_use(&mut self, ctx: &BrainContext<'_>) -> bool {
        !ctx.mob().on_ground()
    }

    fn start(&mut self, ctx: &BrainContext<'_>) {
        let body = ctx.mob();
        body.set_discard_friction(true);
        body.set_pose(EntityPose::LongJumping);
    }

    fn stop(&mut self, ctx: &BrainContext<'_>) {
        let body = ctx.mob();
        if body.on_ground() {
            let velocity = body.velocity();
            body.set_velocity(DVec3::new(
                velocity.x * LANDING_HORIZONTAL_DAMPING,
                velocity.y,
                velocity.z * LANDING_HORIZONTAL_DAMPING,
            ));
            body.play_sound(self.landing_sound, LANDING_SOUND_VOLUME, 1.0);
        }

        body.set_discard_friction(false);
        body.set_pose(EntityPose::Standing);
        ctx.brain()
            .erase_memory(memory_module_types::LONG_JUMP_MID_JUMP.id());
        ctx.brain().set_memory(
            memory_module_types::LONG_JUMP_COOLDOWN_TICKS,
            sample(self.time_between_long_jumps),
        );
    }

    fn debug_name(&self) -> &'static str {
        "LongJumpMidJump"
    }
}

/// Vanilla parity: `LongJumpUtil.calculateJumpVectorForAngle`.
///
/// Solves the projectile equation for the launch speed that lands on
/// `target_pos` at `angle`, then walks the resulting arc to check it clears.
/// `None` means the target is out of reach at this angle, or something is in
/// the way.
#[must_use]
pub fn calculate_jump_vector_for_angle(
    body: &dyn PathfinderMob,
    target_pos: DVec3,
    max_jump_velocity: f64,
    angle: i32,
    check_collision: bool,
) -> Option<DVec3> {
    let mob_pos = body.position();
    let direction_plane = DVec3::new(target_pos.x - mob_pos.x, 0.0, target_pos.z - mob_pos.z)
        .normalize_or_zero()
        * TARGET_PULLBACK;
    let direction = (target_pos - direction_plane) - mob_pos;

    let angrad = f64::from(angle) * PI / 180.0;
    let xz_ang = direction.z.atan2(direction.x);
    let horizontal = DVec3::new(direction.x, 0.0, direction.z);
    let r2 = horizontal.length_squared();
    let r = r2.sqrt();
    let y = direction.y;
    let g = body.get_gravity();

    let sin2ang = (2.0 * angrad).sin();
    let cosangsqr = angrad.cos().powi(2);
    let sinangrad = angrad.sin();
    let cosangrad = angrad.cos();
    let sinxz_ang = xz_ang.sin();
    let cosxz_ang = xz_ang.cos();

    let v0sqr = r2 * g / (r * sin2ang - 2.0 * y * cosangsqr);
    if v0sqr < 0.0 {
        return None;
    }
    let v0 = v0sqr.sqrt();
    if v0 > max_jump_velocity {
        return None;
    }

    let v0r = v0 * cosangrad;
    let v0y = v0 * sinangrad;

    if check_collision {
        let samples = (r / v0r).ceil() as i32 * 2;
        let mut ri = 0.0;
        let mut previous_pos: Option<DVec3> = None;
        let dimensions = body.dimensions_for_pose(EntityPose::LongJumping);

        for _ in 0..(samples - 1) {
            ri += r / f64::from(samples);
            let yi =
                sinangrad / cosangrad * ri - ri.powi(2) * g / (2.0 * v0sqr * cosangrad.powi(2));
            let sample_pos = DVec3::new(
                mob_pos.x + ri * cosxz_ang,
                mob_pos.y + yi,
                mob_pos.z + ri * sinxz_ang,
            );
            if let Some(previous) = previous_pos
                && !is_clear_transition(body, dimensions, previous, sample_pos)
            {
                return None;
            }
            previous_pos = Some(sample_pos);
        }
    }

    Some(DVec3::new(v0r * cosxz_ang, v0y, v0r * sinxz_ang) * JUMP_VELOCITY_SCALE)
}

/// Vanilla parity: `LongJumpUtil.isClearTransition`, which steps the mob's own
/// hitbox along one segment of the arc.
fn is_clear_transition(
    body: &dyn PathfinderMob,
    dimensions: EntityDimensions,
    position1: DVec3,
    position2: DVec3,
) -> bool {
    let Some(world) = body.level() else {
        return false;
    };
    let direction = position2 - position1;
    let min_dimension = f64::from(dimensions.width.min(dimensions.height));
    if min_dimension <= 0.0 {
        return true;
    }
    let checks = (direction.length() / min_dimension).ceil() as i32;
    let normalized = direction.normalize_or_zero();
    let mut next = position1;

    for index in 0..checks {
        next = if index == checks - 1 {
            position2
        } else {
            next + normalized * (min_dimension * TRANSITION_STEP_SCALE)
        };
        if has_collision(
            &WorldCollisionProvider::new(&world),
            body.make_bounding_box_at(next),
        ) {
            return false;
        }
    }

    true
}

/// Vanilla parity: `WeightedRandom.getRandomItem` followed by the `remove`.
fn take_weighted(candidates: &mut Vec<PossibleJump>) -> Option<PossibleJump> {
    let total: i64 = candidates.iter().map(|jump| i64::from(jump.weight)).sum();
    if total <= 0 {
        return (!candidates.is_empty()).then(|| candidates.remove(0));
    }

    let mut roll = rand::random_range(0..total);
    for (index, candidate) in candidates.iter().enumerate() {
        roll -= i64::from(candidate.weight);
        if roll < 0 {
            return Some(candidates.remove(index));
        }
    }
    None
}

/// Draws one value from an inclusive range.
///
/// Vanilla parity: `UniformInt.sample(RandomSource)`. Foton's provider takes an
/// explicit generator; the jump cooldown is incidental live randomness, so it
/// uses the runtime one rather than a seeded stream.
fn sample(provider: UniformIntProvider) -> i32 {
    rand::random_range(provider.min_inclusive..=provider.max_inclusive)
}

/// Vanilla parity: the `Mth.ceil(mobPos.distSqr(pos))` weight.
fn distance_squared_ceil(from: BlockPos, to: BlockPos) -> i32 {
    let dx = f64::from(to.x() - from.x());
    let dy = f64::from(to.y() - from.y());
    let dz = f64::from(to.z() - from.z());
    dx.mul_add(dx, dy.mul_add(dy, dz * dz)).ceil() as i32
}

/// Marks the block tag a mob prefers to land on.
///
/// Vanilla parity: `BlockTags.FROG_PREFER_JUMP_TO`, named here so the frog does
/// not have to reach into the tag module for a single constant.
#[must_use]
pub const fn frog_prefer_jump_to() -> Identifier {
    BlockTag::FROG_PREFER_JUMP_TO
}
