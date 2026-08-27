//! Spawn eggs.
//!
//! Vanilla parity: `SpawnEggItem`. One class, eighty-eight items: every one of
//! them carries the entity it makes in its `ENTITY_DATA` component, which the
//! extracted item registry already fills in, so nothing here is a table of
//! names.

use std::sync::Arc;

use steel_macros::item_behavior;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::data_components::components::EntityData;
use steel_registry::data_components::vanilla_components::{CUSTOM_NAME, ENTITY_DATA};
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::item_stack::ItemStack;
use steel_registry::vanilla_game_events;
use steel_utils::BlockPos;
use steel_utils::types::Difficulty;

use crate::behavior::{InteractionResult, ItemBehavior, UseOnContext};
use crate::entity::{ENTITIES, EntitySpawnReason, Mob, SharedEntity, next_entity_id};
use crate::player::Player;
use crate::world::game_event::GameEventContext;
use crate::world::{LevelReader as _, World};

/// Behavior for every spawn egg.
#[item_behavior]
pub struct SpawnEggItem;

impl SpawnEggItem {
    /// Returns the entity a spawn egg makes.
    ///
    /// Vanilla parity: `SpawnEggItem.getType`, which reads the same component.
    #[must_use]
    pub fn entity_type(stack: &ItemStack) -> Option<EntityTypeRef> {
        stack.get(ENTITY_DATA).map(EntityData::entity_type)
    }

    /// Breeds a baby out of `parent` when `stack` is that mob's own spawn egg.
    ///
    /// Vanilla parity: `SpawnEggItem.spawnOffspringFromSpawnEgg`. The egg has to
    /// match the mob it is used on -- a chicken egg on a cow does nothing -- and
    /// the baby comes from the parent's own breeding path when it has one, so a
    /// mooshroom egg on a brown mooshroom gives a brown calf rather than a
    /// default-variant one.
    ///
    /// Returns the baby, or `None` when nothing was spawned (and the egg is
    /// then left alone, as vanilla's empty `Optional` is).
    pub fn spawn_offspring_from_spawn_egg(
        player: &Player,
        parent: &dyn Mob,
        world: &Arc<World>,
        stack: &mut ItemStack,
    ) -> Option<SharedEntity> {
        // Vanilla parity: `SpawnEggItem.spawnsEntity`.
        if Self::entity_type(stack)?.key != parent.entity_type().key {
            return None;
        }

        // Vanilla asks an `AgeableMob` for its `getBreedOffspring`; Steel hangs
        // that on `Animal`, which is where every ageable mob that has one lives.
        let offspring = match parent.as_animal() {
            Some(animal) => animal.get_breed_offspring(world, animal)?,
            None => ENTITIES.create(
                parent.entity_type(),
                next_entity_id(),
                parent.position(),
                Arc::downgrade(world),
            )?,
        };

        {
            let baby = offspring.as_mob()?;
            baby.set_baby(true);
            // Vanilla bails when the mob refused to be a baby, which is how an
            // egg for something that has no baby form spawns nothing at all.
            if !baby.is_baby() {
                return None;
            }
            baby.try_set_position(parent.position()).ok()?;
            baby.set_rotation((0.0, 0.0));
            baby.set_old_position_to_current();

            // Vanilla parity: `Entity.applyComponentsFromItemStack`, which for an
            // entity means `CUSTOM_NAME` and `CUSTOM_DATA`. Steel entities have no
            // `CUSTOM_DATA`, so a named egg naming its baby is the whole of it.
            if let Some(name) = stack.get(CUSTOM_NAME) {
                baby.set_custom_name(Some(name.clone()));
            }
        }

        world.try_add_entity(Arc::clone(&offspring)).ok()?;

        // Vanilla parity: `ItemStack.consume`, which spares a creative player.
        if !player.has_infinite_materials() {
            stack.shrink(1);
        }

        Some(offspring)
    }
}

impl ItemBehavior for SpawnEggItem {
    fn is_spawn_egg(&self) -> bool {
        true
    }

    /// Vanilla parity: `SpawnEggItem.useOn`.
    fn use_on(&self, context: &mut UseOnContext) -> InteractionResult {
        let Some(entity_type) = context.inv.with_item(|item| Self::entity_type(item)) else {
            return InteractionResult::Pass;
        };

        // Vanilla parity: clicking a spawner retargets it instead of placing
        // the mob, and refuses when the game rule has spawners switched off.
        let clicked_pos = context.hit_result.block_pos;
        if let Some(block_entity) = context.world.get_block_entity(clicked_pos)
            && let Some(spawner) = block_entity.as_spawner()
        {
            if !context.world.is_spawner_block_enabled() {
                return InteractionResult::Fail;
            }
            spawner.set_spawner_entity_id(entity_type);
            context.world.send_block_updated(clicked_pos);
            context.world.game_event(
                &vanilla_game_events::BLOCK_CHANGE,
                clicked_pos,
                &GameEventContext::new(Some(context.player), None),
            );
            context.inv.with_item(|item| item.shrink(1));
            return InteractionResult::Success;
        }

        // Vanilla parity: the egg lands in the clicked block when nothing is
        // there to stand on -- tall grass, a fluid -- and against the clicked
        // face otherwise.
        let clicked = clicked_pos;
        let spawn_pos = if context
            .world
            .get_block_state(clicked)
            .get_collision_shape_at(clicked)
            .is_empty()
        {
            clicked
        } else {
            clicked.relative(context.hit_result.direction)
        };

        if spawn_mob(context.world, entity_type, spawn_pos).is_none() {
            return InteractionResult::Fail;
        }

        context.world.game_event(
            &vanilla_game_events::ENTITY_PLACE,
            spawn_pos,
            &GameEventContext::new(Some(context.player), None),
        );

        if !context.player.has_infinite_materials() {
            context.inv.with_item(|item| item.shrink(1));
        }

        InteractionResult::Success
    }
}

/// Puts one mob of `entity_type` in the world.
///
/// Vanilla parity: the `EntityType.spawn` of `SpawnEggItem.spawnMob`.
fn spawn_mob(world: &Arc<World>, entity_type: EntityTypeRef, pos: BlockPos) -> Option<()> {
    if !World::is_in_spawnable_bounds(pos) {
        return None;
    }
    if world.difficulty() == Difficulty::Peaceful && !entity_type.allowed_in_peaceful {
        return None;
    }

    let (x, y, z) = pos.get_bottom_center();
    let entity = ENTITIES.create(
        entity_type,
        next_entity_id(),
        glam::DVec3::new(x, y, z),
        Arc::downgrade(world),
    )?;

    if let Some(mob) = entity.as_mob() {
        let _ = mob.finalize_spawn(world, EntitySpawnReason::SpawnItemUse, None);
    }

    world.try_add_entity(entity).ok()
}

#[cfg(test)]
mod tests {
    use steel_registry::{init_vanilla_registry, vanilla_entities, vanilla_items};

    use super::*;

    /// Every spawn egg says what it makes, and says the right thing.
    ///
    /// The mapping is extracted data rather than a table here, so this is the
    /// check that the component is actually populated -- an egg with no
    /// `ENTITY_DATA` would silently do nothing when used.
    #[test]
    fn spawn_eggs_carry_the_entity_they_make() {
        init_vanilla_registry();

        for (item, expected) in [
            (
                &vanilla_items::CHICKEN_SPAWN_EGG,
                &vanilla_entities::CHICKEN,
            ),
            (
                &vanilla_items::CREEPER_SPAWN_EGG,
                &vanilla_entities::CREEPER,
            ),
            (
                &vanilla_items::STRIDER_SPAWN_EGG,
                &vanilla_entities::STRIDER,
            ),
            (
                &vanilla_items::MAGMA_CUBE_SPAWN_EGG,
                &vanilla_entities::MAGMA_CUBE,
            ),
        ] {
            let stack = ItemStack::new(item);
            let entity_type = SpawnEggItem::entity_type(&stack)
                .unwrap_or_else(|| panic!("{} carries no entity data", item.key));
            assert_eq!(
                entity_type.key, expected.key,
                "{} makes the wrong entity",
                item.key
            );
        }
    }

    /// Something that is not a spawn egg makes nothing.
    #[test]
    fn an_ordinary_item_makes_no_entity() {
        init_vanilla_registry();
        let stack = ItemStack::new(&vanilla_items::STONE);
        assert!(SpawnEggItem::entity_type(&stack).is_none());
    }
}
