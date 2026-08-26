//! Vanilla `net.minecraft.util.SpawnUtil`.
//!
//! The "put a mob somewhere near here" helper the game uses when a block, not
//! the natural spawner, decides a mob should exist: a creaking heart waking its
//! protector, a sculk shrieker calling a warden, a village raising an iron
//! golem. It rolls a handful of nearby columns, walks each one down until it
//! finds a surface the strategy accepts, and gives up quietly if none of them
//! works.

use std::sync::Arc;

use glam::DVec3;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::blocks::properties::Direction;
use steel_registry::blocks::shapes::is_face_full;
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::vanilla_block_tags::BlockTag;
use steel_utils::{BlockPos, BlockStateId, WorldAabb};

use crate::entity::{ENTITIES, EntitySpawnReason, SharedEntity, next_entity_id};
use crate::physics::WorldCollisionProvider;
use crate::physics::collision::CollisionWorld as _;
use crate::world::World;

/// What counts as a surface a mob may be dropped onto.
///
/// Vanilla parity: `SpawnUtil.Strategy`. Only the strategies a Steel caller
/// needs are here; `LEGACY_IRON_GOLEM` is deprecated upstream and
/// `ON_TOP_OF_COLLIDER` arrives with the first caller that wants it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpawnStrategy {
    /// Vanilla parity: `Strategy.ON_TOP_OF_COLLIDER_NO_LEAVES`, a full upward
    /// collision face with nothing solid above it and no leaves underfoot --
    /// which is what stops a creaking from being dropped into a treetop.
    OnTopOfColliderNoLeaves,
}

impl SpawnStrategy {
    /// Vanilla parity: `Strategy.canSpawnOn`.
    fn can_spawn_on(self, state: BlockStateId, above_state: BlockStateId) -> bool {
        match self {
            Self::OnTopOfColliderNoLeaves => {
                above_state.get_static_collision_shape().is_empty()
                    && !state.get_block().has_tag(&BlockTag::LEAVES)
                    && is_face_full(state.get_static_collision_shape(), Direction::Up)
            }
        }
    }
}

/// Tries to put one mob of `entity_type` on the ground near `start`.
///
/// Vanilla parity: `SpawnUtil.trySpawnMob`. Returns the spawned mob, or `None`
/// when every attempt found nowhere to stand.
///
/// Two approximations worth naming. Vanilla's `level.noCollision(aabb)` also
/// tests entity collisions and the world border's own shape; Steel's
/// [`CollisionWorld::has_block_collision`] is blocks only, so a creaking may be
/// spawned into the space another mob is standing in. And vanilla creates the
/// mob from the entity type and asks `checkSpawnObstruction`, which Steel has
/// no hook for -- the collision test above covers the same ground.
#[must_use]
pub fn try_spawn_mob(
    entity_type: EntityTypeRef,
    spawn_reason: EntitySpawnReason,
    world: &Arc<World>,
    start: BlockPos,
    spawn_attempts: i32,
    spawn_range_xz: i32,
    spawn_range_y: i32,
    strategy: SpawnStrategy,
    check_collisions: bool,
) -> Option<SharedEntity> {
    for _ in 0..spawn_attempts {
        let dx = rand::random_range(-spawn_range_xz..=spawn_range_xz);
        let dz = rand::random_range(-spawn_range_xz..=spawn_range_xz);
        let candidate = start.offset(dx, spawn_range_y, dz);
        if !world.is_block_within_world_border(candidate) {
            continue;
        }
        let Some(surface) =
            move_to_possible_spawn_position(world, spawn_range_y, candidate, strategy)
        else {
            continue;
        };

        let (x, _, z) = surface.get_center();
        let position = DVec3::new(x, f64::from(surface.y()), z);
        if check_collisions {
            let spawn_box = WorldAabb::entity_box(
                position.x,
                position.y,
                position.z,
                f64::from(entity_type.dimensions.half_width()),
                f64::from(entity_type.dimensions.height),
            );
            if WorldCollisionProvider::new(world).has_block_collision(&spawn_box) {
                continue;
            }
        }

        let Some(entity) = ENTITIES.create(
            entity_type,
            next_entity_id(),
            position,
            Arc::downgrade(world),
        ) else {
            return None;
        };
        let Some(mob) = entity.as_mob() else {
            return None;
        };
        if !mob.check_spawn_rules(world, spawn_reason, surface) {
            continue;
        }

        if let Err(error) = world.try_add_entity(Arc::clone(&entity)) {
            log::debug!("spawn util rejected a {}: {error}", entity_type.key);
            continue;
        }
        mob.play_ambient_sound();
        return Some(entity);
    }

    None
}

/// Walks a column down from `candidate` until the strategy accepts a surface.
///
/// Vanilla parity: the private `SpawnUtil.moveToPossibleSpawnPosition`, which
/// returns the block *above* the accepted surface -- the one the mob stands in.
fn move_to_possible_spawn_position(
    world: &Arc<World>,
    spawn_range_y: i32,
    candidate: BlockPos,
    strategy: SpawnStrategy,
) -> Option<BlockPos> {
    let mut search = candidate;
    let mut above_state = world.get_block_state(search);

    for _ in -spawn_range_y..=spawn_range_y {
        search = search.below();
        let state = world.get_block_state(search);
        if strategy.can_spawn_on(state, above_state) {
            return Some(search.above());
        }
        above_state = state;
    }

    None
}

#[cfg(test)]
mod tests {
    use steel_registry::{init_vanilla_registry, vanilla_blocks};
    use steel_utils::ChunkPos;
    use steel_utils::types::UpdateFlags;

    use super::*;
    use crate::behavior::init_behaviors;
    use crate::entity::init_entities;
    use crate::test_support::{fresh_test_world, insert_ready_full_chunk};

    /// The heart's own spawn point, and the middle of the only loaded chunk.
    const ORIGIN: BlockPos = BlockPos::new(8, 64, 8);

    fn spawn_world(key: &'static str, floor: steel_registry::blocks::BlockRef) -> Arc<World> {
        init_vanilla_registry();
        init_behaviors();
        // `try_spawn_mob` builds the mob through the generated factory table,
        // which is empty until this runs.
        init_entities();
        let world = fresh_test_world(key);
        insert_ready_full_chunk(&world, ChunkPos::new(0, 0));
        for x in 0..16 {
            for z in 0..16 {
                assert!(world.set_block(
                    BlockPos::new(x, ORIGIN.y() - 1, z),
                    floor.default_state(),
                    UpdateFlags::UPDATE_NONE,
                ));
            }
        }
        world
    }

    /// The spawn walks *down* from `start + spawnRangeY` looking for a surface,
    /// and lands the mob on top of it. A search that stopped at the first solid
    /// block would leave the creaking inside the floor.
    #[test]
    fn a_mob_is_put_on_top_of_the_first_surface_the_column_offers() {
        let world = spawn_world("spawn_util_on_ground", &vanilla_blocks::STONE);

        // `TrialSpawner` rather than the heart's `Spawner`: a monster spawned
        // for either reason still has to clear `checkMonsterSpawnRules`, and
        // only the trial spawner is exempt from the darkness half of it. A lit
        // test chunk would otherwise make this a test about the light level
        // rather than about finding the ground.
        let spawned = try_spawn_mob(
            &steel_registry::vanilla_entities::CREAKING,
            EntitySpawnReason::TrialSpawner,
            &world,
            ORIGIN,
            5,
            4,
            8,
            SpawnStrategy::OnTopOfColliderNoLeaves,
            true,
        )
        .expect("a flat stone floor should take a creaking somewhere");

        assert!(
            (spawned.position().y - f64::from(ORIGIN.y())).abs() < 1.0e-9,
            "the mob should stand on the floor, not in it: {:?}",
            spawned.position()
        );
        assert!(
            world.get_entity_by_id(spawned.id()).is_some(),
            "a spawned mob has to be in the world, not just constructed"
        );
    }

    /// Vanilla's `ON_TOP_OF_COLLIDER_NO_LEAVES` is the strategy the creaking
    /// heart uses, and the "no leaves" half is the whole reason it exists: a
    /// creaking dropped into a pale oak canopy would be stuck in the treetop.
    #[test]
    fn a_canopy_of_leaves_is_no_place_to_stand() {
        let world = spawn_world("spawn_util_no_leaves", &vanilla_blocks::PALE_OAK_LEAVES);

        let spawned = try_spawn_mob(
            &steel_registry::vanilla_entities::CREAKING,
            EntitySpawnReason::TrialSpawner,
            &world,
            ORIGIN,
            5,
            4,
            8,
            SpawnStrategy::OnTopOfColliderNoLeaves,
            true,
        );

        assert!(
            spawned.is_none(),
            "leaves are the one surface this strategy refuses"
        );
    }
}
