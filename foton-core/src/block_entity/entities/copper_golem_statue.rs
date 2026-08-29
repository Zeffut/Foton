//! Copper golem statue block entity.
//!
//! Vanilla parity: `CopperGolemStatueBlockEntity`. A fully oxidized copper
//! golem freezes into one of these, and the statue is what remembers the name
//! the golem was carrying so that scraping it awake gives the same golem back.

use std::sync::Weak;

use foton_registry::data_components::vanilla_components::CUSTOM_NAME;
use foton_registry::vanilla_block_entity_types;
use foton_utils::{BlockPos, BlockStateId, DowncastType, DowncastTypeKey};
use simdnbt::borrow::BaseNbtCompound as BorrowedNbtCompound;
use simdnbt::owned::NbtCompound;
use text_components::TextComponent;

use crate::block_entity::{BlockEntity, BlockEntityBase};
use crate::world::World;

/// A copper golem statue.
///
/// The name lives in the block entity's stored component map rather than in a
/// field of its own: vanilla's `createStatue` writes straight into
/// `setComponents`, which is why a statue's name survives being mined without
/// any `collectImplicitComponents` override.
pub struct CopperGolemStatueBlockEntity {
    base: BlockEntityBase,
}

// SAFETY: This key is owned by Foton and uniquely identifies
// `CopperGolemStatueBlockEntity`.
unsafe impl DowncastType for CopperGolemStatueBlockEntity {
    const TYPE_KEY: DowncastTypeKey =
        DowncastTypeKey::new("foton:block_entity/copper_golem_statue");
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
        }
    }

    /// Returns the name the frozen golem was carrying.
    ///
    /// Vanilla parity: the `components().get(CUSTOM_NAME)` of `removeStatue`.
    #[must_use]
    pub fn custom_name(&self) -> Option<TextComponent> {
        self.base.components().get(CUSTOM_NAME)
    }

    /// Records the name the frozen golem was carrying.
    ///
    /// Vanilla parity: `CopperGolemStatueBlockEntity.createStatue`, which
    /// copies the golem's `CUSTOM_NAME` component into the stored map.
    pub fn create_statue(&self, custom_name: Option<TextComponent>) {
        let mut components = self.base.components();
        components.set(CUSTOM_NAME, custom_name);
        self.base.set_components(components);
        self.set_changed();
    }
}

impl BlockEntity for CopperGolemStatueBlockEntity {
    fn base(&self) -> &BlockEntityBase {
        &self.base
    }

    /// The statue keeps nothing of its own: the name it remembers rides in the
    /// stored component map, which the base load and save already carry.
    fn load_additional(&self, _nbt: &BorrowedNbtCompound<'_>) {}

    fn save_additional(&self, _nbt: &mut NbtCompound) {}
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use foton_registry::data_components::vanilla_components::CUSTOM_NAME;
    use foton_registry::item_stack::ItemStack;
    use foton_registry::{init_vanilla_registry, vanilla_blocks};
    use foton_utils::types::UpdateFlags;
    use foton_utils::{ChunkPos, Downcast as _};
    use simdnbt::borrow::read_compound as read_borrowed_compound;

    use super::*;
    use crate::behavior::{BlockLootContext, init_behaviors};
    use crate::block_entity::init_block_entities;
    use crate::test_support::{fresh_test_world, insert_ready_full_chunk};

    /// The statue is the one block entity that keeps its name in the stored
    /// component map instead of a field of its own, so it is also the proof
    /// that the map reaches both the chunk file and the loot roll.
    #[test]
    fn a_named_statue_keeps_its_name_through_a_save_and_a_break() {
        init_vanilla_registry();
        init_behaviors();
        init_block_entities();
        let world = fresh_test_world("copper_golem_statue_name");
        let pos = BlockPos::new(8, 64, 8);
        insert_ready_full_chunk(&world, ChunkPos::from_block_pos(pos));
        let state = vanilla_blocks::COPPER_GOLEM_STATUE.default_state();
        assert!(world.set_block(pos, state, UpdateFlags::UPDATE_ALL));

        let block_entity = world
            .get_block_entity(pos)
            .unwrap_or_else(|| panic!("a placed statue should have a block entity"));
        let frozen_golem = block_entity
            .downcast_ref::<CopperGolemStatueBlockEntity>()
            .unwrap_or_else(|| panic!("the statue's block entity should be a statue"));
        frozen_golem.create_statue(Some(TextComponent::plain("Rusty")));

        // The chunk writer stores `saveWithoutMetadata`, which is where the
        // component map rides.
        let saved = block_entity.save_without_metadata();
        let mut bytes = Vec::new();
        saved.write(&mut bytes);
        let borrowed =
            read_borrowed_compound(&mut Cursor::new(bytes.as_slice())).expect("statue nbt");
        let reloaded = CopperGolemStatueBlockEntity::new(Weak::new(), pos, state);
        reloaded.load_with_components(&borrowed);
        assert_eq!(
            reloaded.custom_name(),
            Some(TextComponent::plain("Rusty")),
            "the stored components have to come back off disk"
        );

        let drops = BlockLootContext::new(&world, pos)
            .with_tool(&ItemStack::empty())
            .with_block_entity(Some(&block_entity))
            .get_drops(state);
        assert_eq!(drops.len(), 1, "a statue drops exactly one statue");
        assert_eq!(
            drops[0].get(CUSTOM_NAME).cloned(),
            Some(TextComponent::plain("Rusty")),
            "`copy_components` reads the stored map through `collectComponents`"
        );
    }

    /// The control: a statue nobody named has no name to give.
    #[test]
    fn a_plain_statue_drops_unnamed() {
        init_vanilla_registry();
        init_behaviors();
        init_block_entities();
        let world = fresh_test_world("copper_golem_statue_plain");
        let pos = BlockPos::new(8, 64, 8);
        insert_ready_full_chunk(&world, ChunkPos::from_block_pos(pos));
        let state = vanilla_blocks::COPPER_GOLEM_STATUE.default_state();
        assert!(world.set_block(pos, state, UpdateFlags::UPDATE_ALL));
        let block_entity = world
            .get_block_entity(pos)
            .unwrap_or_else(|| panic!("a placed statue should have a block entity"));

        let drops = BlockLootContext::new(&world, pos)
            .with_tool(&ItemStack::empty())
            .with_block_entity(Some(&block_entity))
            .get_drops(state);
        assert_eq!(drops.len(), 1);
        assert!(drops[0].get(CUSTOM_NAME).is_none());
    }
}
