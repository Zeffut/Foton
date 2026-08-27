//! NBT-preserving fallback block entity.

use std::sync::Weak;

use simdnbt::borrow::{BaseNbtCompound as BorrowedNbtCompound, NbtCompound as NbtCompoundView};
use simdnbt::owned::NbtCompound;
use steel_registry::block_entity_type::BlockEntityTypeRef;
use steel_utils::{BlockPos, BlockStateId, DowncastType, DowncastTypeKey, locks::SyncMutex};

use crate::block_entity::{BlockEntity, BlockEntityBase};
use crate::world::World;

struct RawBlockEntityState {
    data: NbtCompound,
}

/// Steel-specific fallback for block entity types whose runtime behavior is not implemented yet.
///
/// Vanilla has concrete classes for every block entity type. Steel uses this only to preserve
/// worldgen and disk NBT until the corresponding typed implementation is added.
pub struct RawBlockEntity {
    base: BlockEntityBase,
    state: SyncMutex<RawBlockEntityState>,
}

// SAFETY: This key identifies the Steel fallback implementation, independently
// of the Minecraft block-entity registry entry stored inside it.
unsafe impl DowncastType for RawBlockEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:block_entity/raw");
}

impl RawBlockEntity {
    /// Creates a raw block entity without additional NBT.
    #[must_use]
    pub fn new(
        block_entity_type: BlockEntityTypeRef,
        level: Weak<World>,
        pos: BlockPos,
        state: BlockStateId,
    ) -> Self {
        Self::with_data(block_entity_type, level, pos, state, NbtCompound::new())
    }

    /// Creates a raw block entity with already-owned additional NBT.
    #[must_use]
    pub fn with_data(
        block_entity_type: BlockEntityTypeRef,
        level: Weak<World>,
        pos: BlockPos,
        state: BlockStateId,
        data: NbtCompound,
    ) -> Self {
        Self {
            base: BlockEntityBase::new(block_entity_type, level, pos, state),
            state: SyncMutex::new(RawBlockEntityState { data }),
        }
    }
}

impl BlockEntity for RawBlockEntity {
    fn base(&self) -> &BlockEntityBase {
        &self.base
    }

    fn load_additional(&self, nbt: &BorrowedNbtCompound<'_>) {
        let nbt_view: NbtCompoundView<'_, '_> = nbt.into();
        self.state.lock().data = nbt_view.to_owned();
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        *nbt = self.state.lock().data.clone();
    }

    /// Hands back the preserved compound whole, `components` included.
    ///
    /// The generic implementation appends the block entity's own component map,
    /// which is always empty here: a raw entity is created straight from the
    /// stored NBT and never goes through `load_with_components`. Letting it
    /// replace the preserved field would quietly drop the components of every
    /// block entity type Steel has not implemented yet.
    fn save_without_metadata(&self) -> NbtCompound {
        self.save_custom_only()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Weak;

    use steel_registry::{init_vanilla_registry, vanilla_block_entity_types, vanilla_blocks};

    use super::*;

    #[test]
    fn full_metadata_replaces_stale_raw_metadata() {
        init_vanilla_registry();
        let mut data = NbtCompound::new();
        data.insert("id", "minecraft:chest");
        data.insert("x", 100_i32);
        data.insert("custom", 7_i32);
        let entity = RawBlockEntity::with_data(
            &vanilla_block_entity_types::BARREL,
            Weak::new(),
            BlockPos::new(2, 70, -4),
            vanilla_blocks::BARREL.default_state(),
            data,
        );

        let saved = entity.save_with_full_metadata();
        let custom = entity.save_custom_only();

        assert_eq!(
            saved.string("id").map(ToString::to_string),
            Some("minecraft:barrel".to_owned())
        );
        assert_eq!(saved.int("x"), Some(2));
        assert_eq!(saved.int("y"), Some(70));
        assert_eq!(saved.int("z"), Some(-4));
        assert_eq!(saved.int("custom"), Some(7));
        assert!(!custom.contains("id"));
        assert!(!custom.contains("x"));
        assert_eq!(custom.int("custom"), Some(7));
    }

    /// A raw entity exists to hand an unimplemented type's NBT back untouched.
    /// Its own component map is always empty -- nothing ever loads one -- so
    /// the payload the chunk writer stores has to keep the `components` that
    /// came in rather than the empty one.
    #[test]
    fn the_preserved_components_survive_the_chunk_payload() {
        init_vanilla_registry();
        let mut components = NbtCompound::new();
        components.insert("minecraft:custom_name", "\"Kept\"");
        let mut data = NbtCompound::new();
        data.insert("components", components);
        data.insert("custom", 7_i32);
        let entity = RawBlockEntity::with_data(
            &vanilla_block_entity_types::BARREL,
            Weak::new(),
            BlockPos::new(2, 70, -4),
            vanilla_blocks::BARREL.default_state(),
            data,
        );

        let saved = entity.save_without_metadata();
        assert_eq!(saved.int("custom"), Some(7));
        assert_eq!(
            saved
                .compound("components")
                .and_then(|components| components.string("minecraft:custom_name"))
                .map(ToString::to_string),
            Some("\"Kept\"".to_owned()),
            "the components of an unimplemented block entity must not be dropped"
        );
    }

    #[test]
    #[should_panic(expected = "invalid block entity minecraft:barrel state minecraft:stone")]
    fn constructor_rejects_a_type_state_mismatch() {
        init_vanilla_registry();
        let _ = RawBlockEntity::new(
            &vanilla_block_entity_types::BARREL,
            Weak::new(),
            BlockPos::new(2, 70, -4),
            vanilla_blocks::STONE.default_state(),
        );
    }
}
