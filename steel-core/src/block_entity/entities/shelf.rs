//! Shelf block entity implementation.
//!
//! A shelf holds three items on display. Unlike a chest it has no menu: items
//! go on and come off one slot at a time by clicking the slot, or a whole row
//! at a time when the shelf is powered.

use std::{
    mem,
    sync::{Arc, Weak},
};

use simdnbt::ToNbtTag;
use simdnbt::borrow::{BaseNbtCompound as BorrowedNbtCompound, NbtCompound as NbtCompoundView};
use simdnbt::owned::{NbtCompound, NbtList, NbtTag};
use steel_registry::game_events::GameEventRef;
use steel_registry::item_stack::ItemStack;
use steel_registry::vanilla_block_entity_types;
use steel_utils::{BlockPos, BlockStateId, DowncastType, DowncastTypeKey, locks::SyncMutex};

use crate::block_entity::{BlockEntity, BlockEntityBase};
use crate::inventory::container::Container;
use crate::inventory::lock::{ContainerRef, SharedContainer};
use crate::world::World;
use crate::world::game_event::GameEventContext;

/// Vanilla `ShelfBlockEntity.MAX_ITEMS`.
pub const SHELF_SLOTS: usize = 3;

const ITEMS_NBT_KEY: &str = "Items";
const ITEM_SLOT_NBT_KEY: &str = "Slot";
const ALIGN_ITEMS_TO_BOTTOM_NBT_KEY: &str = "align_items_to_bottom";

struct ShelfContainer {
    items: Vec<ItemStack>,
    /// Vanilla `ShelfBlockEntity.alignItemsToBottom`, a render hint the server
    /// only stores and forwards.
    align_items_to_bottom: bool,
}

/// Three-slot display storage for a shelf.
pub struct ShelfBlockEntity {
    base: Arc<BlockEntityBase>,
    container: Arc<SyncMutex<ShelfContainer>>,
    container_ref: ContainerRef,
}

// SAFETY: This key is owned by Steel and uniquely identifies `ShelfBlockEntity`.
unsafe impl DowncastType for ShelfBlockEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:block_entity/shelf");
}

// SAFETY: This key is owned by Steel and uniquely identifies the independently
// lockable inventory data used by a shelf block entity.
unsafe impl DowncastType for ShelfContainer {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:container/shelf");
}

impl ShelfBlockEntity {
    /// Creates a shelf block entity.
    #[must_use]
    pub fn new(level: Weak<World>, pos: BlockPos, state: BlockStateId) -> Self {
        let base = Arc::new(BlockEntityBase::new(
            &vanilla_block_entity_types::SHELF,
            level,
            pos,
            state,
        ));
        let container = Arc::new(SyncMutex::new(ShelfContainer {
            items: vec![ItemStack::empty(); SHELF_SLOTS],
            align_items_to_bottom: false,
        }));
        let shared_container: SharedContainer = container.clone();

        Self {
            container_ref: ContainerRef::owned_by_block_entity(shared_container, Arc::clone(&base)),
            base,
            container,
        }
    }

    /// Vanilla `ShelfBlockEntity.swapItemNoUpdate`.
    ///
    /// Puts `held` on the shelf and hands back whatever was there, without
    /// telling anybody -- the caller decides which game event that was.
    #[must_use]
    pub fn swap_item_no_update(&self, slot: usize, held: ItemStack) -> ItemStack {
        let mut container = self.container.lock();
        let Some(stored) = container.items.get_mut(slot) else {
            return ItemStack::empty();
        };
        mem::replace(stored, held)
    }

    /// Returns a copy of the item on display in `slot`.
    #[must_use]
    pub fn item(&self, slot: usize) -> ItemStack {
        let container = self.container.lock();
        container
            .items
            .get(slot)
            .map_or_else(ItemStack::empty, Clone::clone)
    }

    /// Vanilla `ShelfBlockEntity.setChanged(Holder.Reference<GameEvent>)`.
    ///
    /// The shelf is the only block entity whose contents are visible without
    /// opening anything, so every change is pushed to nearby clients as well as
    /// marked dirty.
    pub fn set_changed_with_event(&self, event: Option<GameEventRef>) {
        BlockEntity::set_changed(self);

        let Some(world) = self.get_level() else {
            return;
        };
        let state = self.base.block_state();
        if let Some(event) = event {
            world.game_event(
                event,
                self.base.pos(),
                &GameEventContext::new(None, Some(state)),
            );
        }
        world.send_block_updated(self.base.pos());
    }

    fn write_contents(&self, nbt: &mut NbtCompound) {
        let container = self.container.lock();
        let mut items: Vec<NbtCompound> = Vec::new();
        for (slot, item) in container.items.iter().enumerate() {
            if item.is_empty() {
                continue;
            }
            if let NbtTag::Compound(mut item_nbt) = item.clone().to_nbt_tag() {
                item_nbt.insert(ITEM_SLOT_NBT_KEY, slot as i8);
                items.push(item_nbt);
            }
        }
        nbt.insert(ITEMS_NBT_KEY, NbtList::Compound(items));
        nbt.insert(
            ALIGN_ITEMS_TO_BOTTOM_NBT_KEY,
            container.align_items_to_bottom,
        );
    }
}

impl BlockEntity for ShelfBlockEntity {
    fn base(&self) -> &BlockEntityBase {
        &self.base
    }

    fn pre_remove_side_effects(&self, pos: BlockPos, _state: BlockStateId) {
        let items = {
            let mut container = self.container.lock();
            mem::replace(&mut container.items, vec![ItemStack::empty(); SHELF_SLOTS])
        };
        let Some(world) = self.get_level() else {
            return;
        };
        for item in items {
            world.drop_item_stack(pos, item);
        }
    }

    fn load_additional(&self, nbt: &BorrowedNbtCompound<'_>) {
        let nbt_view: NbtCompoundView<'_, '_> = nbt.into();
        let mut container = self.container.lock();
        container.items.fill(ItemStack::empty());

        if let Some(items_list) = nbt_view.list(ITEMS_NBT_KEY)
            && let Some(compounds) = items_list.compounds()
        {
            for compound in compounds {
                let Some(slot) = compound.byte(ITEM_SLOT_NBT_KEY) else {
                    continue;
                };
                let Ok(slot) = usize::try_from(slot) else {
                    continue;
                };
                if slot < SHELF_SLOTS
                    && let Some(item) = ItemStack::from_borrowed_compound(&compound)
                {
                    container.items[slot] = item;
                }
            }
        }

        container.align_items_to_bottom = nbt_view
            .byte(ALIGN_ITEMS_TO_BOTTOM_NBT_KEY)
            .is_some_and(|value| value != 0);
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.write_contents(nbt);
    }

    /// Shelves show their contents in the world, so the items travel with the
    /// chunk rather than waiting for somebody to open a menu.
    fn get_update_tag(&self) -> Option<NbtCompound> {
        let mut nbt = NbtCompound::new();
        self.write_contents(&mut nbt);
        Some(nbt)
    }

    fn container_ref(&self) -> Option<ContainerRef> {
        Some(self.container_ref.clone())
    }
}

impl Container for ShelfContainer {
    fn items(&self) -> &[ItemStack] {
        &self.items
    }

    fn items_mut(&mut self) -> &mut [ItemStack] {
        &mut self.items
    }

    fn get_container_size(&self) -> usize {
        SHELF_SLOTS
    }

    fn set_item(&mut self, slot: usize, mut stack: ItemStack) {
        if slot >= SHELF_SLOTS {
            return;
        }
        let max_stack_size = self.get_max_stack_size_for_item(&stack);
        if !stack.is_empty() && stack.count() > max_stack_size {
            stack.set_count(max_stack_size);
        }
        self.items[slot] = stack;
    }

    fn set_changed(&mut self) {}
}

#[cfg(test)]
mod tests {
    use steel_registry::{init_vanilla_registry, vanilla_blocks, vanilla_items};

    use super::*;

    fn test_shelf() -> ShelfBlockEntity {
        init_vanilla_registry();
        ShelfBlockEntity::new(
            Weak::new(),
            BlockPos::new(1, 2, 3),
            vanilla_blocks::OAK_SHELF.default_state(),
        )
    }

    #[test]
    fn swapping_a_slot_hands_back_exactly_what_was_on_display() {
        let shelf = test_shelf();
        let first = ItemStack::new(&vanilla_items::STONE);
        let second = ItemStack::new(&vanilla_items::DIAMOND);

        assert!(shelf.swap_item_no_update(0, first).is_empty());
        let displaced = shelf.swap_item_no_update(0, second.clone());
        assert!(displaced.is(&vanilla_items::STONE));
        assert!(shelf.item(0).is(&vanilla_items::DIAMOND));

        // Out-of-range slots are refused rather than panicking; the hit-slot
        // math is the only caller and it clamps, but a corrupt packet must not
        // take the server down.
        assert!(shelf.swap_item_no_update(SHELF_SLOTS, second).is_empty());
    }

    #[test]
    fn shelf_contents_survive_a_save_and_load_round_trip() {
        use std::io::Cursor;

        use simdnbt::borrow::read_compound as read_borrowed_compound;

        let shelf = test_shelf();
        let _ = shelf.swap_item_no_update(2, ItemStack::new(&vanilla_items::TORCH));

        let mut nbt = NbtCompound::new();
        shelf.save_additional(&mut nbt);
        let mut bytes = Vec::new();
        nbt.write(&mut bytes);

        let borrowed = read_borrowed_compound(&mut Cursor::new(bytes.as_slice()))
            .expect("shelf NBT should re-read");
        let restored = test_shelf();
        restored.load_additional(&borrowed);

        assert!(restored.item(2).is(&vanilla_items::TORCH));
        assert!(restored.item(0).is_empty());
    }
}
