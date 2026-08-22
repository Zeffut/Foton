//! Local difficulty.
//!
//! Vanilla parity: `DifficultyInstance`. Mobs that scale with difficulty read
//! their strength from here rather than from the raw level setting, because
//! vanilla also folds in how long the world has run and how long players have
//! lingered in the chunk.

use steel_registry::vanilla_world_clocks;
use steel_utils::BlockPos;
use steel_utils::types::Difficulty;

use crate::world::World;

/// Ticks of world age that do not count toward difficulty yet.
///
/// Vanilla parity: `DifficultyInstance.DIFFICULTY_TIME_GLOBAL_OFFSET`.
const TIME_GLOBAL_OFFSET: f32 = -72_000.0;

/// World age at which the global term stops growing.
///
/// Vanilla parity: `DifficultyInstance.MAX_DIFFICULTY_TIME_GLOBAL`.
const MAX_TIME_GLOBAL: f32 = 1_440_000.0;

/// Chunk inhabited time at which the local term stops growing.
///
/// Vanilla parity: `DifficultyInstance.MAX_DIFFICULTY_TIME_LOCAL`.
const MAX_TIME_LOCAL: f32 = 3_600_000.0;

/// Share of the scale the world age can add.
const GLOBAL_TERM_WEIGHT: f32 = 0.25;

/// Scale a world starts at before any of the growing terms apply.
const BASE_SCALE: f32 = 0.75;

/// Weight of the inhabited-time term outside hard difficulty.
const LOCAL_TERM_WEIGHT: f32 = 0.75;

/// Factor easy difficulty applies to the whole local term.
const EASY_LOCAL_FACTOR: f32 = 0.5;

/// Effective difficulty below which the special multiplier stays at zero.
const SPECIAL_MULTIPLIER_FLOOR: f32 = 2.0;

/// Effective difficulty at which the special multiplier reaches one.
const SPECIAL_MULTIPLIER_CEILING: f32 = 4.0;

/// The difficulty in effect at one position.
///
/// Vanilla parity: `DifficultyInstance`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DifficultyInstance {
    base: Difficulty,
    effective_difficulty: f32,
}

impl DifficultyInstance {
    /// Computes the local difficulty from its four vanilla inputs.
    #[must_use]
    pub fn new(
        base: Difficulty,
        total_game_time: i64,
        local_game_time: i64,
        moon_brightness: f32,
    ) -> Self {
        Self {
            base,
            effective_difficulty: calculate_difficulty(
                base,
                total_game_time,
                local_game_time,
                moon_brightness,
            ),
        }
    }

    /// Returns the level difficulty setting this was scaled from.
    ///
    /// Vanilla parity: `DifficultyInstance.getDifficulty`.
    #[must_use]
    pub const fn difficulty(self) -> Difficulty {
        self.base
    }

    /// Returns the scaled difficulty mobs read.
    ///
    /// Vanilla parity: `DifficultyInstance.getEffectiveDifficulty`.
    #[must_use]
    pub const fn effective_difficulty(self) -> f32 {
        self.effective_difficulty
    }

    /// Returns whether the local difficulty has reached hard.
    ///
    /// Vanilla parity: `DifficultyInstance.isHard`.
    #[must_use]
    pub fn is_hard(self) -> bool {
        self.effective_difficulty >= f32::from(Difficulty::Hard as u8)
    }

    /// Returns whether the local difficulty is above `required_difficulty`.
    ///
    /// Vanilla parity: `DifficultyInstance.isHarderThan`.
    #[must_use]
    pub fn is_harder_than(self, required_difficulty: f32) -> bool {
        self.effective_difficulty > required_difficulty
    }

    /// Returns the 0-to-1 ramp mobs use for their rarer perks.
    ///
    /// Vanilla parity: `DifficultyInstance.getSpecialMultiplier`.
    #[must_use]
    pub fn special_multiplier(self) -> f32 {
        if self.effective_difficulty < SPECIAL_MULTIPLIER_FLOOR {
            0.0
        } else if self.effective_difficulty > SPECIAL_MULTIPLIER_CEILING {
            1.0
        } else {
            (self.effective_difficulty - SPECIAL_MULTIPLIER_FLOOR)
                / (SPECIAL_MULTIPLIER_CEILING - SPECIAL_MULTIPLIER_FLOOR)
        }
    }
}

/// Vanilla parity: `DifficultyInstance.calculateDifficulty`.
fn calculate_difficulty(
    base: Difficulty,
    total_game_time: i64,
    local_game_time: i64,
    moon_brightness: f32,
) -> f32 {
    if base == Difficulty::Peaceful {
        return 0.0;
    }

    let is_hard = base == Difficulty::Hard;
    let global_scale = ((total_game_time as f32 + TIME_GLOBAL_OFFSET) / MAX_TIME_GLOBAL)
        .clamp(0.0, 1.0)
        * GLOBAL_TERM_WEIGHT;

    let inhabited_weight = if is_hard { 1.0 } else { LOCAL_TERM_WEIGHT };
    let mut local_scale =
        (local_game_time as f32 / MAX_TIME_LOCAL).clamp(0.0, 1.0) * inhabited_weight;
    // A bright moon only counts for as much as the age of the world allows, so a
    // full moon on the first night changes nothing.
    local_scale += (moon_brightness * GLOBAL_TERM_WEIGHT).clamp(0.0, global_scale);
    if base == Difficulty::Easy {
        local_scale *= EASY_LOCAL_FACTOR;
    }

    f32::from(base as u8) * (BASE_SCALE + global_scale + local_scale)
}

impl World {
    /// Returns the difficulty in effect at `pos`.
    ///
    /// Vanilla parity: `ServerLevel.getCurrentDifficultyAt`, minus two inputs
    /// Steel does not track yet. Chunks carry no inhabited time, and the moon
    /// phase is not exposed as an environment attribute, so both are passed as
    /// zero. Neither can lower the result, so Steel is at worst slightly gentler
    /// than vanilla in a world players have lived in for a long time.
    #[must_use]
    pub fn get_current_difficulty_at(&self, _pos: BlockPos) -> DifficultyInstance {
        let clock_time = self
            .clock_total_ticks(&vanilla_world_clocks::OVERWORLD)
            .unwrap_or_default();
        DifficultyInstance::new(self.difficulty(), clock_time, 0, 0.0)
    }
}

#[cfg(test)]
mod tests {
    use steel_utils::types::Difficulty;

    use super::{DifficultyInstance, calculate_difficulty};

    /// Vanilla treats peaceful as no difficulty at all, whatever the world age.
    #[test]
    fn peaceful_is_always_zero() {
        let peaceful = calculate_difficulty(Difficulty::Peaceful, 10_000_000, 3_600_000, 1.0);
        assert!(peaceful.abs() < f32::EPSILON);
    }

    /// A fresh world sits at the base scale, so the `140 * (int) difficulty` of
    /// `Husk.doHurtTarget` yields one step rather than two.
    #[test]
    fn fresh_world_uses_the_base_scale() {
        let normal = calculate_difficulty(Difficulty::Normal, 0, 0, 0.0);
        assert!((normal - 1.5).abs() < 1e-5, "normal was {normal}");
        assert_eq!(normal as i32, 1);

        let hard = calculate_difficulty(Difficulty::Hard, 0, 0, 0.0);
        assert!((hard - 2.25).abs() < 1e-5, "hard was {hard}");
        assert_eq!(hard as i32, 2);
    }

    /// The world-age term only starts after the first hour, and it is capped.
    #[test]
    fn world_age_term_is_offset_and_capped() {
        let before_offset = calculate_difficulty(Difficulty::Normal, 72_000, 0, 0.0);
        assert!((before_offset - 1.5).abs() < 1e-5, "was {before_offset}");

        // 2 * (0.75 + 0.25), with no inhabited time.
        let capped = calculate_difficulty(Difficulty::Normal, i64::from(i32::MAX), 0, 0.0);
        assert!((capped - 2.0).abs() < 1e-5, "capped was {capped}");
    }

    /// Hard difficulty weighs inhabited time more heavily than the rest.
    #[test]
    fn inhabited_time_counts_more_on_hard() {
        // 2 * (0.75 + 0.75).
        let normal = calculate_difficulty(Difficulty::Normal, 0, 3_600_000, 0.0);
        assert!((normal - 3.0).abs() < 1e-5, "normal was {normal}");

        // 3 * (0.75 + 1.0).
        let hard = calculate_difficulty(Difficulty::Hard, 0, 3_600_000, 0.0);
        assert!((hard - 5.25).abs() < 1e-5, "hard was {hard}");
    }

    /// Easy halves the whole local term.
    #[test]
    fn easy_halves_the_local_term() {
        // 1 * (0.75 + 0 + 0.75 * 0.5).
        let easy = calculate_difficulty(Difficulty::Easy, 0, 3_600_000, 0.0);
        assert!((easy - 1.125).abs() < 1e-5, "easy was {easy}");
    }

    /// A full moon cannot outgrow the world-age term that bounds it.
    #[test]
    fn moon_brightness_is_bounded_by_the_world_age_term() {
        let young = calculate_difficulty(Difficulty::Normal, 0, 0, 1.0);
        assert!((young - 1.5).abs() < 1e-5, "young was {young}");

        // The moon adds min(0.25, 0.25) on top of the fully grown global term.
        let old = calculate_difficulty(Difficulty::Normal, i64::from(i32::MAX), 0, 1.0);
        assert!((old - 2.5).abs() < 1e-5, "old was {old}");
    }

    /// Vanilla parity: `DifficultyInstance.getSpecialMultiplier`.
    #[test]
    fn special_multiplier_ramps_between_two_and_four() {
        let instance = |effective_difficulty: f32| DifficultyInstance {
            base: Difficulty::Normal,
            effective_difficulty,
        };
        assert!(instance(1.9).special_multiplier().abs() < f32::EPSILON);
        assert!((instance(3.0).special_multiplier() - 0.5).abs() < 1e-5);
        assert!((instance(5.0).special_multiplier() - 1.0).abs() < f32::EPSILON);
    }

    /// Vanilla parity: `DifficultyInstance.isHard`, which reads the scaled value
    /// rather than the setting, so a brand new hard world is not yet hard.
    #[test]
    fn is_hard_tracks_the_effective_value_not_the_setting() {
        let fresh = DifficultyInstance::new(Difficulty::Hard, 0, 0, 0.0);
        assert!(!fresh.is_hard(), "2.25 is below the hard threshold of 3");
        assert!(fresh.is_harder_than(2.0));

        let lived_in =
            DifficultyInstance::new(Difficulty::Hard, i64::from(i32::MAX), 3_600_000, 1.0);
        assert!(lived_in.is_hard());
    }
}
