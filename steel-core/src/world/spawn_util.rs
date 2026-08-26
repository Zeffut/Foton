//! Finding somewhere for a triggered mob to appear.

use std::sync::Arc;

use glam::DVec3;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::blocks::shapes::is_offset_face_full;
use steel_registry::entity_type::EntityTypeRef;
use steel_utils::{BlockPos, Direction};

use super::World;
use crate::entity::{ENTITIES, EntitySpawnReason, SharedEntity, next_entity_id};

/// Which blocks a triggered mob is allowed to stand on.
///
/// Vanilla parity: `SpawnUtil.Strategy`. Only the one the shrieker's warden uses is
/// ported; the iron golem's legacy strategy arrives with the villager that needs it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpawnStrategy {
    /// Vanilla `SpawnUtil.Strategy.ON_TOP_OF_COLLIDER`: a full upward face with nothing
    /// standing on it.
    OnTopOfCollider,
}

impl SpawnStrategy {
    fn can_spawn_on(self, world: &Arc<World>, pos: BlockPos, above_pos: BlockPos) -> bool {
        match self {
            Self::OnTopOfCollider => {
                world
                    .get_block_state(above_pos)
                    .get_collision_shape_at(above_pos)
                    .is_empty()
                    && is_offset_face_full(
                        world.get_block_state(pos).get_collision_shape_at(pos),
                        Direction::Up,
                    )
            }
        }
    }
}

impl World {
    /// Puts one mob of `entity_type` somewhere near `start`, if anywhere will take it.
    ///
    /// Vanilla parity: `SpawnUtil.trySpawnMob`. It searches from `spawn_range_y` above the
    /// start downwards on each attempt, which is what lets a shrieker in a cave hand its
    /// warden the floor rather than the ceiling.
    ///
    /// Not implemented: the `checkCollisions` argument and the world-border bounds test.
    /// The one caller in Steel -- the sculk shrieker -- passes `false` for the first, and
    /// the second only differs outside the border.
    #[must_use]
    #[expect(
        clippy::too_many_arguments,
        reason = "mirrors Vanilla's SpawnUtil.trySpawnMob argument list"
    )]
    pub fn try_spawn_mob(
        self: &Arc<Self>,
        entity_type: EntityTypeRef,
        spawn_reason: EntitySpawnReason,
        start: BlockPos,
        spawn_attempts: i32,
        spawn_range_xz: i32,
        spawn_range_y: i32,
        strategy: SpawnStrategy,
    ) -> Option<SharedEntity> {
        for _ in 0..spawn_attempts {
            let dx = rand::random_range(-spawn_range_xz..=spawn_range_xz);
            let dz = rand::random_range(-spawn_range_xz..=spawn_range_xz);
            let search_start = start.offset(dx, spawn_range_y, dz);
            let Some(spawn_pos) =
                possible_spawn_position(self, spawn_range_y, search_start, strategy)
            else {
                continue;
            };

            let position = DVec3::new(
                f64::from(spawn_pos.x()) + 0.5,
                f64::from(spawn_pos.y()),
                f64::from(spawn_pos.z()) + 0.5,
            );
            let entity = ENTITIES.create(
                entity_type,
                next_entity_id(),
                position,
                Arc::downgrade(self),
            )?;
            let mob = entity.as_mob()?;

            mob.finalize_spawn(self, spawn_reason, None);
            if self.try_add_entity(Arc::clone(&entity)).is_err() {
                continue;
            }
            // Vanilla announces the arrival with the mob's own ambient sound, which for a
            // warden is the agitated growl its emerge already started.
            mob.play_ambient_sound();
            return Some(entity);
        }

        None
    }
}

/// Vanilla `SpawnUtil.moveToPossibleSpawnPosition`.
fn possible_spawn_position(
    world: &Arc<World>,
    spawn_range_y: i32,
    search_start: BlockPos,
    strategy: SpawnStrategy,
) -> Option<BlockPos> {
    let mut search_pos = search_start;
    for _ in -spawn_range_y..=spawn_range_y {
        search_pos = search_pos.below();
        if strategy.can_spawn_on(world, search_pos, search_pos.above()) {
            return Some(search_pos.above());
        }
    }
    None
}
