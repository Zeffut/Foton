//! Fishing rod item behavior (`FishingRodItem`).
//!
//! One right-click casts a [`FishingHookEntity`]; the next reels it in, taking
//! the durability the hook reports back. Lure and Luck of the Sea are read off
//! the rod at cast time and travel with the bobber, exactly as vanilla does --
//! swapping to a different rod mid-cast does not change the odds.

use std::sync::Arc;

use foton_macros::item_behavior;
use foton_protocol::packets::game::SoundSource;
use foton_registry::data_components::vanilla_components::USE_EFFECTS;
use foton_registry::game_events::GameEventRef;
use foton_registry::{sound_events, vanilla_entities, vanilla_game_events};
use foton_utils::Downcast as _;

use crate::behavior::context::{InteractionResult, UseItemContext};
use crate::behavior::item::ItemBehavior;
use crate::enchantment_helper;
use crate::entity::entities::FishingHookEntity;
use crate::entity::{Entity as _, Projectile as _, SharedEntity, next_entity_id};

/// Vanilla turns the `fishing_time_reduction` value effect, which Lure declares
/// in seconds, into ticks shaved off the lure timer.
const SECONDS_TO_TICKS: f32 = 20.0;

/// Behavior for the fishing rod item.
#[item_behavior(class = "FishingRodItem")]
pub struct FishingRodItem;

impl FishingRodItem {
    /// Vanilla `ItemStack.causeUseVibration`, which a warden can hear.
    fn cause_use_vibration(context: &mut UseItemContext, event: GameEventRef) {
        let interact_vibrations = context.inv.with_item(|item| {
            item.get(USE_EFFECTS)
                .is_some_and(|effects| effects.interact_vibrations)
        });
        if interact_vibrations {
            context.player.game_event(event);
        }
    }
}

impl ItemBehavior for FishingRodItem {
    fn use_item(&self, context: &mut UseItemContext) -> InteractionResult {
        let player = context.player;
        let world = context.world;
        let pitch = 0.4 / (rand::random::<f32>() * 0.4 + 0.8);

        if let Some(hook) = player.fishing_hook() {
            let rod = context.inv.with_item(|item| item.clone());
            let damage = hook
                .as_ref()
                .downcast_ref::<FishingHookEntity>()
                .map_or(0, |hook| hook.retrieve(&rod));
            player.hurt_item_in_hand(context.hand, damage);

            world.play_sound_at(
                &sound_events::ENTITY_FISHING_BOBBER_RETRIEVE,
                SoundSource::Neutral,
                player.position(),
                1.0,
                pitch,
                None,
            );
            Self::cause_use_vibration(context, &vanilla_game_events::ITEM_INTERACT_FINISH);
            return InteractionResult::Success;
        }

        world.play_sound_at(
            &sound_events::ENTITY_FISHING_BOBBER_THROW,
            SoundSource::Neutral,
            player.position(),
            0.5,
            pitch,
            None,
        );

        let rod = context.inv.with_item(|item| item.clone());
        let lure_speed =
            (enchantment_helper::get_fishing_time_reduction(&rod) * SECONDS_TO_TICKS) as i32;
        let luck = enchantment_helper::get_fishing_luck_bonus(&rod);

        let hook = Arc::new(FishingHookEntity::new(
            &vanilla_entities::FISHING_BOBBER,
            next_entity_id(),
            player.position(),
            Arc::downgrade(world),
        ));
        if let Some(owner) = world.players.get_by_uuid(&player.gameprofile.id) {
            let owner: SharedEntity = owner;
            hook.set_owner_entity(Some(&owner));
        } else {
            hook.set_owner_uuid(Some(player.gameprofile.id));
        }
        hook.cast_from(player, luck, lure_speed);

        let entity: SharedEntity = hook;
        if let Err(error) = world.try_add_entity(Arc::clone(&entity)) {
            log::debug!("failed to cast a fishing hook: {error}");
            return InteractionResult::Fail;
        }
        player.set_fishing_hook(Some(&entity));

        // TODO: award the ITEM_USED stat once a stats system exists.
        Self::cause_use_vibration(context, &vanilla_game_events::ITEM_INTERACT_START);
        InteractionResult::Success
    }
}
