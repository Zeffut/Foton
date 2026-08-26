//! Vanilla `VillageBoundRandomStroll`.

use std::f64::consts::FRAC_PI_2;

use glam::DVec3;
use steel_utils::SectionPos;

use super::{BrainContext, Trigger};
use crate::entity::ai::brain::memory::{MemoryModuleId, WalkTarget, memory_module_types};
use crate::entity::ai::goal::{default_random_pos_towards, land_random_pos};

/// Vanilla parity: `VillageBoundRandomStroll.MAX_XZ_DIST`.
const MAX_XZ_DIST: i32 = 10;
/// Vanilla parity: `VillageBoundRandomStroll.MAX_Y_DIST`.
const MAX_Y_DIST: i32 = 7;
/// Vanilla parity: the `radius` of the `findSectionClosestToVillage` call.
const VILLAGE_SEARCH_SECTION_RADIUS: i32 = 2;

/// Wanders, but drifts back toward the village when it has strayed out.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.behavior.VillageBoundRandomStroll`.
/// Inside a village this is a plain land stroll; outside one it aims at the
/// nearest section that is closer to a village center, which is what keeps a
/// panicking or idling villager from wandering off into the dark forever.
pub struct VillageBoundRandomStroll {
    speed_modifier: f64,
    max_xz_dist: i32,
    max_y_dist: i32,
}

impl VillageBoundRandomStroll {
    /// Vanilla parity: the one-argument `VillageBoundRandomStroll.create`.
    #[must_use]
    pub const fn new(speed_modifier: f64) -> Self {
        Self::with_range(speed_modifier, MAX_XZ_DIST, MAX_Y_DIST)
    }

    /// Vanilla parity: the three-argument `VillageBoundRandomStroll.create`.
    #[must_use]
    pub const fn with_range(speed_modifier: f64, max_xz_dist: i32, max_y_dist: i32) -> Self {
        Self {
            speed_modifier,
            max_xz_dist,
            max_y_dist,
        }
    }
}

impl Trigger for VillageBoundRandomStroll {
    fn required_memories(&self) -> Vec<MemoryModuleId> {
        vec![memory_module_types::WALK_TARGET.id()]
    }

    fn trigger(&mut self, ctx: &BrainContext<'_>) -> bool {
        let brain = ctx.brain();
        if brain.has_memory_value(memory_module_types::WALK_TARGET.id()) {
            return false;
        }

        let mob = ctx.mob();
        let world = ctx.world();
        let body_pos = mob.block_position();
        let stroll_to = if world.is_village(body_pos) {
            land_random_pos(mob, self.max_xz_dist, self.max_y_dist)
        } else {
            let section = SectionPos::from_block_pos(body_pos);
            let toward = find_section_closest_to_village(ctx, section);
            if toward == section {
                land_random_pos(mob, self.max_xz_dist, self.max_y_dist)
            } else {
                default_random_pos_towards(
                    mob,
                    self.max_xz_dist,
                    self.max_y_dist,
                    section_bottom_center(toward),
                    FRAC_PI_2,
                )
            }
        };

        brain.set_memory_or_erase(
            memory_module_types::WALK_TARGET,
            stroll_to.map(|pos| WalkTarget::of_position(pos, self.speed_modifier, 0)),
        );
        true
    }

    fn debug_name(&self) -> &'static str {
        "VillageBoundRandomStroll"
    }
}

/// Vanilla parity: `BehaviorUtils.findSectionClosestToVillage`, which walks the
/// cube of sections around `center` and keeps the one nearest a village.
fn find_section_closest_to_village(ctx: &BrainContext<'_>, center: SectionPos) -> SectionPos {
    let world = ctx.world();
    let mut best = center;
    let mut best_distance = world.sections_to_village(center);
    for x in -VILLAGE_SEARCH_SECTION_RADIUS..=VILLAGE_SEARCH_SECTION_RADIUS {
        for y in -VILLAGE_SEARCH_SECTION_RADIUS..=VILLAGE_SEARCH_SECTION_RADIUS {
            for z in -VILLAGE_SEARCH_SECTION_RADIUS..=VILLAGE_SEARCH_SECTION_RADIUS {
                let candidate = SectionPos::new(center.x() + x, center.y() + y, center.z() + z);
                let distance = world.sections_to_village(candidate);
                if distance < best_distance {
                    best_distance = distance;
                    best = candidate;
                }
            }
        }
    }
    best
}

/// Vanilla parity: `Vec3.atBottomCenterOf(sectionPos.center())`.
fn section_bottom_center(section: SectionPos) -> DVec3 {
    DVec3::new(
        f64::from((section.x() << 4) + 8) + 0.5,
        f64::from((section.y() << 4) + 8),
        f64::from((section.z() << 4) + 8) + 0.5,
    )
}
