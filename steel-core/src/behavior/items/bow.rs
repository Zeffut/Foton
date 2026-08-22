//! Bow item behavior.
//!
//! Vanilla parity: `BowItem`. Holding right-click draws the bow; releasing it
//! fires an arrow whose speed depends on how long the string was held.

use std::sync::Arc;

use steel_macros::item_behavior;
use steel_protocol::packets::game::SoundSource;
use steel_registry::item_stack::ItemStack;
use steel_registry::sound_events;
use steel_registry::vanilla_items;

use crate::behavior::context::{InteractionResult, UseItemContext};
use crate::behavior::item::ItemBehavior;
use crate::entity::LivingEntity;
use crate::entity::entities::ArrowEntity;
use crate::inventory::container::Container as _;
use crate::player::Player;
use crate::world::World;

/// Ticks needed for a fully drawn bow.
///
/// Vanilla parity: `BowItem.MAX_DRAW_DURATION`.
const MAX_DRAW_DURATION: f32 = 20.0;

/// Ticks the bow stays usable while held, effectively forever.
const BOW_USE_DURATION: i32 = 72_000;

/// Draw strength below which the shot is cancelled.
const MINIMUM_POWER: f32 = 0.1;

/// Speed multiplier applied to a fully drawn shot.
const SHOT_POWER_SCALE: f32 = 3.0;

/// Spread applied to the shot direction.
const SHOT_UNCERTAINTY: f32 = 1.0;

/// Returns whether the player carries at least one arrow.
fn has_arrow(player: &Player) -> bool {
    let arrow = ItemStack::new(&vanilla_items::ARROW);
    player.inventory.lock().find_slot_matching_item(&arrow) != -1
}

/// Removes one arrow from the player's inventory, reporting whether it succeeded.
fn take_one_arrow(player: &Player) -> bool {
    let arrow = ItemStack::new(&vanilla_items::ARROW);
    let mut inventory = player.inventory.lock();
    let slot = inventory.find_slot_matching_item(&arrow);
    if slot < 0 {
        return false;
    }

    let slot = slot as usize;
    let remaining = inventory.get_item(slot).count() - 1;
    if remaining <= 0 {
        inventory.set_item(slot, ItemStack::empty());
    } else {
        let mut stack = inventory.get_item(slot).clone();
        stack.set_count(remaining);
        inventory.set_item(slot, stack);
    }
    true
}

/// Behavior for the bow.
#[item_behavior]
pub struct BowItem;

impl BowItem {
    /// Creates a new bow behavior.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Converts a draw time into a shot strength between 0 and 1.
    ///
    /// Vanilla parity: `BowItem.getPowerForTime`. The curve is deliberately not
    /// linear: the last few ticks of the draw add far more speed than the first.
    #[must_use]
    pub fn power_for_time(time_held: i32) -> f32 {
        let drawn = time_held as f32 / MAX_DRAW_DURATION;
        drawn.mul_add(drawn, drawn * 2.0).min(3.0) / 3.0
    }
}

impl Default for BowItem {
    fn default() -> Self {
        Self::new()
    }
}

impl ItemBehavior for BowItem {
    fn use_item(&self, context: &mut UseItemContext) -> InteractionResult {
        // Vanilla parity: `BowItem.use` refuses to draw without ammunition.
        if !context.player.has_infinite_materials() && !has_arrow(context.player) {
            return InteractionResult::Fail;
        }
        context.player.start_using_item(context.hand);
        InteractionResult::Consume
    }

    fn get_use_duration(&self, _stack: &ItemStack, _user: &dyn LivingEntity) -> i32 {
        BOW_USE_DURATION
    }

    /// Fires the arrow when the string is released.
    ///
    /// Vanilla parity: `BowItem.releaseUsing`.
    fn release_using(
        &self,
        _stack: &mut ItemStack,
        world: &Arc<World>,
        user: &dyn LivingEntity,
        time_left: i32,
    ) -> bool {
        let Some(player) = user.as_player() else {
            return false;
        };

        let time_held = BOW_USE_DURATION - time_left;
        let power = Self::power_for_time(time_held);
        if power < MINIMUM_POWER {
            return false;
        }

        // Vanilla consumes the arrow only when the shot actually leaves the bow.
        let infinite = player.has_infinite_materials();
        if !infinite && !take_one_arrow(player) {
            return false;
        }

        let arrow =
            ArrowEntity::shoot_from(world, user, power * SHOT_POWER_SCALE, SHOT_UNCERTAINTY);
        drop(arrow);

        world.play_sound_at(
            &sound_events::ENTITY_ARROW_SHOOT,
            SoundSource::Players,
            user.position(),
            1.0,
            0.4f32.mul_add(rand::random::<f32>(), 1.0),
            None,
        );

        if let Some(hand) = player.active_item_use_hand() {
            player.inventory.lock().hurt_item_in_hand(hand, 1, infinite);
        }

        // TODO: apply the Power, Punch, Flame and Infinity enchantments, and mark a
        // fully drawn shot as critical.
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draw_power_matches_the_vanilla_curve() {
        // Vanilla: pow = t/20; pow = (pow*pow + pow*2)/3, capped at 1.
        assert!((BowItem::power_for_time(0) - 0.0).abs() < f32::EPSILON);
        assert!((BowItem::power_for_time(20) - 1.0).abs() < f32::EPSILON);

        // A half-drawn bow is far weaker than half strength: 0.5 gives 0.4166...
        let half = BowItem::power_for_time(10);
        assert!((half - 0.416_666_66).abs() < 1e-5, "got {half}");
        assert!(half < 0.5, "the draw curve must not be linear");
    }

    #[test]
    fn a_full_draw_never_exceeds_full_power() {
        assert!((BowItem::power_for_time(40) - 1.0).abs() < f32::EPSILON);
        assert!((BowItem::power_for_time(72_000) - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn a_tap_is_below_the_firing_threshold() {
        assert!(BowItem::power_for_time(1) < MINIMUM_POWER);
        assert!(BowItem::power_for_time(3) >= MINIMUM_POWER);
    }
}
