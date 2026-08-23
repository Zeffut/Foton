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
use steel_registry::data_components::vanilla_components::ENTITY_DATA;
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::item_stack::ItemStack;
use steel_registry::vanilla_game_events;
use steel_utils::BlockPos;
use steel_utils::types::Difficulty;

use crate::behavior::{InteractionResult, ItemBehavior, UseOnContext};
use crate::entity::{ENTITIES, EntitySpawnReason, next_entity_id};
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
}

impl ItemBehavior for SpawnEggItem {
    /// Vanilla parity: `SpawnEggItem.useOn`.
    ///
    /// TODO: vanilla also retargets a spawner block entity when one is clicked,
    /// setting the mob it spawns. Steel has no spawner block behavior yet, so
    /// clicking one falls through to placing the mob beside it.
    fn use_on(&self, context: &mut UseOnContext) -> InteractionResult {
        let Some(entity_type) = context.inv.with_item(|item| Self::entity_type(item)) else {
            return InteractionResult::Pass;
        };

        // Vanilla parity: the egg lands in the clicked block when nothing is
        // there to stand on -- tall grass, a fluid -- and against the clicked
        // face otherwise.
        let clicked = context.hit_result.block_pos;
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
