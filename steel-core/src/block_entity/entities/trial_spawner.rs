//! The trial spawner's block entity.
//!
//! Vanilla parity:
//! `net.minecraft.world.level.block.entity.TrialSpawnerBlockEntity`.

use std::sync::{Arc, Weak};

use simdnbt::borrow::BaseNbtCompound as BorrowedNbtCompound;
use simdnbt::owned::NbtCompound;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::blocks::properties::{BlockStateProperties, TrialSpawnerState};
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::vanilla_block_entity_types;
use steel_utils::types::UpdateFlags;
use steel_utils::{BlockPos, BlockStateId, DowncastType, DowncastTypeKey};

use crate::block_entity::trialspawner::{TrialSpawner, TrialSpawnerStateAccessor};
use crate::block_entity::{BlockEntity, BlockEntityBase};
use crate::world::World;
use crate::world::base_spawner::Spawner;

/// Vanilla `TrialSpawnerBlockEntity`.
pub struct TrialSpawnerBlockEntity {
    base: BlockEntityBase,
    trial_spawner: TrialSpawner,
}

// SAFETY: This key is owned by Steel and uniquely identifies `TrialSpawnerBlockEntity`.
unsafe impl DowncastType for TrialSpawnerBlockEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:block_entity/trial_spawner");
}

impl TrialSpawnerBlockEntity {
    /// Creates the storage behind one trial spawner block.
    #[must_use]
    pub fn new(world: Weak<World>, pos: BlockPos, state: BlockStateId) -> Self {
        Self {
            base: BlockEntityBase::new(
                &vanilla_block_entity_types::TRIAL_SPAWNER,
                world,
                pos,
                state,
            ),
            trial_spawner: TrialSpawner::new(),
        }
    }

    /// Returns the spawner behind this block.
    ///
    /// Vanilla parity: `TrialSpawnerBlockEntity.getTrialSpawner`.
    #[must_use]
    pub const fn trial_spawner(&self) -> &TrialSpawner {
        &self.trial_spawner
    }
}

impl TrialSpawnerStateAccessor for TrialSpawnerBlockEntity {
    /// Vanilla parity: `TrialSpawnerBlockEntity.getState`, which falls back to
    /// `INACTIVE` for a block that has no such property -- which happens while
    /// a block entity outlives the block it belonged to.
    fn trial_spawner_state(&self) -> TrialSpawnerState {
        self.get_block_state()
            .try_get_value(&BlockStateProperties::TRIAL_SPAWNER_STATE)
            .unwrap_or(TrialSpawnerState::Inactive)
    }

    /// Vanilla parity: `TrialSpawnerBlockEntity.setState`.
    fn set_trial_spawner_state(&self, world: &Arc<World>, state: TrialSpawnerState) {
        self.set_changed();
        let pos = self.get_block_pos();
        let updated = world
            .get_block_state(pos)
            .set_value(&BlockStateProperties::TRIAL_SPAWNER_STATE, state);
        world.set_block(pos, updated, UpdateFlags::UPDATE_ALL);
    }

    /// Vanilla parity: `TrialSpawnerBlockEntity.markUpdated`.
    fn mark_trial_spawner_updated(&self) {
        self.set_changed();
        if let Some(world) = self.get_level() {
            world.send_block_updated(self.get_block_pos());
        }
    }
}

impl Spawner for TrialSpawnerBlockEntity {
    /// Vanilla parity: `TrialSpawnerBlockEntity.setEntityId`.
    fn set_spawner_entity_id(&self, entity_type: EntityTypeRef) {
        let Some(world) = self.get_level() else {
            log::warn!("trial spawner retargeted with no level");
            return;
        };
        self.trial_spawner
            .override_entity_to_spawn(self, &world, &entity_type.key);
        self.set_changed();
    }
}

impl BlockEntity for TrialSpawnerBlockEntity {
    fn base(&self) -> &BlockEntityBase {
        &self.base
    }

    fn load_additional(&self, nbt: &BorrowedNbtCompound<'_>) {
        self.trial_spawner.load(&nbt.into());
        if self.get_level().is_some() {
            self.mark_trial_spawner_updated();
        }
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.trial_spawner.save(nbt);
    }

    /// Vanilla parity: `TrialSpawnerBlockEntity.getUpdateTag`, which sends only
    /// the live state -- never the configuration.
    fn get_update_tag(&self) -> Option<NbtCompound> {
        Some(
            self.trial_spawner
                .with_data(|data| data.update_tag(self.trial_spawner_state())),
        )
    }

    fn tick(&self, world: &Arc<World>) {
        let is_ominous = self
            .get_block_state()
            .try_get_value(&BlockStateProperties::OMINOUS)
            .unwrap_or(false);
        self.trial_spawner
            .tick_server(self, world, self.get_block_pos(), is_ominous);
    }
}
