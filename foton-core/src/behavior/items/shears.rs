//! Vanilla `ShearsItem`.
//!
//! Only the two rules that live on the item class are here. The shearing this
//! item is named for belongs to whatever is being sheared and is already
//! implemented there: sheep and mooshrooms in `entity/entities/mobs/passive/`,
//! pumpkin carving in `blocks/vegetation/pumpkin_block.rs`, beehive harvesting
//! in `blocks/container/beehive_block.rs`, tripwire disarming in
//! `blocks/redstone/tripwire/`, and the dispenser variants in
//! `blocks/container/dispense_behavior.rs`. Mining speed comes from the item's
//! `minecraft:tool` component, so vanilla's `createToolProperties` needs no port.

use foton_macros::item_behavior;
use foton_registry::blocks::block_state_ext::BlockStateExt;
use foton_registry::item_stack::ItemStack;
use foton_registry::vanilla_block_tags::BlockTag;
use foton_registry::{sound_events, vanilla_game_events};
use foton_utils::BlockStateId;
use foton_utils::types::UpdateFlags;

use crate::behavior::{BLOCK_BEHAVIORS, InteractionResult, ItemBehavior, UseOnContext};
use crate::entity::{Entity, LivingEntity};
use crate::world::game_event::GameEventContext;

/// Behavior for the shears item.
#[item_behavior]
pub struct ShearsItem;

impl ItemBehavior for ShearsItem {
    fn use_on(&self, context: &mut UseOnContext) -> InteractionResult {
        let pos = context.hit_result.block_pos;
        let state = context.world.get_block_state(pos);
        let Some(plant_head) = BLOCK_BEHAVIORS
            .get_behavior(state.get_block())
            .as_growing_plant_head()
        else {
            return InteractionResult::Pass;
        };
        if plant_head.is_max_age(state) {
            return InteractionResult::Pass;
        }

        // TODO: Trigger CriteriaTriggers.ITEM_USED_ON_BLOCK once Foton has
        // advancement criteria.

        // Vanilla `level.playSound(player, ..)` excludes the acting player, who
        // already played the sound locally.
        context.world.play_block_sound(
            &sound_events::BLOCK_GROWING_PLANT_CROP,
            pos,
            1.0,
            1.0,
            Some(context.player.id()),
        );

        // Capping the head at its maximum age is what stops the vine growing.
        let new_state = plant_head.get_max_age_state(state);
        context
            .world
            .set_block(pos, new_state, UpdateFlags::UPDATE_ALL);
        context.world.game_event(
            &vanilla_game_events::BLOCK_CHANGE,
            pos,
            &GameEventContext::new(Some(context.player), Some(new_state)),
        );

        let has_infinite_materials = context.player.has_infinite_materials();
        context
            .inv
            .with_item(|item| item.hurt_and_break(1, has_infinite_materials));

        InteractionResult::Success
    }

    /// Vanilla `ShearsItem.mineBlock`.
    ///
    /// Two departures from `Item.mineBlock`: shears spend durability even on a
    /// block that breaks instantly -- grass, vines, wool are all zero hardness --
    /// and they never spend it on fire, which they put out for free.
    fn mine_block(
        &self,
        stack: &mut ItemStack,
        state: BlockStateId,
        miner: &dyn LivingEntity,
    ) -> bool {
        let Some(damage_per_block) = stack.get_tool().map(|tool| tool.damage_per_block) else {
            return false;
        };

        if !state.get_block().has_tag(&BlockTag::FIRE) && damage_per_block > 0 {
            stack.hurt_and_break(damage_per_block, miner.has_infinite_materials());
        }

        true
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use foton_registry::blocks::properties::{BlockStateProperties, Direction, IntProperty};
    use foton_registry::{vanilla_blocks, vanilla_items};
    use foton_utils::types::InteractionHand;
    use foton_utils::{BlockPos, ChunkPos};
    use glam::DVec3;

    use super::*;
    use crate::behavior::BlockHitResult;
    use crate::behavior::blocks::vegetation::growing_plant_head_block::MAX_AGE;
    use crate::bootstrap::init_globals_once;
    use crate::player::Player;
    use crate::test_support::{TestPlayerBuilder, fresh_test_world, insert_ready_full_chunk};
    use crate::world::World;

    const AGE: &IntProperty = &BlockStateProperties::AGE_25;
    const VINE_POS: BlockPos = BlockPos::new(8, 64, 8);

    /// Twisting vines grow upward, so the tip needs a solid block underneath or
    /// `can_survive` schedules it for removal.
    fn world_with_twisting_vine(key: &'static str, age: u8) -> Arc<World> {
        let world = fresh_test_world(key);
        insert_ready_full_chunk(&world, ChunkPos::from_block_pos(VINE_POS));
        world.set_block(
            VINE_POS.below(),
            vanilla_blocks::NETHERRACK.default_state(),
            UpdateFlags::UPDATE_NONE,
        );
        world.set_block(
            VINE_POS,
            vanilla_blocks::TWISTING_VINES
                .default_state()
                .set_value(AGE, age),
            UpdateFlags::UPDATE_NONE,
        );
        world
    }

    fn vine_hit() -> BlockHitResult {
        BlockHitResult {
            location: DVec3::new(8.5, 65.0, 8.5),
            direction: Direction::Up,
            block_pos: VINE_POS,
            miss: false,
            inside: false,
            world_border_hit: false,
        }
    }

    fn shear_the_vine(world: &Arc<World>, player: &Player) -> InteractionResult {
        let mut context = UseOnContext::new(
            player,
            InteractionHand::MainHand,
            vine_hit(),
            world,
            player.inventory.clone(),
        );
        ShearsItem.use_on(&mut context)
    }

    fn player_holding_shears(world: &Arc<World>, name: &'static str) -> Arc<Player> {
        let player = TestPlayerBuilder::new(Arc::clone(world), name, 1).build();
        player
            .inventory
            .lock()
            .set_selected_item(ItemStack::new(&vanilla_items::SHEARS));
        player
    }

    #[test]
    fn shearing_a_growing_vine_tip_stops_it_growing_and_costs_a_durability_point() {
        init_globals_once();
        let world = world_with_twisting_vine("shears_cap_growing_vine", 0);
        let player = player_holding_shears(&world, "VineShearer");

        assert_eq!(shear_the_vine(&world, &player), InteractionResult::Success);
        assert_eq!(world.get_block_state(VINE_POS).get_value(AGE), MAX_AGE);
        assert_eq!(
            player
                .inventory
                .lock()
                .get_selected_item()
                .get_damage_value(),
            1
        );
    }

    #[test]
    fn shearing_a_vine_tip_that_already_stopped_growing_does_nothing() {
        init_globals_once();
        let world = world_with_twisting_vine("shears_capped_vine_is_left_alone", MAX_AGE);
        let player = player_holding_shears(&world, "VineShearer");

        assert_eq!(shear_the_vine(&world, &player), InteractionResult::Pass);
        assert_eq!(
            player
                .inventory
                .lock()
                .get_selected_item()
                .get_damage_value(),
            0
        );
    }

    #[test]
    fn shears_pay_durability_for_a_zero_hardness_plant_but_never_for_fire() {
        init_globals_once();
        let world = fresh_test_world("shears_mine_block_durability");
        let player = TestPlayerBuilder::new(world, "GrassShearer", 1).build();

        let mut cutting_grass = ItemStack::new(&vanilla_items::SHEARS);
        assert!(ShearsItem.mine_block(
            &mut cutting_grass,
            vanilla_blocks::SHORT_GRASS.default_state(),
            player.as_ref(),
        ));
        assert_eq!(
            cutting_grass.get_damage_value(),
            1,
            "grass has zero hardness, which the generic Item.mineBlock rule would treat as free"
        );

        let mut putting_out_fire = ItemStack::new(&vanilla_items::SHEARS);
        assert!(ShearsItem.mine_block(
            &mut putting_out_fire,
            vanilla_blocks::FIRE.default_state(),
            player.as_ref(),
        ));
        assert_eq!(putting_out_fire.get_damage_value(), 0);
    }
}
