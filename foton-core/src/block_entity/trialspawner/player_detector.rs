//! Who a trial spawner or a vault counts as being watched by.
//!
//! Vanilla parity:
//! `net.minecraft.world.level.block.entity.trialspawner.PlayerDetector`.
//!
//! Vanilla threads a second interface, `PlayerDetector.EntitySelector`, so a
//! game test can hand the detector a fixed list of players instead of the
//! level. Foton has no such test harness, so the level is the only source and
//! the indirection is left out. `PlayerDetector.SHEEP` is likewise absent: it
//! exists only behind `SharedConstants.DEBUG_TRIAL_SPAWNER_DETECTS_SHEEP_AS_PLAYERS`.

use std::sync::Arc;

use foton_utils::BlockPos;
use glam::DVec3;
use uuid::Uuid;

use crate::entity::Entity as _;
use crate::world::{ClipBlockShape, ClipFluid, World};

/// Which players a spawner is willing to see.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerDetector {
    /// Vanilla parity: `PlayerDetector.NO_CREATIVE_PLAYERS`, the trial spawner's.
    NoCreativePlayers,
    /// Vanilla parity: `PlayerDetector.INCLUDING_CREATIVE_PLAYERS`, the vault's.
    IncludingCreativePlayers,
}

impl PlayerDetector {
    /// Returns the players inside `required_player_range` of `spawner_pos`.
    ///
    /// Vanilla parity: `PlayerDetector.detect`. The range test is vanilla's
    /// `BlockPos.closerThan`, which measures from block position to block
    /// position, not from the player's feet.
    #[must_use]
    pub fn detect(
        self,
        world: &Arc<World>,
        spawner_pos: BlockPos,
        required_player_range: f64,
        require_line_of_sight: bool,
    ) -> Vec<Uuid> {
        let center = center_of(spawner_pos);
        let range_sqr = required_player_range * required_player_range;
        let mut detected = Vec::new();

        world.players.iter_players(|_, player| {
            if player.is_spectator() {
                return true;
            }
            if self == Self::NoCreativePlayers && player.has_infinite_materials() {
                return true;
            }
            if !closer_than(player.block_position(), spawner_pos, range_sqr) {
                return true;
            }
            let eye = DVec3::new(player.position().x, player.get_eye_y(), player.position().z);
            if require_line_of_sight && !in_line_of_sight(world, center, eye) {
                return true;
            }
            detected.push(player.uuid());
            true
        });

        detected
    }
}

/// Vanilla parity: `Vec3.atCenterOf`.
pub(crate) fn center_of(pos: BlockPos) -> DVec3 {
    DVec3::new(
        f64::from(pos.x()) + 0.5,
        f64::from(pos.y()) + 0.5,
        f64::from(pos.z()) + 0.5,
    )
}

/// Vanilla parity: `BlockPos.closerThan(Vec3i, double)`, which compares squared
/// distances between block positions.
fn closer_than(left: BlockPos, right: BlockPos, range_sqr: f64) -> bool {
    let dx = f64::from(left.x() - right.x());
    let dy = f64::from(left.y() - right.y());
    let dz = f64::from(left.z() - right.z());
    dz.mul_add(dz, dx.mul_add(dx, dy * dy)) < range_sqr
}

/// Returns whether nothing solid stands between the two points.
///
/// Vanilla parity: the private `inLineOfSight` of `PlayerDetector` and
/// `TrialSpawner`, which clip from the destination back to the origin and
/// accept a hit on the origin block itself.
pub(crate) fn in_line_of_sight(world: &Arc<World>, origin: DVec3, destination: DVec3) -> bool {
    let hit = world.clip(destination, origin, ClipBlockShape::Visual, ClipFluid::None);
    hit.miss
        || hit.block_pos
            == BlockPos::new(
                origin.x.floor() as i32,
                origin.y.floor() as i32,
                origin.z.floor() as i32,
            )
}
