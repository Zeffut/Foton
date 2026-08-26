//! The goals a bee is built from.
//!
//! Vanilla parity: the inner classes of `Bee`. Almost all of them extend
//! `Bee.BaseBeeGoal`, whose only job is to switch the whole set off while the
//! bee is angry -- an angry bee stops keeping house and attacks.

use std::collections::HashMap;
use std::f64::consts::{FRAC_PI_2, PI};

use glam::DVec3;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::vanilla_block_tags::BlockTag;
use steel_registry::vanilla_poi_type_tags::PoiTag;
use steel_registry::{REGISTRY, RegistryExt as _, TaggedRegistryExt as _, sound_events};
use steel_utils::types::UpdateFlags;
use steel_utils::{BlockPos, ChunkPos, Downcast as _};

use crate::behavior::BLOCK_BEHAVIORS;
use crate::block_entity::entities::BeehiveBlockEntity;
use crate::entity::ai::goal::{
    Goal, GoalControls, HurtByTargetGoal, MeleeAttackGoal, NearestAttackableTargetGoal,
    air_and_water_random_pos, air_random_pos_towards, hover_random_pos,
};
use crate::entity::ai::path::Path;
use crate::entity::neutral_mob::NeutralMob;
use crate::entity::{Entity, Mob, PathfinderMob};
use crate::poi::poi_storage::OccupationStatus;
use crate::world::LevelReader;

use super::{
    BeeEntity, COOLDOWN_BEFORE_LOCATING_NEW_FLOWER, COOLDOWN_BEFORE_LOCATING_NEW_HIVE,
    HIVE_CLOSE_ENOUGH_DISTANCE, MAX_CROPS_GROWABLE, MAX_FIND_FLOWER_RETRY_COOLDOWN,
    MIN_FIND_FLOWER_RETRY_COOLDOWN, TICKS_BEFORE_GOING_TO_KNOWN_FLOWER,
};

/// Vanilla `Bee.BeeGoToHiveGoal.MAX_TRAVELLING_TICKS`, shared with the flower goal.
const MAX_TRAVELLING_TICKS: i32 = 2400;
/// Vanilla `Bee.BeeGoToHiveGoal.MAX_BLACKLISTED_TARGETS`.
const MAX_BLACKLISTED_TARGETS: usize = 3;
/// Vanilla `Bee.BeeGoToHiveGoal.TICKS_BEFORE_HIVE_DROP`.
const TICKS_BEFORE_HIVE_DROP: i32 = 60;
/// Vanilla `Bee.PATHFIND_TO_HIVE_WHEN_CLOSER_THAN`.
const PATHFIND_TO_HIVE_WHEN_CLOSER_THAN: i32 = 16;
/// Vanilla `Bee.HIVE_SEARCH_DISTANCE`.
const HIVE_SEARCH_DISTANCE: i32 = 20;

/// Vanilla `Bee.BeePollinateGoal.MIN_POLLINATION_TICKS`.
const MIN_POLLINATION_TICKS: i32 = 400;
/// Vanilla `Bee.BeePollinateGoal.MAX_POLLINATING_TICKS`.
const MAX_POLLINATING_TICKS: i32 = 600;
/// Vanilla `Bee.BeePollinateGoal.ARRIVAL_THRESHOLD`.
const ARRIVAL_THRESHOLD: f64 = 0.1;
/// Vanilla `Bee.BeePollinateGoal.POSITION_CHANGE_CHANCE`.
const POSITION_CHANGE_CHANCE: i32 = 25;
/// Vanilla `Bee.BeePollinateGoal.SPEED_MODIFIER`.
const POLLINATE_SPEED_MODIFIER: f64 = 0.35;
/// Vanilla `Bee.BeePollinateGoal.HOVER_HEIGHT_WITHIN_FLOWER`.
const HOVER_HEIGHT_WITHIN_FLOWER: f64 = 0.6;
/// Vanilla `Bee.BeePollinateGoal.HOVER_POS_OFFSET`.
const HOVER_POS_OFFSET: f32 = 0.333_333_34;
/// Vanilla `Bee.BeePollinateGoal.FLOWER_SEARCH_RADIUS`.
const FLOWER_SEARCH_RADIUS: i32 = 5;
/// How long an unreachable flower stays out of the search.
///
/// Vanilla parity: the `getGameTime() + 600L` of `findNearbyFlower`.
const UNREACHABLE_FLOWER_COOLDOWN: i64 = 600;
/// The speed a bee approaches the flower it has just chosen at.
const POLLINATE_APPROACH_SPEED: f64 = 1.2;
/// How far from the flower the bee switches from pathing to hovering.
const POLLINATE_HOVER_RANGE: f64 = 1.0;
/// Chance per tick that a pollinating bee buzzes.
const POLLINATE_SOUND_CHANCE: f32 = 0.05;
/// Shortest gap between two pollination buzzes.
const POLLINATE_SOUND_INTERVAL: i32 = 60;
/// Chance per tick a bee that has enough nectar actually leaves.
///
/// Vanilla parity: the `random.nextFloat() < 0.2F` of `canBeeContinueToUse`.
const POLLINATE_LEAVE_CHANCE: f32 = 0.2;

/// Vanilla `Bee.BeeGrowCropGoal.GROW_CHANCE`.
const GROW_CHANCE: i32 = 30;
/// The odds a grow tick is skipped outright.
///
/// Vanilla parity: the `random.nextFloat() < 0.3F` of `BeeGrowCropGoal.canBeeUse`.
const GROW_SKIP_CHANCE: f32 = 0.3;
/// The `15` data of the plant-growth level event a grown crop plays.
const GROWTH_PARTICLE_COUNT: i32 = 15;
/// How far below itself a bee looks for something to grow.
///
/// Vanilla parity: the `for (int i = 1; i <= 2; i++)` of `BeeGrowCropGoal.tick`.
const GROW_DEPTH: i32 = 2;

/// Vanilla `Bee.RESTRICTED_WANDER_DISTANCE_REDUCTION`.
const RESTRICTED_WANDER_DISTANCE_REDUCTION: i32 = 24;
/// Vanilla `Bee.DEFAULT_WANDER_DISTANCE_REDUCTION`.
const DEFAULT_WANDER_DISTANCE_REDUCTION: i32 = 16;
/// Vanilla `Bee.TOO_FAR_DISTANCE`, the ceiling the wander threshold subtracts from.
const WANDER_MAX_DISTANCE: i32 = 48;
/// One in this many idle ticks a bee decides to wander.
const WANDER_CHANCE: i32 = 10;
/// How far a wandering bee looks.
const WANDER_HORIZONTAL_DIST: i32 = 8;
/// How high a wandering bee will perch.
const WANDER_HOVER_VERTICAL_DIST: i32 = 7;
/// The hover band a wandering bee settles into.
const WANDER_HOVER_MAX_HEIGHT: i32 = 3;
/// The floor of that band.
const WANDER_HOVER_MIN_HEIGHT: i32 = 1;
/// How high the fallback air search reaches.
const WANDER_AIR_VERTICAL_DIST: i32 = 4;
/// The offset that fallback search starts from.
const WANDER_AIR_FLYING_HEIGHT: i32 = -2;

/// Vanilla `Bee.pathfindRandomlyTowards`'s wide search box.
const PATHFIND_TOWARDS_XZ_DIST: i32 = 6;
/// The vertical half of that box.
const PATHFIND_TOWARDS_Y_DIST: i32 = 8;
/// Below this Manhattan distance the box shrinks to half the remaining gap.
const PATHFIND_TOWARDS_NARROW_BELOW: i32 = 15;
/// How far above or below a distant target the search is offset.
const PATHFIND_TOWARDS_Y_ADJUST: i32 = 4;
/// The vertical gap that offset kicks in past.
const PATHFIND_TOWARDS_Y_TRIGGER: i32 = 2;
/// How much of its node budget a randomly-steering bee spends.
const PATHFIND_TOWARDS_NODE_MULTIPLIER: f32 = 0.5;
/// How much of it a bee homing in on the hive spends.
const PATHFIND_DIRECTLY_NODE_MULTIPLIER: f32 = 10.0;
/// How close the direct approach asks to get once it is nearly there.
const PATHFIND_DIRECTLY_CLOSE_RANGE: i32 = 3;

/// Shortest gap between two hive or flower validations.
const VALIDATE_COOLDOWN_MIN: i32 = 20;
/// Longest gap between them.
const VALIDATE_COOLDOWN_MAX: i32 = 40;

/// Returns the bee behind a goal's mob handle.
fn bee_of(mob: &dyn PathfinderMob) -> Option<&BeeEntity> {
    mob.downcast_ref::<BeeEntity>()
}

/// Returns whether a bee is calm enough to run a `BaseBeeGoal`.
///
/// Vanilla parity: `Bee.BaseBeeGoal.canUse`, which is `canBeeUse() && !isAngry()`.
fn base_bee_goal_allows(mob: &dyn PathfinderMob) -> Option<&BeeEntity> {
    let bee = bee_of(mob)?;
    (!bee.is_angry()).then_some(bee)
}

/// Vanilla parity: `Bee.pathfindRandomlyTowards`, the wide arc a bee takes
/// toward something it cannot yet path to directly.
fn pathfind_randomly_towards(bee: &BeeEntity, target_pos: BlockPos) {
    let (tx, ty, tz) = target_pos.get_bottom_center();
    let target_vec = DVec3::new(tx, ty, tz);
    let bee_pos = bee.block_position();

    let y_delta = target_vec.y as i32 - bee_pos.y();
    let y_adjust = if y_delta > PATHFIND_TOWARDS_Y_TRIGGER {
        PATHFIND_TOWARDS_Y_ADJUST
    } else if y_delta < -PATHFIND_TOWARDS_Y_TRIGGER {
        -PATHFIND_TOWARDS_Y_ADJUST
    } else {
        0
    };

    let manhattan = (bee_pos.x() - target_pos.x()).abs()
        + (bee_pos.y() - target_pos.y()).abs()
        + (bee_pos.z() - target_pos.z()).abs();
    let (xz_dist, y_dist) = if manhattan < PATHFIND_TOWARDS_NARROW_BELOW {
        (manhattan / 2, manhattan / 2)
    } else {
        (PATHFIND_TOWARDS_XZ_DIST, PATHFIND_TOWARDS_Y_DIST)
    };

    let Some(next) = air_random_pos_towards(bee, xz_dist, y_dist, y_adjust, target_vec, PI / 10.0)
    else {
        return;
    };

    bee.mob_base()
        .navigation()
        .lock()
        .set_max_visited_nodes_multiplier(PATHFIND_TOWARDS_NODE_MULTIPLIER);
    bee.move_to_pos(next, 1.0);
}

/// Returns whether `pos` sits in a chunk the bee can actually read.
///
/// Vanilla parity: the `level().isLoaded(pos)` guard both validate goals use.
fn is_loaded(bee: &BeeEntity, pos: BlockPos) -> bool {
    bee.level()
        .is_some_and(|world| world.has_full_chunk(ChunkPos::from_block_pos(pos)))
}

/// Vanilla parity: `Bee.BeeAttackGoal`, a melee goal that only runs while the
/// bee is angry and has not already spent its sting.
pub(super) struct BeeAttackGoal {
    inner: MeleeAttackGoal,
}

impl BeeAttackGoal {
    pub(super) const fn new(speed_modifier: f64) -> Self {
        Self {
            inner: MeleeAttackGoal::new(speed_modifier, true),
        }
    }

    fn bee_can_attack(mob: &dyn PathfinderMob) -> bool {
        bee_of(mob).is_some_and(|bee| bee.is_angry() && !bee.has_stung())
    }
}

impl Goal for BeeAttackGoal {
    fn controls(&self) -> GoalControls {
        self.inner.controls()
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        self.inner.can_use(mob) && Self::bee_can_attack(mob)
    }

    fn can_continue_to_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        self.inner.can_continue_to_use(mob) && Self::bee_can_attack(mob)
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        self.inner.start(mob);
    }

    fn stop(&mut self, mob: &dyn PathfinderMob) {
        self.inner.stop(mob);
    }

    fn requires_update_every_tick(&self) -> bool {
        self.inner.requires_update_every_tick()
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        self.inner.tick(mob);
    }
}

/// Vanilla parity: `Bee.BeeEnterHiveGoal`, one tick long: the bee is at the
/// door, so it goes in and stops being an entity.
pub(super) struct BeeEnterHiveGoal;

impl BeeEnterHiveGoal {
    pub(super) const fn new() -> Self {
        Self
    }
}

impl Goal for BeeEnterHiveGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::EMPTY
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        let Some(bee) = base_bee_goal_allows(mob) else {
            return false;
        };
        let Some(hive_pos) = bee.hive_pos() else {
            return false;
        };
        if !bee.wants_to_enter_hive() {
            return false;
        }
        let (cx, cy, cz) = hive_pos.get_center();
        if DVec3::new(cx, cy, cz).distance_squared(bee.position())
            >= HIVE_CLOSE_ENOUGH_DISTANCE * HIVE_CLOSE_ENOUGH_DISTANCE
        {
            return false;
        }

        let Some(is_full) = bee.with_beehive(BeehiveBlockEntity::is_full) else {
            return false;
        };
        if !is_full {
            return true;
        }

        // A hive that filled up while the bee was flying home is forgotten
        // outright rather than queued for.
        bee.clear_hive_pos();
        false
    }

    fn can_continue_to_use(&mut self, _mob: &dyn PathfinderMob) -> bool {
        false
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        let Some(bee) = bee_of(mob) else {
            return;
        };
        bee.with_beehive(|hive| hive.add_occupant(bee));
    }
}

/// Vanilla parity: `Bee.ValidateHiveGoal`, which is how a bee notices its hive
/// has been mined rather than flying to where it used to be.
pub(super) struct ValidateHiveGoal {
    cooldown: i32,
    last_validate_tick: i64,
}

impl ValidateHiveGoal {
    pub(super) fn new() -> Self {
        Self {
            cooldown: rand::random_range(VALIDATE_COOLDOWN_MIN..VALIDATE_COOLDOWN_MAX),
            last_validate_tick: -1,
        }
    }
}

impl Goal for ValidateHiveGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::EMPTY
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        let Some(bee) = base_bee_goal_allows(mob) else {
            return false;
        };
        bee.level().is_some_and(|world| {
            world.game_time() > self.last_validate_tick + i64::from(self.cooldown)
        })
    }

    fn can_continue_to_use(&mut self, _mob: &dyn PathfinderMob) -> bool {
        false
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        let Some(bee) = bee_of(mob) else {
            return;
        };
        let Some(world) = bee.level() else {
            return;
        };
        if let Some(hive_pos) = bee.hive_pos()
            && is_loaded(bee, hive_pos)
            && !bee.is_hive_valid()
        {
            bee.drop_hive();
        }
        self.last_validate_tick = world.game_time();
    }
}

/// Vanilla parity: `Bee.ValidateFlowerGoal`, the same idea for the flower.
pub(super) struct ValidateFlowerGoal {
    cooldown: i32,
    last_validate_tick: i64,
}

impl ValidateFlowerGoal {
    pub(super) fn new() -> Self {
        Self {
            cooldown: rand::random_range(VALIDATE_COOLDOWN_MIN..VALIDATE_COOLDOWN_MAX),
            last_validate_tick: -1,
        }
    }
}

impl Goal for ValidateFlowerGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::EMPTY
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        let Some(bee) = base_bee_goal_allows(mob) else {
            return false;
        };
        bee.level().is_some_and(|world| {
            world.game_time() > self.last_validate_tick + i64::from(self.cooldown)
        })
    }

    fn can_continue_to_use(&mut self, _mob: &dyn PathfinderMob) -> bool {
        false
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        let Some(bee) = bee_of(mob) else {
            return;
        };
        let Some(world) = bee.level() else {
            return;
        };
        if let Some(flower_pos) = bee.saved_flower_pos()
            && is_loaded(bee, flower_pos)
            && !BeeEntity::attracts_bees(world.get_block_state(flower_pos))
        {
            bee.drop_flower();
        }
        self.last_validate_tick = world.game_time();
    }
}

/// Vanilla parity: `Bee.BeeLocateHiveGoal`, the POI query that gives a homeless
/// bee somewhere to go.
pub(super) struct BeeLocateHiveGoal;

impl BeeLocateHiveGoal {
    pub(super) const fn new() -> Self {
        Self
    }

    /// Vanilla parity: `findNearbyHivesWithSpace`.
    ///
    /// The POI lock is released before the hives are filtered: checking for
    /// space reads a block entity, and holding the POI mutex across that would
    /// take two world locks in an order nothing else in Steel takes them in.
    fn find_nearby_hives_with_space(bee: &BeeEntity) -> Vec<BlockPos> {
        let Some(world) = bee.level() else {
            return Vec::new();
        };
        let bee_pos = bee.block_position();

        let is_bee_home = |poi_type_id: usize| {
            REGISTRY
                .poi_types
                .by_id(poi_type_id)
                .is_some_and(|poi_type| REGISTRY.poi_types.is_in_tag(poi_type, &PoiTag::BEE_HOME))
        };

        let candidates = {
            let storage = world.poi_storage.lock();
            storage.get_in_range(
                &is_bee_home,
                bee_pos,
                HIVE_SEARCH_DISTANCE,
                OccupationStatus::Any,
            )
        };

        let mut with_space: Vec<BlockPos> = candidates
            .into_iter()
            .map(|(pos, _)| pos)
            .filter(|pos| bee.does_hive_have_space(*pos))
            .collect();
        with_space.sort_by_key(|pos| {
            let dx = i64::from(pos.x() - bee_pos.x());
            let dy = i64::from(pos.y() - bee_pos.y());
            let dz = i64::from(pos.z() - bee_pos.z());
            dx * dx + dy * dy + dz * dz
        });
        with_space
    }
}

impl Goal for BeeLocateHiveGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::EMPTY
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        let Some(bee) = base_bee_goal_allows(mob) else {
            return false;
        };
        bee.hive_locate_cooldown() == 0 && !bee.has_hive() && bee.wants_to_enter_hive()
    }

    fn can_continue_to_use(&mut self, _mob: &dyn PathfinderMob) -> bool {
        false
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        let Some(bee) = bee_of(mob) else {
            return;
        };
        bee.set_hive_locate_cooldown(COOLDOWN_BEFORE_LOCATING_NEW_HIVE);

        let hives = Self::find_nearby_hives_with_space(bee);
        if hives.is_empty() {
            return;
        }
        bee.adopt_first_unblacklisted_hive(&hives);
    }
}

/// Vanilla parity: `Bee.BeeGoToHiveGoal`, the trip home, including the two ways
/// it can give up: the travel timer and sixty ticks of not moving.
pub(super) struct BeeGoToHiveGoal {
    travelling_ticks: i32,
    ticks_stuck: i32,
    last_path_target: Option<BlockPos>,
}

impl BeeGoToHiveGoal {
    pub(super) const fn new() -> Self {
        Self {
            travelling_ticks: 0,
            ticks_stuck: 0,
            last_path_target: None,
        }
    }

    fn has_reached_target(bee: &BeeEntity, target_pos: BlockPos) -> bool {
        if bee.closer_than(target_pos, HIVE_CLOSE_ENOUGH_DISTANCE as i32) {
            return true;
        }
        let navigation = bee.mob_base().navigation().lock();
        navigation
            .path()
            .is_some_and(|path| path.target() == target_pos && path.can_reach() && path.is_done())
    }

    /// Vanilla parity: `pathfindDirectlyTowards`, which spends ten times the
    /// usual node budget because the last stretch to a hive is worth it.
    fn pathfind_directly_towards(bee: &BeeEntity, target_pos: BlockPos) -> bool {
        let close_enough = if bee.closer_than(target_pos, PATHFIND_DIRECTLY_CLOSE_RANGE) {
            1
        } else {
            2
        };
        bee.mob_base()
            .navigation()
            .lock()
            .set_max_visited_nodes_multiplier(PATHFIND_DIRECTLY_NODE_MULTIPLIER);
        let (x, y, z) = target_pos.get_bottom_center();
        bee.move_to_pos_with_reach(DVec3::new(x, y, z), close_enough, 1.0);

        bee.mob_base()
            .navigation()
            .lock()
            .path()
            .is_some_and(Path::can_reach)
    }
}

impl Goal for BeeGoToHiveGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::MOVE
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        let Some(bee) = base_bee_goal_allows(mob) else {
            return false;
        };
        let Some(hive_pos) = bee.hive_pos() else {
            return false;
        };
        if bee.is_too_far_away(hive_pos) || bee.has_home() || !bee.wants_to_enter_hive() {
            return false;
        }
        if Self::has_reached_target(bee, hive_pos) {
            return false;
        }
        bee.level().is_some_and(|world| {
            world
                .get_block_state(hive_pos)
                .get_block()
                .has_tag(&BlockTag::BEEHIVES)
        })
    }

    fn start(&mut self, _mob: &dyn PathfinderMob) {
        self.travelling_ticks = 0;
        self.ticks_stuck = 0;
    }

    fn stop(&mut self, mob: &dyn PathfinderMob) {
        self.travelling_ticks = 0;
        self.ticks_stuck = 0;
        let mut navigation = mob.mob_base().navigation().lock();
        navigation.stop();
        navigation.reset_max_visited_nodes_multiplier();
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        let Some(bee) = bee_of(mob) else {
            return;
        };
        let Some(hive_pos) = bee.hive_pos() else {
            return;
        };

        self.travelling_ticks += 1;
        if self.travelling_ticks > MAX_TRAVELLING_TICKS {
            bee.blacklist_hive(MAX_BLACKLISTED_TARGETS);
            bee.drop_hive();
            return;
        }
        if bee.is_path_finding() {
            return;
        }

        if !bee.closer_than(hive_pos, PATHFIND_TO_HIVE_WHEN_CLOSER_THAN) {
            if bee.is_too_far_away(hive_pos) {
                bee.drop_hive();
            } else {
                pathfind_randomly_towards(bee, hive_pos);
            }
            return;
        }

        if !Self::pathfind_directly_towards(bee, hive_pos) {
            bee.blacklist_hive(MAX_BLACKLISTED_TARGETS);
            bee.drop_hive();
            return;
        }

        // Vanilla compares the whole path object; Steel compares the target it
        // was built for, which is the part that changes when a repath helps.
        let current_target = bee.mob_base().navigation().lock().target_pos();
        if self.last_path_target.is_some() && self.last_path_target == current_target {
            self.ticks_stuck += 1;
            if self.ticks_stuck > TICKS_BEFORE_HIVE_DROP {
                bee.drop_hive();
                self.ticks_stuck = 0;
            }
        } else {
            self.last_path_target = current_target;
        }
    }
}

/// Vanilla parity: `Bee.BeeGoToKnownFlowerGoal`, which only runs once the bee
/// has been out for half a minute without finding anything new.
pub(super) struct BeeGoToKnownFlowerGoal {
    travelling_ticks: i32,
}

impl BeeGoToKnownFlowerGoal {
    pub(super) const fn new() -> Self {
        Self {
            travelling_ticks: 0,
        }
    }
}

impl Goal for BeeGoToKnownFlowerGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::MOVE
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        let Some(bee) = base_bee_goal_allows(mob) else {
            return false;
        };
        let Some(flower_pos) = bee.saved_flower_pos() else {
            return false;
        };
        if bee.has_home() {
            return false;
        }
        bee.ticks_without_nectar() > TICKS_BEFORE_GOING_TO_KNOWN_FLOWER
            && !bee.closer_than(flower_pos, 2)
    }

    fn start(&mut self, _mob: &dyn PathfinderMob) {
        self.travelling_ticks = 0;
    }

    fn stop(&mut self, mob: &dyn PathfinderMob) {
        self.travelling_ticks = 0;
        let mut navigation = mob.mob_base().navigation().lock();
        navigation.stop();
        navigation.reset_max_visited_nodes_multiplier();
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        let Some(bee) = bee_of(mob) else {
            return;
        };
        let Some(flower_pos) = bee.saved_flower_pos() else {
            return;
        };

        self.travelling_ticks += 1;
        if self.travelling_ticks > MAX_TRAVELLING_TICKS {
            bee.drop_flower();
            return;
        }
        if bee.is_path_finding() {
            return;
        }

        if bee.is_too_far_away(flower_pos) {
            bee.drop_flower();
        } else {
            pathfind_randomly_towards(bee, flower_pos);
        }
    }
}

/// Vanilla parity: `Bee.BeePollinateGoal`, the hover that turns a flower into
/// nectar. It is the one goal that steers the bee itself rather than through
/// the navigation, which is why the navigation stands still while it runs.
pub(super) struct BeePollinateGoal {
    successful_pollinating_ticks: i32,
    last_sound_played_tick: i32,
    hover_pos: Option<DVec3>,
    pollinating_ticks: i32,
    unreachable_flower_cache: HashMap<BlockPos, i64>,
}

impl BeePollinateGoal {
    pub(super) fn new() -> Self {
        Self {
            successful_pollinating_ticks: 0,
            last_sound_played_tick: 0,
            hover_pos: None,
            pollinating_ticks: 0,
            unreachable_flower_cache: HashMap::new(),
        }
    }

    const fn has_pollinated_long_enough(&self) -> bool {
        self.successful_pollinating_ticks > MIN_POLLINATION_TICKS
    }

    /// Vanilla parity: `findNearbyFlower`, whose unreachable cache is what stops
    /// a bee from re-pathing to the same walled-off flower every tick.
    fn find_nearby_flower(&mut self, bee: &BeeEntity) -> Option<BlockPos> {
        let world = bee.level()?;
        let game_time = world.game_time();
        let origin = bee.block_position();
        let mut next_cache = HashMap::new();

        // Vanilla parity: `BlockPos.withinManhattan(pos, 5, 5, 5)`, whose order
        // is what decides which of several flowers a bee settles on.
        for radius in 0..=(FLOWER_SEARCH_RADIUS * 3) {
            for dx in -FLOWER_SEARCH_RADIUS..=FLOWER_SEARCH_RADIUS {
                for dy in -FLOWER_SEARCH_RADIUS..=FLOWER_SEARCH_RADIUS {
                    for dz in -FLOWER_SEARCH_RADIUS..=FLOWER_SEARCH_RADIUS {
                        if dx.abs() + dy.abs() + dz.abs() != radius {
                            continue;
                        }
                        let pos = BlockPos::new(origin.x() + dx, origin.y() + dy, origin.z() + dz);

                        if let Some(&unreachable_until) = self.unreachable_flower_cache.get(&pos)
                            && game_time < unreachable_until
                        {
                            next_cache.insert(pos, unreachable_until);
                            continue;
                        }

                        if !BeeEntity::attracts_bees(world.get_block_state(pos)) {
                            continue;
                        }

                        if bee
                            .create_path_to(pos, 1)
                            .is_some_and(|path| path.can_reach())
                        {
                            return Some(pos);
                        }
                        next_cache.insert(pos, game_time + UNREACHABLE_FLOWER_COOLDOWN);
                    }
                }
            }
        }

        self.unreachable_flower_cache = next_cache;
        None
    }

    fn set_wanted_pos(&self, bee: &BeeEntity) {
        let Some(hover_pos) = self.hover_pos else {
            return;
        };
        bee.set_wanted_position(hover_pos, POLLINATE_SPEED_MODIFIER);
    }

    fn offset() -> f64 {
        f64::from(rand::random::<f32>().mul_add(2.0, -1.0) * HOVER_POS_OFFSET)
    }
}

impl Goal for BeePollinateGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::MOVE
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        let Some(bee) = base_bee_goal_allows(mob) else {
            return false;
        };
        if bee.flower_locate_cooldown() > 0 || bee.has_nectar() {
            return false;
        }
        let Some(world) = bee.level() else {
            return false;
        };
        if world.is_raining() {
            return false;
        }

        if let Some(flower_pos) = self.find_nearby_flower(bee) {
            bee.set_saved_flower_pos(flower_pos);
            let (x, y, z) = flower_pos.get_center();
            bee.move_to_pos(DVec3::new(x, y, z), POLLINATE_APPROACH_SPEED);
            return true;
        }

        bee.set_flower_locate_cooldown(rand::random_range(
            MIN_FIND_FLOWER_RETRY_COOLDOWN..MAX_FIND_FLOWER_RETRY_COOLDOWN,
        ));
        false
    }

    fn can_continue_to_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        let Some(bee) = base_bee_goal_allows(mob) else {
            return false;
        };
        if !bee.is_pollinating() || !bee.has_saved_flower_pos() {
            return false;
        }
        if bee.level().is_some_and(|world| world.is_raining()) {
            return false;
        }
        if self.has_pollinated_long_enough() {
            // Vanilla keeps rolling once the bee has enough nectar, so it lingers
            // a random few seconds instead of leaving on the exact tick.
            return rand::random::<f32>() < POLLINATE_LEAVE_CHANCE;
        }
        true
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        let Some(bee) = bee_of(mob) else {
            return;
        };
        self.successful_pollinating_ticks = 0;
        self.pollinating_ticks = 0;
        self.last_sound_played_tick = 0;
        self.hover_pos = None;
        bee.set_pollinating(true);
        bee.reset_ticks_without_nectar_since_exiting_hive();
    }

    fn stop(&mut self, mob: &dyn PathfinderMob) {
        let Some(bee) = bee_of(mob) else {
            return;
        };
        if self.has_pollinated_long_enough() {
            bee.set_has_nectar(true);
        }
        bee.set_pollinating(false);
        bee.set_flower_locate_cooldown(COOLDOWN_BEFORE_LOCATING_NEW_FLOWER);
        bee.mob_base().navigation().lock().stop();
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        let Some(bee) = bee_of(mob) else {
            return;
        };
        let Some(flower_pos) = bee.saved_flower_pos() else {
            return;
        };

        self.pollinating_ticks += 1;
        if self.pollinating_ticks > MAX_POLLINATING_TICKS {
            bee.drop_flower();
            bee.set_pollinating(false);
            bee.set_flower_locate_cooldown(COOLDOWN_BEFORE_LOCATING_NEW_FLOWER);
            return;
        }

        let (fx, fy, fz) = flower_pos.get_bottom_center();
        let flower_vec = DVec3::new(fx, fy + HOVER_HEIGHT_WITHIN_FLOWER, fz);

        if flower_vec.distance(bee.position()) > POLLINATE_HOVER_RANGE {
            self.hover_pos = Some(flower_vec);
            self.set_wanted_pos(bee);
            return;
        }

        let hover_pos = *self.hover_pos.get_or_insert(flower_vec);
        let arrived = bee.position().distance(hover_pos) <= ARRIVAL_THRESHOLD;
        let mut should_set_wanted_pos = true;

        if arrived {
            if rand::random_range(0..POSITION_CHANGE_CHANCE) == 0 {
                self.hover_pos = Some(DVec3::new(
                    flower_vec.x + Self::offset(),
                    flower_vec.y,
                    flower_vec.z + Self::offset(),
                ));
                bee.mob_base().navigation().lock().stop();
            } else {
                should_set_wanted_pos = false;
            }

            let (y_max_rot, x_max_rot) = (bee.max_head_y_rot(), bee.max_head_x_rot());
            bee.mob_base()
                .controls()
                .lock()
                .look_control
                .set_look_at(flower_vec, y_max_rot, x_max_rot);
        }

        if should_set_wanted_pos {
            self.set_wanted_pos(bee);
        }

        self.successful_pollinating_ticks += 1;
        if rand::random::<f32>() < POLLINATE_SOUND_CHANCE
            && self.successful_pollinating_ticks
                > self.last_sound_played_tick + POLLINATE_SOUND_INTERVAL
        {
            self.last_sound_played_tick = self.successful_pollinating_ticks;
            bee.play_sound(&sound_events::ENTITY_BEE_POLLINATE, 1.0, 1.0);
        }
    }
}

/// Vanilla parity: `Bee.BeeGrowCropGoal`, the reason a bee is worth keeping over
/// a farm: it advances whatever it passes over by one growth stage.
pub(super) struct BeeGrowCropGoal;

impl BeeGrowCropGoal {
    pub(super) const fn new() -> Self {
        Self
    }
}

impl Goal for BeeGrowCropGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::EMPTY
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        let Some(bee) = base_bee_goal_allows(mob) else {
            return false;
        };
        if bee.crops_grown_since_pollination() >= MAX_CROPS_GROWABLE {
            return false;
        }
        if rand::random::<f32>() < GROW_SKIP_CHANCE {
            return false;
        }
        bee.has_nectar() && bee.is_hive_valid()
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        let Some(bee) = bee_of(mob) else {
            return;
        };
        if rand::random_range(0..GROW_CHANCE) != 0 {
            return;
        }
        let Some(world) = bee.level() else {
            return;
        };

        for depth in 1..=GROW_DEPTH {
            let below_pos = bee.block_position().below_n(depth);
            let state = world.get_block_state(below_pos);
            if !state.get_block().has_tag(&BlockTag::BEE_GROWABLES) {
                continue;
            }
            let Some(grown) = BLOCK_BEHAVIORS
                .get_behavior(state.get_block())
                .bee_grown_state(state, &world, below_pos)
            else {
                continue;
            };

            world.level_event(
                steel_registry::level_events::PARTICLES_AND_SOUND_PLANT_GROWTH,
                below_pos,
                GROWTH_PARTICLE_COUNT,
                None,
            );
            world.set_block(below_pos, grown, UpdateFlags::UPDATE_ALL);
            bee.increment_crops_grown_since_pollination();
        }
    }
}

/// Vanilla parity: `Bee.BeeWanderGoal`, the drift a bee falls back on. It is the
/// only bee goal that is not a `BaseBeeGoal`, so an angry bee still wanders.
pub(super) struct BeeWanderGoal;

impl BeeWanderGoal {
    pub(super) const fn new() -> Self {
        Self
    }

    fn wander_threshold(bee: &BeeEntity) -> i32 {
        let reduction = if bee.has_hive() || bee.has_saved_flower_pos() {
            RESTRICTED_WANDER_DISTANCE_REDUCTION
        } else {
            DEFAULT_WANDER_DISTANCE_REDUCTION
        };
        WANDER_MAX_DISTANCE - reduction
    }

    fn find_pos(bee: &BeeEntity) -> Option<DVec3> {
        let heading = match bee.hive_pos() {
            Some(hive_pos)
                if bee.is_hive_valid()
                    && !bee.closer_than(hive_pos, Self::wander_threshold(bee)) =>
            {
                let (x, y, z) = hive_pos.get_center();
                (DVec3::new(x, y, z) - bee.position()).normalize_or_zero()
            }
            _ => bee.look_angle(),
        };

        hover_random_pos(
            bee,
            WANDER_HORIZONTAL_DIST,
            WANDER_HOVER_VERTICAL_DIST,
            heading.x,
            heading.z,
            FRAC_PI_2,
            WANDER_HOVER_MAX_HEIGHT,
            WANDER_HOVER_MIN_HEIGHT,
        )
        .or_else(|| {
            air_and_water_random_pos(
                bee,
                WANDER_HORIZONTAL_DIST,
                WANDER_AIR_VERTICAL_DIST,
                WANDER_AIR_FLYING_HEIGHT,
                heading.x,
                heading.z,
                FRAC_PI_2,
            )
        })
    }
}

impl Goal for BeeWanderGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::MOVE
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        !mob.is_path_finding() && rand::random_range(0..WANDER_CHANCE) == 0
    }

    fn can_continue_to_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        mob.is_path_finding()
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        let Some(bee) = bee_of(mob) else {
            return;
        };
        let Some(target) = Self::find_pos(bee) else {
            return;
        };
        let path = bee.create_path_to(BlockPos::containing(target.x, target.y, target.z), 1);
        bee.move_to_path(path, 1.0);
    }
}

/// Vanilla parity: `Bee.BeeHurtByOtherGoal`, which calls in the swarm -- but
/// only the bees that can actually see who threw the punch.
pub(super) struct BeeHurtByOtherGoal {
    inner: HurtByTargetGoal,
}

impl BeeHurtByOtherGoal {
    pub(super) fn new() -> Self {
        Self {
            // Vanilla parity: `BeeHurtByOtherGoal.alertOther`, which only alerts
            // another bee, and only one the hurt bee has line of sight to.
            inner: HurtByTargetGoal::new()
                .set_alert_others([])
                .with_alert_filter(|other| other.downcast_ref::<BeeEntity>().is_some()),
        }
    }
}

impl Goal for BeeHurtByOtherGoal {
    fn controls(&self) -> GoalControls {
        self.inner.controls()
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        self.inner.can_use(mob)
    }

    fn can_continue_to_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        bee_of(mob).is_some_and(NeutralMob::is_angry) && self.inner.can_continue_to_use(mob)
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        self.inner.start(mob);
    }

    fn stop(&mut self, mob: &dyn PathfinderMob) {
        self.inner.stop(mob);
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        self.inner.tick(mob);
    }
}

/// Vanilla parity: `Bee.BeeBecomeAngryTargetGoal`, which turns the persistent
/// anger into an actual target -- and stops the moment the sting is spent.
pub(super) struct BeeBecomeAngryTargetGoal {
    inner: NearestAttackableTargetGoal,
}

impl BeeBecomeAngryTargetGoal {
    pub(super) fn new() -> Self {
        Self {
            inner: NearestAttackableTargetGoal::new_for_players_with_interval(
                ANGRY_TARGET_INTERVAL,
                true,
                false,
                |mob, target, _| {
                    let Some(mob) = mob else {
                        return false;
                    };
                    let Some(world) = mob.level() else {
                        return false;
                    };
                    mob.as_neutral_mob()
                        .is_some_and(|bee| bee.is_angry_at(target, &world))
                },
            ),
        }
    }

    fn bee_can_target(mob: &dyn PathfinderMob) -> bool {
        bee_of(mob).is_some_and(|bee| bee.is_angry() && !bee.has_stung())
    }
}

/// Vanilla parity: the `10` of `new NearestAttackableTargetGoal<>(bee, Player.class, 10, ...)`.
const ANGRY_TARGET_INTERVAL: i32 = 10;

impl Goal for BeeBecomeAngryTargetGoal {
    fn controls(&self) -> GoalControls {
        self.inner.controls()
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        Self::bee_can_target(mob) && self.inner.can_use(mob)
    }

    fn can_continue_to_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        if Self::bee_can_target(mob) && mob.target().is_some() {
            return self.inner.can_continue_to_use(mob);
        }
        false
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        self.inner.start(mob);
    }

    fn stop(&mut self, mob: &dyn PathfinderMob) {
        self.inner.stop(mob);
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        self.inner.tick(mob);
    }
}
