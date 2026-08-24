//! Crafter block entity.
//!
//! Vanilla parity: `CrafterBlockEntity`. Nine slots, but unlike every other
//! nine-slot block each one can be switched off, and a slot that is off counts
//! as full: it takes no items and it still feeds a comparator. That is the
//! whole point of the block -- it lets a recipe keep a hole in the middle of
//! its grid while a hopper pours items in from above.

use std::mem;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::{Arc, Weak};

use simdnbt::ToNbtTag as _;
use simdnbt::borrow::{BaseNbtCompound as BorrowedNbtCompound, NbtCompound as NbtCompoundView};
use simdnbt::owned::{NbtCompound, NbtList, NbtTag};
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::blocks::properties::BlockStateProperties;
use steel_registry::item_stack::ItemStack;
use steel_registry::recipe::CraftingInput;
use steel_registry::vanilla_block_entity_types;
use steel_utils::types::UpdateFlags;
use steel_utils::{BlockPos, BlockStateId, DowncastType, DowncastTypeKey, locks::SyncMutex};

use crate::block_entity::{BlockEntity, BlockEntityBase, ContainerLoot};
use crate::inventory::container::Container;
use crate::inventory::lock::{ContainerRef, SharedContainer};
use crate::world::{LevelReader as _, World};

/// Slots in a crafter.
///
/// Vanilla parity: `CrafterBlockEntity.CONTAINER_SIZE`.
pub const CRAFTER_SLOTS: usize = 9;

/// Width of the crafting grid.
pub const CRAFTER_WIDTH: usize = 3;

/// Height of the crafting grid.
pub const CRAFTER_HEIGHT: usize = 3;

/// Values the crafter menu mirrors to the client.
///
/// Vanilla parity: `CrafterBlockEntity.NUM_DATA` -- nine slot states and the
/// triggered flag.
pub const CRAFTER_DATA_SLOTS: usize = 10;

/// Vanilla parity: `CrafterBlockEntity.SLOT_DISABLED`.
const SLOT_DISABLED: i16 = 1;

/// Vanilla parity: `CrafterBlockEntity.SLOT_ENABLED`.
const SLOT_ENABLED: i16 = 0;

/// The switched-off slots and the redstone flag, shared with the menu.
///
/// Vanilla hands the menu the block entity's own `ContainerData`; Steel keeps
/// this behind an `Arc` so both the container (which consults it on every
/// insert) and the menu (which mirrors it to the client) can read it without
/// taking the block entity's lock.
pub struct CrafterDataSlots {
    /// One flag per slot; set means the slot refuses items.
    disabled: [AtomicBool; CRAFTER_SLOTS],
    /// Whether redstone is currently holding the block.
    triggered: AtomicBool,
}

impl Default for CrafterDataSlots {
    fn default() -> Self {
        Self {
            disabled: [const { AtomicBool::new(false) }; CRAFTER_SLOTS],
            triggered: AtomicBool::new(false),
        }
    }
}

impl CrafterDataSlots {
    /// Returns whether `slot` refuses items.
    #[must_use]
    pub fn is_slot_disabled(&self, slot: usize) -> bool {
        self.disabled
            .get(slot)
            .is_some_and(|flag| flag.load(Ordering::Relaxed))
    }

    /// Switches `slot` on or off.
    pub fn set_slot_disabled(&self, slot: usize, disabled: bool) {
        if let Some(flag) = self.disabled.get(slot) {
            flag.store(disabled, Ordering::Relaxed);
        }
    }

    /// Returns whether redstone is holding the block.
    #[must_use]
    pub fn is_triggered(&self) -> bool {
        self.triggered.load(Ordering::Relaxed)
    }

    /// Records whether redstone is holding the block.
    pub fn set_triggered(&self, triggered: bool) {
        self.triggered.store(triggered, Ordering::Relaxed);
    }

    /// Reads the ten values in the order the vanilla protocol expects.
    #[must_use]
    pub fn snapshot(&self) -> [i16; CRAFTER_DATA_SLOTS] {
        let mut values = [SLOT_ENABLED; CRAFTER_DATA_SLOTS];
        for (slot, value) in values.iter_mut().take(CRAFTER_SLOTS).enumerate() {
            *value = if self.is_slot_disabled(slot) {
                SLOT_DISABLED
            } else {
                SLOT_ENABLED
            };
        }
        values[CRAFTER_SLOTS] = i16::from(self.is_triggered());
        values
    }
}

/// Crafter block entity.
pub struct CrafterBlockEntity {
    base: Arc<BlockEntityBase>,
    container: Arc<SyncMutex<CrafterContainer>>,
    container_ref: ContainerRef,
    data: Arc<CrafterDataSlots>,
    /// Ticks left before the block drops out of its crafting pose.
    crafting_ticks_remaining: AtomicI32,
    /// Vanilla parity: the `RandomizableContainer` half of a crafter. Nothing
    /// in vanilla worldgen generates one stocked, but the block entity carries
    /// the pair like every other `RandomizableContainerBlockEntity`.
    loot: Arc<ContainerLoot>,
}

/// The nine slots of a crafter.
pub struct CrafterContainer {
    items: Vec<ItemStack>,
    data: Arc<CrafterDataSlots>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `CrafterBlockEntity`.
unsafe impl DowncastType for CrafterBlockEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:block_entity/crafter");
}

// SAFETY: This key is owned by Steel and uniquely identifies the independently
// lockable inventory data used by a crafter block entity.
unsafe impl DowncastType for CrafterContainer {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:container/crafter");
}

impl CrafterBlockEntity {
    /// Creates a crafter block entity.
    #[must_use]
    pub fn new(level: Weak<World>, pos: BlockPos, state: BlockStateId) -> Self {
        let base = Arc::new(BlockEntityBase::new(
            &vanilla_block_entity_types::CRAFTER,
            level,
            pos,
            state,
        ));
        let data = Arc::new(CrafterDataSlots::default());
        let container = Arc::new(SyncMutex::new(CrafterContainer {
            items: vec![ItemStack::empty(); CRAFTER_SLOTS],
            data: Arc::clone(&data),
        }));
        let shared_container: SharedContainer = container.clone();
        let loot = Arc::new(ContainerLoot::new());
        Self {
            container_ref: ContainerRef::owned_by_randomizable_block_entity(
                shared_container,
                Arc::clone(&base),
                Arc::clone(&loot),
            ),
            base,
            container,
            data,
            crafting_ticks_remaining: AtomicI32::new(0),
            loot,
        }
    }

    /// Returns the slot states and redstone flag shared with the menu.
    #[must_use]
    pub fn data(&self) -> Arc<CrafterDataSlots> {
        Arc::clone(&self.data)
    }

    /// Returns the independently lockable container behind this block entity.
    #[must_use]
    pub fn container_ref(&self) -> ContainerRef {
        self.container_ref.clone()
    }

    /// Returns a copy of the stack in `slot`.
    #[must_use]
    pub fn get_item(&self, slot: usize) -> ItemStack {
        self.container_ref.unpack_loot_table(None);
        let container = self.container.lock();
        container
            .items
            .get(slot)
            .map_or_else(ItemStack::empty, Clone::clone)
    }

    /// Replaces the stack in `slot`.
    ///
    /// Vanilla parity: `CrafterBlockEntity.setItem`, which switches a disabled
    /// slot back on rather than dropping the item -- putting something in a
    /// slot is how you say you want it used.
    pub fn set_item(&self, slot: usize, stack: ItemStack) {
        self.container_ref.unpack_loot_table(None);
        self.container.lock().set_item(slot, stack);
        self.set_changed();
    }

    /// Switches `slot` on or off, reporting whether anything changed.
    ///
    /// Vanilla parity: `CrafterBlockEntity.setSlotState`, which refuses to
    /// switch off a slot that holds something.
    pub fn set_slot_state(&self, slot: usize, enabled: bool) -> bool {
        if !self.slot_can_be_disabled(slot) {
            return false;
        }
        self.data.set_slot_disabled(slot, !enabled);
        self.set_changed();
        true
    }

    /// Returns whether `slot` refuses items.
    #[must_use]
    pub fn is_slot_disabled(&self, slot: usize) -> bool {
        self.data.is_slot_disabled(slot)
    }

    /// Records whether redstone is holding the block.
    pub fn set_triggered(&self, triggered: bool) {
        self.data.set_triggered(triggered);
    }

    /// Returns whether redstone is holding the block.
    #[must_use]
    pub fn is_triggered(&self) -> bool {
        self.data.is_triggered()
    }

    /// Puts the block into its crafting pose for `ticks`.
    pub fn set_crafting_ticks_remaining(&self, ticks: i32) {
        self.crafting_ticks_remaining
            .store(ticks, Ordering::Relaxed);
    }

    /// Counts down the crafting pose, reporting the tick it runs out on.
    ///
    /// Vanilla parity: `CrafterBlockEntity.serverTick`. Returns `true` exactly
    /// once per craft, which is the tick the block has to leave its pose.
    pub fn tick_crafting_pose(&self) -> bool {
        let remaining = self.crafting_ticks_remaining.load(Ordering::Relaxed) - 1;
        if remaining < 0 {
            return false;
        }
        self.crafting_ticks_remaining
            .store(remaining, Ordering::Relaxed);
        remaining == 0
    }

    /// Returns the grid as a crafting input.
    ///
    /// Vanilla parity: `CraftingContainer.asCraftInput`, which vanilla trims to
    /// the smallest box holding every item. Steel's shaped matcher slides the
    /// pattern over the input itself, so the untrimmed 3x3 is what it wants.
    #[must_use]
    pub fn as_craft_input(&self) -> CraftingInput {
        self.container_ref.unpack_loot_table(None);
        let container = self.container.lock();
        CraftingInput::new(CRAFTER_WIDTH, CRAFTER_HEIGHT, container.items.clone())
    }

    /// Takes one item out of every filled slot.
    ///
    /// Vanilla parity: the `getItems().forEach(shrink(1))` of
    /// `CrafterBlock.dispenseFrom`.
    pub fn consume_one_of_each(&self) {
        self.container_ref.unpack_loot_table(None);
        {
            let mut container = self.container.lock();
            for item in &mut container.items {
                if !item.is_empty() {
                    item.shrink(1);
                }
            }
        }
        self.set_changed();
    }

    /// What a comparator reads off the crafter.
    ///
    /// Vanilla parity: `CrafterBlockEntity.getRedstoneSignal`, which counts
    /// filled *or* switched-off slots -- a crafter set up for a recipe with
    /// holes reads full even though it is not.
    #[must_use]
    pub fn redstone_signal(&self) -> i32 {
        self.container_ref.unpack_loot_table(None);
        let container = self.container.lock();
        let mut count = 0;
        for slot in 0..CRAFTER_SLOTS {
            if !container.items[slot].is_empty() || self.data.is_slot_disabled(slot) {
                count += 1;
            }
        }
        count
    }

    /// Vanilla parity: `CrafterBlockEntity.slotCanBeDisabled`.
    fn slot_can_be_disabled(&self, slot: usize) -> bool {
        let container = self.container.lock();
        container.items.get(slot).is_some_and(ItemStack::is_empty)
    }
}

impl BlockEntity for CrafterBlockEntity {
    fn base(&self) -> &BlockEntityBase {
        &self.base
    }

    /// Vanilla parity: `CrafterBlockEntity.serverTick`, which does nothing but
    /// take the block out of its crafting pose six ticks after a craft.
    fn tick(&self, world: &Arc<World>) {
        if !self.tick_crafting_pose() {
            return;
        }
        let pos = self.get_block_pos();
        let state = world.get_block_state(pos);
        world.set_block(
            pos,
            state.set_value(&BlockStateProperties::CRAFTING, false),
            UpdateFlags::UPDATE_ALL,
        );
    }

    fn pre_remove_side_effects(&self, pos: BlockPos, _state: BlockStateId) {
        self.container_ref.unpack_loot_table(None);
        let items = {
            let mut container = self.container.lock();
            mem::replace(
                &mut container.items,
                vec![ItemStack::empty(); CRAFTER_SLOTS],
            )
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
        // Vanilla parity: a crafter stores either a loot table or its
        // items; the crafting ticks and slot states are read either way.
        let packed = self.loot.try_load_loot_table(&nbt_view);
        {
            let mut container = self.container.lock();
            container.items.fill(ItemStack::empty());

            if !packed
                && let Some(items_list) = nbt_view.list("Items")
                && let Some(compounds) = items_list.compounds()
            {
                for compound in compounds {
                    if let Some(slot) = compound.byte("Slot") {
                        let slot = slot as usize;
                        if slot < CRAFTER_SLOTS
                            && let Some(item) = ItemStack::from_borrowed_compound(&compound)
                        {
                            container.items[slot] = item;
                        }
                    }
                }
            }
        }

        self.crafting_ticks_remaining.store(
            nbt_view.int("crafting_ticks_remaining").unwrap_or(0),
            Ordering::Relaxed,
        );

        for slot in 0..CRAFTER_SLOTS {
            self.data.set_slot_disabled(slot, false);
        }
        if let Some(disabled) = nbt_view.int_array("disabled_slots") {
            for slot in disabled {
                if let Ok(slot) = usize::try_from(slot) {
                    // A slot that holds something cannot be off, the same guard
                    // vanilla applies on load rather than trusting the tag.
                    if self.slot_can_be_disabled(slot) {
                        self.data.set_slot_disabled(slot, true);
                    }
                }
            }
        }
        self.data
            .set_triggered(nbt_view.int("triggered").unwrap_or(0) != 0);
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        let container = self.container.lock();
        if !self.loot.try_save_loot_table(nbt) {
            let mut items: Vec<NbtCompound> = Vec::new();
            for (slot, item) in container.items.iter().enumerate() {
                if !item.is_empty()
                    && let NbtTag::Compound(mut item_nbt) = item.clone().to_nbt_tag()
                {
                    item_nbt.insert("Slot", slot as i8);
                    items.push(item_nbt);
                }
            }
            nbt.insert("Items", NbtList::Compound(items));
        }

        nbt.insert(
            "crafting_ticks_remaining",
            self.crafting_ticks_remaining.load(Ordering::Relaxed),
        );
        let disabled: Vec<i32> = (0..CRAFTER_SLOTS)
            .filter(|slot| self.data.is_slot_disabled(*slot))
            .map(|slot| slot as i32)
            .collect();
        nbt.insert("disabled_slots", NbtTag::IntArray(disabled));
        nbt.insert("triggered", i32::from(self.data.is_triggered()));
    }

    fn container_ref(&self) -> Option<ContainerRef> {
        Some(self.container_ref.clone())
    }
}

impl Container for CrafterContainer {
    fn items(&self) -> &[ItemStack] {
        &self.items
    }

    fn items_mut(&mut self) -> &mut [ItemStack] {
        &mut self.items
    }

    fn get_container_size(&self) -> usize {
        CRAFTER_SLOTS
    }

    /// Vanilla parity: `CrafterBlockEntity.setItem`, which switches a disabled
    /// slot back on rather than refusing the item.
    fn set_item(&mut self, slot: usize, mut stack: ItemStack) {
        if slot >= CRAFTER_SLOTS {
            return;
        }
        if self.data.is_slot_disabled(slot) {
            self.data.set_slot_disabled(slot, false);
        }
        let max_stack_size = self.get_max_stack_size_for_item(&stack);
        if !stack.is_empty() && stack.count() > max_stack_size {
            stack.set_count(max_stack_size);
        }
        self.items[slot] = stack;
    }

    /// Vanilla parity: `CrafterBlockEntity.canPlaceItem`, which is what spreads
    /// a hopper's output across the grid one item at a time instead of piling
    /// it all into the first slot: a slot only accepts an item while no other
    /// enabled slot is emptier than it is.
    fn can_place_item(&self, slot: usize, _stack: &ItemStack) -> bool {
        if slot >= CRAFTER_SLOTS || self.data.is_slot_disabled(slot) {
            return false;
        }
        let current = &self.items[slot];
        // The empty case comes first. Vanilla asks whether the slot is full
        // before it asks whether it is empty, which works there because an
        // empty stack reports a stack limit of 64; Steel's reports 1, so that
        // order would call every empty slot full.
        if current.is_empty() {
            return true;
        }
        let count = current.count();
        if count >= current.max_stack_size() {
            return false;
        }
        !self.smaller_stack_exists(count, current, slot)
    }

    fn get_max_stack_size(&self) -> i32 {
        64
    }

    fn set_changed(&mut self) {}
}

impl CrafterContainer {
    /// Vanilla parity: `CrafterBlockEntity.smallerStackExist`.
    ///
    /// Only slots *after* `base_slot` count, which is what makes the fill go
    /// left to right instead of oscillating between two equally empty slots.
    fn smaller_stack_exists(
        &self,
        base_size: i32,
        base_item: &ItemStack,
        base_slot: usize,
    ) -> bool {
        for slot in (base_slot + 1)..CRAFTER_SLOTS {
            if self.data.is_slot_disabled(slot) {
                continue;
            }
            let other = &self.items[slot];
            if other.is_empty()
                || (other.count() < base_size
                    && ItemStack::is_same_item_same_components(other, base_item))
            {
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use steel_registry::{init_vanilla_registry, vanilla_blocks, vanilla_items};

    use super::*;

    fn crafter() -> CrafterBlockEntity {
        init_vanilla_registry();
        CrafterBlockEntity::new(
            Weak::new(),
            BlockPos::new(1, 2, 3),
            vanilla_blocks::CRAFTER.default_state(),
        )
    }

    #[test]
    fn an_empty_crafter_feeds_a_comparator_nothing() {
        assert_eq!(crafter().redstone_signal(), 0);
    }

    /// A switched-off slot counts as full, which is the whole reason the
    /// comparator reading is not just "how many items are in here".
    #[test]
    fn a_disabled_slot_counts_towards_the_comparator() {
        let crafter = crafter();
        assert!(crafter.set_slot_state(4, false));

        assert_eq!(crafter.redstone_signal(), 1);
    }

    #[test]
    fn a_slot_that_holds_something_cannot_be_switched_off() {
        let crafter = crafter();
        crafter.set_item(0, ItemStack::new(&vanilla_items::STONE));

        assert!(!crafter.set_slot_state(0, false));
        assert!(!crafter.is_slot_disabled(0));
    }

    /// Vanilla parity: `setItem` switches the slot back on rather than
    /// refusing, so a player dropping an item into a disabled slot gets what
    /// they asked for.
    #[test]
    fn putting_an_item_in_a_disabled_slot_switches_it_back_on() {
        let crafter = crafter();
        assert!(crafter.set_slot_state(7, false));

        crafter.set_item(7, ItemStack::new(&vanilla_items::STONE));

        assert!(!crafter.is_slot_disabled(7));
    }

    /// The plainest case there is, and the one an over-faithful port of
    /// vanilla's ordering gets wrong.
    #[test]
    fn an_empty_enabled_slot_takes_an_item() {
        let crafter = crafter();

        let container = crafter.container.lock();
        assert!(container.can_place_item(0, &ItemStack::new(&vanilla_items::STONE)));
    }

    #[test]
    fn a_disabled_slot_takes_no_items() {
        let crafter = crafter();
        assert!(crafter.set_slot_state(3, false));

        let container = crafter.container.lock();
        assert!(!container.can_place_item(3, &ItemStack::new(&vanilla_items::STONE)));
    }

    /// The spread rule: while slot 8 is still empty, slot 0 will not take a
    /// second item.
    #[test]
    fn a_slot_refuses_a_second_item_while_a_later_slot_is_emptier() {
        let crafter = crafter();
        crafter.set_item(0, ItemStack::new(&vanilla_items::STONE));

        let container = crafter.container.lock();
        assert!(!container.can_place_item(0, &ItemStack::new(&vanilla_items::STONE)));
    }

    #[test]
    fn a_slot_takes_a_second_item_once_every_later_slot_is_level() {
        let crafter = crafter();
        for slot in 0..CRAFTER_SLOTS {
            crafter.set_item(slot, ItemStack::new(&vanilla_items::STONE));
        }

        let container = crafter.container.lock();
        assert!(container.can_place_item(0, &ItemStack::new(&vanilla_items::STONE)));
    }

    /// Switched-off slots are skipped by the spread rule, or a recipe with a
    /// hole in it would jam after one round.
    #[test]
    fn the_spread_rule_ignores_switched_off_slots() {
        let crafter = crafter();
        for slot in 1..CRAFTER_SLOTS {
            assert!(crafter.set_slot_state(slot, false));
        }
        crafter.set_item(0, ItemStack::new(&vanilla_items::STONE));

        let container = crafter.container.lock();
        assert!(container.can_place_item(0, &ItemStack::new(&vanilla_items::STONE)));
    }

    #[test]
    fn crafting_consumes_one_of_each_filled_slot() {
        let crafter = crafter();
        crafter.set_item(0, ItemStack::with_count(&vanilla_items::STONE, 3));
        crafter.set_item(4, ItemStack::with_count(&vanilla_items::DIRT, 1));

        crafter.consume_one_of_each();

        assert_eq!(crafter.get_item(0).count(), 2);
        assert!(crafter.get_item(4).is_empty());
    }

    /// The pose lasts a fixed number of ticks and reports its own end exactly
    /// once, so the block leaves its crafting state on the right tick.
    #[test]
    fn the_crafting_pose_ends_once() {
        let crafter = crafter();
        crafter.set_crafting_ticks_remaining(3);

        assert!(!crafter.tick_crafting_pose());
        assert!(!crafter.tick_crafting_pose());
        assert!(crafter.tick_crafting_pose());
        assert!(!crafter.tick_crafting_pose());
    }
}
