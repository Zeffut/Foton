//! The six states a trial spawner moves through.
//!
//! Vanilla parity:
//! `net.minecraft.world.level.block.entity.trialspawner.TrialSpawnerState`.
//!
//! The enum itself is a block-state property and already lives in
//! `steel-registry`; what vanilla attaches to its constants lives here, because
//! the interesting half of it needs a level.

use steel_registry::blocks::properties::TrialSpawnerState;

/// Vanilla parity: `TrialSpawnerState.DELAY_BEFORE_EJECT_AFTER_KILLING_LAST_MOB`.
pub const DELAY_BEFORE_EJECT_AFTER_KILLING_LAST_MOB: f32 = 40.0;

/// Vanilla parity: `TrialSpawnerState.TIME_BETWEEN_EACH_EJECTION`, which is
/// `Mth.floor(30.0F)`.
pub const TIME_BETWEEN_EACH_EJECTION: f32 = 30.0;

/// Vanilla parity: `TrialSpawnerState.SpinningMob.NONE`.
const SPIN_NONE: f64 = -1.0;
/// Vanilla parity: `TrialSpawnerState.SpinningMob.SLOW`.
const SPIN_SLOW: f64 = 200.0;
/// Vanilla parity: `TrialSpawnerState.SpinningMob.FAST`.
const SPIN_FAST: f64 = 1000.0;

/// The per-state constants vanilla holds in the enum's fields.
pub trait TrialSpawnerStateExt {
    /// Vanilla parity: `TrialSpawnerState.lightLevel`.
    fn light_level(&self) -> i32;

    /// Vanilla parity: `TrialSpawnerState.spinningMobSpeed`.
    fn spinning_mob_speed(&self) -> f64;

    /// Vanilla parity: `TrialSpawnerState.hasSpinningMob`.
    fn has_spinning_mob(&self) -> bool;

    /// Vanilla parity: `TrialSpawnerState.isCapableOfSpawning`.
    fn is_capable_of_spawning(&self) -> bool;
}

impl TrialSpawnerStateExt for TrialSpawnerState {
    fn light_level(&self) -> i32 {
        match self {
            Self::Inactive | Self::Cooldown => 0,
            Self::WaitingForPlayers => 4,
            Self::Active | Self::WaitingForRewardEjection | Self::EjectingReward => 8,
        }
    }

    fn spinning_mob_speed(&self) -> f64 {
        match self {
            Self::WaitingForPlayers => SPIN_SLOW,
            Self::Active => SPIN_FAST,
            _ => SPIN_NONE,
        }
    }

    fn has_spinning_mob(&self) -> bool {
        self.spinning_mob_speed() >= 0.0
    }

    fn is_capable_of_spawning(&self) -> bool {
        matches!(self, Self::WaitingForPlayers | Self::Active)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The light level is what a trial chamber is lit by, and the spinning-mob
    /// flag is what decides whether the block entity keeps a display entity at
    /// all. Both are per-state constants with no other source, so a state added
    /// or reordered later has to be looked at here.
    #[test]
    fn only_the_two_spawning_states_spin_a_mob() {
        for state in [
            TrialSpawnerState::WaitingForPlayers,
            TrialSpawnerState::Active,
        ] {
            assert!(state.has_spinning_mob(), "{state:?} should spin a mob");
            assert!(state.is_capable_of_spawning(), "{state:?} should spawn");
        }
        for state in [
            TrialSpawnerState::Inactive,
            TrialSpawnerState::WaitingForRewardEjection,
            TrialSpawnerState::EjectingReward,
            TrialSpawnerState::Cooldown,
        ] {
            assert!(!state.has_spinning_mob(), "{state:?} should not spin a mob");
            assert!(
                !state.is_capable_of_spawning(),
                "{state:?} should not spawn"
            );
        }
    }
}
