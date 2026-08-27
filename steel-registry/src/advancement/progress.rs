//! How far one player has got through one advancement.
//!
//! Vanilla parity: `AdvancementProgress` and `CriterionProgress`. This lives
//! next to the definitions rather than in `steel-core` because the protocol
//! crate has to encode it, and the protocol crate cannot see the game.

use std::io::{Result, Write};

use steel_utils::{
    codec::VarInt,
    serial::{PrefixedWrite as _, WriteTo},
};

use super::AdvancementRequirements;

/// When one criterion was met, if it was.
///
/// Vanilla parity: `CriterionProgress`. Vanilla holds an `Instant`; the wire
/// form is epoch milliseconds and so is Steel's save form, so the millisecond
/// count is the whole state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CriterionProgress {
    obtained_epoch_millis: Option<i64>,
}

impl CriterionProgress {
    /// A criterion that has not been met.
    pub const NOT_OBTAINED: Self = Self {
        obtained_epoch_millis: None,
    };

    /// A criterion met at the given moment.
    #[must_use]
    pub const fn obtained_at(epoch_millis: i64) -> Self {
        Self {
            obtained_epoch_millis: Some(epoch_millis),
        }
    }

    /// Whether the criterion has been met.
    #[must_use]
    pub const fn is_done(&self) -> bool {
        self.obtained_epoch_millis.is_some()
    }

    /// When it was met, in epoch milliseconds.
    #[must_use]
    pub const fn obtained(&self) -> Option<i64> {
        self.obtained_epoch_millis
    }

    /// Marks the criterion met at `epoch_millis`.
    ///
    /// Vanilla parity: `CriterionProgress.grant`.
    pub const fn grant(&mut self, epoch_millis: i64) {
        self.obtained_epoch_millis = Some(epoch_millis);
    }

    /// Marks the criterion unmet.
    ///
    /// Vanilla parity: `CriterionProgress.revoke`.
    pub const fn revoke(&mut self) {
        self.obtained_epoch_millis = None;
    }
}

impl WriteTo for CriterionProgress {
    fn write(&self, writer: &mut impl Write) -> Result<()> {
        // Vanilla: `writeNullable(obtained, writeInstant)`, and `writeInstant`
        // is a plain big-endian long of `toEpochMilli`.
        self.obtained_epoch_millis.write(writer)
    }
}

/// How far a player has got through one advancement.
///
/// Vanilla parity: `AdvancementProgress`. The criterion names borrow from the
/// generated definition, so a progress value is always tied to the advancement
/// it was started from, and a criterion that no longer exists cannot survive a
/// reload -- which is exactly what vanilla's `update` enforces at runtime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdvancementProgress {
    /// One entry per criterion the requirements mention, in declaration order.
    criteria: Vec<(&'static str, CriterionProgress)>,
    requirements: AdvancementRequirements,
}

impl AdvancementProgress {
    /// Progress that has not been attached to an advancement yet.
    ///
    /// Vanilla parity: `new AdvancementProgress()`, whose `requirements` start
    /// out empty and therefore report `isDone() == false`.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            criteria: Vec::new(),
            requirements: AdvancementRequirements::EMPTY,
        }
    }

    /// Attaches the progress to an advancement's requirements.
    ///
    /// Drops criteria the requirements no longer mention and adds an unmet
    /// entry for each one they do.
    ///
    /// Vanilla parity: `AdvancementProgress.update`.
    pub fn update(&mut self, requirements: AdvancementRequirements) {
        self.criteria
            .retain(|(name, _)| requirements.names().any(|known| known == *name));
        for name in requirements.names() {
            if !self.criteria.iter().any(|(known, _)| *known == name) {
                self.criteria.push((name, CriterionProgress::NOT_OBTAINED));
            }
        }
        self.requirements = requirements;
    }

    /// The requirements this progress was last attached to.
    #[must_use]
    pub const fn requirements(&self) -> AdvancementRequirements {
        self.requirements
    }

    /// Whether the advancement is earned.
    ///
    /// Vanilla parity: `AdvancementProgress.isDone`.
    #[must_use]
    pub fn is_done(&self) -> bool {
        self.requirements.test(|name| self.is_criterion_done(name))
    }

    /// Whether any criterion at all has been met.
    ///
    /// Vanilla parity: `AdvancementProgress.hasProgress`, which is what
    /// decides whether the progress is worth saving.
    #[must_use]
    pub fn has_progress(&self) -> bool {
        self.criteria.iter().any(|(_, progress)| progress.is_done())
    }

    /// Whether one named criterion has been met.
    #[must_use]
    pub fn is_criterion_done(&self, name: &str) -> bool {
        self.criterion(name).is_some_and(CriterionProgress::is_done)
    }

    /// The progress of one named criterion, if the advancement has it.
    #[must_use]
    pub fn criterion(&self, name: &str) -> Option<&CriterionProgress> {
        self.criteria
            .iter()
            .find(|(known, _)| *known == name)
            .map(|(_, progress)| progress)
    }

    /// Every criterion and its progress, in declaration order.
    pub fn criteria(&self) -> impl Iterator<Item = (&'static str, CriterionProgress)> + '_ {
        self.criteria.iter().copied()
    }

    /// Marks a criterion met, returning whether that changed anything.
    ///
    /// Vanilla parity: `AdvancementProgress.grantProgress`, which refuses
    /// unknown names and re-grants.
    pub fn grant(&mut self, name: &str, epoch_millis: i64) -> bool {
        let Some((_, progress)) = self.criteria.iter_mut().find(|(known, _)| *known == name) else {
            return false;
        };
        if progress.is_done() {
            return false;
        }
        progress.grant(epoch_millis);
        true
    }

    /// Marks a criterion unmet, returning whether that changed anything.
    ///
    /// Vanilla parity: `AdvancementProgress.revokeProgress`.
    pub fn revoke(&mut self, name: &str) -> bool {
        let Some((_, progress)) = self.criteria.iter_mut().find(|(known, _)| *known == name) else {
            return false;
        };
        if !progress.is_done() {
            return false;
        }
        progress.revoke();
        true
    }

    /// The earliest moment any criterion was met.
    ///
    /// Vanilla parity: `AdvancementProgress.getFirstProgressDate`, which is
    /// what orders a save file so older advancements load first.
    #[must_use]
    pub fn first_progress_date(&self) -> Option<i64> {
        self.criteria
            .iter()
            .filter_map(|(_, progress)| progress.obtained())
            .min()
    }
}

impl Default for AdvancementProgress {
    fn default() -> Self {
        Self::new()
    }
}

impl WriteTo for AdvancementProgress {
    fn write(&self, writer: &mut impl Write) -> Result<()> {
        // Vanilla: `writeMap(criteria, writeUtf, CriterionProgress::write)`.
        // The requirements are not sent; the client rebuilds them from the
        // advancement it already received.
        VarInt(i32::try_from(self.criteria.len()).unwrap_or(i32::MAX)).write(writer)?;
        for (name, progress) in &self.criteria {
            name.to_string().write_prefixed::<VarInt>(writer)?;
            progress.write(writer)?;
        }
        Ok(())
    }
}
