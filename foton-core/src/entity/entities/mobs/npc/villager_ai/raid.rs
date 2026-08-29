//! What a village does when it survives a raid.
//!
//! Vanilla parity: `net.minecraft.world.entity.ai.behavior.CelebrateVillagersSurvivedRaid`,
//! which vanilla types to `Villager` even though it sits in the shared
//! `ai/behavior` package -- so it lives here rather than beside the raid
//! behaviors that are typed on `LivingEntity`.

use std::sync::Arc;

use foton_registry::DyeColor;
use foton_registry::data_components::components::{
    FireworkExplosion, FireworkExplosionShape, Fireworks,
};
use foton_registry::data_components::vanilla_components::FIREWORKS;
use foton_registry::item_stack::ItemStack;
use foton_registry::{vanilla_entities, vanilla_items};
use glam::DVec3;

use super::villager;
use crate::entity::ai::brain::behavior::{
    BrainContext, MemoryModuleId, MemoryStatus, TimedBehavior, has_no_blocks_above,
};
use crate::entity::entities::FireworkRocketEntity;
use crate::entity::{Projectile as _, SharedEntity, next_entity_id};
use crate::raid::Raid;

/// How long the celebration runs.
///
/// Vanilla parity: the `new CelebrateVillagersSurvivedRaid(600, 600)` of the
/// raid package -- a fixed thirty seconds rather than a range.
pub const CELEBRATION_DURATION: i32 = 600;

/// One tick in this many gets a cheer.
///
/// Vanilla parity: the `random.nextInt(100) == 0` of the tick.
const CELEBRATE_SOUND_CHANCE_IN: i32 = 100;

/// One tick in this many gets a rocket.
///
/// Vanilla parity: the `random.nextInt(200) == 0` of the tick.
const FIREWORK_CHANCE_IN: i32 = 200;

/// The flight durations a celebration rocket is built with.
///
/// Vanilla parity: the `random.nextInt(3)` of `tick`.
const FIREWORK_FLIGHT_DURATIONS: i32 = 3;

/// Cheers and lets off fireworks over a village that held.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.behavior.CelebrateVillagersSurvivedRaid`.
///
/// It holds the raid it started on rather than looking it up again, because
/// what ends the celebration is that *that* raid was cleaned up -- and a raid
/// that has been dropped from the manager can no longer be found by position.
/// The rockets only go up from a block open to the sky, which is what
/// [`MoveToSkySeeingSpot`] spends the same package's rolls arranging.
///
/// [`MoveToSkySeeingSpot`]: crate::entity::ai::brain::behavior::MoveToSkySeeingSpot
pub struct CelebrateVillagersSurvivedRaid {
    min_duration: i32,
    max_duration: i32,
    current_raid: Option<Arc<Raid>>,
}

impl CelebrateVillagersSurvivedRaid {
    /// Vanilla parity: `new CelebrateVillagersSurvivedRaid(minDuration, maxDuration)`.
    #[must_use]
    pub const fn new(min_duration: i32, max_duration: i32) -> Self {
        Self {
            min_duration,
            max_duration,
            current_raid: None,
        }
    }

    /// Vanilla parity: the private `CelebrateVillagersSurvivedRaid.getFirework`.
    fn firework(color: DyeColor, flight_duration: i32) -> Option<ItemStack> {
        let explosion = FireworkExplosion::new(
            FireworkExplosionShape::Burst,
            vec![color.firework_color()],
            Vec::new(),
            false,
            false,
        );
        let fireworks = Fireworks::new(flight_duration, vec![explosion]).ok()?;
        let mut rocket = ItemStack::new(&vanilla_items::FIREWORK_ROCKET);
        rocket.set(FIREWORKS, fireworks);
        Some(rocket)
    }

    /// Vanilla parity: the `Projectile.spawnProjectile(new FireworkRocketEntity(..))`
    /// half of the tick.
    fn launch_firework(ctx: &BrainContext<'_>) {
        let color = DyeColor::VALUES[rand::random_range(0..DyeColor::VALUES.len())];
        let Some(item) = Self::firework(color, rand::random_range(0..FIREWORK_FLIGHT_DURATIONS))
        else {
            return;
        };

        let mob = ctx.mob();
        let position = DVec3::new(mob.position().x, mob.get_eye_y(), mob.position().z);
        let rocket = FireworkRocketEntity::launched(
            &vanilla_entities::FIREWORK_ROCKET,
            next_entity_id(),
            position,
            Arc::downgrade(ctx.world()),
            item,
        );
        rocket.set_owner_uuid(Some(mob.uuid()));
        if let Err(error) = ctx.world().try_add_entity(Arc::new(rocket) as SharedEntity) {
            log::debug!("failed to spawn a celebration firework: {error}");
        }
    }
}

impl TimedBehavior for CelebrateVillagersSurvivedRaid {
    fn entry_condition(&self) -> &[(MemoryModuleId, MemoryStatus)] {
        // Vanilla parity: the `ImmutableMap.of()` of the `super(...)` call.
        &[]
    }

    fn duration(&self) -> (i32, i32) {
        (self.min_duration, self.max_duration)
    }

    fn check_extra_start_conditions(&mut self, ctx: &BrainContext<'_>) -> bool {
        let pos = ctx.mob().block_position();
        self.current_raid = ctx.world().get_raid_at(pos);
        self.current_raid
            .as_ref()
            .is_some_and(|raid| raid.is_victory())
            && has_no_blocks_above(ctx.world(), ctx.mob(), pos)
    }

    fn can_still_use(&mut self, _ctx: &BrainContext<'_>) -> bool {
        self.current_raid
            .as_ref()
            .is_some_and(|raid| !raid.is_stopped())
    }

    fn stop(&mut self, ctx: &BrainContext<'_>) {
        self.current_raid = None;
        ctx.brain()
            .update_activity_from_schedule(ctx.world(), ctx.game_time());
    }

    fn tick(&mut self, ctx: &BrainContext<'_>) {
        if rand::random_range(0..CELEBRATE_SOUND_CHANCE_IN) == 0
            && let Some(villager) = villager(ctx)
        {
            villager.play_celebrate_sound();
        }

        if rand::random_range(0..FIREWORK_CHANCE_IN) == 0
            && has_no_blocks_above(ctx.world(), ctx.mob(), ctx.mob().block_position())
        {
            Self::launch_firework(ctx);
        }
    }

    fn debug_name(&self) -> &'static str {
        "CelebrateVillagersSurvivedRaid"
    }
}
