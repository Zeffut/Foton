//! Snowball and egg items.
//!
//! Vanilla parity: `SnowballItem` and `EggItem`. Both are the same gesture --
//! throw the thing in your hand -- and differ only in what they spawn and the
//! sound they make, so they share the throwing here rather than in two files
//! that would drift apart.

use std::sync::Arc;

use glam::DVec3;
use steel_macros::item_behavior;
use steel_protocol::packets::game::SoundSource;
use steel_registry::sound_event::SoundEventRef;
use steel_registry::{sound_events, vanilla_entities};

use crate::behavior::context::{InteractionResult, UseItemContext};
use crate::behavior::item::ItemBehavior;
use crate::entity::entities::{SnowballEntity, ThrownEggEntity};
use crate::entity::{Entity, SharedEntity, ThrowableItemProjectile, next_entity_id};

/// How hard a thrown item is thrown.
///
/// Vanilla parity: the `1.5F` shared by `SnowballItem` and `EggItem`.
const SHOOT_POWER: f32 = 1.5;

/// Behavior for the snowball item.
#[item_behavior]
pub struct SnowballItem;

/// Behavior for the egg item.
#[item_behavior]
pub struct EggItem;

impl ItemBehavior for SnowballItem {
    /// Vanilla parity: `SnowballItem.use`.
    fn use_item(&self, context: &mut UseItemContext) -> InteractionResult {
        let world = Arc::downgrade(context.world);
        throw(context, &sound_events::ENTITY_SNOWBALL_THROW, |position| {
            Arc::new(SnowballEntity::new(
                &vanilla_entities::SNOWBALL,
                next_entity_id(),
                position,
                world,
            ))
        })
    }
}

impl ItemBehavior for EggItem {
    /// Vanilla parity: `EggItem.use`.
    fn use_item(&self, context: &mut UseItemContext) -> InteractionResult {
        let world = Arc::downgrade(context.world);
        throw(context, &sound_events::ENTITY_EGG_THROW, |position| {
            Arc::new(ThrownEggEntity::new(
                &vanilla_entities::EGG,
                next_entity_id(),
                position,
                world,
            ))
        })
    }
}

/// Throws whatever `spawn` makes, from the thrower's eye along their look.
///
/// Vanilla parity: the body `SnowballItem.use` and `EggItem.use` share, down to
/// the pitch, which is deliberately jittery so a handful of snowballs does not
/// sound like one long note.
fn throw<P, F>(context: &mut UseItemContext, sound: SoundEventRef, spawn: F) -> InteractionResult
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
    projectile.shoot_from_rotation(player, player_pitch, yaw, 0.0, SHOOT_POWER, 1.0);

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
