//! Pick the fallen captain's banner up and take command of the wave.
//!
//! Vanilla parity: `Raider.ObtainRaidLeaderBannerGoal`. When the captain of a
//! wave dies its banner drops, and the first raider that can reach it puts it
//! on and becomes the new captain -- which is why killing a captain mid-raid
//! rarely ends the wave. Banners that cannot be pathed to are remembered as
//! unreachable for thirty seconds so the goal does not spend every tick
//! failing on the same item.

use rustc_hash::FxHashMap;

use super::selector::{Goal, GoalControls};
use crate::entity::ai::path::Path;
use crate::entity::entities::ItemEntity;
use crate::entity::raider::{is_ominous_banner, pick_up_banner};
use crate::entity::{Entity, LivingEntity, PathfinderMob, SharedEntity};
use crate::inventory::equipment::EquipmentSlot;
use foton_registry::vanilla_attributes;
use foton_utils::Downcast as _;

/// Ticks a banner stays written off as unreachable.
///
/// Vanilla parity: the `getGameTime() + 600L` of `canUse`.
const UNREACHABLE_TIMEOUT: i64 = 600;

/// Vertical reach of the banner search.
///
/// Vanilla parity: the `inflate(followRange, 8.0, followRange)` of `canUse`.
const SEARCH_HEIGHT: f64 = 8.0;

/// Speed the raider walks to the banner at.
///
/// Vanilla parity: the `moveTo(path, 1.15)` of `start`.
const APPROACH_SPEED: f64 = 1.15;

/// How close counts as picking the banner up.
///
/// Vanilla parity: the `closerThan(mob, 1.414)` of `tick`.
const PICKUP_DISTANCE: f64 = 1.414;

/// Fetches the captain's banner off the ground.
///
/// Vanilla parity: `Raider.ObtainRaidLeaderBannerGoal`.
pub(crate) struct ObtainRaidLeaderBannerGoal {
    /// Banners that could not be pathed to, and the game time to retry at.
    unreachable_banners: FxHashMap<i32, i64>,
    path_to_banner: Option<Path>,
    pursued_banner: Option<SharedEntity>,
}

impl ObtainRaidLeaderBannerGoal {
    /// Creates the goal.
    #[must_use]
    pub(crate) fn new() -> Self {
        Self {
            unreachable_banners: FxHashMap::default(),
            path_to_banner: None,
            pursued_banner: None,
        }
    }

    /// Returns whether this raider has no business chasing a banner.
    ///
    /// Vanilla parity: `cannotPickUpBanner`.
    fn cannot_pick_up_banner(mob: &dyn PathfinderMob) -> bool {
        let Some(raider) = mob.as_raider() else {
            return true;
        };
        let Some(status) = raider.current_raid_status() else {
            return true;
        };
        if !status.active || status.over || !raider.can_be_leader() {
            return true;
        }

        let mut already_wearing = false;
        raider.with_equipment_slot(EquipmentSlot::Head, &mut |item| {
            already_wearing = is_ominous_banner(item);
        });
        if already_wearing {
            return true;
        }

        // Vanilla asks the raid for the wave's leader and refuses while it is
        // alive; a leader entry whose mob is gone is a leader that died.
        let Some(raid) = raider.current_raid() else {
            return true;
        };
        let Some(leader_id) = raid.leader(raider.wave()) else {
            return false;
        };
        mob.level()
            .and_then(|world| world.get_entity_by_id(leader_id))
            .is_some_and(|leader| {
                leader
                    .as_living_entity()
                    .is_some_and(LivingEntity::is_alive)
            })
    }

    /// Returns whether `entity` is a banner on the ground worth walking to.
    ///
    /// Vanilla parity: `Raider.ALLOWED_ITEMS`.
    fn is_loose_banner(entity: &dyn Entity) -> bool {
        let Some(item) = entity.downcast_ref::<ItemEntity>() else {
            return false;
        };
        !item.has_pickup_delay() && !entity.is_removed() && is_ominous_banner(&item.get_item())
    }
}

impl Default for ObtainRaidLeaderBannerGoal {
    fn default() -> Self {
        Self::new()
    }
}

impl Goal for ObtainRaidLeaderBannerGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::MOVE
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        if Self::cannot_pick_up_banner(mob) {
            return false;
        }
        let Some(world) = mob.level() else {
            return false;
        };

        let follow_range = mob
            .attributes()
            .lock()
            .required_value(vanilla_attributes::FOLLOW_RANGE);
        let search = mob
            .bounding_box()
            .inflate_xyz(follow_range, SEARCH_HEIGHT, follow_range);
        let banners = world.get_entities_in_aabb_matching(&search, Self::is_loose_banner);

        let game_time = world.game_time();
        let mut still_unreachable = FxHashMap::default();
        for banner in banners {
            let retry_at = self
                .unreachable_banners
                .get(&banner.id())
                .copied()
                .unwrap_or(i64::MIN);
            if game_time < retry_at {
                still_unreachable.insert(banner.id(), retry_at);
                continue;
            }
            let path = mob.create_path_to(banner.block_position(), 1);
            if let Some(path) = path
                && path.can_reach()
            {
                self.path_to_banner = Some(path);
                self.pursued_banner = Some(banner);
                return true;
            }
            still_unreachable.insert(banner.id(), game_time + UNREACHABLE_TIMEOUT);
        }

        self.unreachable_banners = still_unreachable;
        false
    }

    fn can_continue_to_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        let (Some(banner), Some(path)) =
            (self.pursued_banner.as_ref(), self.path_to_banner.as_ref())
        else {
            return false;
        };
        if banner.is_removed() || path.is_done() {
            return false;
        }
        !Self::cannot_pick_up_banner(mob)
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        mob.move_to_path(self.path_to_banner.clone(), APPROACH_SPEED);
    }

    fn stop(&mut self, _mob: &dyn PathfinderMob) {
        self.path_to_banner = None;
        self.pursued_banner = None;
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        let Some(banner) = self.pursued_banner.as_ref() else {
            return;
        };
        if banner.position().distance_squared(mob.position()) > PICKUP_DISTANCE * PICKUP_DISTANCE {
            return;
        }
        let Some(raider) = mob.as_raider() else {
            return;
        };
        let Some(world) = mob.level() else {
            return;
        };
        pick_up_banner(raider, &world, banner);
    }
}
