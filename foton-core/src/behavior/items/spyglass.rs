//! Spyglass item behavior.
//!
//! Vanilla parity: `SpyglassItem`. Right-click raises the spyglass and holds it
//! until the player lets go or the minute runs out. The zoom is entirely
//! client-side -- the client narrows its field of view whenever the item it is
//! using renders with the `spyglass` animation -- so the server's whole job is
//! to start the use, report the animation and duration, and play the two
//! sounds. There is no server hook to look for.

use std::sync::Arc;

use foton_macros::item_behavior;
use foton_protocol::packets::game::SoundSource;
use foton_registry::item_stack::ItemStack;
use foton_registry::sound_events;

use crate::behavior::ItemUseAnimation;
use crate::behavior::context::{InteractionResult, UseItemContext};
use crate::behavior::item::ItemBehavior;
use crate::entity::{Entity, LivingEntity};
use crate::world::World;

/// Vanilla parity: `SpyglassItem.USE_DURATION`. A minute of holding, after
/// which vanilla finishes the use and lowers the spyglass on its own.
const USE_DURATION: i32 = 1200;

/// Behavior for the spyglass.
#[item_behavior]
pub struct SpyglassItem;

impl ItemBehavior for SpyglassItem {
    /// Vanilla parity: `SpyglassItem.getUseDuration`.
    fn get_use_duration(&self, _stack: &ItemStack, _user: &dyn LivingEntity) -> i32 {
        USE_DURATION
    }

    /// Vanilla parity: `SpyglassItem.getUseAnimation`. This is what makes the
    /// client zoom; nothing on the server reads it.
    fn get_use_animation(&self, _stack: &ItemStack) -> ItemUseAnimation {
        ItemUseAnimation::Spyglass
    }

    /// Vanilla parity: `SpyglassItem.use`, which plays the raise sound and then
    /// defers to `ItemUtils.startUsingInstantly`.
    fn use_item(&self, context: &mut UseItemContext) -> InteractionResult {
        // Vanilla `Player.playSound` passes the player as the excluded listener:
        // their own client plays this one locally.
        context.world.play_sound_at(
            &sound_events::ITEM_SPYGLASS_USE,
            SoundSource::Players,
            context.player.position(),
            1.0,
            1.0,
            Some(context.player.id()),
        );

        // TODO: award the ITEM_USED stat once a stats system exists.

        context.player.start_using_item(context.hand);
        InteractionResult::Consume
    }

    /// Vanilla parity: `SpyglassItem.releaseUsing`. Returning true asks vanilla
    /// to run one more use tick before the hold ends.
    fn release_using(
        &self,
        _stack: &mut ItemStack,
        _world: &Arc<World>,
        user: &dyn LivingEntity,
        _time_left: i32,
    ) -> bool {
        stop_using(user);
        true
    }

    /// Vanilla parity: `SpyglassItem.finishUsingItem`, which lowers the
    /// spyglass without touching the stack.
    fn finish_using(
        &self,
        stack: &mut ItemStack,
        _world: &Arc<World>,
        user: &dyn LivingEntity,
    ) -> ItemStack {
        stop_using(user);
        stack.copy_with_count(stack.count())
    }
}

/// Vanilla parity: `SpyglassItem.stopUsing`.
///
/// `LivingEntity.playSound` excludes nobody, unlike the raise sound in `use`.
fn stop_using(user: &dyn LivingEntity) {
    user.play_sound(&sound_events::ITEM_SPYGLASS_STOP_USING, 1.0, 1.0);
}

#[cfg(test)]
mod tests {
    use foton_registry::{init_vanilla_registry, vanilla_items};
    use foton_utils::types::InteractionHand;

    use super::*;
    use crate::behavior::UseItemContext;
    use crate::bootstrap::init_globals_once;
    use crate::test_support::{TestPlayerBuilder, fresh_test_world};

    #[test]
    fn raising_the_spyglass_starts_holding_it() {
        init_globals_once();
        let world = fresh_test_world("spyglass_raise");
        let player = TestPlayerBuilder::new(Arc::clone(&world), "Stargazer", 1).build();
        player.inventory.lock().set_item_in_hand(
            InteractionHand::MainHand,
            ItemStack::new(&vanilla_items::SPYGLASS),
        );

        let mut context = UseItemContext::new(
            &player,
            InteractionHand::MainHand,
            &world,
            player.inventory.clone(),
        );

        assert_eq!(
            SpyglassItem.use_item(&mut context),
            InteractionResult::Consume
        );
        assert_eq!(
            player.active_item_use_hand(),
            Some(InteractionHand::MainHand),
            "the spyglass is a held use, not an instant one"
        );
    }

    #[test]
    fn the_client_is_told_to_zoom_for_a_full_minute() {
        init_vanilla_registry();
        let stack = ItemStack::new(&vanilla_items::SPYGLASS);
        let world = fresh_test_world("spyglass_zoom");
        let player = TestPlayerBuilder::new(Arc::clone(&world), "Stargazer", 1).build();

        // The zoom lives on the client and is keyed off this animation, so a
        // spyglass reporting anything else simply would not zoom.
        assert_eq!(
            SpyglassItem.get_use_animation(&stack),
            ItemUseAnimation::Spyglass
        );
        assert_eq!(SpyglassItem.get_use_duration(&stack, player.as_ref()), 1200);
    }
}
