//! Throwable potion item behavior.
//!
//! Vanilla parity: `ThrowablePotionItem` and its two subclasses. Brewing can
//! turn a potion into a splash potion with gunpowder; this is what that
//! gunpowder buys.
//!
//! The two subclasses differ in exactly two things -- the sound they play and
//! the entity their `createPotion` builds -- and the entity is what decides
//! whether the bottle splashes or leaves a cloud. That is why this file throws
//! `SplashPotionEntity` for one item and `LingeringPotionEntity` for the other
//! rather than letting one entity read the bottle it carries.

use std::borrow::Cow;
use std::sync::Arc;

use glam::DVec3;
use steel_macros::item_behavior;
use steel_protocol::packets::game::SoundSource;
use steel_registry::item_stack::ItemStack;
use steel_registry::sound_event::SoundEventRef;
use steel_registry::{sound_events, vanilla_entities};
use text_components::TextComponent;

use crate::behavior::ItemBehavior;
use crate::behavior::context::{InteractionResult, UseItemContext};
use crate::entity::entities::{LingeringPotionEntity, SplashPotionEntity};
use crate::entity::{Entity, SharedEntity, ThrowableItemProjectile, next_entity_id};

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

/// Volume both throw sounds are played at.
///
/// Vanilla parity: the `0.5F` shared by `SplashPotionItem.use` and
/// `LingeringPotionItem.use`.
const THROW_VOLUME: f32 = 0.5;

/// Returns the pitch a thrown potion's sound is played at.
///
/// Vanilla parity: the `0.4F / (random.nextFloat() * 0.4F + 0.8F)` shared by
/// both potion items. Note the division: this lands between 0.36 and 0.5, well
/// below the usual `0.8`-to-`1.2` jitter, which is what makes a thrown potion
/// sound heavier than a snowball.
fn throw_pitch() -> f32 {
    0.4 / 0.4f32.mul_add(rand::random::<f32>(), 0.8)
}

/// Throws one potion from the player's hand.
///
/// `spawn` builds the concrete entity, standing in for vanilla's
/// `createPotion`.
fn throw_potion(
    context: &mut UseItemContext<'_>,
    sound: SoundEventRef,
    source: SoundSource,
    spawn: fn(DVec3, &Arc<crate::world::World>) -> Arc<dyn ThrownPotion>,
) -> InteractionResult {
    let player = context.player;
    let world = context.world;

    let thrown_item = context.inv.with_item(|item| item.clone());

    let player_pos = player.position();
    let spawn_pos = DVec3::new(player_pos.x, player.get_eye_y() - 0.1, player_pos.z);

    let potion = spawn(spawn_pos, world);
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

    if let Err(error) = world.try_add_entity(potion.as_shared_entity()) {
        log::debug!("failed to spawn thrown potion: {error}");
        return InteractionResult::Fail;
    }

    world.play_sound_at(
        sound,
        source,
        player.position(),
        THROW_VOLUME,
        throw_pitch(),
        None,
    );

    // TODO: award the ITEM_USED stat once a stats system exists.
    context.inv.with_item(|item| item.shrink(1));
    InteractionResult::Success
}

/// What the throw path needs from either thrown-potion entity.
///
/// Vanilla parity: the `AbstractThrownPotion` return type of `createPotion`.
/// Steel has no shared supertype for the two entities, so this is the narrow
/// slice `ThrowablePotionItem.use` actually calls.
trait ThrownPotion: ThrowableItemProjectile {
    /// Returns this potion as the shared entity the world stores.
    fn as_shared_entity(self: Arc<Self>) -> SharedEntity;
}

impl ThrownPotion for SplashPotionEntity {
    fn as_shared_entity(self: Arc<Self>) -> SharedEntity {
        self
    }
}

impl ThrownPotion for LingeringPotionEntity {
    fn as_shared_entity(self: Arc<Self>) -> SharedEntity {
        self
    }
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

    /// Vanilla parity: `SplashPotionItem.use`, whose `createPotion` builds a
    /// `ThrownSplashPotion`.
    fn use_item(&self, context: &mut UseItemContext<'_>) -> InteractionResult {
        throw_potion(
            context,
            &sound_events::ENTITY_SPLASH_POTION_THROW,
            // Vanilla passes `SoundSource.PLAYERS` here and `NEUTRAL` for the
            // lingering bottle. The two really do differ.
            SoundSource::Players,
            |position, world| {
                Arc::new(SplashPotionEntity::new(
                    &vanilla_entities::SPLASH_POTION,
                    next_entity_id(),
                    position,
                    Arc::downgrade(world),
                ))
            },
        )
    }
}

/// Lingering-potion behavior.
// TODO: Implement inherited PotionItem.useOn water-to-mud conversion.
// TODO: Add the inherited water default instance when Steel has item-specific
// default-stack factories.
#[item_behavior]
pub struct LingeringPotionItem;

impl ItemBehavior for LingeringPotionItem {
    fn get_name<'a>(&self, stack: &'a ItemStack) -> Cow<'a, TextComponent> {
        potion_name(stack)
    }

    /// Vanilla parity: `LingeringPotionItem.use`, whose `createPotion` builds a
    /// `ThrownLingeringPotion`.
    fn use_item(&self, context: &mut UseItemContext<'_>) -> InteractionResult {
        throw_potion(
            context,
            &sound_events::ENTITY_LINGERING_POTION_THROW,
            SoundSource::Neutral,
            |position, world| {
                Arc::new(LingeringPotionEntity::new(
                    &vanilla_entities::LINGERING_POTION,
                    next_entity_id(),
                    position,
                    Arc::downgrade(world),
                ))
            },
        )
    }
}
