//! Vanilla `TurtleEggBlock` behavior.

use std::sync::Arc;

use steel_macros::block_behavior;
use steel_protocol::packets::game::SoundSource;
use steel_registry::blocks::BlockRef;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::blocks::properties::{BlockStateProperties, IntProperty};
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::item_stack::ItemStack;
use steel_registry::vanilla_block_tags::BlockTag;
use steel_registry::{
    REGISTRY, level_events, sound_events, vanilla_entities, vanilla_game_events, vanilla_game_rules,
};
use steel_utils::types::UpdateFlags;
use steel_utils::{BlockPos, BlockStateId};

use crate::behavior::block::{
    BlockBehavior, EntityFallDamage, EntityFallOnContext, default_can_be_replaced,
};
use crate::behavior::context::BlockPlaceContext;
use crate::block_entity::SharedBlockEntity;
use crate::entity::Entity;
use crate::player::Player;
use crate::world::game_event::GameEventContext;
use crate::world::{LevelReader, World};

/// Vanilla `TurtleEggBlock` behavior.
#[block_behavior]
pub struct TurtleEggBlock {
    block: BlockRef,
}

const HATCH: &IntProperty = &BlockStateProperties::HATCH;
const EGGS: &IntProperty = &BlockStateProperties::EGGS;

/// Vanilla `TurtleEggBlock.MAX_HATCH_LEVEL`.
const MAX_HATCH_LEVEL: u8 = 2;
/// Vanilla `TurtleEggBlock.MIN_EGGS`.
const MIN_EGGS: u8 = 1;
/// Vanilla `TurtleEggBlock.MAX_EGGS`.
const MAX_EGGS: u8 = 4;

/// The `randomness` vanilla `stepOn` passes to `destroyEgg`.
const STEP_ON_RANDOMNESS: i32 = 100;
/// The `randomness` vanilla `fallOn` passes to `destroyEgg`.
const FALL_ON_RANDOMNESS: i32 = 3;

/// The `data` of the `onPlace` sand-placement level event.
const PLACEMENT_PARTICLE_COUNT: i32 = 15;

impl TurtleEggBlock {
    /// Creates a turtle egg behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }

    /// Vanilla `TurtleEggBlock.isSand`.
    fn is_sand(world: &dyn LevelReader, pos: BlockPos) -> bool {
        world
            .get_block_state(pos)
            .get_block()
            .has_tag(&BlockTag::SAND)
    }

    /// Vanilla `TurtleEggBlock.onSand`.
    fn on_sand(world: &dyn LevelReader, pos: BlockPos) -> bool {
        Self::is_sand(world, pos.below())
    }

    /// Vanilla `TurtleEggBlock.canDestroyEgg`.
    ///
    /// Vanilla tests `entity instanceof Turtle`, `instanceof Bat`,
    /// `instanceof LivingEntity` and `instanceof Player`. Neither turtles nor
    /// bats have subclasses, so the two type comparisons are exact.
    #[must_use]
    fn can_destroy_egg(
        entity_type: EntityTypeRef,
        is_living_entity: bool,
        mob_griefing: bool,
    ) -> bool {
        if entity_type == &vanilla_entities::TURTLE || entity_type == &vanilla_entities::BAT {
            return false;
        }

        is_living_entity && (entity_type == &vanilla_entities::PLAYER || mob_griefing)
    }

    /// Whether vanilla `TurtleEggBlock.fallOn` exempts this entity from trampling.
    ///
    /// Vanilla writes `entity instanceof Zombie`. Steel has no entity class
    /// hierarchy to test, so the four vanilla subclasses of `Zombie` are listed
    /// beside it: `Husk`, `Drowned`, `ZombieVillager` and `ZombifiedPiglin`.
    /// A zoglin is not one of them despite the `minecraft:zombies` tag.
    #[must_use]
    fn is_zombie(entity_type: EntityTypeRef) -> bool {
        entity_type == &vanilla_entities::ZOMBIE
            || entity_type == &vanilla_entities::ZOMBIE_VILLAGER
            || entity_type == &vanilla_entities::HUSK
            || entity_type == &vanilla_entities::DROWNED
            || entity_type == &vanilla_entities::ZOMBIFIED_PIGLIN
    }

    /// Vanilla `TurtleEggBlock.destroyEgg`.
    fn destroy_egg(
        &self,
        world: &Arc<World>,
        state: BlockStateId,
        pos: BlockPos,
        entity_type: EntityTypeRef,
        is_living_entity: bool,
        randomness: i32,
    ) {
        // Vanilla's `state.is(Blocks.TURTLE_EGG)` guard: the state handed to the
        // hook can be stale by the time trampling runs.
        if state.get_block() != self.block {
            return;
        }

        let mob_griefing = world.get_game_rule(&vanilla_game_rules::MOB_GRIEFING);
        if !Self::can_destroy_egg(entity_type, is_living_entity, mob_griefing) {
            return;
        }

        if rand::random_range(0..randomness) != 0 {
            return;
        }

        Self::decrease_eggs(world, pos, state);
    }

    /// Vanilla `TurtleEggBlock.decreaseEggs`.
    fn decrease_eggs(world: &Arc<World>, pos: BlockPos, state: BlockStateId) {
        world.play_sound(
            &sound_events::ENTITY_TURTLE_EGG_BREAK,
            SoundSource::Blocks,
            pos,
            0.7,
            0.9 + rand::random::<f32>() * 0.2,
            None,
        );

        let eggs = state.get_value(EGGS);
        if eggs <= MIN_EGGS {
            world.destroy_block(pos, false);
            return;
        }

        world.set_block(
            pos,
            state.set_value(EGGS, eggs - 1),
            UpdateFlags::UPDATE_CLIENTS,
        );
        world.game_event(
            &vanilla_game_events::BLOCK_DESTROY,
            pos,
            &GameEventContext::new(None, Some(state)),
        );
        world.destroy_block_effect(pos, u32::from(state.0), None);
    }

    /// Vanilla `TurtleEggBlock.shouldUpdateHatchLevel`.
    ///
    /// Vanilla samples the `gameplay/turtle_egg_hatch_chance` environment
    /// attribute at the egg's position. Steel resolves environment attributes
    /// per dimension, not per block, so the position is not consulted; no
    /// vanilla source raises this attribute per position today.
    fn should_update_hatch_level(world: &Arc<World>) -> bool {
        let chance = world.turtle_egg_hatch_chance();
        chance > 0.0 && rand::random::<f32>() < chance
    }
}

impl BlockBehavior for TurtleEggBlock {
    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        let state = context.world.get_block_state(context.place_pos());
        if state.get_block() == self.block {
            return Some(state.set_value(EGGS, MAX_EGGS.min(state.get_value(EGGS) + 1)));
        }

        Some(self.block.default_state())
    }

    fn can_be_replaced(&self, state: BlockStateId, context: &BlockPlaceContext<'_>) -> bool {
        if !context.is_secondary_use_active()
            && context.with_item(|item| item.item() == REGISTRY.items.by_block(self.block))
            && state.get_value(EGGS) < MAX_EGGS
        {
            return true;
        }

        default_can_be_replaced(state, context)
    }

    fn on_place(
        &self,
        _state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        _old_state: BlockStateId,
        _moved_by_piston: bool,
    ) {
        if Self::on_sand(world.as_ref(), pos) {
            world.level_event(
                level_events::PARTICLES_TURTLE_EGG_PLACEMENT,
                pos,
                PLACEMENT_PARTICLE_COUNT,
                None,
            );
        }
    }

    fn step_on(&self, state: BlockStateId, world: &Arc<World>, pos: BlockPos, entity: &dyn Entity) {
        if !entity.is_stepping_carefully() {
            self.destroy_egg(
                world,
                state,
                pos,
                entity.entity_type(),
                entity.is_living_entity(),
                STEP_ON_RANDOMNESS,
            );
        }

        self.default_step_on(state, world, pos, entity);
    }

    fn fall_on(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        context: EntityFallOnContext<'_>,
    ) -> Option<EntityFallDamage> {
        if !Self::is_zombie(context.entity.entity_type) {
            self.destroy_egg(
                world,
                state,
                pos,
                context.entity.entity_type,
                context.entity.is_living_entity,
                FALL_ON_RANDOMNESS,
            );
        }

        self.default_fall_on(state, world, pos, context)
    }

    fn player_destroy(
        &self,
        world: &Arc<World>,
        _player: &Player,
        pos: BlockPos,
        state: BlockStateId,
        _block_entity: Option<&SharedBlockEntity>,
        _tool: &ItemStack,
    ) {
        // Vanilla's `super.playerDestroy` only awards stats and food exhaustion,
        // which the caller already did; the egg-specific half is `decreaseEggs`.
        // The block is already air here, so a single-egg cluster stays gone and
        // a larger one is put back with one egg fewer.
        Self::decrease_eggs(world, pos, state);
    }

    fn random_tick(&self, state: BlockStateId, world: &Arc<World>, pos: BlockPos) {
        if !Self::should_update_hatch_level(world) || !Self::on_sand(world.as_ref(), pos) {
            return;
        }

        let hatch = state.get_value(HATCH);
        if hatch < MAX_HATCH_LEVEL {
            world.play_sound(
                &sound_events::ENTITY_TURTLE_EGG_CRACK,
                SoundSource::Blocks,
                pos,
                0.7,
                0.9 + rand::random::<f32>() * 0.2,
                None,
            );
            world.set_block(
                pos,
                state.set_value(HATCH, hatch + 1),
                UpdateFlags::UPDATE_CLIENTS,
            );
            world.game_event(
                &vanilla_game_events::BLOCK_CHANGE,
                pos,
                &GameEventContext::new(None, Some(state)),
            );
            return;
        }

        world.play_sound(
            &sound_events::ENTITY_TURTLE_EGG_HATCH,
            SoundSource::Blocks,
            pos,
            0.7,
            0.9 + rand::random::<f32>() * 0.2,
            None,
        );
        world.remove_block(pos, false);
        world.game_event(
            &vanilla_game_events::BLOCK_DESTROY,
            pos,
            &GameEventContext::new(None, Some(state)),
        );

        for _ in 0..state.get_value(EGGS) {
            world.destroy_block_effect(pos, u32::from(state.0), None);
            // Vanilla spawns one baby `Turtle` per egg here, aged -24000, with
            // its home position set to this block. Steel has no `Turtle`
            // entity, so the eggs vanish without hatching anything; the spawn
            // belongs on this line once `EntityTypes.TURTLE` is implemented.
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use glam::DVec3;
    use steel_registry::{init_vanilla_registry, vanilla_blocks};
    use steel_utils::ChunkPos;

    use super::*;
    use crate::behavior::init_behaviors;
    use crate::entity::EntityBase;
    use crate::test_support::{fresh_test_world, insert_ready_full_chunk};

    struct StepEntity {
        base: EntityBase,
        entity_type: EntityTypeRef,
        careful: bool,
    }

    crate::entity::impl_test_downcast_type!(StepEntity);

    impl StepEntity {
        fn new(id: i32, world: &Arc<World>, entity_type: EntityTypeRef, careful: bool) -> Self {
            Self {
                base: EntityBase::new(
                    id,
                    DVec3::new(8.5, 65.0, 8.5),
                    entity_type.dimensions,
                    Arc::downgrade(world),
                ),
                entity_type,
                careful,
            }
        }
    }

    impl Entity for StepEntity {
        fn base(&self) -> &EntityBase {
            &self.base
        }

        fn entity_type(&self) -> EntityTypeRef {
            self.entity_type
        }

        fn is_living_entity(&self) -> bool {
            true
        }

        fn is_stepping_carefully(&self) -> bool {
            self.careful
        }
    }

    #[test]
    fn turtles_and_bats_never_crack_an_egg_they_walk_over() {
        init_vanilla_registry();

        assert!(!TurtleEggBlock::can_destroy_egg(
            &vanilla_entities::TURTLE,
            true,
            true
        ));
        assert!(!TurtleEggBlock::can_destroy_egg(
            &vanilla_entities::BAT,
            true,
            true
        ));
    }

    #[test]
    fn only_players_crack_eggs_when_mob_griefing_is_off() {
        init_vanilla_registry();

        assert!(TurtleEggBlock::can_destroy_egg(
            &vanilla_entities::PLAYER,
            true,
            false
        ));
        assert!(!TurtleEggBlock::can_destroy_egg(
            &vanilla_entities::ZOMBIE,
            true,
            false
        ));
        assert!(TurtleEggBlock::can_destroy_egg(
            &vanilla_entities::ZOMBIE,
            true,
            true
        ));
    }

    #[test]
    fn non_living_entities_never_crack_eggs() {
        init_vanilla_registry();

        assert!(!TurtleEggBlock::can_destroy_egg(
            &vanilla_entities::FALLING_BLOCK,
            false,
            true
        ));
    }

    #[test]
    fn every_zombie_kind_lands_on_eggs_without_breaking_them() {
        init_vanilla_registry();

        for entity_type in [
            &vanilla_entities::ZOMBIE,
            &vanilla_entities::ZOMBIE_VILLAGER,
            &vanilla_entities::HUSK,
            &vanilla_entities::DROWNED,
            &vanilla_entities::ZOMBIFIED_PIGLIN,
        ] {
            assert!(TurtleEggBlock::is_zombie(entity_type));
        }

        assert!(!TurtleEggBlock::is_zombie(&vanilla_entities::SKELETON));
        assert!(!TurtleEggBlock::is_zombie(&vanilla_entities::ZOGLIN));
    }

    #[test]
    fn trampling_a_cluster_takes_one_egg_at_a_time_and_then_the_block() {
        init_vanilla_registry();
        init_behaviors();
        let world = fresh_test_world("turtle_egg_trample");
        let pos = BlockPos::new(8, 64, 8);
        insert_ready_full_chunk(&world, ChunkPos::from_block_pos(pos));

        let three_eggs = vanilla_blocks::TURTLE_EGG
            .default_state()
            .set_value(EGGS, 3);
        assert!(world.set_block(pos, three_eggs, UpdateFlags::UPDATE_NONE));

        TurtleEggBlock::decrease_eggs(&world, pos, three_eggs);
        let two_eggs = world.get_block_state(pos);
        assert_eq!(two_eggs.get_block(), &vanilla_blocks::TURTLE_EGG);
        assert_eq!(two_eggs.get_value(EGGS), 2);

        TurtleEggBlock::decrease_eggs(&world, pos, two_eggs);
        let one_egg = world.get_block_state(pos);
        assert_eq!(one_egg.get_value(EGGS), MIN_EGGS);

        TurtleEggBlock::decrease_eggs(&world, pos, one_egg);
        assert!(world.get_block_state(pos).is_air());
    }

    #[test]
    fn a_careful_step_leaves_the_eggs_alone() {
        init_vanilla_registry();
        init_behaviors();
        let world = fresh_test_world("turtle_egg_careful_step");
        let pos = BlockPos::new(8, 64, 8);
        insert_ready_full_chunk(&world, ChunkPos::from_block_pos(pos));

        let behavior = TurtleEggBlock::new(&vanilla_blocks::TURTLE_EGG);
        let state = vanilla_blocks::TURTLE_EGG.default_state();
        assert!(world.set_block(pos, state, UpdateFlags::UPDATE_NONE));

        let sneaking = StepEntity::new(9_101, &world, &vanilla_entities::PLAYER, true);
        // Vanilla rolls `nextInt(100)`, so one careful step is not proof on its
        // own; a hundred of them would crack an unprotected egg many times over.
        for _ in 0..1_000 {
            behavior.step_on(state, &world, pos, &sneaking);
        }

        assert_eq!(
            world.get_block_state(pos).get_block(),
            &vanilla_blocks::TURTLE_EGG
        );
    }

    #[test]
    fn hatching_only_advances_while_the_eggs_sit_on_sand() {
        init_vanilla_registry();
        init_behaviors();
        let world = fresh_test_world("turtle_egg_sand");
        let pos = BlockPos::new(8, 64, 8);
        insert_ready_full_chunk(&world, ChunkPos::from_block_pos(pos));

        assert!(world.set_block(
            pos.below(),
            vanilla_blocks::STONE.default_state(),
            UpdateFlags::UPDATE_NONE
        ));
        assert!(!TurtleEggBlock::on_sand(world.as_ref(), pos));

        assert!(world.set_block(
            pos.below(),
            vanilla_blocks::RED_SAND.default_state(),
            UpdateFlags::UPDATE_NONE
        ));
        assert!(TurtleEggBlock::on_sand(world.as_ref(), pos));
    }
}
