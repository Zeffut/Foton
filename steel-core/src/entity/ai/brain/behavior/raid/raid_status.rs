//! Vanilla `SetRaidStatus` and `ResetRaidStatus`.

use crate::entity::ai::brain::Activity;
use crate::entity::ai::brain::behavior::{BrainContext, Trigger};

/// One tick in this many actually looks the raid up.
///
/// Vanilla parity: the `level.getRandom().nextInt(20) != 0` both behaviors open
/// with. It is a throttle rather than a coin flip: every villager in the
/// village runs both of these on every tick of the core package and of its raid
/// package, and each lookup walks the raid manager.
const RAID_LOOKUP_CHANCE_IN: i32 = 20;

/// Puts a villager standing in a raid into the matching activity.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.behavior.SetRaidStatus`.
///
/// Both the default activity and the active one are set: the active one is what
/// the villager does now, and the default is where
/// [`Brain::set_active_activity_if_possible`] lands when some other activity's
/// memory conditions fail -- without it a villager whose bell was destroyed
/// mid-raid would fall back to IDLE and stroll through the pillagers.
///
/// [`Brain::set_active_activity_if_possible`]: crate::entity::ai::brain::Brain::set_active_activity_if_possible
pub struct SetRaidStatus;

impl Trigger for SetRaidStatus {
    fn trigger(&mut self, ctx: &BrainContext<'_>) -> bool {
        if rand::random_range(0..RAID_LOOKUP_CHANCE_IN) != 0 {
            return false;
        }

        // Vanilla returns `true` once the roll passes, whether or not there was
        // a raid to find -- the roll is the work this behavior did.
        let Some(raid) = ctx.world().get_raid_at(ctx.mob().block_position()) else {
            return true;
        };

        let activity = if raid.has_first_wave_spawned() && !raid.is_between_waves() {
            Activity::Raid
        } else {
            Activity::PreRaid
        };
        let brain = ctx.brain();
        brain.set_default_activity(activity);
        brain.set_active_activity_if_possible(activity);
        true
    }

    fn debug_name(&self) -> &'static str {
        "SetRaidStatus"
    }
}

/// Gives a villager its day back once the raid over it has ended.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.behavior.ResetRaidStatus`.
/// It sits at priority 99 of both the `PRE_RAID` and RAID packages, which is the
/// slot [`UpdateActivityFromSchedule`] holds in every other package -- a
/// villager in a raid reads the clock only through this.
///
/// A *victory* deliberately does not end it: the raid stays until its
/// celebration ticks run out, which is what keeps
/// `CelebrateVillagersSurvivedRaid` running.
///
/// [`UpdateActivityFromSchedule`]: crate::entity::ai::brain::behavior::UpdateActivityFromSchedule
pub struct ResetRaidStatus;

impl Trigger for ResetRaidStatus {
    fn trigger(&mut self, ctx: &BrainContext<'_>) -> bool {
        if rand::random_range(0..RAID_LOOKUP_CHANCE_IN) != 0 {
            return false;
        }

        let raid = ctx.world().get_raid_at(ctx.mob().block_position());
        let over = raid.is_none_or(|raid| raid.is_stopped() || raid.is_loss());
        if over {
            let brain = ctx.brain();
            brain.set_default_activity(Activity::Idle);
            brain.update_activity_from_schedule(ctx.world(), ctx.game_time());
        }
        true
    }

    fn debug_name(&self) -> &'static str {
        "ResetRaidStatus"
    }
}
