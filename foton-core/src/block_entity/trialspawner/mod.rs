//! The trial spawner's machinery.
//!
//! Vanilla parity: the
//! `net.minecraft.world.level.block.entity.trialspawner` package.

mod player_detector;
mod spawner;
mod state;
mod state_data;

pub use player_detector::PlayerDetector;
pub use spawner::{FlameParticle, FullConfig, TrialSpawner, TrialSpawnerStateAccessor};
pub use state::{
    DELAY_BEFORE_EJECT_AFTER_KILLING_LAST_MOB, TIME_BETWEEN_EACH_EJECTION, TrialSpawnerStateExt,
};
pub use state_data::TrialSpawnerStateData;
