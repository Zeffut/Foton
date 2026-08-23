//! Copper golem statue block entity.
//!
//! Vanilla parity: `CopperGolemStatueBlockEntity`. A fully oxidized copper
//! golem freezes into one of these, and the statue is what remembers the name
//! the golem was carrying so that scraping it awake gives the same golem back.

use std::sync::Weak;

use simdnbt::borrow::{BaseNbtCompound as BorrowedNbtCompound, NbtCompound as NbtCompoundView};
use simdnbt::owned::NbtCompound;
use steel_registry::vanilla_block_entity_types;
use steel_utils::locks::SyncMutex;
use steel_utils::{BlockPos, BlockStateId, DowncastType, DowncastTypeKey};
use text_components::TextComponent;

use crate::block_entity::{BlockEntity, BlockEntityBase};
use crate::world::World;

/// A copper golem statue.
pub struct CopperGolemStatueBlockEntity {
    base: BlockEntityBase,
    /// The name the golem was carrying when it seized up.
    custom_name: SyncMutex<Option<TextComponent>>,
}

// SAFETY: This key is owned by Steel and uniquely identifies
// `CopperGolemStatueBlockEntity`.
unsafe impl DowncastType for CopperGolemStatueBlockEntity {
    const TYPE_KEY: DowncastTypeKey =
        DowncastTypeKey::new("steel:block_entity/copper_golem_statue");
}

impl CopperGolemStatueBlockEntity {
    /// Creates a copper golem statue block entity.
    #[must_use]
    pub fn new(level: Weak<World>, pos: BlockPos, state: BlockStateId) -> Self {
        Self {
            base: BlockEntityBase::new(
                &vanilla_block_entity_types::COPPER_GOLEM_STATUE,
                level,
                pos,
                state,
            ),
            custom_name: SyncMutex::new(None),
        }
    }

    /// Returns the name the frozen golem was carrying.
    #[must_use]
    pub fn custom_name(&self) -> Option<TextComponent> {
        self.custom_name.lock().clone()
    }

    /// Records the name the frozen golem was carrying.
    ///
    /// Vanilla parity: `CopperGolemStatueBlockEntity.createStatue`, which
    /// copies the golem's `CUSTOM_NAME` component onto the block entity.
    pub fn create_statue(&self, custom_name: Option<TextComponent>) {
        *self.custom_name.lock() = custom_name;
        self.set_changed();
    }
}

impl BlockEntity for CopperGolemStatueBlockEntity {
    fn base(&self) -> &BlockEntityBase {
        &self.base
    }

    fn load_additional(&self, nbt: &BorrowedNbtCompound<'_>) {
        let nbt: NbtCompoundView<'_, '_> = nbt.into();
        *self.custom_name.lock() = nbt
            .get("custom_name")
            .map(|tag| tag.to_owned())
            .as_ref()
            .and_then(TextComponent::from_nbt);
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        if let Some(name) = self.custom_name.lock().as_ref() {
            nbt.insert("custom_name", name.to_codec_nbt());
        }
    }

    /// Vanilla parity: `CopperGolemStatueBlockEntity.getUpdatePacket`, which
    /// sends the whole saved compound so the name plate shows up with the chunk.
    fn get_update_tag(&self) -> Option<NbtCompound> {
        let mut nbt = NbtCompound::new();
        self.save_additional(&mut nbt);
        Some(nbt)
    }
}
