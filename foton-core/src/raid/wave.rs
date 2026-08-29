//! What a raid wave is made of.
//!
//! Vanilla parity: `Raid.RaiderType`, the private enum that holds one row per
//! mob and one column per wave. Everything about the shape of a raid is in
//! this table: why wave one is four pillagers and nothing else, why the
//! ravager arrives on wave three, why the witches all turn up at once on wave
//! four.

use foton_registry::entity_type::EntityTypeRef;
use foton_registry::vanilla_entities;
use foton_utils::types::Difficulty;
use rand::RngExt as _;

/// One kind of mob a raid wave can contain.
///
/// Vanilla parity: `Raid.RaiderType`. The declaration order matters: vanilla
/// iterates `RaiderType.VALUES` and spawns each kind in turn, and the first
/// mob spawned that can carry a banner becomes the wave's captain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RaiderType {
    /// Vanilla parity: `RaiderType.VINDICATOR`.
    Vindicator,
    /// Vanilla parity: `RaiderType.EVOKER`.
    Evoker,
    /// Vanilla parity: `RaiderType.PILLAGER`.
    Pillager,
    /// Vanilla parity: `RaiderType.WITCH`.
    Witch,
    /// Vanilla parity: `RaiderType.RAVAGER`.
    Ravager,
}

impl RaiderType {
    /// Every kind, in vanilla's declaration order.
    ///
    /// Vanilla parity: `Raid.RaiderType.VALUES`.
    pub const VALUES: [Self; 5] = [
        Self::Vindicator,
        Self::Evoker,
        Self::Pillager,
        Self::Witch,
        Self::Ravager,
    ];

    /// Returns the entity this kind spawns.
    #[must_use]
    pub const fn entity_type(self) -> EntityTypeRef {
        match self {
            Self::Vindicator => &vanilla_entities::VINDICATOR,
            Self::Evoker => &vanilla_entities::EVOKER,
            Self::Pillager => &vanilla_entities::PILLAGER,
            Self::Witch => &vanilla_entities::WITCH,
            Self::Ravager => &vanilla_entities::RAVAGER,
        }
    }

    /// Returns how many of this kind each wave brings, indexed by wave number.
    ///
    /// Vanilla parity: the `spawnsPerWaveBeforeBonus` array of each
    /// `RaiderType`. Index zero is unused -- waves are numbered from one -- and
    /// index seven is what a hard-difficulty final wave reads.
    #[must_use]
    pub const fn spawns_per_wave_before_bonus(self) -> &'static [i32; 8] {
        match self {
            Self::Vindicator => &[0, 0, 2, 0, 1, 4, 2, 5],
            Self::Evoker => &[0, 0, 0, 0, 0, 1, 1, 2],
            Self::Pillager => &[0, 4, 3, 3, 4, 4, 4, 2],
            Self::Witch => &[0, 0, 0, 0, 3, 0, 0, 1],
            Self::Ravager => &[0, 0, 0, 1, 0, 1, 0, 2],
        }
    }

    /// Returns how many of this kind a wave brings before the difficulty bonus.
    ///
    /// Vanilla parity: `Raid.getDefaultNumSpawns`. A bonus wave reads the row
    /// at the raid's group count rather than at its own wave number, which is
    /// what makes it a repeat of the final wave rather than a ninth column.
    #[must_use]
    pub fn default_num_spawns(self, wave: i32, num_groups: i32, is_bonus_wave: bool) -> i32 {
        let index = if is_bonus_wave { num_groups } else { wave };
        let table = self.spawns_per_wave_before_bonus();
        usize::try_from(index)
            .ok()
            .and_then(|index| table.get(index))
            .copied()
            .unwrap_or(0)
    }

    /// Returns the extra mobs difficulty adds to this kind's share of a wave.
    ///
    /// Vanilla parity: `Raid.getPotentialBonusSpawns`. Vanilla rolls
    /// `nextInt(bonus + 1)`, so the number it names is a ceiling and not a
    /// guarantee -- except for the easy-difficulty pillager and vindicator,
    /// whose `nextInt(2)` ceiling is itself rolled and can come out zero twice
    /// over.
    #[must_use]
    pub fn potential_bonus_spawns(
        self,
        wave: i32,
        difficulty: Difficulty,
        is_bonus_wave: bool,
        rng: &mut impl rand::Rng,
    ) -> i32 {
        let is_easy = difficulty == Difficulty::Easy;
        let is_normal = difficulty == Difficulty::Normal;
        let bonus_spawns = match self {
            Self::Vindicator | Self::Pillager => {
                if is_easy {
                    rng.random_range(0..2)
                } else if is_normal {
                    1
                } else {
                    2
                }
            }
            Self::Evoker => return 0,
            Self::Witch => {
                if is_easy || wave <= 2 || wave == 4 {
                    return 0;
                }
                1
            }
            Self::Ravager => i32::from(!is_easy && is_bonus_wave),
        };

        if bonus_spawns > 0 {
            rng.random_range(0..bonus_spawns + 1)
        } else {
            0
        }
    }
}

/// Returns how many waves a raid at this difficulty runs.
///
/// Vanilla parity: `Raid.getNumGroups`.
#[must_use]
pub const fn num_groups(difficulty: Difficulty) -> i32 {
    match difficulty {
        Difficulty::Peaceful => 0,
        Difficulty::Easy => 3,
        Difficulty::Normal => 5,
        Difficulty::Hard => 7,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Vanilla indexes the wave table with the raid's group count on a bonus
    /// wave and with the wave number otherwise. Reading the wave number in both
    /// cases is the natural mistake, and on hard difficulty it would index one
    /// past the final wave and spawn nothing at all.
    #[test]
    fn a_bonus_wave_repeats_the_final_wave_rather_than_reading_past_it() {
        assert_eq!(
            RaiderType::Vindicator.default_num_spawns(8, 7, true),
            RaiderType::Vindicator.default_num_spawns(7, 7, false)
        );
        assert_eq!(RaiderType::Vindicator.default_num_spawns(8, 7, true), 5);
    }

    /// Wave one is four pillagers and nothing else; that is the shape a player
    /// recognizes as the start of a raid.
    #[test]
    fn the_first_wave_is_pillagers_alone() {
        for raider_type in RaiderType::VALUES {
            let expected = i32::from(raider_type == RaiderType::Pillager) * 4;
            assert_eq!(raider_type.default_num_spawns(1, 5, false), expected);
        }
    }

    /// The witch's bonus is the only one with a wave-number condition, and it
    /// reads as a list of the waves it *does* apply to unless read carefully.
    #[test]
    fn the_witch_takes_no_bonus_before_wave_three_or_on_wave_four() {
        let mut rng = rand::rng();
        for wave in [1, 2, 4] {
            assert_eq!(
                RaiderType::Witch.potential_bonus_spawns(wave, Difficulty::Hard, false, &mut rng),
                0
            );
        }
        // Waves three, five and up roll `nextInt(2)`, so the ceiling is one.
        for wave in [3, 5, 6, 7] {
            assert!(
                RaiderType::Witch.potential_bonus_spawns(wave, Difficulty::Hard, false, &mut rng)
                    <= 1
            );
        }
    }

    /// A ravager bonus only exists on a bonus wave above easy, and confusing
    /// that with "any wave" would double the ravagers of a normal raid.
    #[test]
    fn the_ravager_takes_a_bonus_only_on_a_bonus_wave() {
        let mut rng = rand::rng();
        assert_eq!(
            RaiderType::Ravager.potential_bonus_spawns(5, Difficulty::Hard, false, &mut rng),
            0
        );
        assert_eq!(
            RaiderType::Ravager.potential_bonus_spawns(5, Difficulty::Easy, true, &mut rng),
            0
        );
        assert!(
            RaiderType::Ravager.potential_bonus_spawns(5, Difficulty::Hard, true, &mut rng) <= 1
        );
    }

    #[test]
    fn peaceful_runs_no_waves_at_all() {
        assert_eq!(num_groups(Difficulty::Peaceful), 0);
        assert_eq!(num_groups(Difficulty::Easy), 3);
        assert_eq!(num_groups(Difficulty::Normal), 5);
        assert_eq!(num_groups(Difficulty::Hard), 7);
    }
}
