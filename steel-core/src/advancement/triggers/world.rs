//! Triggers that fire from where a player is and what the world did to them.

use glam::DVec3;
use steel_registry::advancement::TriggerInstance;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::item_stack::ItemStack;
use steel_utils::{BlockPos, BlockStateId, Identifier};

use super::fire;
use crate::advancement::predicate::{PredicateContext, Subject, state_properties_match};
use crate::player::Player;

/// Fired once per player tick.
///
/// Vanilla parity: `CriteriaTriggers.TICK`, invoked from `ServerPlayer.tick`.
/// `PlayerTrigger` carries nothing but its `player` predicate, so that
/// predicate is the whole test -- which is how `nether/all_effects` and the
/// other "be in this state right now" advancements are written.
pub fn tick(player: &Player) {
    fire(player, "minecraft:tick", |_| true);
}

/// Fired once every twenty player ticks.
///
/// Vanilla parity: `CriteriaTriggers.LOCATION`, invoked from
/// `ServerPlayer.doTick` under `this.tickCount % 20 == 0`. It shares
/// `PlayerTrigger` with [`tick`]; the twenty-tick spacing is the only
/// difference, and it is what keeps the biome and structure checks off the
/// per-tick path.
pub fn location(player: &Player) {
    fire(player, "minecraft:location", |_| true);
}

/// Vanilla parity: `CriteriaTriggers.ENTER_BLOCK`, fired from the
/// `onInsideBlock` of every block an entity is standing in.
pub fn enter_block(player: &Player, state: BlockStateId) {
    block_state_trigger(player, "minecraft:enter_block", state);
}

/// Vanilla parity: `CriteriaTriggers.HONEY_BLOCK_SLIDE`, whose registry name is
/// `minecraft:slide_down_block`.
pub fn slide_down_block(player: &Player, state: BlockStateId) {
    block_state_trigger(player, "minecraft:slide_down_block", state);
}

/// Vanilla parity: `EnterBlockTrigger.TriggerInstance.matches`. A named block
/// that does not match fails; an absent state predicate accepts every state.
fn block_state_trigger(player: &Player, trigger_id: &'static str, state: BlockStateId) {
    fire(player, trigger_id, |instance| {
        let (wanted_block, wanted_state) = match instance {
            TriggerInstance::EnterBlock { block, state, .. }
            | TriggerInstance::SlideDownBlock { block, state, .. } => (block, *state),
            _ => return false,
        };
        if let Some(wanted_block) = wanted_block
            && state.get_block().key != *wanted_block
        {
            return false;
        }
        state_properties_match(wanted_state, state)
    });
}

/// Vanilla parity: `CriteriaTriggers.PLACED_BLOCK`.
pub fn placed_block(player: &Player, pos: BlockPos, state: BlockStateId, tool: &ItemStack) {
    item_used_on_location(player, "minecraft:placed_block", pos, state, tool);
}

/// Vanilla parity: `CriteriaTriggers.ITEM_USED_ON_BLOCK`.
pub fn item_used_on_block(player: &Player, pos: BlockPos, state: BlockStateId, tool: &ItemStack) {
    item_used_on_location(player, "minecraft:item_used_on_block", pos, state, tool);
}

/// Vanilla parity: `ItemUsedOnLocationTrigger.trigger`, whose loot context is
/// the block center as `ORIGIN`, the block state as `BLOCK_STATE` and the item
/// as `TOOL` -- the three things a `location` predicate reads. The `player`
/// predicate is checked by [`fire`] against a *different* context, the player
/// own one, exactly as vanilla does.
fn item_used_on_location(
    player: &Player,
    trigger_id: &'static str,
    pos: BlockPos,
    state: BlockStateId,
    tool: &ItemStack,
) {
    let context = PredicateContext {
        player,
        origin: DVec3::new(
            f64::from(pos.x()) + 0.5,
            f64::from(pos.y()) + 0.5,
            f64::from(pos.z()) + 0.5,
        ),
        subject: Subject::Player,
        block_state: Some(state),
        tool: Some(tool),
    };
    fire(player, trigger_id, |instance| {
        let location = match instance {
            TriggerInstance::PlacedBlock { location, .. }
            | TriggerInstance::ItemUsedOnBlock { location, .. }
            | TriggerInstance::AllayDropItemOnBlock { location, .. } => *location,
            _ => return false,
        };
        context.matches_conditions(location)
    });
}

/// Vanilla parity: `CriteriaTriggers.CHANGED_DIMENSION`.
pub fn changed_dimension(player: &Player, from: &Identifier, to: &Identifier) {
    fire(player, "minecraft:changed_dimension", |instance| {
        let TriggerInstance::ChangedDimension {
            from: wanted_from,
            to: wanted_to,
            ..
        } = instance
        else {
            return false;
        };
        wanted_from.as_ref().is_none_or(|wanted| wanted == from)
            && wanted_to.as_ref().is_none_or(|wanted| wanted == to)
    });
}

/// Vanilla parity: `CriteriaTriggers.SLEPT_IN_BED`.
pub fn slept_in_bed(player: &Player) {
    fire(player, "minecraft:slept_in_bed", |_| true);
}

/// Vanilla parity: `CriteriaTriggers.GENERATE_LOOT`, whose registry name is
/// `minecraft:player_generates_container_loot`.
pub fn player_generates_container_loot(player: &Player, loot_table: &Identifier) {
    fire(
        player,
        "minecraft:player_generates_container_loot",
        |instance| {
            let TriggerInstance::PlayerGeneratesContainerLoot {
                loot_table: wanted, ..
            } = instance
            else {
                return false;
            };
            wanted == loot_table
        },
    );
}

/// Vanilla parity: `CriteriaTriggers.CONSTRUCT_BEACON`.
pub fn construct_beacon(player: &Player, levels: i32) {
    fire(player, "minecraft:construct_beacon", |instance| {
        let TriggerInstance::ConstructBeacon { level, .. } = instance else {
            return false;
        };
        level.matches(levels)
    });
}
