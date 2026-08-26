//! The saved state of one vibration listener.

use simdnbt::borrow::NbtCompound as NbtCompoundView;
use simdnbt::owned::NbtCompound;

use super::{VibrationInfo, VibrationSelector};

/// Vanilla `VibrationSystem.Data.NBT_TAG_KEY`.
pub const VIBRATION_DATA_TAG: &str = "listener";

/// Vanilla `VibrationSystem.Data`.
#[derive(Default)]
pub struct VibrationData {
    current_vibration: Option<VibrationInfo>,
    travel_time_in_ticks: i32,
    selection_strategy: VibrationSelector,
    reload_vibration_particle: bool,
}

impl VibrationData {
    /// Vanilla `VibrationSystem.Data.getSelectionStrategy`.
    pub const fn selection_strategy(&mut self) -> &mut VibrationSelector {
        &mut self.selection_strategy
    }

    /// Vanilla `VibrationSystem.Data.getCurrentVibration`.
    #[must_use]
    pub const fn current_vibration(&self) -> Option<&VibrationInfo> {
        self.current_vibration.as_ref()
    }

    /// Vanilla `VibrationSystem.Data.setCurrentVibration`.
    pub fn set_current_vibration(&mut self, current_vibration: Option<VibrationInfo>) {
        self.current_vibration = current_vibration;
    }

    /// Vanilla `VibrationSystem.Data.getTravelTimeInTicks`.
    #[must_use]
    pub const fn travel_time_in_ticks(&self) -> i32 {
        self.travel_time_in_ticks
    }

    /// Vanilla `VibrationSystem.Data.setTravelTimeInTicks`.
    pub const fn set_travel_time_in_ticks(&mut self, travel_time_in_ticks: i32) {
        self.travel_time_in_ticks = travel_time_in_ticks;
    }

    /// Vanilla `VibrationSystem.Data.decrementTravelTime`.
    pub const fn decrement_travel_time(&mut self) {
        self.travel_time_in_ticks = if self.travel_time_in_ticks > 0 {
            self.travel_time_in_ticks - 1
        } else {
            0
        };
    }

    /// Vanilla `VibrationSystem.Data.shouldReloadVibrationParticle`.
    #[must_use]
    pub const fn should_reload_vibration_particle(&self) -> bool {
        self.reload_vibration_particle
    }

    /// Vanilla `VibrationSystem.Data.setReloadVibrationParticle`.
    pub const fn set_reload_vibration_particle(&mut self, reload_vibration_particle: bool) {
        self.reload_vibration_particle = reload_vibration_particle;
    }

    /// Writes vanilla's `VibrationSystem.Data.CODEC` shape.
    pub fn save(&self, nbt: &mut NbtCompound) {
        if let Some(current_vibration) = &self.current_vibration {
            let mut event = NbtCompound::new();
            current_vibration.save(&mut event);
            nbt.insert("event", event);
        }
        let mut selector = NbtCompound::new();
        self.selection_strategy.save(&mut selector);
        nbt.insert("selector", selector);
        nbt.insert("event_delay", self.travel_time_in_ticks);
    }

    /// Reads vanilla's `VibrationSystem.Data.CODEC` shape.
    ///
    /// Vanilla's codec constructs the loaded data with `reloadVibrationParticle` set, so a
    /// vibration that was already travelling when the chunk unloaded gets its particle sent
    /// again to whoever is watching now.
    #[must_use]
    pub fn load(nbt: &NbtCompoundView<'_, '_>) -> Self {
        Self {
            current_vibration: nbt
                .compound("event")
                .and_then(|event| VibrationInfo::load(&event)),
            travel_time_in_ticks: nbt.int("event_delay").unwrap_or(0).max(0),
            selection_strategy: nbt
                .compound("selector")
                .map(|selector| VibrationSelector::load(&selector))
                .unwrap_or_default(),
            reload_vibration_particle: true,
        }
    }
}
