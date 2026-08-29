//! Where a brain is looking or walking.

use foton_utils::BlockPos;
use glam::DVec3;

use super::memory::EntityMemory;
use super::memory::memory_module_types;
use crate::entity::{LivingEntity, Mob, SharedEntity};

/// A point a behavior can aim at, either a fixed block or a moving entity.
///
/// Vanilla parity: the `PositionTracker` interface with its two
/// implementations, `BlockPosTracker` and `EntityTracker`. Vanilla needs an
/// interface because a memory holds an arbitrary implementation; Foton closes
/// it into an enum because a memory value has to be `Clone` and because those
/// two are the only implementations vanilla has ever had.
#[derive(Debug, Clone)]
pub enum PositionTracker {
    /// Vanilla parity: `BlockPosTracker`.
    Block {
        /// The tracked block.
        block_pos: BlockPos,
        /// The point within it that is actually aimed at.
        position: DVec3,
    },
    /// Vanilla parity: `EntityTracker`.
    Entity {
        /// The tracked entity.
        entity: EntityMemory,
        /// Whether [`Self::current_position`] aims at the eyes.
        track_eye_height: bool,
        /// Whether [`Self::current_block_position`] uses the eye block.
        target_eye_height: bool,
    },
}

impl PositionTracker {
    /// Tracks the center of a block.
    ///
    /// Vanilla parity: `new BlockPosTracker(BlockPos)`.
    #[must_use]
    pub fn of_block(block_pos: BlockPos) -> Self {
        let (x, y, z) = block_pos.get_center();
        Self::Block {
            block_pos,
            position: DVec3::new(x, y, z),
        }
    }

    /// Tracks an exact point.
    ///
    /// Vanilla parity: `new BlockPosTracker(Vec3)`.
    #[must_use]
    pub const fn of_position(position: DVec3) -> Self {
        Self::Block {
            block_pos: BlockPos::containing(position.x, position.y, position.z),
            position,
        }
    }

    /// Tracks an entity.
    ///
    /// Vanilla parity: `new EntityTracker(Entity, boolean)`.
    #[must_use]
    pub fn of_entity(entity: &SharedEntity, track_eye_height: bool) -> Self {
        Self::of_entity_targeting(entity, track_eye_height, false)
    }

    /// Tracks an entity and says which of its heights to aim at.
    ///
    /// Vanilla parity: the three-argument `EntityTracker(entity, trackEyeHeight,
    /// targetEyeHeight)`. A mob that hovers walks to the eyes of what it is
    /// following, not to its feet.
    #[must_use]
    pub fn of_entity_targeting(
        entity: &SharedEntity,
        track_eye_height: bool,
        target_eye_height: bool,
    ) -> Self {
        Self::Entity {
            entity: EntityMemory::new(entity),
            track_eye_height,
            target_eye_height,
        }
    }

    /// Returns the point to aim at, or `None` once a tracked entity is gone.
    ///
    /// Vanilla parity: `PositionTracker.currentPosition`. Vanilla holds the
    /// entity strongly and cannot fail; Foton holds it weakly so a brain does
    /// not keep a removed entity alive.
    #[must_use]
    pub fn current_position(&self) -> Option<DVec3> {
        match self {
            Self::Block { position, .. } => Some(*position),
            Self::Entity {
                entity,
                track_eye_height,
                ..
            } => {
                let entity = entity.get()?;
                let position = entity.position();
                if *track_eye_height {
                    Some(position + DVec3::new(0.0, entity.get_eye_height(), 0.0))
                } else {
                    Some(position)
                }
            }
        }
    }

    /// Returns the block to walk to, or `None` once a tracked entity is gone.
    ///
    /// Vanilla parity: `PositionTracker.currentBlockPosition`.
    #[must_use]
    pub fn current_block_position(&self) -> Option<BlockPos> {
        match self {
            Self::Block { block_pos, .. } => Some(*block_pos),
            Self::Entity {
                entity,
                target_eye_height,
                ..
            } => {
                let entity = entity.get()?;
                if *target_eye_height {
                    let eye = entity.position() + DVec3::new(0.0, entity.get_eye_height(), 0.0);
                    Some(BlockPos::containing(eye.x, eye.y, eye.z))
                } else {
                    Some(entity.block_position())
                }
            }
        }
    }

    /// Returns whether `body` can currently see this target.
    ///
    /// Vanilla parity: `PositionTracker.isVisibleBy`. A block is always
    /// visible; an entity has to be alive and in `body`'s
    /// `NEAREST_VISIBLE_LIVING_ENTITIES` memory.
    #[must_use]
    pub fn is_visible_by(&self, body: &dyn Mob) -> bool {
        let Self::Entity { entity, .. } = self else {
            return true;
        };
        let Some(tracked) = entity.get() else {
            return false;
        };
        let Some(living) = tracked.as_living_entity() else {
            return true;
        };
        if !LivingEntity::is_alive(living) {
            return false;
        }
        let Some(brain) = body.brain() else {
            return false;
        };
        brain
            .get_memory(memory_module_types::NEAREST_VISIBLE_LIVING_ENTITIES)
            .is_some_and(|visible| visible.contains_entity(entity.id()))
    }

    /// Returns the tracked entity, if this tracker follows one.
    #[must_use]
    pub fn entity(&self) -> Option<SharedEntity> {
        match self {
            Self::Block { .. } => None,
            Self::Entity { entity, .. } => entity.get(),
        }
    }
}
