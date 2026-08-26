//! Vibrations: the layer sculk and the warden hear the world through.
//!
//! Vanilla parity: `net.minecraft.world.level.gameevent.vibrations`. A game event is
//! instantaneous and reaches every listener in range at once; a vibration is a game event
//! that had to travel, one tick per block, and only the best one heard in a tick is allowed
//! to travel at all. Those two rules plus the occlusion test are what separate a sculk
//! sensor from a pressure plate.

mod data;
mod info;
mod listener;
mod selector;
#[cfg(test)]
mod tests;
mod user;

use std::sync::LazyLock;

use steel_registry::game_events::GameEventRef;
use steel_registry::{REGISTRY, RegistryEntry as _, RegistryExt as _, vanilla_game_events};

pub use data::{VIBRATION_DATA_TAG, VibrationData};
pub use info::VibrationInfo;
pub use listener::{VibrationListener, distance_between_in_blocks};
pub use selector::VibrationSelector;
pub use user::{VibrationPositionSource, VibrationUser};

/// Vanilla `VibrationSystem.NO_VIBRATION_FREQUENCY`.
pub const NO_VIBRATION_FREQUENCY: i32 = 0;

/// Vanilla `VibrationSystem.VIBRATION_FREQUENCY_FOR_EVENT`, in registry order.
///
/// Vanilla keeps an identity map from game event to frequency. Steel indexes the same
/// mapping by registry id, which is fixed once the vanilla registry is frozen.
static VIBRATION_FREQUENCY_FOR_EVENT: LazyLock<Vec<i32>> = LazyLock::new(|| {
    let mut frequencies = vec![NO_VIBRATION_FREQUENCY; REGISTRY.game_events.len()];
    let mut put = |event: GameEventRef, frequency: i32| {
        if let Some(id) = event.try_id() {
            frequencies[id] = frequency;
        }
    };

    put(&vanilla_game_events::STEP, 1);
    put(&vanilla_game_events::SWIM, 1);
    put(&vanilla_game_events::FLAP, 1);
    put(&vanilla_game_events::PROJECTILE_LAND, 2);
    put(&vanilla_game_events::HIT_GROUND, 2);
    put(&vanilla_game_events::SPLASH, 2);
    put(&vanilla_game_events::BOUNCE, 2);
    put(&vanilla_game_events::ITEM_INTERACT_FINISH, 3);
    put(&vanilla_game_events::PROJECTILE_SHOOT, 3);
    put(&vanilla_game_events::INSTRUMENT_PLAY, 3);
    put(&vanilla_game_events::ENTITY_ACTION, 4);
    put(&vanilla_game_events::ELYTRA_GLIDE, 4);
    put(&vanilla_game_events::UNEQUIP, 4);
    put(&vanilla_game_events::ENTITY_DISMOUNT, 5);
    put(&vanilla_game_events::EQUIP, 5);
    put(&vanilla_game_events::ENTITY_INTERACT, 6);
    put(&vanilla_game_events::SHEAR, 6);
    put(&vanilla_game_events::ENTITY_MOUNT, 6);
    put(&vanilla_game_events::ENTITY_DAMAGE, 7);
    put(&vanilla_game_events::DRINK, 8);
    put(&vanilla_game_events::EAT, 8);
    put(&vanilla_game_events::CONTAINER_CLOSE, 9);
    put(&vanilla_game_events::BLOCK_CLOSE, 9);
    put(&vanilla_game_events::BLOCK_DEACTIVATE, 9);
    put(&vanilla_game_events::BLOCK_DETACH, 9);
    put(&vanilla_game_events::CONTAINER_OPEN, 10);
    put(&vanilla_game_events::BLOCK_OPEN, 10);
    put(&vanilla_game_events::BLOCK_ACTIVATE, 10);
    put(&vanilla_game_events::BLOCK_ATTACH, 10);
    put(&vanilla_game_events::PRIME_FUSE, 10);
    put(&vanilla_game_events::NOTE_BLOCK_PLAY, 10);
    put(&vanilla_game_events::BLOCK_CHANGE, 11);
    put(&vanilla_game_events::BLOCK_DESTROY, 12);
    put(&vanilla_game_events::FLUID_PICKUP, 12);
    put(&vanilla_game_events::BLOCK_PLACE, 13);
    put(&vanilla_game_events::FLUID_PLACE, 13);
    put(&vanilla_game_events::ENTITY_PLACE, 14);
    put(&vanilla_game_events::LIGHTNING_STRIKE, 14);
    put(&vanilla_game_events::TELEPORT, 14);
    put(&vanilla_game_events::ENTITY_DIE, 15);
    put(&vanilla_game_events::EXPLODE, 15);

    for frequency in 1..=RESONANCE_EVENTS.len() as i32 {
        if let Some(event) = resonance_event_by_frequency(frequency) {
            put(event, frequency);
        }
    }

    frequencies
});

/// Vanilla `VibrationSystem.RESONANCE_EVENTS`.
static RESONANCE_EVENTS: [GameEventRef; 15] = [
    &vanilla_game_events::RESONATE_1,
    &vanilla_game_events::RESONATE_2,
    &vanilla_game_events::RESONATE_3,
    &vanilla_game_events::RESONATE_4,
    &vanilla_game_events::RESONATE_5,
    &vanilla_game_events::RESONATE_6,
    &vanilla_game_events::RESONATE_7,
    &vanilla_game_events::RESONATE_8,
    &vanilla_game_events::RESONATE_9,
    &vanilla_game_events::RESONATE_10,
    &vanilla_game_events::RESONATE_11,
    &vanilla_game_events::RESONATE_12,
    &vanilla_game_events::RESONATE_13,
    &vanilla_game_events::RESONATE_14,
    &vanilla_game_events::RESONATE_15,
];

/// Vanilla `VibrationSystem.getGameEventFrequency`.
///
/// A frequency of zero means the event is not a vibration at all, which is how a sensor
/// tells the difference between something it can hear and something it cannot.
#[must_use]
pub fn game_event_frequency(event: GameEventRef) -> i32 {
    event
        .try_id()
        .and_then(|id| VIBRATION_FREQUENCY_FOR_EVENT.get(id).copied())
        .unwrap_or(NO_VIBRATION_FREQUENCY)
}

/// Vanilla `VibrationSystem.getResonanceEventByFrequency`.
#[must_use]
pub fn resonance_event_by_frequency(vibration_frequency: i32) -> Option<GameEventRef> {
    usize::try_from(vibration_frequency)
        .ok()
        .and_then(|frequency| RESONANCE_EVENTS.get(frequency.checked_sub(1)?))
        .copied()
}

/// Vanilla `VibrationSystem.getRedstoneStrengthForDistance`.
///
/// Vanilla returns a bare `int` and only clamps the low end; the high end cannot exceed
/// fifteen for a non-negative distance, so returning a redstone power makes that explicit.
#[must_use]
pub fn redstone_strength_for_distance(distance: f32, listener_radius: i32) -> u8 {
    let power_scale = 15.0 / f64::from(listener_radius);
    let strength = 15 - (power_scale * f64::from(distance)).floor() as i32;
    strength.clamp(1, 15) as u8
}
