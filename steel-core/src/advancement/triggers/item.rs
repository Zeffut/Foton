//! Triggers that fire from an item being used.

use steel_registry::advancement::TriggerInstance;
use steel_registry::item_stack::ItemStack;

use super::fire;
use crate::advancement::predicate::item_matches;
use crate::player::Player;

/// Vanilla parity: `CriteriaTriggers.CONSUME_ITEM`, fired from
/// `Consumable.onConsume` -- so it covers food and potions alike.
pub fn consume_item(player: &Player, stack: &ItemStack) {
    item_only(player, "minecraft:consume_item", stack);
}

/// Vanilla parity: `CriteriaTriggers.FILLED_BUCKET`.
pub fn filled_bucket(player: &Player, bucket: &ItemStack) {
    item_only(player, "minecraft:filled_bucket", bucket);
}

/// Vanilla parity: `CriteriaTriggers.USING_ITEM`, fired every tick a player
/// keeps an item raised.
pub fn using_item(player: &Player, stack: &ItemStack) {
    item_only(player, "minecraft:using_item", stack);
}

/// Vanilla parity: `CriteriaTriggers.SHOT_CROSSBOW`.
pub fn shot_crossbow(player: &Player, weapon: &ItemStack) {
    item_only(player, "minecraft:shot_crossbow", weapon);
}

/// Vanilla parity: `CriteriaTriggers.USED_TOTEM`.
pub fn used_totem(player: &Player, totem: &ItemStack) {
    item_only(player, "minecraft:used_totem", totem);
}

/// The five triggers above share a shape: one optional item predicate, which
/// accepts anything when absent.
fn item_only(player: &Player, trigger_id: &'static str, stack: &ItemStack) {
    fire(player, trigger_id, |instance| {
        let (TriggerInstance::ConsumeItem { item: wanted, .. }
        | TriggerInstance::FilledBucket { item: wanted, .. }
        | TriggerInstance::UsingItem { item: wanted, .. }
        | TriggerInstance::ShotCrossbow { item: wanted, .. }
        | TriggerInstance::UsedTotem { item: wanted, .. }) = instance
        else {
            return false;
        };
        wanted
            .as_ref()
            .is_none_or(|wanted| item_matches(wanted, stack))
    });
}
