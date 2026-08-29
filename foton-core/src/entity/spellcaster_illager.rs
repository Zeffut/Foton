//! The illagers that cast.
//!
//! Vanilla parity: `SpellcasterIllager`. Two mobs share it, the evoker and the
//! illusioner, and between them they have five spells. The class itself is the
//! rhythm all five run on: a warmup during which the caster stands still with
//! its arms up, a moment where the spell fires, and a cooldown before the next
//! one. A spell is therefore three numbers and a body, which is exactly the
//! shape [`crate::entity::ai::goal::SpellcasterUseSpellBase`] takes.

use foton_registry::sound_event::SoundEventRef;
use foton_utils::locks::SyncMutex;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;

use crate::entity::abstract_illager::AbstractIllager;

/// NBT key vanilla stores the remaining casting time under.
pub const TAG_SPELL_TICKS: &str = "SpellTicks";

/// One of the five illager spells.
///
/// Vanilla parity: `SpellcasterIllager.IllagerSpell`. The discriminants are the
/// wire values the client reads out of data index 17 to pick the particle
/// color, so they are not free to renumber.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i8)]
pub enum IllagerSpell {
    /// Not casting.
    None = 0,
    /// The evoker's vex summon.
    SummonVex = 1,
    /// The evoker's fang line.
    Fangs = 2,
    /// The evoker's sheep recolor.
    Wololo = 3,
    /// The illusioner's mirror images.
    Disappear = 4,
    /// The illusioner's blindness.
    Blindness = 5,
}

impl IllagerSpell {
    /// Returns the wire value the client reads.
    #[must_use]
    pub const fn id(self) -> i8 {
        self as i8
    }
}

/// How long a caster has left to cast, and what.
///
/// Vanilla keeps these two on the mob; Foton groups them so an entity holds one
/// field, the way it holds a [`crate::entity::MobBase`].
#[derive(Debug)]
pub struct SpellcasterState {
    /// Ticks left of the current cast, counted down in the custom AI step.
    spell_casting_tick_count: SyncMutex<i32>,
    /// The spell being cast, which the client reads for the particle color.
    current_spell: SyncMutex<IllagerSpell>,
}

impl SpellcasterState {
    /// Creates the state of a caster that is not casting.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            spell_casting_tick_count: SyncMutex::new(0),
            current_spell: SyncMutex::new(IllagerSpell::None),
        }
    }
}

impl Default for SpellcasterState {
    fn default() -> Self {
        Self::new()
    }
}

/// An illager that casts spells.
///
/// Vanilla parity: the `SpellcasterIllager` class.
pub trait SpellcasterIllager: AbstractIllager {
    /// Returns this caster's spell state.
    fn spellcaster_state(&self) -> &SpellcasterState;

    /// Sets the synced spell id the client colors its particles from.
    ///
    /// Vanilla parity: the `DATA_SPELL_CASTING_ID` half of `setIsCastingSpell`.
    fn set_synced_spell_id(&self, id: i8);

    /// Returns the sound this caster makes when a spell goes off.
    ///
    /// Vanilla parity: `getCastingSoundEvent`.
    fn casting_sound_event(&self) -> SoundEventRef;

    /// Returns how many ticks of the current cast are left.
    ///
    /// Vanilla parity: `getSpellCastingTime`.
    fn spell_casting_time(&self) -> i32 {
        *self.spellcaster_state().spell_casting_tick_count.lock()
    }

    /// Sets how many ticks the current cast runs for.
    fn set_spell_casting_time(&self, ticks: i32) {
        *self.spellcaster_state().spell_casting_tick_count.lock() = ticks;
    }

    /// Returns whether this caster is mid-spell.
    ///
    /// Vanilla parity: the server branch of `isCastingSpell`.
    fn is_casting_spell(&self) -> bool {
        self.spell_casting_time() > 0
    }

    /// Returns the spell being cast.
    ///
    /// Vanilla parity: the server branch of `getCurrentSpell`.
    fn current_spell(&self) -> IllagerSpell {
        *self.spellcaster_state().current_spell.lock()
    }

    /// Starts showing `spell`.
    ///
    /// Vanilla parity: `setIsCastingSpell`.
    fn set_is_casting_spell(&self, spell: IllagerSpell) {
        *self.spellcaster_state().current_spell.lock() = spell;
        self.set_synced_spell_id(spell.id());
    }

    /// Counts the current cast down by one tick.
    ///
    /// Vanilla parity: the body of `SpellcasterIllager.customServerAiStep`.
    fn spellcaster_custom_server_ai_step(&self) {
        let mut remaining = self.spellcaster_state().spell_casting_tick_count.lock();
        if *remaining > 0 {
            *remaining -= 1;
        }
    }
}

/// Writes the remaining cast the way vanilla does.
///
/// Vanilla parity: `SpellcasterIllager.addAdditionalSaveData`.
pub fn write_spellcaster_state(caster: &dyn SpellcasterIllager, nbt: &mut NbtCompound) {
    nbt.insert(TAG_SPELL_TICKS, caster.spell_casting_time());
}

/// Reads the remaining cast the way vanilla does.
///
/// Vanilla parity: `SpellcasterIllager.readAdditionalSaveData`, which restores
/// the tick count but not the spell -- a caster reloaded mid-cast finishes the
/// timer with no spell showing, exactly as in vanilla.
pub fn read_spellcaster_state(
    caster: &dyn SpellcasterIllager,
    nbt: BorrowedNbtCompoundView<'_, '_>,
) {
    caster.set_spell_casting_time(nbt.int(TAG_SPELL_TICKS).unwrap_or(0));
}
