//! Throwable potion item behavior.
//!
//! Vanilla parity: `ThrowablePotionItem` and its two subclasses. Brewing can
//! turn a potion into a splash potion with gunpowder; this is what that
//! gunpowder buys.

use std::borrow::Cow;
use std::sync::Arc;

use glam::DVec3;
use steel_macros::item_behavior;
use steel_protocol::packets::game::SoundSource;
use steel_registry::item_stack::ItemStack;
use steel_registry::{sound_events, vanilla_entities};
use text_components::TextComponent;

use crate::behavior::ItemBehavior;
use crate::behavior::context::{InteractionResult, UseItemContext};
use crate::entity::entities::SplashPotionEntity;
use crate::entity::{Entity, Projectile, SharedEntity, ThrowableItemProjectile, next_entity_id};

use super::dynamic_name::potion_name;

/// How hard a potion is thrown.
///
/// Vanilla parity: `ThrowablePotionItem.PROJECTILE_SHOOT_POWER`. Slower than an
/// ender pearl, which together with the heavier gravity is why a splash potion
/// lobs rather than flies.
const SHOOT_POWER: f32 = 0.5;

/// How far below straight ahead a thrown potion is aimed.
///
/// Vanilla parity: the `-20.0F` pitch offset, which arcs the throw down so it
/// lands at the player's feet rather than sailing over the target.
const PITCH_OFFSET: f32 = -20.0;

/// Throws one potion from the player's hand.
fn throw_potion(context: &mut UseItemContext<'_>) -> InteractionResult {
    let player = context.player;
    let world = context.world;

    let thrown_item = context.inv.with_item(|item| item.clone());

    let player_pos = player.position();
    let spawn_pos = DVec3::new(player_pos.x, player.get_eye_y() - 0.1, player_pos.z);

    let potion = Arc::new(SplashPotionEntity::new(
        &vanilla_entities::SPLASH_POTION,
        next_entity_id(),
        spawn_pos,
        Arc::downgrade(world),
    ));
    if let Some(owner) = world.players.get_by_uuid(&player.gameprofile.id) {
        let owner: SharedEntity = owner;
        potion.set_owner_entity(Some(&owner));
    } else {
        potion.set_owner_uuid(Some(player.gameprofile.id));
    }
    potion.set_item_clamped(thrown_item);

    let (yaw, player_pitch) = player.rotation();
    potion.shoot_from_rotation(
        player,
        player_pitch + PITCH_OFFSET,
        yaw,
        0.0,
        SHOOT_POWER,
        1.0,
    );

    let entity: SharedEntity = potion;
    if let Err(error) = world.try_add_entity(entity) {
        log::debug!("failed to spawn thrown potion: {error}");
        return InteractionResult::Fail;
    }

    let pitch = 0.4f32.mul_add(rand::random::<f32>(), 0.8);
    world.play_sound_at(
        &sound_events::ENTITY_SPLASH_POTION_THROW,
        SoundSource::Neutral,
        player.position(),
        0.5,
        pitch,
        None,
    );

    // TODO: award the ITEM_USED stat once a stats system exists.
    context.inv.with_item(|item| item.shrink(1));
    InteractionResult::Success
}

/// Splash-potion behavior.
// TODO: Implement inherited PotionItem.useOn water-to-mud conversion.
// TODO: Add the inherited water default instance when Steel has item-specific
// default-stack factories.
#[item_behavior]
pub struct SplashPotionItem;

impl ItemBehavior for SplashPotionItem {
    fn get_name<'a>(&self, stack: &'a ItemStack) -> Cow<'a, TextComponent> {
        potion_name(stack)
    }

    /// Vanilla parity: `ThrowablePotionItem.use`.
    fn use_item(&self, context: &mut UseItemContext<'_>) -> InteractionResult {
        throw_potion(context)
    }
}

/// Lingering-potion behavior.
// TODO: Implement inherited PotionItem.useOn water-to-mud conversion.
// TODO: A lingering potion should leave an area-effect cloud rather than
// splashing once; the cloud entity does not exist, so throwing one currently
// behaves as a splash. That is wrong and visible, and it is the next piece.
// TODO: Add the inherited water default instance when Steel has item-specific
// default-stack factories.
#[item_behavior]
pub struct LingeringPotionItem;

impl ItemBehavior for LingeringPotionItem {
    fn get_name<'a>(&self, stack: &'a ItemStack) -> Cow<'a, TextComponent> {
        potion_name(stack)
    }
}
