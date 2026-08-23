//! Goat horn item behavior.
//!
//! Vanilla parity: `InstrumentItem`. The horn carries its instrument in the
//! `minecraft:instrument` component; playing it sounds that instrument at the
//! instrument's own range and blocks the horn for as long as the note lasts.

use steel_macros::item_behavior;
use steel_protocol::packets::game::SoundSource;
use steel_registry::data_components::InstrumentComponent;
use steel_registry::data_components::vanilla_components::INSTRUMENT;
use steel_registry::instrument::InstrumentValue;
use steel_registry::item_stack::ItemStack;
use steel_registry::vanilla_game_events;

use crate::behavior::ItemUseAnimation;
use crate::behavior::context::{InteractionResult, UseItemContext};
use crate::behavior::item::ItemBehavior;
use crate::entity::{Entity, LivingEntity};
use crate::world::game_event::GameEventContext;

/// Vanilla parity: the `Mth.floor(instrument.useDuration() * 20.0F)` that
/// `InstrumentItem` computes twice, once for the hold and once for the cooldown.
fn use_duration_ticks(instrument: &InstrumentValue) -> i32 {
    (instrument.use_duration() * 20.0).floor() as i32
}

/// Vanilla parity: `Instrument.range() / 16.0F`. Steel's sound range is derived
/// from the volume the same way vanilla's is, so a 256-block instrument reaches
/// 256 blocks by asking for volume 16.
fn play_volume(instrument: &InstrumentValue) -> f32 {
    instrument.range() / 16.0
}

/// Behavior for instrument items such as the goat horn.
#[item_behavior]
pub struct InstrumentItem;

impl ItemBehavior for InstrumentItem {
    /// Vanilla parity: `InstrumentItem.use`.
    fn use_item(&self, context: &mut UseItemContext) -> InteractionResult {
        let held = context
            .inv
            .with_item(|item| item.copy_with_count(item.count()));
        let Some(instrument) = held.get(INSTRUMENT).map(InstrumentComponent::value) else {
            // Vanilla refuses a horn whose component was stripped away.
            return InteractionResult::Fail;
        };

        context.player.start_using_item(context.hand);
        play(context, instrument);
        context
            .player
            .add_item_cooldown(&held, use_duration_ticks(instrument));

        // TODO: award the ITEM_USED stat once a stats system exists.

        InteractionResult::Consume
    }

    /// Vanilla parity: `InstrumentItem.getUseDuration`. A horn with no
    /// instrument reports zero, which is what makes `use` unable to hold it.
    fn get_use_duration(&self, stack: &ItemStack, _user: &dyn LivingEntity) -> i32 {
        stack
            .get(INSTRUMENT)
            .map_or(0, |component| use_duration_ticks(component.value()))
    }

    /// Vanilla parity: `InstrumentItem.getUseAnimation`.
    fn get_use_animation(&self, _stack: &ItemStack) -> ItemUseAnimation {
        ItemUseAnimation::TootHorn
    }
}

/// Vanilla parity: `InstrumentItem.play`.
fn play(context: &UseItemContext, instrument: &InstrumentValue) {
    let player = context.player;
    let position = player.position();

    // An inline instrument can name a sound that is in no registry. The sound
    // packet carries a registry id, so Steel cannot send one yet -- the same
    // gap `Player.play_sound_holder` records. The horn still goes on cooldown
    // and still emits the game event, so only the audio is missing.
    if let Some(sound) = instrument.sound_event().registry_ref() {
        // Vanilla `Level.playSound(player, player, ...)` excludes the player
        // playing the horn: their own client sounds it locally.
        context.world.play_sound_at(
            sound,
            SoundSource::Records,
            position,
            play_volume(instrument),
            1.0,
            Some(player.id()),
        );
    }

    context.world.game_event_at(
        &vanilla_game_events::INSTRUMENT_PLAY,
        position,
        &GameEventContext::new(Some(player), None),
    );
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use steel_registry::{init_vanilla_registry, vanilla_instruments, vanilla_items};
    use steel_utils::types::InteractionHand;

    use super::*;
    use crate::behavior::UseItemContext;
    use crate::bootstrap::init_globals_once;
    use crate::test_support::{TestPlayerBuilder, fresh_test_world};

    /// The extracted goat horn carries the ponder instrument: 7 seconds long,
    /// 256 blocks wide.
    const PONDER_TICKS: i32 = 140;

    fn goat_horn() -> ItemStack {
        ItemStack::new(&vanilla_items::GOAT_HORN)
    }

    #[test]
    fn a_horn_is_held_for_exactly_as_long_as_its_instrument_sounds() {
        init_vanilla_registry();
        let world = fresh_test_world("goat_horn_duration");
        let player = TestPlayerBuilder::new(Arc::clone(&world), "Hornblower", 1).build();

        assert_eq!(
            use_duration_ticks(vanilla_instruments::PONDER_GOAT_HORN.value()),
            PONDER_TICKS
        );
        assert_eq!(
            InstrumentItem.get_use_duration(&goat_horn(), player.as_ref()),
            PONDER_TICKS
        );
    }

    #[test]
    fn the_note_carries_as_far_as_the_instruments_range() {
        init_vanilla_registry();

        // Vanilla asks for range/16 as the volume, and Steel's sound range is
        // 16 * volume for volumes above one, so the two agree at 256 blocks.
        let ponder = vanilla_instruments::PONDER_GOAT_HORN.value();
        let volume = play_volume(ponder);
        assert!((volume - 16.0).abs() < f32::EPSILON, "got {volume}");
        assert!(
            (ponder
                .sound_event()
                .registry_ref()
                .map_or(0.0, |sound| sound.range(volume))
                - ponder.range())
            .abs()
                < f32::EPSILON
        );
    }

    #[test]
    fn tooting_a_horn_blocks_it_until_the_note_ends() {
        init_globals_once();
        let world = fresh_test_world("goat_horn_cooldown");
        let player = TestPlayerBuilder::new(Arc::clone(&world), "Hornblower", 1).build();
        player
            .inventory
            .lock()
            .set_item_in_hand(InteractionHand::MainHand, goat_horn());

        let mut context = UseItemContext::new(
            &player,
            InteractionHand::MainHand,
            &world,
            player.inventory.clone(),
        );

        assert_eq!(
            InstrumentItem.use_item(&mut context),
            InteractionResult::Consume
        );
        assert_eq!(
            player.active_item_use_hand(),
            Some(InteractionHand::MainHand)
        );
        assert!(player.is_item_on_cooldown(&goat_horn()));
    }

    #[test]
    fn a_horn_with_no_instrument_cannot_be_played() {
        init_globals_once();
        let world = fresh_test_world("goat_horn_stripped");
        let player = TestPlayerBuilder::new(Arc::clone(&world), "Hornblower", 1).build();
        let mut stripped = goat_horn();
        stripped.remove(INSTRUMENT);
        player
            .inventory
            .lock()
            .set_item_in_hand(InteractionHand::MainHand, stripped.clone());

        let mut context = UseItemContext::new(
            &player,
            InteractionHand::MainHand,
            &world,
            player.inventory.clone(),
        );

        assert_eq!(
            InstrumentItem.use_item(&mut context),
            InteractionResult::Fail
        );
        assert_eq!(player.active_item_use_hand(), None);
        assert!(!player.is_item_on_cooldown(&stripped));
        assert_eq!(
            InstrumentItem.get_use_duration(&stripped, player.as_ref()),
            0
        );
    }
}
