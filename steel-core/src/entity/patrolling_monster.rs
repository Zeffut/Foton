//! Monsters that march a long line across the world rather than stand around.
//!
//! Vanilla parity: `PatrollingMonster`. An illager patrol is the only reason a
//! pillager appears hundreds of blocks from anything: the group picks a point
//! up to five hundred blocks away, walks to it, and picks another. The leader
//! carries the ominous banner and drags the rest along, which is also the seam
//! raids hang off -- killing a banner-carrying captain is what gives a player
//! Bad Omen.

use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::{NbtCompound, NbtTag};
use steel_utils::BlockPos;
use steel_utils::locks::SyncMutex;

use crate::entity::Mob;

/// NBT key vanilla stores the patrol destination under.
pub const TAG_PATROL_TARGET: &str = "patrol_target";
/// NBT key vanilla stores the leader flag under.
pub const TAG_PATROL_LEADER: &str = "PatrolLeader";
/// NBT key vanilla stores the patrolling flag under.
pub const TAG_PATROLLING: &str = "Patrolling";

/// Chance a patroller that spawned on its own turns out to be the captain.
///
/// Vanilla parity: the `nextFloat() < 0.06F` of
/// `PatrollingMonster.finalizeSpawn`.
pub const PATROL_LEADER_SPAWN_CHANCE: f32 = 0.06;

/// How far a patroller throws its next waypoint, in each direction.
///
/// Vanilla parity: the `-500 + random.nextInt(1000)` of `findPatrolTarget`.
const PATROL_TARGET_SPREAD: i32 = 500;

/// Squared distance past which even a patrol is allowed to despawn.
///
/// Vanilla parity: the `distSqr > 16384.0` of `removeWhenFarAway`.
const PATROL_PERSISTENCE_DISTANCE_SQR: f64 = 16_384.0;

/// Where a patroller is going and whether it leads.
///
/// Vanilla keeps these three on the mob; Steel groups them so an entity holds
/// one field, the way it holds a [`crate::entity::MobBase`].
#[derive(Debug)]
pub struct PatrolState {
    /// The point this mob is walking towards, if it has one.
    target: SyncMutex<Option<BlockPos>>,
    /// Whether this mob is the banner-carrying captain.
    leader: SyncMutex<bool>,
    /// Whether this mob is on patrol at all.
    patrolling: SyncMutex<bool>,
}

impl PatrolState {
    /// Creates the state of a mob that is not patrolling.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            target: SyncMutex::new(None),
            leader: SyncMutex::new(false),
            patrolling: SyncMutex::new(false),
        }
    }
}

impl Default for PatrolState {
    fn default() -> Self {
        Self::new()
    }
}

/// A monster that walks a patrol.
///
/// Vanilla parity: the `PatrollingMonster` class.
pub trait PatrollingMonster: Mob {
    /// Returns this mob's patrol state.
    fn patrol_state(&self) -> &PatrolState;

    /// Returns whether this mob may carry the ominous banner.
    ///
    /// Vanilla parity: `canBeLeader`, which only the ravager answers no to.
    fn can_be_leader(&self) -> bool {
        true
    }

    /// Returns whether this mob may be swept into a passing patrol.
    ///
    /// Vanilla parity: `canJoinPatrol`.
    fn can_join_patrol(&self) -> bool {
        true
    }

    /// Returns the point this mob is walking towards.
    fn patrol_target(&self) -> Option<BlockPos> {
        *self.patrol_state().target.lock()
    }

    /// Returns whether this mob has somewhere to walk to.
    fn has_patrol_target(&self) -> bool {
        self.patrol_target().is_some()
    }

    /// Sends this mob to `target`, putting it on patrol.
    ///
    /// Vanilla parity: `setPatrolTarget`, which also flips `patrolling`.
    fn set_patrol_target(&self, target: BlockPos) {
        *self.patrol_state().target.lock() = Some(target);
        *self.patrol_state().patrolling.lock() = true;
    }

    /// Picks a fresh waypoint up to five hundred blocks away.
    ///
    /// Vanilla parity: `findPatrolTarget`.
    fn find_patrol_target(&self) {
        let position = self.block_position();
        let target = position.offset(
            rand::random_range(-PATROL_TARGET_SPREAD..PATROL_TARGET_SPREAD),
            0,
            rand::random_range(-PATROL_TARGET_SPREAD..PATROL_TARGET_SPREAD),
        );
        self.set_patrol_target(target);
    }

    /// Returns whether this mob carries the ominous banner.
    fn is_patrol_leader(&self) -> bool {
        *self.patrol_state().leader.lock()
    }

    /// Makes this mob the captain, putting it on patrol.
    ///
    /// Vanilla parity: `setPatrolLeader`.
    fn set_patrol_leader(&self, leader: bool) {
        *self.patrol_state().leader.lock() = leader;
        *self.patrol_state().patrolling.lock() = true;
    }

    /// Returns whether this mob is on patrol.
    fn is_patrolling(&self) -> bool {
        *self.patrol_state().patrolling.lock()
    }

    /// Sets whether this mob is on patrol.
    fn set_patrolling(&self, patrolling: bool) {
        *self.patrol_state().patrolling.lock() = patrolling;
    }

    /// Returns vanilla `PatrollingMonster.removeWhenFarAway`.
    ///
    /// A patrol is kept loaded so it can actually arrive somewhere, but not
    /// past the distance at which the chunks are gone anyway.
    fn remove_when_far_away_patrolling(&self, dist_sqr: f64) -> bool {
        !self.is_patrolling() || dist_sqr > PATROL_PERSISTENCE_DISTANCE_SQR
    }
}

/// Writes the patrol state the way vanilla does.
///
/// Vanilla parity: `PatrollingMonster.addAdditionalSaveData`.
pub fn write_patrol_state(mob: &dyn PatrollingMonster, nbt: &mut NbtCompound) {
    if let Some(target) = mob.patrol_target() {
        nbt.insert(
            TAG_PATROL_TARGET,
            NbtTag::IntArray(vec![target.x(), target.y(), target.z()]),
        );
    }
    nbt.insert(TAG_PATROL_LEADER, i8::from(mob.is_patrol_leader()));
    nbt.insert(TAG_PATROLLING, i8::from(mob.is_patrolling()));
}

/// Reads the patrol state the way vanilla does.
///
/// Vanilla parity: `PatrollingMonster.readAdditionalSaveData`. The two flags go
/// straight into the state rather than through the setters, because vanilla's
/// setters also force `patrolling` on and a saved idle captain would come back
/// walking.
pub fn read_patrol_state(mob: &dyn PatrollingMonster, nbt: BorrowedNbtCompoundView<'_, '_>) {
    let target = nbt
        .int_array(TAG_PATROL_TARGET)
        .filter(|position| position.len() == 3)
        .map(|position| BlockPos::new(position[0], position[1], position[2]));
    *mob.patrol_state().target.lock() = target;
    *mob.patrol_state().leader.lock() = nbt.byte(TAG_PATROL_LEADER).is_some_and(|value| value != 0);
    *mob.patrol_state().patrolling.lock() =
        nbt.byte(TAG_PATROLLING).is_some_and(|value| value != 0);
}
