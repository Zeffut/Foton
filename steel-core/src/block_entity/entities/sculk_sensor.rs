//! Sculk sensor block-entity storage.
//!
//! Vanilla parity: `SculkSensorBlockEntity` plus its `CalibratedSculkSensorBlockEntity`
//! subclass. The two differ only in their `VibrationSystem.User` -- listener radius and
//! the frequency filter read off the calibrating redstone signal -- and Steel has no
//! vibration system, so one storage type serves both here.
//!
//! Not implemented: `VibrationSystem.Data`. Steel has game events and game-event
//! listeners but no vibration layer on top of them (no `VibrationSystem`, no
//! `VibrationSelector`, no `getGameEventFrequency`), so the vanilla `listener` compound
//! has nothing to deserialize into. It is carried through load/save untouched instead of
//! being dropped, so a world written by vanilla keeps its in-flight vibration.

use std::sync::Weak;

use simdnbt::borrow::{BaseNbtCompound as BorrowedNbtCompound, NbtCompound as NbtCompoundView};
use simdnbt::owned::NbtCompound;
use steel_registry::block_entity_type::BlockEntityTypeRef;
use steel_registry::vanilla_block_entity_types;
use steel_utils::{BlockPos, BlockStateId, DowncastType, DowncastTypeKey, locks::SyncMutex};

use crate::block_entity::{BlockEntity, BlockEntityBase};
use crate::world::World;

/// Vanilla `SculkSensorBlockEntity.DEFAULT_LAST_VIBRATION_FREQUENCY`.
const DEFAULT_LAST_VIBRATION_FREQUENCY: i32 = 0;

struct SculkSensorState {
    last_vibration_frequency: i32,
    listener: Option<NbtCompound>,
}

/// Vanilla `SculkSensorBlockEntity`.
///
/// Holds the frequency a comparator reads off an active sensor.
pub struct SculkSensorBlockEntity {
    base: BlockEntityBase,
    state: SyncMutex<SculkSensorState>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `SculkSensorBlockEntity`.
unsafe impl DowncastType for SculkSensorBlockEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:block_entity/sculk_sensor");
}

impl SculkSensorBlockEntity {
    /// Creates storage for a plain sculk sensor.
    #[must_use]
    pub fn new(world: Weak<World>, pos: BlockPos, state: BlockStateId) -> Self {
        Self::with_type(&vanilla_block_entity_types::SCULK_SENSOR, world, pos, state)
    }

    /// Creates storage for a calibrated sculk sensor.
    ///
    /// Vanilla parity: `CalibratedSculkSensorBlockEntity`, which only replaces the
    /// vibration user. With no vibration system the two hold the same fields, so this
    /// differs from [`Self::new`] by registered type alone.
    #[must_use]
    pub fn new_calibrated(world: Weak<World>, pos: BlockPos, state: BlockStateId) -> Self {
        Self::with_type(
            &vanilla_block_entity_types::CALIBRATED_SCULK_SENSOR,
            world,
            pos,
            state,
        )
    }

    fn with_type(
        block_entity_type: BlockEntityTypeRef,
        world: Weak<World>,
        pos: BlockPos,
        state: BlockStateId,
    ) -> Self {
        Self {
            base: BlockEntityBase::new(block_entity_type, world, pos, state),
            state: SyncMutex::new(SculkSensorState {
                last_vibration_frequency: DEFAULT_LAST_VIBRATION_FREQUENCY,
                listener: None,
            }),
        }
    }

    /// Returns vanilla `SculkSensorBlockEntity.getLastVibrationFrequency`.
    #[must_use]
    pub fn last_vibration_frequency(&self) -> i32 {
        self.state.lock().last_vibration_frequency
    }

    /// Runs vanilla `SculkSensorBlockEntity.setLastVibrationFrequency`.
    pub fn set_last_vibration_frequency(&self, last_vibration_frequency: i32) {
        self.state.lock().last_vibration_frequency = last_vibration_frequency;
    }
}

impl BlockEntity for SculkSensorBlockEntity {
    fn base(&self) -> &BlockEntityBase {
        &self.base
    }

    fn load_additional(&self, nbt: &BorrowedNbtCompound<'_>) {
        let nbt: NbtCompoundView<'_, '_> = nbt.into();
        let mut state = self.state.lock();
        state.last_vibration_frequency = nbt
            .int("last_vibration_frequency")
            .unwrap_or(DEFAULT_LAST_VIBRATION_FREQUENCY);
        state.listener = nbt.compound("listener").map(|listener| listener.to_owned());
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        let state = self.state.lock();
        nbt.insert("last_vibration_frequency", state.last_vibration_frequency);
        if let Some(listener) = state.listener.clone() {
            nbt.insert("listener", listener);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use simdnbt::borrow::read_compound as read_borrowed_compound;
    use steel_registry::{init_vanilla_registry, vanilla_blocks};

    use super::*;

    fn sensor() -> SculkSensorBlockEntity {
        init_vanilla_registry();
        SculkSensorBlockEntity::new(
            Weak::new(),
            BlockPos::new(3, 12, -7),
            vanilla_blocks::SCULK_SENSOR.default_state(),
        )
    }

    fn reborrow(nbt: &NbtCompound) -> Vec<u8> {
        let mut bytes = Vec::new();
        nbt.write(&mut bytes);
        bytes
    }

    /// A comparator reads the frequency out of this block entity, so it has to
    /// survive a save/load round trip or every sculk-sensor comparator clock
    /// would reset itself whenever the chunk unloaded.
    #[test]
    fn the_last_frequency_survives_a_save_and_load() {
        let saved = sensor();
        saved.set_last_vibration_frequency(11);
        let mut nbt = NbtCompound::new();
        saved.save_additional(&mut nbt);
        assert_eq!(nbt.int("last_vibration_frequency"), Some(11));

        let bytes = reborrow(&nbt);
        let borrowed =
            read_borrowed_compound(&mut Cursor::new(bytes.as_slice())).expect("test NBT reborrows");
        let loaded = sensor();
        loaded.load_additional(&borrowed);
        assert_eq!(loaded.last_vibration_frequency(), 11);
    }

    /// Steel cannot interpret vanilla's vibration listener, but it must not
    /// delete it either: a sensor loaded from a vanilla world and saved again
    /// would otherwise silently lose the vibration already on its way to it.
    #[test]
    fn an_unreadable_vibration_listener_is_written_back_unchanged() {
        let mut listener = NbtCompound::new();
        listener.insert("event_delay", 7_i32);
        let mut disk = NbtCompound::new();
        disk.insert("last_vibration_frequency", 3_i32);
        disk.insert("listener", listener);

        let bytes = reborrow(&disk);
        let borrowed =
            read_borrowed_compound(&mut Cursor::new(bytes.as_slice())).expect("test NBT reborrows");
        let loaded = sensor();
        loaded.load_additional(&borrowed);

        let mut written = NbtCompound::new();
        loaded.save_additional(&mut written);
        assert_eq!(
            written
                .compound("listener")
                .and_then(|listener| listener.int("event_delay")),
            Some(7)
        );
    }

    /// A sensor placed by a player has no NBT at all; vanilla starts it at
    /// frequency zero, which is what makes a fresh sensor read as zero on a
    /// comparator rather than inheriting whatever the last one measured.
    #[test]
    fn a_sensor_with_no_stored_data_starts_at_frequency_zero() {
        let nbt = NbtCompound::new();
        let bytes = reborrow(&nbt);
        let borrowed =
            read_borrowed_compound(&mut Cursor::new(bytes.as_slice())).expect("test NBT reborrows");
        let loaded = sensor();
        loaded.set_last_vibration_frequency(9);
        loaded.load_additional(&borrowed);
        assert_eq!(loaded.last_vibration_frequency(), 0);
    }
}
