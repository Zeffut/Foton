//! Sculk sensor block-entity storage and its vibration listener.
//!
//! Vanilla parity: `SculkSensorBlockEntity` plus its `CalibratedSculkSensorBlockEntity`
//! subclass. The two differ only in their `VibrationSystem.User` -- the listener radius and
//! the frequency filter read off the calibrating redstone signal -- so one storage type
//! carries both here and the user branches on which one it belongs to.

use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Arc, Weak};

use foton_registry::block_entity_type::BlockEntityTypeRef;
use foton_registry::game_events::GameEventRef;
use foton_registry::{vanilla_block_entity_types, vanilla_game_events};
use foton_utils::{BlockPos, BlockStateId, Downcast as _, DowncastType, DowncastTypeKey};
use simdnbt::borrow::{BaseNbtCompound as BorrowedNbtCompound, NbtCompound as NbtCompoundView};
use simdnbt::owned::NbtCompound;

use crate::behavior::blocks::{
    CalibratedSculkSensorBlock, activate_sculk_sensor, can_activate_sculk_sensor,
};
use crate::block_entity::{BlockEntity, BlockEntityBase};
use crate::entity::Entity;
use crate::world::World;
use crate::world::game_event::vibrations::{
    VIBRATION_DATA_TAG, VibrationListener, VibrationPositionSource, VibrationUser,
    game_event_frequency, redstone_strength_for_distance,
};
use crate::world::game_event::{GameEventContext, SharedGameEventListener};

/// Vanilla `SculkSensorBlockEntity.DEFAULT_LAST_VIBRATION_FREQUENCY`.
const DEFAULT_LAST_VIBRATION_FREQUENCY: i32 = 0;
/// Vanilla `SculkSensorBlockEntity.VibrationUser.LISTENER_RANGE`.
const LISTENER_RANGE: i32 = 8;
/// Vanilla `CalibratedSculkSensorBlockEntity.VibrationUser.getListenerRadius`.
const CALIBRATED_LISTENER_RANGE: i32 = 16;

/// Vanilla `SculkSensorBlockEntity`.
///
/// Holds the frequency a comparator reads off an active sensor, and the vibration listener
/// that sets it.
pub struct SculkSensorBlockEntity {
    base: BlockEntityBase,
    last_vibration_frequency: AtomicI32,
    listener: Arc<VibrationListener>,
}

// SAFETY: This key is owned by Foton and uniquely identifies `SculkSensorBlockEntity`.
unsafe impl DowncastType for SculkSensorBlockEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:block_entity/sculk_sensor");
}

impl SculkSensorBlockEntity {
    /// Creates storage for a plain sculk sensor.
    #[must_use]
    pub fn new(world: Weak<World>, pos: BlockPos, state: BlockStateId) -> Self {
        Self::with_type(
            &vanilla_block_entity_types::SCULK_SENSOR,
            false,
            world,
            pos,
            state,
        )
    }

    /// Creates storage for a calibrated sculk sensor.
    ///
    /// Vanilla parity: `CalibratedSculkSensorBlockEntity`, which only replaces the vibration
    /// user.
    #[must_use]
    pub fn new_calibrated(world: Weak<World>, pos: BlockPos, state: BlockStateId) -> Self {
        Self::with_type(
            &vanilla_block_entity_types::CALIBRATED_SCULK_SENSOR,
            true,
            world,
            pos,
            state,
        )
    }

    fn with_type(
        block_entity_type: BlockEntityTypeRef,
        calibrated: bool,
        world: Weak<World>,
        pos: BlockPos,
        state: BlockStateId,
    ) -> Self {
        let user = Arc::new(SculkSensorVibrationUser {
            world: Weak::clone(&world),
            block_pos: pos,
            calibrated,
        });
        Self {
            base: BlockEntityBase::new(block_entity_type, world, pos, state),
            last_vibration_frequency: AtomicI32::new(DEFAULT_LAST_VIBRATION_FREQUENCY),
            listener: Arc::new(VibrationListener::new(user)),
        }
    }

    /// Returns vanilla `SculkSensorBlockEntity.getLastVibrationFrequency`.
    #[must_use]
    pub fn last_vibration_frequency(&self) -> i32 {
        self.last_vibration_frequency.load(Ordering::Relaxed)
    }

    /// Runs vanilla `SculkSensorBlockEntity.setLastVibrationFrequency`.
    pub fn set_last_vibration_frequency(&self, last_vibration_frequency: i32) {
        self.last_vibration_frequency
            .store(last_vibration_frequency, Ordering::Relaxed);
    }

    /// Returns vanilla `SculkSensorBlockEntity.getListener`.
    #[must_use]
    pub const fn listener(&self) -> &Arc<VibrationListener> {
        &self.listener
    }
}

impl BlockEntity for SculkSensorBlockEntity {
    fn base(&self) -> &BlockEntityBase {
        &self.base
    }

    /// Vanilla `SculkSensorBlock.getTicker`, which is `VibrationSystem.Ticker.tick`.
    fn tick(&self, world: &Arc<World>) {
        self.listener.tick(world);
    }

    fn load_additional(&self, nbt: &BorrowedNbtCompound<'_>) {
        let nbt: NbtCompoundView<'_, '_> = nbt.into();
        self.set_last_vibration_frequency(
            nbt.int("last_vibration_frequency")
                .unwrap_or(DEFAULT_LAST_VIBRATION_FREQUENCY),
        );
        self.listener
            .load(nbt.compound(VIBRATION_DATA_TAG).as_ref());
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        nbt.insert("last_vibration_frequency", self.last_vibration_frequency());
        let mut listener = NbtCompound::new();
        self.listener.save(&mut listener);
        nbt.insert(VIBRATION_DATA_TAG, listener);
    }

    fn game_event_listener(&self) -> Option<SharedGameEventListener> {
        Some(Arc::clone(&self.listener) as SharedGameEventListener)
    }
}

/// Vanilla `SculkSensorBlockEntity.VibrationUser` and its calibrated override.
///
/// The user cannot hold its block entity without a reference cycle, so it holds the position
/// and reads the live block entity back out of the world -- the same route the comparator
/// output takes.
struct SculkSensorVibrationUser {
    world: Weak<World>,
    block_pos: BlockPos,
    calibrated: bool,
}

impl SculkSensorVibrationUser {
    fn with_sensor<R>(&self, action: impl FnOnce(&SculkSensorBlockEntity) -> R) -> Option<R> {
        let world = self.world.upgrade()?;
        let block_entity = world.get_block_entity(self.block_pos)?;
        let sensor = block_entity.downcast_ref::<SculkSensorBlockEntity>()?;
        Some(action(sensor))
    }
}

impl VibrationUser for SculkSensorVibrationUser {
    fn listener_radius(&self) -> i32 {
        if self.calibrated {
            CALIBRATED_LISTENER_RANGE
        } else {
            LISTENER_RANGE
        }
    }

    fn position_source(&self) -> VibrationPositionSource {
        VibrationPositionSource::Block(self.block_pos)
    }

    fn can_trigger_avoid_vibration(&self) -> bool {
        true
    }

    fn requires_adjacent_chunks_to_be_ticking(&self) -> bool {
        true
    }

    /// Vanilla `SculkSensorBlockEntity.VibrationUser.canReceiveVibration`, plus the
    /// calibrated sensor's frequency filter.
    fn can_receive_vibration(
        &self,
        world: &Arc<World>,
        pos: BlockPos,
        event: GameEventRef,
        _context: &GameEventContext<'_>,
    ) -> bool {
        let state = world.get_block_state(self.block_pos);

        if self.calibrated {
            // Vanilla `CalibratedSculkSensorBlockEntity.VibrationUser.canReceiveVibration`:
            // an uncalibrated face hears everything, a calibrated one hears one frequency.
            let comparison_type =
                CalibratedSculkSensorBlock::back_signal(world, self.block_pos, state);
            if comparison_type != 0 && game_event_frequency(event) != comparison_type {
                return false;
            }
        }

        // A sensor does not hear itself being placed or broken.
        if pos == self.block_pos
            && (event.key == vanilla_game_events::BLOCK_DESTROY.key
                || event.key == vanilla_game_events::BLOCK_PLACE.key)
        {
            return false;
        }

        game_event_frequency(event) != 0 && can_activate_sculk_sensor(state)
    }

    /// Vanilla `SculkSensorBlockEntity.VibrationUser.onReceiveVibration`.
    fn on_receive_vibration(
        &self,
        world: &Arc<World>,
        _pos: BlockPos,
        event: GameEventRef,
        source_entity: Option<&dyn Entity>,
        _projectile_owner: Option<&dyn Entity>,
        receiving_distance: f32,
    ) {
        let state = world.get_block_state(self.block_pos);
        if !can_activate_sculk_sensor(state) {
            return;
        }

        let event_frequency = game_event_frequency(event);
        self.with_sensor(|sensor| sensor.set_last_vibration_frequency(event_frequency));
        let calculated_power =
            redstone_strength_for_distance(receiving_distance, self.listener_radius());
        activate_sculk_sensor(
            source_entity,
            world,
            self.block_pos,
            state,
            calculated_power,
            event_frequency,
        );
    }

    fn on_data_changed(&self) {
        self.with_sensor(BlockEntity::set_changed);
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use foton_registry::{init_vanilla_registry, vanilla_blocks};
    use simdnbt::borrow::read_compound as read_borrowed_compound;

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
