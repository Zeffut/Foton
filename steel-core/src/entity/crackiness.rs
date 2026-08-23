//! How visibly broken a repairable piece of gear looks.
//!
//! Vanilla parity: `Crackiness`. Wolf armor and the iron golem both change
//! appearance in steps rather than continuously, and the server needs the step
//! boundaries because crossing one is what plays the cracking sound.

use steel_registry::item_stack::ItemStack;

/// One of the four appearances vanilla draws.
///
/// Vanilla parity: `Crackiness.Level`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrackinessLevel {
    /// Unmarked.
    None,
    /// The first cracks show.
    Low,
    /// Visibly damaged.
    Medium,
    /// About to give.
    High,
}

/// The three remaining-durability fractions that separate the levels.
///
/// Vanilla parity: the `Crackiness` constructor.
#[derive(Debug, Clone, Copy)]
pub struct Crackiness {
    fraction_low: f32,
    fraction_medium: f32,
    fraction_high: f32,
}

impl Crackiness {
    /// Where an iron golem starts to show cracks.
    ///
    /// Vanilla parity: `Crackiness.GOLEM`.
    pub const GOLEM: Self = Self::new(0.75, 0.5, 0.25);

    /// Where wolf armor starts to show cracks.
    ///
    /// Vanilla parity: `Crackiness.WOLF_ARMOR`.
    pub const WOLF_ARMOR: Self = Self::new(0.95, 0.69, 0.32);

    const fn new(fraction_low: f32, fraction_medium: f32, fraction_high: f32) -> Self {
        Self {
            fraction_low,
            fraction_medium,
            fraction_high,
        }
    }

    /// Returns the level for a remaining-durability fraction.
    ///
    /// Vanilla parity: `Crackiness.byFraction`.
    #[must_use]
    pub fn by_fraction(self, fraction: f32) -> CrackinessLevel {
        if fraction < self.fraction_high {
            CrackinessLevel::High
        } else if fraction < self.fraction_medium {
            CrackinessLevel::Medium
        } else if fraction < self.fraction_low {
            CrackinessLevel::Low
        } else {
            CrackinessLevel::None
        }
    }

    /// Returns the level for a damage value against a maximum.
    ///
    /// Vanilla parity: `Crackiness.byDamage(int, int)`.
    #[must_use]
    pub fn by_damage(self, damage: i32, max_damage: i32) -> CrackinessLevel {
        if max_damage <= 0 {
            return CrackinessLevel::None;
        }
        self.by_fraction((max_damage - damage) as f32 / max_damage as f32)
    }

    /// Returns the level a stack currently shows.
    ///
    /// Vanilla parity: `Crackiness.byDamage(ItemStack)`.
    #[must_use]
    pub fn by_stack(self, stack: &ItemStack) -> CrackinessLevel {
        if !stack.is_damageable_item() {
            return CrackinessLevel::None;
        }
        self.by_damage(stack.get_damage_value(), stack.get_max_damage())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The boundaries are what the cracking sound fires on, so an off-by-one
    /// here is silently audible rather than a compile error.
    #[test]
    fn wolf_armor_crosses_its_three_thresholds_at_the_vanilla_fractions() {
        let crackiness = Crackiness::WOLF_ARMOR;

        assert_eq!(crackiness.by_fraction(1.0), CrackinessLevel::None);
        assert_eq!(crackiness.by_fraction(0.94), CrackinessLevel::Low);
        assert_eq!(crackiness.by_fraction(0.68), CrackinessLevel::Medium);
        assert_eq!(crackiness.by_fraction(0.31), CrackinessLevel::High);
    }

    #[test]
    fn a_full_durability_item_reads_as_uncracked() {
        assert_eq!(
            Crackiness::WOLF_ARMOR.by_damage(0, 64),
            CrackinessLevel::None
        );
        assert_eq!(
            Crackiness::WOLF_ARMOR.by_damage(64, 64),
            CrackinessLevel::High
        );
    }
}
