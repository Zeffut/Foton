//! The `NEAREST_VISIBLE_LIVING_ENTITIES` memory value.

use rustc_hash::FxHashSet;

use super::EntityMemory;
use crate::entity::{LivingEntity, SharedEntity};

/// The living entities near a brain, with the ones it can see marked.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.memory.NearestVisibleLivingEntities`.
///
/// Vanilla evaluates the line-of-sight test lazily behind a memoising map. Steel
/// evaluates it once for every entity when the sensor builds the list, because
/// the closure vanilla memoises captures the level and the body and cannot be
/// stored in a `Clone` memory value. The observable behavior is the same:
/// vanilla's cache is also never invalidated for the twenty ticks the object
/// lives, so both answer from a snapshot taken at sensor time.
#[derive(Debug, Clone, Default)]
pub struct NearestVisibleLivingEntities {
    nearby: Vec<EntityMemory>,
    visible: FxHashSet<i32>,
}

impl NearestVisibleLivingEntities {
    /// Records `nearby` sorted nearest-first, with `visible` holding the ids
    /// that passed the targeting test.
    #[must_use]
    pub const fn new(nearby: Vec<EntityMemory>, visible: FxHashSet<i32>) -> Self {
        Self { nearby, visible }
    }

    /// Returns everything nearby, visible or not, nearest first.
    ///
    /// Vanilla parity: `NearestVisibleLivingEntities.nearbyEntities`.
    #[must_use]
    pub fn nearby(&self) -> &[EntityMemory] {
        &self.nearby
    }

    /// Returns the nearest visible entity matching `filter`.
    ///
    /// Vanilla parity: `NearestVisibleLivingEntities.findClosest`.
    pub fn find_closest(
        &self,
        mut filter: impl FnMut(&dyn LivingEntity) -> bool,
    ) -> Option<SharedEntity> {
        for candidate in &self.nearby {
            if !self.visible.contains(&candidate.id()) {
                continue;
            }
            let Some(entity) = candidate.get() else {
                continue;
            };
            let Some(living) = entity.as_living_entity() else {
                continue;
            };
            if filter(living) {
                return Some(entity);
            }
        }
        None
    }

    /// Returns whether the entity with this id is nearby and visible.
    ///
    /// Vanilla parity: `NearestVisibleLivingEntities.contains(LivingEntity)`.
    #[must_use]
    pub fn contains_entity(&self, id: i32) -> bool {
        self.visible.contains(&id) && self.nearby.iter().any(|entity| entity.id() == id)
    }
}
