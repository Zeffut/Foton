//! How close a player is to being handed a warden.
//!
//! Vanilla parity: `net.minecraft.world.entity.monster.warden.WardenSpawnTracker`. The
//! counter lives on the player, not on the shrieker, which is why running to a fresh
//! shrieker does not reset it -- four shrieks anywhere in the deep dark is four shrieks.

use std::sync::Arc;

use glam::DVec3;
use steel_registry::vanilla_entities;
use steel_utils::{BlockPos, WorldAabb};

use crate::entity::Entity;
use crate::player::Player;
use crate::world::World;

/// Vanilla `WardenSpawnTracker.MAX_WARNING_LEVEL`.
pub const MAX_WARNING_LEVEL: i32 = 4;
/// Vanilla `WardenSpawnTracker.PLAYER_SEARCH_RADIUS`.
const PLAYER_SEARCH_RADIUS: f64 = 16.0;
/// Vanilla `WardenSpawnTracker.WARNING_CHECK_DIAMETER`.
const WARNING_CHECK_DIAMETER: f64 = 48.0;
/// Vanilla `WardenSpawnTracker.DECREASE_WARNING_LEVEL_EVERY_INTERVAL`, ten minutes.
const DECREASE_WARNING_LEVEL_EVERY_INTERVAL: i32 = 12000;
/// Vanilla `WardenSpawnTracker.WARNING_LEVEL_INCREASE_COOLDOWN`, ten seconds.
const WARNING_LEVEL_INCREASE_COOLDOWN: i32 = 200;

/// Vanilla `WardenSpawnTracker`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WardenSpawnTracker {
    ticks_since_last_warning: i32,
    warning_level: i32,
    cooldown_ticks: i32,
}

impl WardenSpawnTracker {
    /// Vanilla `WardenSpawnTracker(int, int, int)`.
    #[must_use]
    pub const fn new(
        ticks_since_last_warning: i32,
        warning_level: i32,
        cooldown_ticks: i32,
    ) -> Self {
        Self {
            ticks_since_last_warning,
            warning_level,
            cooldown_ticks,
        }
    }

    /// Vanilla `WardenSpawnTracker.tick`.
    ///
    /// Ten minutes without a shriek gives a warning level back, which is what makes
    /// leaving the deep dark a way to un-anger it.
    pub const fn tick(&mut self) {
        if self.ticks_since_last_warning >= DECREASE_WARNING_LEVEL_EVERY_INTERVAL {
            self.set_warning_level(self.warning_level - 1);
            self.ticks_since_last_warning = 0;
        } else {
            self.ticks_since_last_warning += 1;
        }

        if self.cooldown_ticks > 0 {
            self.cooldown_ticks -= 1;
        }
    }

    /// Vanilla `WardenSpawnTracker.reset`.
    pub const fn reset(&mut self) {
        self.ticks_since_last_warning = 0;
        self.warning_level = 0;
        self.cooldown_ticks = 0;
    }

    /// Vanilla `WardenSpawnTracker.getWarningLevel`.
    #[must_use]
    pub const fn warning_level(self) -> i32 {
        self.warning_level
    }

    /// Vanilla `WardenSpawnTracker.setWarningLevel`.
    pub const fn set_warning_level(&mut self, warning_level: i32) {
        self.warning_level = if warning_level < 0 {
            0
        } else if warning_level > MAX_WARNING_LEVEL {
            MAX_WARNING_LEVEL
        } else {
            warning_level
        };
    }

    /// Vanilla `WardenSpawnTracker.ticksSinceLastWarning`, for save and load.
    #[must_use]
    pub const fn ticks_since_last_warning(self) -> i32 {
        self.ticks_since_last_warning
    }

    /// Vanilla `WardenSpawnTracker.cooldownTicks`, for save and load.
    #[must_use]
    pub const fn cooldown_ticks(self) -> i32 {
        self.cooldown_ticks
    }

    /// Vanilla `WardenSpawnTracker.onCooldown`.
    const fn on_cooldown(self) -> bool {
        self.cooldown_ticks > 0
    }

    /// Vanilla `WardenSpawnTracker.increaseWarningLevel`.
    const fn increase_warning_level(&mut self) {
        if self.on_cooldown() {
            return;
        }
        self.ticks_since_last_warning = 0;
        self.cooldown_ticks = WARNING_LEVEL_INCREASE_COOLDOWN;
        self.set_warning_level(self.warning_level + 1);
    }
}

/// Vanilla `WardenSpawnTracker.tryWarn`.
///
/// Returns the new warning level every nearby player now shares, or `None` when the
/// warning is refused: a warden is already there, or somebody nearby is still inside the
/// ten-second cooldown from the last one.
#[must_use]
pub fn try_warn(world: &Arc<World>, pos: BlockPos, trigger_player: &Player) -> Option<i32> {
    if has_nearby_warden(world, pos) {
        return None;
    }

    let mut players = nearby_players(world, pos);
    if !players
        .iter()
        .any(|player| player.id() == trigger_player.id())
    {
        // Vanilla adds the shrieker's own player even when the search missed them.
        players.push(nearby_player_handle(world, trigger_player)?);
    }

    if players
        .iter()
        .any(|player| player.warden_spawn_tracker().on_cooldown())
    {
        return None;
    }

    let highest = players
        .iter()
        .map(|player| player.warden_spawn_tracker())
        .max_by_key(|tracker| tracker.warning_level())?;

    // Vanilla raises the highest tracker and then copies it onto everybody nearby, so a
    // group in the deep dark walks toward the same warden together.
    let mut raised = highest;
    raised.increase_warning_level();
    for player in &players {
        player.set_warden_spawn_tracker(raised);
    }
    Some(raised.warning_level)
}

/// Vanilla `WardenSpawnTracker.hasNearbyWarden`.
fn has_nearby_warden(world: &Arc<World>, pos: BlockPos) -> bool {
    let center = DVec3::new(
        f64::from(pos.x()) + 0.5,
        f64::from(pos.y()) + 0.5,
        f64::from(pos.z()) + 0.5,
    );
    let area = WorldAabb::of_size(
        center,
        WARNING_CHECK_DIAMETER,
        WARNING_CHECK_DIAMETER,
        WARNING_CHECK_DIAMETER,
    );
    !world
        .get_entities_in_aabb_matching(&area, |entity| {
            entity.entity_type() == &vanilla_entities::WARDEN
        })
        .is_empty()
}

/// Vanilla `WardenSpawnTracker.getNearbyPlayers`.
fn nearby_players(world: &Arc<World>, pos: BlockPos) -> Vec<Arc<Player>> {
    let origin = DVec3::new(
        f64::from(pos.x()) + 0.5,
        f64::from(pos.y()) + 0.5,
        f64::from(pos.z()) + 0.5,
    );
    let mut nearby = Vec::new();
    world.players.iter_players(|_, player| {
        if !player.is_spectator()
            && Entity::is_alive(player.as_ref())
            && origin.distance(player.position()) < PLAYER_SEARCH_RADIUS
        {
            nearby.push(Arc::clone(player));
        }
        true
    });
    nearby
}

fn nearby_player_handle(world: &Arc<World>, player: &Player) -> Option<Arc<Player>> {
    let mut found = None;
    world.players.iter_players(|_, candidate| {
        if candidate.id() == player.id() {
            found = Some(Arc::clone(candidate));
            return false;
        }
        true
    });
    found
}
