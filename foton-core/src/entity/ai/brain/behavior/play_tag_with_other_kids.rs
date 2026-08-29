//! Vanilla `PlayTagWithOtherKids`.

use foton_utils::BlockPos;
use rustc_hash::FxHashMap;

use super::{BrainContext, Trigger, utils};
use crate::entity::ai::brain::memory::{
    EntityMemory, MemoryModuleId, WalkTarget, memory_module_types,
};
use crate::entity::ai::brain::position_tracker::PositionTracker;
use crate::entity::ai::goal::land_random_pos;
use crate::entity::{Mob, SharedEntity};

/// How far a child runs from whoever is chasing it.
///
/// Vanilla parity: `PlayTagWithOtherKids.MAX_FLEE_XZ_DIST` and `MAX_FLEE_Y_DIST`.
const MAX_FLEE_XZ_DIST: i32 = 20;
const MAX_FLEE_Y_DIST: i32 = 8;

/// How fast, either way. Vanilla gives the chase and the escape the same pace.
///
/// Vanilla parity: `PlayTagWithOtherKids.FLEE_SPEED_MODIFIER` and
/// `CHASE_SPEED_MODIFIER`.
const PLAY_SPEED_MODIFIER: f64 = 0.6;

/// How many children may pile onto one of their own before the rest look for
/// somebody else to chase.
///
/// Vanilla parity: `PlayTagWithOtherKids.MAX_CHASERS_PER_TARGET`.
const MAX_CHASERS_PER_TARGET: usize = 5;

/// One tick in this many starts or changes a game.
///
/// Vanilla parity: `PlayTagWithOtherKids.AVERAGE_WAIT_TIME_BETWEEN_RUNS`, the
/// `nextInt(10) != 0` the whole behavior opens with.
const AVERAGE_WAIT_TIME_BETWEEN_RUNS: i32 = 10;

/// How many spots a fleeing child tries before giving up on this tick.
///
/// Vanilla parity: the `for (int j = 0; j < 10; j++)` of the flee branch.
const FLEE_ATTEMPTS: i32 = 10;

/// The game the village's children play with each other.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.behavior.PlayTagWithOtherKids`.
///
/// It is one behavior for both halves of tag, and which half a child is playing
/// is read off everybody else's `INTERACTION_TARGET`: a child somebody is
/// already chasing runs, and a child nobody is chasing joins the chase --
/// preferring one that is already being chased, so the village plays one game
/// rather than a dozen. Running is bounded to the village, which is what keeps
/// a game of tag from emptying it.
pub struct PlayTagWithOtherKids;

impl Trigger for PlayTagWithOtherKids {
    fn required_memories(&self) -> Vec<MemoryModuleId> {
        vec![
            memory_module_types::VISIBLE_VILLAGER_BABIES.id(),
            memory_module_types::WALK_TARGET.id(),
            memory_module_types::LOOK_TARGET.id(),
            memory_module_types::INTERACTION_TARGET.id(),
        ]
    }

    fn trigger(&mut self, ctx: &BrainContext<'_>) -> bool {
        let brain = ctx.brain();
        // Vanilla parity: `i.present(VISIBLE_VILLAGER_BABIES)` and
        // `i.absent(WALK_TARGET)` -- a child already going somewhere is left to
        // get there.
        let Some(friends) = brain.get_memory(memory_module_types::VISIBLE_VILLAGER_BABIES) else {
            return false;
        };
        if brain.has_memory_value(memory_module_types::WALK_TARGET.id()) {
            return false;
        }
        if rand::random_range(0..AVERAGE_WAIT_TIME_BETWEEN_RUNS) != 0 {
            return false;
        }

        let friends: Vec<SharedEntity> = friends.iter().filter_map(EntityMemory::get).collect();
        let me = ctx.mob().id();
        if friends.iter().any(|friend| chasing(friend) == Some(me)) {
            run_away(ctx);
            return true;
        }

        // Vanilla parity: the preference for somebody already being chased,
        // then `findAny` over the rest -- which is what turns a crowd of
        // children into one game with one `it` rather than a dozen pairs.
        let target = being_chased(&friends).or_else(|| friends.first().cloned());
        if let Some(kid) = target {
            chase_kid(ctx, &kid);
        }
        true
    }

    fn debug_name(&self) -> &'static str {
        "PlayTagWithOtherKids"
    }
}

/// Vanilla parity: the flee branch, which only accepts a spot still inside the
/// village.
fn run_away(ctx: &BrainContext<'_>) {
    let mob = ctx.mob();
    for _ in 0..FLEE_ATTEMPTS {
        let Some(pos) = land_random_pos(mob, MAX_FLEE_XZ_DIST, MAX_FLEE_Y_DIST) else {
            continue;
        };
        if !ctx
            .world()
            .is_village(BlockPos::containing(pos.x, pos.y, pos.z))
        {
            continue;
        }
        ctx.brain().set_memory(
            memory_module_types::WALK_TARGET,
            WalkTarget::of_position(pos, PLAY_SPEED_MODIFIER, 0),
        );
        break;
    }
}

/// Vanilla parity: the private `PlayTagWithOtherKids.chaseKid`.
fn chase_kid(ctx: &BrainContext<'_>, kid: &SharedEntity) {
    let brain = ctx.brain();
    brain.set_memory(
        memory_module_types::INTERACTION_TARGET,
        utils::remember(kid),
    );
    brain.set_memory(
        memory_module_types::LOOK_TARGET,
        PositionTracker::of_entity(kid, true),
    );
    brain.set_memory(
        memory_module_types::WALK_TARGET,
        WalkTarget::new(
            PositionTracker::of_entity(kid, false),
            PLAY_SPEED_MODIFIER,
            1,
        ),
    );
}

/// Vanilla parity: the private `PlayTagWithOtherKids.findSomeoneBeingChased`,
/// which takes the least-chased child that somebody is already after and that
/// has not already drawn a crowd.
fn being_chased(friends: &[SharedEntity]) -> Option<SharedEntity> {
    let mut chasers: FxHashMap<i32, usize> = FxHashMap::default();
    for friend in friends {
        if let Some(chased) = chasing(friend) {
            *chasers.entry(chased).or_default() += 1;
        }
    }

    let mut candidates: Vec<(i32, usize)> = chasers
        .into_iter()
        .filter(|&(_, count)| count > 0 && count <= MAX_CHASERS_PER_TARGET)
        .collect();
    // Vanilla sorts by chaser count and takes the first, so the child with the
    // shortest tail behind it is the one joined.
    candidates.sort_unstable_by_key(|&(id, count)| (count, id));
    let least_chased = candidates.first()?.0;
    friends
        .iter()
        .find(|friend| friend.id() == least_chased)
        .cloned()
}

/// Who this child is chasing, if anybody.
///
/// Vanilla parity: the private `whoAreYouChasing` and `isChasingSomeone`, both
/// of which read the other child's own `INTERACTION_TARGET`.
fn chasing(friend: &SharedEntity) -> Option<i32> {
    let brain = friend.as_mob().and_then(Mob::brain)?;
    brain
        .get_memory(memory_module_types::INTERACTION_TARGET)
        .map(|target| target.id())
}
