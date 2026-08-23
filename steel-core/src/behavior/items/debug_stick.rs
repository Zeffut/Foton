//! Debug stick: selects and cycles one block-state property.

use std::sync::Arc;

use steel_macros::item_behavior;
use steel_registry::REGISTRY;
use steel_registry::blocks::block_state_ext::BlockStateExt;
use steel_registry::data_components::vanilla_components::DEBUG_STICK_STATE;
use steel_registry::item_stack::ItemStack;
use steel_utils::types::UpdateFlags;
use steel_utils::{BlockPos, BlockStateId};
use text_components::TextComponent;
use text_components::translation::TranslatedMessage;

use crate::behavior::{InteractionResult, ItemBehavior, UseOnContext};
use crate::entity::LivingEntity;
use crate::player::Player;
use crate::world::World;

/// Vanilla parity: the `18` passed to `level.setBlock` in
/// `DebugStickItem.handleInteraction` -- tell the clients, keep the shapes.
const DEBUG_STICK_UPDATE_FLAGS: UpdateFlags =
    UpdateFlags::UPDATE_CLIENTS.union(UpdateFlags::UPDATE_KNOWN_SHAPE);

/// The creative-only block-state editor.
#[item_behavior]
pub struct DebugStickItem;

impl ItemBehavior for DebugStickItem {
    /// Vanilla parity: `DebugStickItem.canDestroyBlock` -- a left click selects
    /// the next property instead of breaking anything.
    fn can_destroy_block(
        &self,
        stack: &mut ItemStack,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        user: &dyn LivingEntity,
    ) -> bool {
        if let Some(player) = user.as_player() {
            handle_interaction(player, state, world, pos, false, stack);
        }
        false
    }

    /// Vanilla parity: `DebugStickItem.useOn` -- a right click cycles the
    /// selected property's value.
    fn use_on(&self, context: &mut UseOnContext) -> InteractionResult {
        let pos = context.hit_result.block_pos;
        let state = context.world.get_block_state(pos);
        let player = context.player;
        let world = context.world;

        let handled = context
            .inv
            .with_item(|stack| handle_interaction(player, state, world, pos, true, stack));
        if handled {
            InteractionResult::Success
        } else {
            InteractionResult::Fail
        }
    }
}

/// Vanilla parity: `DebugStickItem.handleInteraction`.
fn handle_interaction(
    player: &Player,
    state: BlockStateId,
    world: &Arc<World>,
    pos: BlockPos,
    cycle: bool,
    stack: &mut ItemStack,
) -> bool {
    if !player.can_use_game_master_blocks() {
        return false;
    }

    let block = state.get_block();
    let property_names: Vec<&'static str> = block
        .properties
        .iter()
        .map(|property| property.get_name())
        .collect();
    if property_names.is_empty() {
        message(
            player,
            "item.minecraft.debug_stick.empty",
            vec![TextComponent::plain(block.key.to_string())],
        );
        return false;
    }

    let Some(debug_stick_state) = stack.get(DEBUG_STICK_STATE).cloned() else {
        return false;
    };
    let selected = debug_stick_state.get(block);

    if cycle {
        let property = selected.unwrap_or(property_names[0]);
        let Some(new_state) = cycle_state(state, property, player.is_secondary_use_active()) else {
            return false;
        };
        world.set_block(pos, new_state, DEBUG_STICK_UPDATE_FLAGS);
        message(
            player,
            "item.minecraft.debug_stick.update",
            vec![
                TextComponent::plain(property),
                TextComponent::plain(value_name(new_state, property).unwrap_or_default()),
            ],
        );
        return true;
    }

    let property = relative(&property_names, selected, player.is_secondary_use_active());
    match debug_stick_state.with_property(block, property) {
        Ok(updated) => stack.set(DEBUG_STICK_STATE, updated),
        Err(error) => {
            log::error!("Could not select debug stick property {property}: {error}");
            return false;
        }
    }
    message(
        player,
        "item.minecraft.debug_stick.select",
        vec![
            TextComponent::plain(property),
            TextComponent::plain(value_name(state, property).unwrap_or_default()),
        ],
    );
    true
}

/// Vanilla parity: `DebugStickItem.cycleState`.
///
/// Steel deviation: Vanilla holds a typed `Property<T>` and calls
/// `state.setValue`. Steel's properties are only object-safe through their
/// serialized names, so the cycle is done over those and the state rebuilt from
/// the full name/value list -- the same state, reached by name.
fn cycle_state(state: BlockStateId, property: &str, backward: bool) -> Option<BlockStateId> {
    let block = state.get_block();
    let values = block
        .properties
        .iter()
        .find(|candidate| candidate.get_name() == property)?
        .get_possible_value_names();
    let current = value_name(state, property)?;
    let next = relative(&values, Some(current), backward);

    let properties: Vec<(&str, &str)> = REGISTRY
        .blocks
        .get_properties(state)
        .into_iter()
        .map(|(name, value)| {
            if name == property {
                (name, next)
            } else {
                (name, value)
            }
        })
        .collect();
    REGISTRY
        .blocks
        .state_id_from_block_properties(block, properties)
}

/// Vanilla parity: `DebugStickItem.getNameHelper`, i.e.
/// `property.getName(state.getValue(property))`.
fn value_name(state: BlockStateId, property: &str) -> Option<&'static str> {
    REGISTRY
        .blocks
        .get_properties(state)
        .into_iter()
        .find(|(name, _)| *name == property)
        .map(|(_, value)| value)
}

/// Vanilla parity: `DebugStickItem.getRelative`, which is
/// `Util.findNextInIterable` / `Util.findPreviousInIterable`.
///
/// Both wrap around; with no current value the forward search yields the first
/// entry and the backward one the last.
fn relative<'a>(values: &[&'a str], current: Option<&str>, backward: bool) -> &'a str {
    let position = current.and_then(|current| values.iter().position(|value| *value == current));
    let Some(position) = position else {
        return if backward {
            values[values.len() - 1]
        } else {
            values[0]
        };
    };

    if backward {
        values[(position + values.len() - 1) % values.len()]
    } else {
        values[(position + 1) % values.len()]
    }
}

fn message(player: &Player, key: &'static str, args: Vec<TextComponent>) {
    player.send_overlay_message(&TextComponent::translated(TranslatedMessage {
        key: key.into(),
        fallback: None,
        args: Some(args.into_boxed_slice()),
    }));
}

#[cfg(test)]
mod tests {
    use super::relative;

    #[test]
    fn cycling_forward_wraps_past_the_last_value() {
        let values = ["false", "true"];
        assert_eq!(relative(&values, Some("false"), false), "true");
        assert_eq!(relative(&values, Some("true"), false), "false");
    }

    #[test]
    fn cycling_backward_wraps_past_the_first_value() {
        let values = ["0", "1", "2"];
        assert_eq!(relative(&values, Some("0"), true), "2");
        assert_eq!(relative(&values, Some("2"), true), "1");
    }

    #[test]
    fn no_current_value_starts_at_the_end_the_search_comes_from() {
        let values = ["north", "south", "west"];
        assert_eq!(relative(&values, None, false), "north");
        assert_eq!(relative(&values, None, true), "west");
    }

    #[test]
    fn a_value_that_is_not_in_the_list_restarts_the_search() {
        let values = ["north", "south"];
        assert_eq!(relative(&values, Some("east"), false), "north");
        assert_eq!(relative(&values, Some("east"), true), "south");
    }
}
