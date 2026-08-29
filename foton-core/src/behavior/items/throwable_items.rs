//! Snowball, egg and bottle o' enchanting items.
//!
//! Vanilla parity: `SnowballItem`, `EggItem` and `ExperienceBottleItem`. All
//! three are the same gesture -- throw the thing in your hand -- and differ
//! only in what they spawn, the sound they make, and how hard and how flat it
//! is thrown, so they share the throwing here rather than in three files that
//! would drift apart.

use std::sync::Arc;

use foton_macros::item_behavior;
use foton_protocol::packets::game::SoundSource;
use foton_registry::sound_event::SoundEventRef;
use foton_registry::{sound_events, vanilla_entities};
use glam::DVec3;

use crate::behavior::context::{InteractionResult, UseItemContext};
use crate::behavior::item::ItemBehavior;
use crate::entity::entities::{ExperienceBottleEntity, SnowballEntity, ThrownEggEntity};
use crate::entity::{Entity, SharedEntity, ThrowableItemProjectile, next_entity_id};

/// How hard a snowball or an egg is thrown.
///
/// Vanilla parity: the `1.5F` shared by `SnowballItem` and `EggItem`.
const SHOOT_POWER: f32 = 1.5;

/// And how hard a bottle is, which is much gentler.
///
/// Vanilla parity: the `0.7F` of `ExperienceBottleItem.use`.
const BOTTLE_POWER: f32 = 0.7;

/// How far above the player's aim a bottle is lobbed.
///
/// Vanilla parity: the `-20.0F` pitch offset of `ExperienceBottleItem.use`,
/// which is why a bottle arcs rather than flying flat.
const BOTTLE_PITCH_OFFSET: f32 = -20.0;

/// Behavior for the snowball item.
#[item_behavior]
pub struct SnowballItem;

/// Behavior for the egg item.
#[item_behavior]
pub struct EggItem;

/// Behavior for the bottle o' enchanting.
#[item_behavior]
pub struct ExperienceBottleItem;

impl ItemBehavior for SnowballItem {
    /// Vanilla parity: `SnowballItem.use`.
    fn use_item(&self, context: &mut UseItemContext) -> InteractionResult {
        let world = Arc::downgrade(context.world);
        throw(
            context,
            &sound_events::ENTITY_SNOWBALL_THROW,
            SHOOT_POWER,
            0.0,
            |position| {
                Arc::new(SnowballEntity::new(
                    &vanilla_entities::SNOWBALL,
                    next_entity_id(),
                    position,
                    world,
                ))
            },
        )
    }
}

impl ItemBehavior for EggItem {
    /// Vanilla parity: `EggItem.use`.
    fn use_item(&self, context: &mut UseItemContext) -> InteractionResult {
        let world = Arc::downgrade(context.world);
        throw(
            context,
            &sound_events::ENTITY_EGG_THROW,
            SHOOT_POWER,
            0.0,
            |position| {
                Arc::new(ThrownEggEntity::new(
                    &vanilla_entities::EGG,
                    next_entity_id(),
                    position,
                    world,
                ))
            },
        )
    }
}

impl ItemBehavior for ExperienceBottleItem {
    /// Vanilla parity: `ExperienceBottleItem.use`.
    fn use_item(&self, context: &mut UseItemContext) -> InteractionResult {
        let world = Arc::downgrade(context.world);
        throw(
            context,
            &sound_events::ENTITY_EXPERIENCE_BOTTLE_THROW,
            BOTTLE_POWER,
            BOTTLE_PITCH_OFFSET,
            |position| {
                Arc::new(ExperienceBottleEntity::new(
                    &vanilla_entities::EXPERIENCE_BOTTLE,
                    next_entity_id(),
                    position,
                    world,
                ))
            },
        )
    }
}

/// Throws whatever `spawn` makes, from the thrower's eye along their look.
///
/// Vanilla parity: the body `SnowballItem.use` and `EggItem.use` share, down to
/// the pitch, which is deliberately jittery so a handful of snowballs does not
/// sound like one long note.
fn throw<P, F>(
    context: &mut UseItemContext,
    sound: SoundEventRef,
    power: f32,
    pitch_offset: f32,
    spawn: F,
) -> InteractionResult
where
    P: ThrowableItemProjectile + 'static,
    F: FnOnce(DVec3) -> Arc<P>,
{
    let player = context.player;
    let world = context.world;

    let pitch = 0.4 / (rand::random::<f32>() * 0.4 + 0.8);
    world.play_sound_at(
        sound,
        SoundSource::Neutral,
        player.position(),
        0.5,
        pitch,
        None,
    );

    let thrown_item = context.inv.with_item(|item| item.clone());

    // Vanilla `ThrowableItemProjectile` spawns at the shooter's eye minus 0.1.
    let player_pos = player.position();
    let projectile = spawn(DVec3::new(
        player_pos.x,
        player.get_eye_y() - 0.1,
        player_pos.z,
    ));

    if let Some(owner) = world.players.get_by_uuid(&player.gameprofile.id) {
        let owner: SharedEntity = owner;
        projectile.set_owner_entity(Some(&owner));
    } else {
        projectile.set_owner_uuid(Some(player.gameprofile.id));
    }
    projectile.set_item_clamped(thrown_item);

    let (yaw, player_pitch) = player.rotation();
    projectile.shoot_from_rotation(player, player_pitch + pitch_offset, yaw, 0.0, power, 1.0);

    let projectile: SharedEntity = projectile;
    if world.try_add_entity(projectile).is_err() {
        return InteractionResult::Fail;
    }

    // TODO: award the ITEM_USED stat once a stats system exists.
    if !player.has_infinite_materials() {
        context.inv.with_item(|item| item.shrink(1));
    }

    InteractionResult::Success
}
