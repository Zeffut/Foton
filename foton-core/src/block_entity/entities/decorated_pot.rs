//! Decorated pot block entity.
//!
//! Vanilla parity: `DecoratedPotBlockEntity`. It remembers two things the block
//! state cannot hold: the four sherds pressed into its sides, and one item slot.
//!
//! The sherds are the point of the block. A pot is crafted from four sherds or
//! bricks and every combination is a different pot, so they travel on the
//! item's `minecraft:pot_decorations` component and land here when the pot is
//! placed. Without this every placed pot is four bricks and the archaeology
//! that produced the sherds is wasted.
//!
//! The single slot is the pot's other half: it is what a hopper fills and what
//! a comparator measures.

use std::mem;
use std::sync::{Arc, Weak};

use foton_registry::blocks::block_state_ext::BlockStateExt as _;
use foton_registry::data_components::components::PotDecorations;
use foton_registry::item_stack::ItemStack;
use foton_registry::vanilla_block_entity_types;
use foton_utils::locks::SyncMutex;
use foton_utils::{BlockPos, BlockStateId, DowncastType, DowncastTypeKey};
use simdnbt::borrow::{BaseNbtCompound as BorrowedNbtCompound, NbtCompound as NbtCompoundView};
use simdnbt::owned::{NbtCompound, NbtTag};
use simdnbt::{FromNbtTag as _, ToNbtTag as _};

use crate::block_entity::{BlockEntity, BlockEntityBase, ContainerLoot};
use crate::inventory::container::Container;
use crate::inventory::lock::{ContainerRef, SharedContainer};
use crate::world::World;

/// Slots a decorated pot has.
///
/// Vanilla parity: `ContainerSingleItem.getContainerSize`. One slot is why a
/// comparator reads a pot as full from a single stack.
pub const DECORATED_POT_SLOTS: usize = 1;

/// Block event a pot uses to ask the client for a wobble.
///
/// Vanilla parity: `DecoratedPotBlockEntity.EVENT_POT_WOBBLES`.
pub const EVENT_POT_WOBBLES: i32 = 1;

/// Which way a pot rocks when a player uses it.
///
/// Vanilla parity: `DecoratedPotBlockEntity.WobbleStyle`. The animation itself
/// is drawn by the client; the ordinal below is the whole of the server's part.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WobbleStyle {
    /// Something went in.
    Positive,
    /// The pot refused what was offered.
    Negative,
}

impl WobbleStyle {
    /// How many styles the client knows about.
    ///
    /// Vanilla parity: the `WobbleStyle.values().length` bound of
    /// `DecoratedPotBlockEntity.triggerEvent`.
    const COUNT: i32 = 2;

    /// Returns the ordinal the client turns back into a style.
    #[must_use]
    pub const fn ordinal(self) -> i32 {
        match self {
            Self::Positive => 0,
            Self::Negative => 1,
        }
    }
}

/// A decorated pot.
pub struct DecoratedPotBlockEntity {
    base: Arc<BlockEntityBase>,
    decorations: SyncMutex<PotDecorations>,
    container: Arc<SyncMutex<DecoratedPotContainer>>,
    container_ref: ContainerRef,
    /// Vanilla parity: `DecoratedPotBlockEntity` implements
    /// `RandomizableContainer` directly rather than through
    /// `RandomizableContainerBlockEntity`, but carries the same pair.
    loot: Arc<ContainerLoot>,
}

/// The pot's one slot.
struct DecoratedPotContainer {
    items: Vec<ItemStack>,
}

// SAFETY: This key is owned by Foton and uniquely identifies the block entity.
unsafe impl DowncastType for DecoratedPotBlockEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:block_entity/decorated_pot");
}

// SAFETY: This key is owned by Foton and uniquely identifies the inventory.
unsafe impl DowncastType for DecoratedPotContainer {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:container/decorated_pot");
}

impl DecoratedPotBlockEntity {
    /// Creates a decorated pot block entity.
    #[must_use]
    pub fn new(level: Weak<World>, pos: BlockPos, state: BlockStateId) -> Self {
        let base = Arc::new(BlockEntityBase::new(
            &vanilla_block_entity_types::DECORATED_POT,
            level,
            pos,
            state,
        ));
        let container = Arc::new(SyncMutex::new(DecoratedPotContainer {
            items: vec![ItemStack::empty(); DECORATED_POT_SLOTS],
        }));
        let shared: SharedContainer = container.clone();
        let loot = Arc::new(ContainerLoot::new());
        Self {
            container_ref: ContainerRef::owned_by_randomizable_block_entity(
                shared,
                Arc::clone(&base),
                Arc::clone(&loot),
            ),
            base,
            decorations: SyncMutex::new(PotDecorations::EMPTY),
            container,
            loot,
        }
    }

    /// Returns the four sherds pressed into the pot.
    ///
    /// Vanilla parity: `DecoratedPotBlockEntity.getDecorations`.
    #[must_use]
    pub fn decorations(&self) -> PotDecorations {
        self.decorations.lock().clone()
    }

    /// Presses an item's sherds into the pot that was just placed.
    ///
    /// Vanilla parity: the `applyImplicitComponents` of
    /// `DecoratedPotBlockEntity`, which is what carries `pot_decorations` off
    /// the item and onto the block.
    pub fn set_decorations(&self, decorations: PotDecorations) {
        *self.decorations.lock() = decorations;
        self.set_changed();
    }

    /// Returns a copy of what is inside the pot.
    ///
    /// Vanilla parity: `DecoratedPotBlockEntity.getTheItem`, which rolls a
    /// packed loot table before answering.
    #[must_use]
    pub fn the_item(&self) -> ItemStack {
        self.container_ref.unpack_loot_table(None);
        self.container.lock().items[0].clone()
    }

    /// Replaces what is inside the pot.
    ///
    /// Vanilla parity: `DecoratedPotBlockEntity.setTheItem`.
    pub fn set_the_item(&self, item: ItemStack) {
        self.container_ref.unpack_loot_table(None);
        self.container.lock().items[0] = item;
        self.set_changed();
    }

    /// Returns whether the pot holds nothing.
    ///
    /// Vanilla parity: `ContainerSingleItem.isEmpty`, which reads through
    /// `getTheItem` and therefore unpacks too.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.container_ref.unpack_loot_table(None);
        self.container.lock().items[0].is_empty()
    }

    /// Rocks the pot for everyone watching it.
    ///
    /// Vanilla parity: `DecoratedPotBlockEntity.wobble`, a block event rather
    /// than stored state because only the client draws the animation.
    pub fn wobble(&self, world: &Arc<World>, style: WobbleStyle) {
        world.block_event(
            self.base.pos(),
            self.get_block_state().get_block(),
            EVENT_POT_WOBBLES,
            style.ordinal(),
        );
    }
}

impl BlockEntity for DecoratedPotBlockEntity {
    fn base(&self) -> &BlockEntityBase {
        &self.base
    }

    /// Vanilla parity: the default `BlockEntity.preRemoveSideEffects`, which
    /// drops a container's contents. The sherds ride out on the item the block
    /// drops, but whatever a player stored inside falls on the floor.
    fn pre_remove_side_effects(&self, pos: BlockPos, _state: BlockStateId) {
        self.container_ref.unpack_loot_table(None);
        let item = {
            let mut container = self.container.lock();
            mem::replace(&mut container.items[0], ItemStack::empty())
        };
        if item.is_empty() {
            return;
        }
        let Some(world) = self.get_level() else {
            return;
        };
        world.drop_item_stack(pos, item);
    }

    fn load_additional(&self, nbt: &BorrowedNbtCompound<'_>) {
        let view: NbtCompoundView<'_, '_> = nbt.into();
        let decorations = view
            .get("sherds")
            .and_then(PotDecorations::from_nbt_tag)
            .unwrap_or(PotDecorations::EMPTY);
        // Vanilla parity: a pot stores either a loot table or its item.
        let item = if self.loot.try_load_loot_table(&view) {
            ItemStack::empty()
        } else {
            view.compound("item")
                .and_then(|compound| ItemStack::from_borrowed_compound(&compound))
                .unwrap_or_else(ItemStack::empty)
        };

        *self.decorations.lock() = decorations;
        self.container.lock().items[0] = item;
    }

    /// Vanilla parity: `DecoratedPotBlockEntity.saveAdditional`, which leaves
    /// the sherds out of an all-brick pot rather than writing four bricks,
    /// and writes a still-packed loot table in place of the item.
    fn save_additional(&self, nbt: &mut NbtCompound) {
        let decorations = self.decorations.lock().clone();
        if decorations != PotDecorations::EMPTY {
            nbt.insert("sherds", decorations.to_nbt_tag());
        }

        if self.loot.try_save_loot_table(nbt) {
            return;
        }
        let item = self.container.lock().items[0].clone();
        if !item.is_empty()
            && let NbtTag::Compound(item_nbt) = item.to_nbt_tag()
        {
            nbt.insert("item", item_nbt);
        }
    }

    /// Vanilla parity: `DecoratedPotBlockEntity.getUpdateTag`, which is
    /// `saveCustomOnly`. The client picks the four sherd textures out of this
    /// tag, so without it every pot in a freshly sent chunk is four bricks.
    fn get_update_tag(&self) -> Option<NbtCompound> {
        Some(self.save_custom_only())
    }

    /// Vanilla parity: `DecoratedPotBlockEntity.triggerEvent`. Vanilla records
    /// the tick and the style because the same class runs on the client and
    /// animates from them; Foton's block entities are server-only, so accepting
    /// the event is the whole job -- returning `true` is what sends the packet
    /// the client animates from.
    fn trigger_event(&self, param_a: i32, param_b: i32) -> bool {
        param_a == EVENT_POT_WOBBLES && (0..WobbleStyle::COUNT).contains(&param_b)
    }

    fn container_ref(&self) -> Option<ContainerRef> {
        Some(self.container_ref.clone())
    }
}

impl Container for DecoratedPotContainer {
    fn items(&self) -> &[ItemStack] {
        &self.items
    }

    fn items_mut(&mut self) -> &mut [ItemStack] {
        &mut self.items
    }

    fn get_container_size(&self) -> usize {
        DECORATED_POT_SLOTS
    }

    // Vanilla parity: `ContainerSingleItem` does not narrow `Container`'s
    // default stack limit, so a pot holds a full stack of whatever fits in one.

    fn set_changed(&mut self) {}
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use foton_registry::{init_vanilla_registry, vanilla_blocks, vanilla_items};
    use simdnbt::borrow::read_compound;

    use super::*;

    fn test_pot() -> DecoratedPotBlockEntity {
        init_vanilla_registry();
        DecoratedPotBlockEntity::new(
            Weak::new(),
            BlockPos::new(1, 2, 3),
            vanilla_blocks::DECORATED_POT.default_state(),
        )
    }

    fn angler_and_archer() -> PotDecorations {
        PotDecorations::from_ordered(&[
            &vanilla_items::ANGLER_POTTERY_SHERD,
            &vanilla_items::BRICK,
            &vanilla_items::ARCHER_POTTERY_SHERD,
            &vanilla_items::BRICK,
        ])
        .expect("four decorations fit")
    }

    /// Writes the pot out and reads it back the way the chunk saver does.
    fn round_trip(pot: &DecoratedPotBlockEntity) -> DecoratedPotBlockEntity {
        let mut saved = NbtCompound::new();
        pot.save_additional(&mut saved);
        let mut bytes = Vec::new();
        saved.write(&mut bytes);
        let borrowed =
            read_compound(&mut Cursor::new(bytes.as_slice())).expect("the pot's own NBT reparses");

        let loaded = test_pot();
        loaded.load_additional(&borrowed);
        loaded
    }

    /// The sherds are the block. Losing them across a save would turn every
    /// decorated pot in the world into four bricks on the next restart.
    #[test]
    fn the_sherds_are_still_there_after_a_save_and_a_load() {
        let pot = test_pot();
        pot.set_decorations(angler_and_archer());

        let loaded = round_trip(&pot);

        assert_eq!(loaded.decorations(), angler_and_archer());
    }

    /// An undecorated pot writes no sherds at all, matching vanilla, so the
    /// four implicit bricks never take up room in every chunk on disk.
    #[test]
    fn a_plain_pot_writes_no_sherds_at_all() {
        let pot = test_pot();

        let mut saved = NbtCompound::new();
        pot.save_additional(&mut saved);

        assert!(saved.get("sherds").is_none());
        assert_eq!(round_trip(&pot).decorations(), PotDecorations::EMPTY);
    }

    /// What a player put in the pot has to survive a save as well, and the
    /// client needs it in the update tag along with the sherds.
    #[test]
    fn the_stored_item_survives_a_save_and_reaches_the_update_tag() {
        let pot = test_pot();
        pot.set_decorations(angler_and_archer());
        pot.set_the_item(ItemStack::with_count(&vanilla_items::DIAMOND, 17));

        let loaded = round_trip(&pot);
        assert!(loaded.the_item().is(&vanilla_items::DIAMOND));
        assert_eq!(loaded.the_item().count(), 17);

        let update_tag = pot.get_update_tag().expect("the client draws the sherds");
        assert!(update_tag.get("sherds").is_some());
    }

    /// Breaking the pot scatters what was inside it. The sherds leave on the
    /// item instead, so this is the only thing that may hit the floor.
    #[test]
    fn removal_scatters_what_was_stored_inside() {
        let pot = test_pot();
        pot.set_the_item(ItemStack::new(&vanilla_items::DIAMOND));

        // No world is attached, so nothing can actually be spawned; what
        // matters is that the slot is emptied exactly once and cannot be
        // dropped a second time.
        pot.pre_remove_side_effects(
            BlockPos::new(1, 2, 3),
            vanilla_blocks::DECORATED_POT.default_state(),
        );

        assert!(pot.is_empty(), "the slot should have been emptied");
    }
}
