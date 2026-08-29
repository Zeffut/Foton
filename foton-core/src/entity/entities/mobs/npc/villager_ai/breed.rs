//! How a village grows.
//!
//! Vanilla parity: `VillagerMakeLove`, the behavior the IDLE package gates on
//! `BREED_TARGET`.

use std::sync::Arc;

use foton_registry::{REGISTRY, RegistryExt as _, vanilla_entities, vanilla_poi_types};
use foton_utils::entity_events::EntityStatus;
use foton_utils::{BlockPos, Downcast as _, GlobalPos};

use super::villager;
use crate::entity::ai::brain::behavior::{
    BrainContext, MemoryModuleId, MemoryStatus, TimedBehavior, utils,
};
use crate::entity::ai::brain::memory::memory_module_types;
use crate::entity::entities::mobs::npc::VillagerEntity;
use crate::entity::{
    AgeableMob, Entity as _, Mob, PathfinderMob as _, SharedEntity, next_entity_id,
};
use crate::poi::poi_storage::OccupationStatus;
use crate::world::World;

/// Vanilla parity: the `350` min and max duration of `VillagerMakeLove`.
const MAKE_LOVE_DURATION: i32 = 350;
/// Vanilla parity: the `275 + random.nextInt(50)` wait before a birth.
const MIN_BIRTH_DELAY: i32 = 275;
const BIRTH_DELAY_SPREAD: i32 = 50;
/// Vanilla parity: the `0.5F` and `2` of the `lockGazeAndWalkToEachOther` calls.
const COURTING_SPEED_MODIFIER: f64 = 0.5;
const COURTING_CLOSE_ENOUGH_DIST: i32 = 2;
/// Vanilla parity: the `distanceToSqr(target) > 5.0` that pauses the courtship
/// while the pair are still walking toward each other.
const COURTING_DISTANCE_SQR: f64 = 5.0;
/// Vanilla parity: the `nextInt(35) == 0` heart particles while courting.
const HEART_CHANCE_IN: i32 = 35;
/// Vanilla parity: the `48` block radius `takeVacantBed` searches.
const BED_SEARCH_RANGE: i32 = 48;
/// Vanilla parity: the `setAge(6000)` both parents get after a birth.
const PARENT_BREEDING_COOLDOWN: i32 = 6_000;
/// Vanilla parity: the `setAge(-24000)` the child starts at.
const CHILD_AGE: i32 = -24_000;
/// Vanilla parity: the `0.75` above which `getBreedOffspring` takes the other
/// parent's variant rather than its own.
const BIOME_VARIANT_CHANCE: f64 = 0.5;
/// Vanilla parity: the `biomeRoll < 0.75` arm of the same roll.
const OWN_VARIANT_CHANCE: f64 = 0.75;

/// Two villagers court, and a village gains a child.
///
/// Vanilla parity: `net.minecraft.world.entity.ai.behavior.VillagerMakeLove`.
/// The child is only born if there is a free bed for it, and the ticket is
/// taken before the child exists -- that is what caps a village's population at
/// its bed count, and what makes adding beds the way a player grows one.
pub struct VillagerMakeLove {
    birth_timestamp: i64,
}

impl VillagerMakeLove {
    /// Vanilla parity: `new VillagerMakeLove()`.
    #[must_use]
    pub const fn new() -> Self {
        Self { birth_timestamp: 0 }
    }

    /// Vanilla parity: the private `VillagerMakeLove.isBreedingPossible`.
    fn is_breeding_possible(ctx: &BrainContext<'_>) -> bool {
        if !utils::target_is_valid(
            ctx.brain(),
            memory_module_types::BREED_TARGET,
            &vanilla_entities::VILLAGER,
        ) {
            return false;
        }
        let Some(body) = villager(ctx) else {
            return false;
        };
        let Some(partner_entity) = breed_target(ctx) else {
            return false;
        };
        let Some(partner) = partner_entity.downcast_ref::<VillagerEntity>() else {
            return false;
        };
        body.can_breed() && partner.can_breed()
    }
}

impl Default for VillagerMakeLove {
    fn default() -> Self {
        Self::new()
    }
}

/// Vanilla parity: the `ImmutableMap` handed to `VillagerMakeLove`'s `super(...)`.
const MAKE_LOVE_ENTRY_CONDITION: &[(MemoryModuleId, MemoryStatus)] = &[
    (
        memory_module_types::BREED_TARGET.id(),
        MemoryStatus::ValuePresent,
    ),
    (
        memory_module_types::NEAREST_VISIBLE_LIVING_ENTITIES.id(),
        MemoryStatus::ValuePresent,
    ),
];

impl TimedBehavior for VillagerMakeLove {
    fn entry_condition(&self) -> &[(MemoryModuleId, MemoryStatus)] {
        MAKE_LOVE_ENTRY_CONDITION
    }

    fn duration(&self) -> (i32, i32) {
        (MAKE_LOVE_DURATION, MAKE_LOVE_DURATION)
    }

    fn check_extra_start_conditions(&mut self, ctx: &BrainContext<'_>) -> bool {
        Self::is_breeding_possible(ctx)
    }

    fn can_still_use(&mut self, ctx: &BrainContext<'_>) -> bool {
        ctx.game_time() <= self.birth_timestamp && Self::is_breeding_possible(ctx)
    }

    fn start(&mut self, ctx: &BrainContext<'_>) {
        let (Some(body), Some(partner)) = (body_entity(ctx), breed_target(ctx)) else {
            return;
        };
        utils::lock_gaze_and_walk_to_each_other(
            &body,
            &partner,
            COURTING_SPEED_MODIFIER,
            COURTING_CLOSE_ENOUGH_DIST,
        );
        partner.broadcast_entity_event(EntityStatus::InLoveHearts);
        body.broadcast_entity_event(EntityStatus::InLoveHearts);
        let duration = MIN_BIRTH_DELAY + rand::random_range(0..BIRTH_DELAY_SPREAD);
        self.birth_timestamp = ctx.game_time() + i64::from(duration);
    }

    fn tick(&mut self, ctx: &BrainContext<'_>) {
        let (Some(body), Some(partner)) = (body_entity(ctx), breed_target(ctx)) else {
            return;
        };
        if body.position().distance_squared(partner.position()) > COURTING_DISTANCE_SQR {
            return;
        }
        utils::lock_gaze_and_walk_to_each_other(
            &body,
            &partner,
            COURTING_SPEED_MODIFIER,
            COURTING_CLOSE_ENOUGH_DIST,
        );

        if ctx.game_time() < self.birth_timestamp {
            if rand::random_range(0..HEART_CHANCE_IN) == 0 {
                partner.broadcast_entity_event(EntityStatus::LoveHearts);
                body.broadcast_entity_event(EntityStatus::LoveHearts);
            }
            return;
        }

        let (Some(body), Some(partner)) = (
            body.downcast_ref::<VillagerEntity>(),
            partner.downcast_ref::<VillagerEntity>(),
        ) else {
            return;
        };
        body.eat_and_digest_food();
        partner.eat_and_digest_food();
        try_to_give_birth(ctx.world(), body, partner);
    }

    fn stop(&mut self, ctx: &BrainContext<'_>) {
        ctx.brain()
            .erase_memory(memory_module_types::BREED_TARGET.id());
    }

    fn debug_name(&self) -> &'static str {
        "VillagerMakeLove"
    }
}

/// The body as the world holds it, which is what the gaze helpers need.
fn body_entity(ctx: &BrainContext<'_>) -> Option<SharedEntity> {
    ctx.world().get_entity_by_id(ctx.mob().id())
}

/// The villager this one is courting, if the memory still names a live one.
fn breed_target(ctx: &BrainContext<'_>) -> Option<SharedEntity> {
    ctx.brain()
        .get_memory(memory_module_types::BREED_TARGET)?
        .get()
}

/// Vanilla parity: the private `VillagerMakeLove.tryToGiveBirth`.
fn try_to_give_birth(world: &Arc<World>, body: &VillagerEntity, partner: &VillagerEntity) {
    let Some(child_bed) = take_vacant_bed(world, body) else {
        // Vanilla's angry particles: the pair wanted a child and the village had
        // no bed to put it in.
        partner.broadcast_entity_event(EntityStatus::VillagerAngry);
        body.broadcast_entity_event(EntityStatus::VillagerAngry);
        return;
    };

    let Some(child) = breed(world, body, partner) else {
        let mut storage = world.poi_storage.lock();
        let _released = storage.release_ticket(child_bed);
        return;
    };
    // Vanilla parity: `giveBedToChild`, which hands the ticket the parents just
    // took straight to the newborn.
    if let Some(brain) = Mob::brain(child.as_ref()) {
        brain.set_memory(
            memory_module_types::HOME,
            GlobalPos::new(world.key.clone(), child_bed),
        );
    }
}

/// Vanilla parity: the private `VillagerMakeLove.takeVacantBed`, which claims a
/// bed's ticket before the child that will sleep in it exists.
fn take_vacant_bed(world: &Arc<World>, body: &VillagerEntity) -> Option<BlockPos> {
    let accepts = |poi_type_id: usize| {
        REGISTRY
            .poi_types
            .by_id(poi_type_id)
            .is_some_and(|poi_type| poi_type.key == vanilla_poi_types::HOME.key)
    };
    // Pathing cannot be done under the POI lock, so the candidates are collected
    // first and the ticket taken afterward.
    let candidates = {
        let storage = world.poi_storage.lock();
        storage.find_all_closest_first_with_type(
            &accepts,
            &|_| true,
            body.block_position(),
            BED_SEARCH_RANGE,
            OccupationStatus::Free,
        )
    };
    let reach = i32::try_from(vanilla_poi_types::HOME.search_distance).unwrap_or(1);
    let (bed, _) = candidates.into_iter().find(|&(pos, _)| {
        body.create_path_to(pos, reach)
            .is_some_and(|path| path.can_reach())
    })?;

    let mut storage = world.poi_storage.lock();
    storage.take(&accepts, &|_, candidate| candidate == bed, bed, 1)
}

/// Makes the child two villagers have just earned.
///
/// Vanilla parity: the private `VillagerMakeLove.breed` plus
/// `Villager.getBreedOffspring`, whose roll has three arms -- the variant of
/// the biome the parents are standing in below 0.5, this parent's below 0.75,
/// and the other parent's above. A village on a border therefore drifts toward
/// the ground it stands on rather than breeding true.
fn breed(
    world: &Arc<World>,
    body: &VillagerEntity,
    partner: &VillagerEntity,
) -> Option<Arc<VillagerEntity>> {
    let variant_roll = rand::random::<f64>();
    let child_type = if variant_roll < BIOME_VARIANT_CHANCE {
        body.biome_variant(world)
    } else if variant_roll < OWN_VARIANT_CHANCE {
        body.villager_type()
    } else {
        partner.villager_type()
    };

    // Constructed rather than pulled out of the entity factory, the way a cure
    // builds the villager it gives back: the factory is a spawn-time seam and
    // this is a birth, with the child's data written before it reaches a world.
    let child = Arc::new(VillagerEntity::new(
        &vanilla_entities::VILLAGER,
        next_entity_id(),
        body.position(),
        Arc::downgrade(world),
    ));
    child.set_villager_type(child_type);
    child.set_villager_data_finalized(true);

    AgeableMob::set_age(body, PARENT_BREEDING_COOLDOWN);
    AgeableMob::set_age(partner, PARENT_BREEDING_COOLDOWN);
    AgeableMob::set_age(child.as_ref(), CHILD_AGE);
    if let Err(error) = child.try_set_position(body.position()) {
        log::warn!("could not place a newborn villager beside its parent: {error}");
        return None;
    }
    child.set_rotation((0.0, 0.0));
    child.set_old_position_to_current();
    if let Err(error) = world.try_add_entity(Arc::clone(&child) as SharedEntity) {
        log::warn!("could not add a newborn villager to the world: {error}");
        return None;
    }
    child.broadcast_entity_event(EntityStatus::LoveHearts);
    Some(child)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use foton_registry::{
        init_vanilla_registry, vanilla_biomes, vanilla_entities, vanilla_villager_types,
    };
    use foton_utils::ChunkPos;
    use glam::DVec3;

    use super::breed;
    use crate::behavior::init_behaviors;
    use crate::block_entity::init_block_entities;
    use crate::entity::SharedEntity;
    use crate::entity::entities::VillagerEntity;
    use crate::entity::next_entity_id;
    use crate::test_support::{fresh_test_world, insert_ready_full_chunk_in_biome};

    /// Enough births that the biome arm coming up zero times is a one in a
    /// million million accident -- and, with the arm gone, a certainty.
    const BIRTHS: usize = 40;

    /// Vanilla rolls a newborn's variant three ways: half the time the biome
    /// under its parents, a quarter its mother's, a quarter its father's. Two
    /// plains parents standing in a desert can only produce a desert child
    /// through the first of those, so a run of births with no desert child in
    /// it says that arm is not there.
    #[test]
    fn a_villager_born_in_a_desert_is_sometimes_a_desert_villager() {
        init_vanilla_registry();
        init_behaviors();
        init_block_entities();
        let world = fresh_test_world("villager_breed_desert_variant");
        insert_ready_full_chunk_in_biome(&world, ChunkPos::new(0, 0), &vanilla_biomes::DESERT);

        let parent = |offset: f64| {
            let villager = Arc::new(VillagerEntity::new(
                &vanilla_entities::VILLAGER,
                next_entity_id(),
                DVec3::new(8.5 + offset, 64.0, 8.5),
                Arc::downgrade(&world),
            ));
            world
                .try_add_entity(Arc::clone(&villager) as SharedEntity)
                .expect("the test chunk is loaded, so the parent should attach");
            villager
        };
        let body = parent(0.0);
        let partner = parent(1.0);
        assert_eq!(
            body.villager_type().key,
            vanilla_villager_types::PLAINS.key,
            "both parents are plains, so a desert child can only come from the biome"
        );
        assert_eq!(
            partner.villager_type().key,
            vanilla_villager_types::PLAINS.key
        );

        let desert_children = (0..BIRTHS)
            .filter_map(|_| breed(&world, &body, &partner))
            .filter(|child| child.villager_type().key == vanilla_villager_types::DESERT.key)
            .count();

        assert!(
            desert_children > 0,
            "none of {BIRTHS} newborns took the desert it was born in"
        );
    }
}
