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
use steel_registry::{REGISTRY, TaggedRegistryExt as _, vanilla_blocks};
use steel_utils::{BlockPos, BlockStateId, WorldAabb};

use crate::entity::{ENTITIES, EntitySpawnReason, SharedEntity, next_entity_id};
use crate::physics::WorldCollisionProvider;
use crate::physics::collision::CollisionWorld as _;
use crate::world::World;

/// What counts as a surface a mob may be dropped onto.
///
/// Vanilla parity: `SpawnUtil.Strategy`. Only the strategies a Steel caller
/// needs are here; `ON_TOP_OF_COLLIDER` arrives with the first caller that
/// wants it -- Steel's sculk shrieker reaches its own copy in
/// [`crate::world::spawn_util`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpawnStrategy {
    /// Vanilla parity: `Strategy.ON_TOP_OF_COLLIDER_NO_LEAVES`, a full upward
    /// collision face with nothing solid above it and no leaves underfoot --
    /// which is what stops a creaking from being dropped into a treetop.
    OnTopOfColliderNoLeaves,
    /// Vanilla parity: `Strategy.LEGACY_IRON_GOLEM`, deprecated upstream and
    /// kept because a village raising a golem is the one caller that still
    /// uses it. It is a hand-written list of blocks a golem may not be put on
    /// rather than a rule -- glass and leaves it would fall through, and a
    /// handful of blocks a heavy mob landing on them would make a nuisance of.
    LegacyIronGolem,
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
            Self::LegacyIronGolem => {
                !is_refused_underfoot(state)
                    && (above_state.is_air() || !above_state.get_fluid_state().is_empty())
                    && (state.is_solid() || state.get_block() == &vanilla_blocks::POWDER_SNOW)
            }
        }
    }
}

/// Whether `Strategy.LEGACY_IRON_GOLEM` refuses to stand a golem on this block.
///
/// Vanilla parity: the fourteen-term rejection list of
/// `SpawnUtil.Strategy.LEGACY_IRON_GOLEM`. Three of its terms are `instanceof`
/// checks on block classes Steel does not model one-for-one, so each is read
/// off the tag that holds exactly that class:
///
/// * `LeavesBlock` is `#minecraft:leaves`, the same reading the ravager's
///   leaf-trampling already takes.
/// * `StainedGlassBlock` is `#minecraft:impermeable` less its three other
///   members -- `glass` and `tinted_glass` are refused by name in the same
///   predicate anyway, so only `barrier` has to be excused to leave exactly the
///   sixteen dyed glass blocks.
/// * `StainedGlassPaneBlock` is `#c:glass_panes` less `glass_pane`, which is
///   likewise refused by name.
fn is_refused_underfoot(state: BlockStateId) -> bool {
    let block = state.get_block();
    let named = [
        &vanilla_blocks::COBWEB,
        &vanilla_blocks::CACTUS,
        &vanilla_blocks::GLASS_PANE,
        &vanilla_blocks::CONDUIT,
        &vanilla_blocks::ICE,
        &vanilla_blocks::TNT,
        &vanilla_blocks::GLOWSTONE,
        &vanilla_blocks::BEACON,
        &vanilla_blocks::SEA_LANTERN,
        &vanilla_blocks::FROSTED_ICE,
        &vanilla_blocks::TINTED_GLASS,
        &vanilla_blocks::GLASS,
    ];
    if named.into_iter().any(|refused| block == refused) {
        return true;
    }
    if block == &vanilla_blocks::BARRIER {
        // In `#minecraft:impermeable` but not on vanilla's list.
        return false;
    }
    REGISTRY.blocks.is_in_tag(block, &BlockTag::LEAVES)
        || REGISTRY.blocks.is_in_tag(block, &BlockTag::IMPERMEABLE)
        || REGISTRY.blocks.is_in_tag(block, &BlockTag::C_GLASS_PANES)
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
#[expect(
    clippy::too_many_arguments,
    reason = "vanilla's `SpawnUtil.trySpawnMob` signature, kept argument for argument"
)]
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

        // No factory, or a factory that built something that is not a mob,
        // means this entity type can never be spawned this way; a later attempt
        // would fail the same, so give up rather than spin.
        let entity = ENTITIES.create(
            entity_type,
            next_entity_id(),
            position,
            Arc::downgrade(world),
        )?;
        let mob = entity.as_mob()?;
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
    use steel_registry::blocks::BlockRef;
    use steel_registry::{init_vanilla_registry, vanilla_blocks, vanilla_entities};
    use steel_utils::ChunkPos;
    use steel_utils::types::UpdateFlags;

    use super::*;
    use crate::behavior::init_behaviors;
    use crate::entity::init_entities;
    use crate::test_support::{fresh_test_world, insert_ready_full_chunk};

    /// The heart's own spawn point, and the middle of the only loaded chunk.
    const ORIGIN: BlockPos = BlockPos::new(8, 64, 8);

    fn spawn_world(key: &'static str, floor: BlockRef) -> Arc<World> {
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
            &vanilla_entities::CREAKING,
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
            &vanilla_entities::CREAKING,
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
