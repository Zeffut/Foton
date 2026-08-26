//! Picks one vibration per tick out of everything a listener heard that tick.

use simdnbt::borrow::NbtCompound as NbtCompoundView;
use simdnbt::owned::NbtCompound;

use super::VibrationInfo;
use super::game_event_frequency;

/// Vanilla `VibrationSelector`.
///
/// A listener can hear several game events in the same tick. Vanilla keeps the best
/// candidate for one tick and only commits it on the next one, which is what makes a sensor
/// react to the nearest, loudest thing rather than to whichever event was dispatched first.
#[derive(Default, Clone)]
pub struct VibrationSelector {
    current_vibration_data: Option<(VibrationInfo, i64)>,
}

impl VibrationSelector {
    /// Vanilla `VibrationSelector.addCandidate`.
    pub fn add_candidate(&mut self, new_vibration: VibrationInfo, tick_time: i64) {
        if self.should_replace_vibration(&new_vibration, tick_time) {
            self.current_vibration_data = Some((new_vibration, tick_time));
        }
    }

    /// Vanilla `VibrationSelector.shouldReplaceVibration`.
    fn should_replace_vibration(&self, new_vibration: &VibrationInfo, tick_time: i64) -> bool {
        let Some((previous_vibration, previous_tick)) = &self.current_vibration_data else {
            return true;
        };
        if tick_time != *previous_tick {
            return false;
        }
        if new_vibration.distance() < previous_vibration.distance() {
            return true;
        }
        if new_vibration.distance() > previous_vibration.distance() {
            return false;
        }
        game_event_frequency(new_vibration.game_event())
            > game_event_frequency(previous_vibration.game_event())
    }

    /// Vanilla `VibrationSelector.chosenCandidate`.
    #[must_use]
    pub fn chosen_candidate(&self, time: i64) -> Option<&VibrationInfo> {
        let (vibration, tick) = self.current_vibration_data.as_ref()?;
        (*tick < time).then_some(vibration)
    }

    /// Vanilla `VibrationSelector.startOver`.
    pub fn start_over(&mut self) {
        self.current_vibration_data = None;
    }

    /// Writes vanilla's `VibrationSelector.CODEC` shape.
    pub fn save(&self, nbt: &mut NbtCompound) {
        match &self.current_vibration_data {
            Some((vibration, tick)) => {
                let mut event = NbtCompound::new();
                vibration.save(&mut event);
                nbt.insert("event", event);
                nbt.insert("tick", *tick);
            }
            // Vanilla always writes the tick; only the event is optional.
            None => nbt.insert("tick", -1_i64),
        }
    }

    /// Reads vanilla's `VibrationSelector.CODEC` shape.
    #[must_use]
    pub fn load(nbt: &NbtCompoundView<'_, '_>) -> Self {
        let tick = nbt.long("tick").unwrap_or(-1);
        Self {
            current_vibration_data: nbt
                .compound("event")
                .and_then(|event| VibrationInfo::load(&event))
                .map(|vibration| (vibration, tick)),
        }
    }
}
