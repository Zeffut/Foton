//! Brewing stand block entity.
//!
//! Vanilla parity: `BrewingStandBlockEntity`. Three bottles brew at once from
//! one ingredient, which is what makes brewing worth the walk to the nether:
//! the ingredient is spent once and every bottle it applies to is converted.
//! Bottles it does not apply to are left alone rather than blocking the brew.

use std::{
    mem, ptr,
    sync::{
        Arc, Weak,
        atomic::{AtomicI32, AtomicU64, Ordering},
    },
};

#[cfg(test)]
use std::cell::Cell;

use foton_registry::blocks::block_state_ext::BlockStateExt;
use foton_registry::blocks::properties::{BlockStateProperties, BoolProperty};
use foton_registry::item_predicate::LockCode;
use foton_registry::item_stack::ItemStack;
use foton_registry::level_events::SOUND_BREWING_STAND_BREW;
use foton_registry::{potion_brewing, vanilla_block_entity_types, vanilla_items};
use foton_utils::types::UpdateFlags;
use foton_utils::{
    BlockPos, BlockStateId, Direction, DowncastType, DowncastTypeKey, locks::SyncMutex,
};
use simdnbt::borrow::{BaseNbtCompound as BorrowedNbtCompound, NbtCompound as NbtCompoundView};
use simdnbt::owned::{NbtCompound, NbtList, NbtTag};
use simdnbt::{FromNbtTag, ToNbtTag};

use crate::block_entity::{BlockEntity, BlockEntityBase, ImplicitComponentInput};
use crate::event::BrewEvent;
use crate::inventory::container::{Container, SlotsForFace};
use crate::inventory::lock::{ContainerLockGuard, ContainerRef, SharedContainer};
use crate::world::World;
use foton_registry::data_components::{
    DataComponentMap,
    vanilla_components::{CUSTOM_NAME, LOCK},
};
use std::array;
use text_components::TextComponent;

/// First of the three bottle slots.
pub const SLOT_FIRST_BOTTLE: usize = 0;
/// Slot holding the ingredient being brewed in.
pub const SLOT_INGREDIENT: usize = 3;
/// Slot holding the blaze powder.
pub const SLOT_FUEL: usize = 4;
/// Total slots in a brewing stand.
pub const BREWING_STAND_SLOTS: usize = 5;
/// Bottles a stand brews at once.
pub const BOTTLE_SLOTS: usize = 3;
/// Vanilla and Paper default brewing duration.
pub const DEFAULT_RECIPE_BREW_TIME: i32 = 400;

const CUSTOM_NAME_TAG: &str = "CustomName";
const LOCK_TAG: &str = "lock";
static NEXT_BREWING_STAND_IDENTITY: AtomicU64 = AtomicU64::new(1);

/// The three bottle-occupied flags on the block.
///
/// Vanilla parity: `BrewingStandBlock.HAS_BOTTLE`, which is what draws the
/// bottles on the model.
static HAS_BOTTLE: [&BoolProperty; BOTTLE_SLOTS] = [
    &BlockStateProperties::HAS_BOTTLE_0,
    &BlockStateProperties::HAS_BOTTLE_1,
    &BlockStateProperties::HAS_BOTTLE_2,
];

/// Brewing progress mirrored to every open menu.
///
/// Vanilla parity: the two-entry `ContainerData` of `BrewingStandBlockEntity`.
/// As with the furnace, vanilla hands the menu the block entity itself; Foton
/// republishes the two values so a menu never takes the block entity's lock.
#[derive(Debug, Default)]
pub struct BrewingStandDataSlots {
    /// Ticks left on the current brew, counting down.
    pub brew_time: AtomicI32,
    /// Brews left in the blaze powder already consumed.
    pub fuel: AtomicI32,
}

impl BrewingStandDataSlots {
    /// Reads the two values in the order the vanilla protocol expects.
    #[must_use]
    pub fn snapshot(&self) -> [i16; 2] {
        [
            clamp_to_i16(self.brew_time.load(Ordering::Relaxed)),
            clamp_to_i16(self.fuel.load(Ordering::Relaxed)),
        ]
    }
}

/// Narrows a counter to the `i16` the protocol carries.
fn clamp_to_i16(value: i32) -> i16 {
    i16::try_from(value).unwrap_or(i16::MAX)
}

/// Brewing stand block entity.
pub struct BrewingStandBlockEntity {
    base: Arc<BlockEntityBase>,
    identity: u64,
    container: Arc<SyncMutex<BrewingStandContainer>>,
    container_ref: ContainerRef,
    data: Arc<BrewingStandDataSlots>,
}

/// Items and brewing state, held under one lock because a tick mutates both.
struct BrewingStandContainer {
    items: Vec<ItemStack>,
    /// Ticks left on the current brew.
    brew_time: i32,
    /// Brews left in the blaze powder already consumed.
    fuel: i32,
    /// Paper's mutable duration for the next brew; not persisted by Vanilla.
    recipe_brew_time: i32,
    custom_name: Option<TextComponent>,
    lock: LockCode,
    /// The ingredient the current brew started with.
    ///
    /// Vanilla keeps this to notice a player swapping the ingredient mid-brew,
    /// which cancels the brew rather than quietly producing the wrong potion.
    brewing_ingredient: Option<ItemStack>,
    /// Which bottle slots were filled when the block state was last written.
    last_bottle_flags: [bool; BOTTLE_SLOTS],
}

/// State whose identity must survive the unlocked plugin callback.
#[derive(Clone, PartialEq)]
struct BrewCompletionSnapshot {
    items: Vec<ItemStack>,
    brew_time: i32,
    fuel: i32,
    recipe_brew_time: i32,
    custom_name: Option<TextComponent>,
    lock: LockCode,
    brewing_ingredient: Option<ItemStack>,
    last_bottle_flags: [bool; BOTTLE_SLOTS],
}

/// Complete block-entity snapshot exposed to the Paper bridge.
#[derive(Clone)]
pub struct BrewingStandStateSnapshot {
    /// Instance identity used to reject wrappers captured before replacement.
    pub identity: u64,
    /// The five live container slots.
    pub items: Vec<ItemStack>,
    /// Remaining ticks in the current brew.
    pub brew_time: i32,
    /// Duration assigned to the next brew.
    pub recipe_brew_time: i32,
    /// Remaining fuel uses.
    pub fuel: i32,
    /// Optional custom display name.
    pub custom_name: Option<TextComponent>,
    /// Vanilla item predicate lock.
    pub lock: LockCode,
}

struct BrewCompletionEffects {
    remainder: Option<ItemStack>,
    completed: bool,
    flags: [bool; BOTTLE_SLOTS],
    flags_changed: bool,
}

#[cfg(test)]
thread_local! {
    static BREW_SNAPSHOT_CAPTURES: Cell<usize> = const { Cell::new(0) };
    static BREW_WORLD_EFFECTS: Cell<usize> = const { Cell::new(0) };
}

impl BrewCompletionSnapshot {
    fn capture(container: &BrewingStandContainer) -> Self {
        #[cfg(test)]
        BREW_SNAPSHOT_CAPTURES.with(|count| count.set(count.get() + 1));
        Self {
            items: container.items.clone(),
            brew_time: container.brew_time,
            fuel: container.fuel,
            recipe_brew_time: container.recipe_brew_time,
            custom_name: container.custom_name.clone(),
            lock: container.lock.clone(),
            brewing_ingredient: container.brewing_ingredient.clone(),
            last_bottle_flags: container.last_bottle_flags,
        }
    }

    fn still_matches(&self, container: &BrewingStandContainer) -> bool {
        self.items == container.items
            && self.brew_time == container.brew_time
            && self.fuel == container.fuel
            && self.recipe_brew_time == container.recipe_brew_time
            && self.custom_name == container.custom_name
            && self.lock == container.lock
            && self.brewing_ingredient == container.brewing_ingredient
            && self.last_bottle_flags == container.last_bottle_flags
    }
}

// SAFETY: This key is owned by Foton and uniquely identifies `BrewingStandBlockEntity`.
unsafe impl DowncastType for BrewingStandBlockEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:block_entity/brewing_stand");
}

// SAFETY: This key is owned by Foton and uniquely identifies the independently
// lockable inventory data used by a brewing stand block entity.
unsafe impl DowncastType for BrewingStandContainer {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:container/brewing_stand");
}

impl BrewingStandContainer {
    /// Returns whether the ingredient converts at least one bottle.
    ///
    /// Vanilla parity: `BrewingStandBlockEntity.isBrewable`.
    fn is_brewable(&self) -> bool {
        let ingredient = &self.items[SLOT_INGREDIENT];
        if ingredient.is_empty() || !potion_brewing::is_ingredient(ingredient) {
            return false;
        }
        self.items[..BOTTLE_SLOTS]
            .iter()
            .any(|bottle| !bottle.is_empty() && potion_brewing::has_mix(bottle, ingredient))
    }

    /// Converts every bottle the ingredient applies to and spends it.
    ///
    /// Vanilla parity: `BrewingStandBlockEntity.doBrew`. Returns the remainder
    /// the caller has to drop when the ingredient slot could not hold it, which
    /// is how a dragon's breath leaves its empty bottle behind.
    fn brew(&mut self) -> Option<ItemStack> {
        let ingredient = self.items[SLOT_INGREDIENT].clone();
        for slot in 0..BOTTLE_SLOTS {
            self.items[slot] = potion_brewing::mix_with(&ingredient, &self.items[slot]);
        }

        self.consume_ingredient()
    }

    /// Spends the ingredient currently in the event-mutated snapshot.
    fn consume_ingredient(&mut self) -> Option<ItemStack> {
        if self.items[SLOT_INGREDIENT].is_empty() {
            return None;
        }

        let remainder = self.items[SLOT_INGREDIENT].item().get_crafting_remainder();
        let count = self.items[SLOT_INGREDIENT].count() - 1;
        self.items[SLOT_INGREDIENT].set_count(count);

        if remainder.is_empty() {
            return None;
        }
        if self.items[SLOT_INGREDIENT].is_empty() {
            self.items[SLOT_INGREDIENT] = remainder;
            return None;
        }
        Some(remainder)
    }

    /// Returns which bottle slots are occupied.
    ///
    /// Vanilla parity: `BrewingStandBlockEntity.getPotionBits`.
    fn bottle_flags(&self) -> [bool; BOTTLE_SLOTS] {
        array::from_fn(|slot| !self.items[slot].is_empty())
    }
}

impl BrewingStandBlockEntity {
    /// Creates a brewing stand block entity.
    #[must_use]
    pub fn new(level: Weak<World>, pos: BlockPos, state: BlockStateId) -> Self {
        let base = Arc::new(BlockEntityBase::new(
            &vanilla_block_entity_types::BREWING_STAND,
            level,
            pos,
            state,
        ));
        let container = Arc::new(SyncMutex::new(BrewingStandContainer {
            items: vec![ItemStack::empty(); BREWING_STAND_SLOTS],
            brew_time: 0,
            fuel: 0,
            recipe_brew_time: DEFAULT_RECIPE_BREW_TIME,
            custom_name: None,
            lock: LockCode::NO_LOCK,
            brewing_ingredient: None,
            last_bottle_flags: [false; BOTTLE_SLOTS],
        }));
        let shared_container: SharedContainer = container.clone();
        Self {
            container_ref: ContainerRef::owned_by_block_entity(shared_container, Arc::clone(&base)),
            base,
            identity: NEXT_BREWING_STAND_IDENTITY.fetch_add(1, Ordering::Relaxed),
            container,
            data: Arc::new(BrewingStandDataSlots::default()),
        }
    }

    /// Returns the progress values shared with open menus.
    #[must_use]
    pub fn data(&self) -> Arc<BrewingStandDataSlots> {
        Arc::clone(&self.data)
    }

    /// Atomically snapshots every value exposed by Bukkit's block state.
    #[must_use]
    pub fn state_snapshot(&self) -> BrewingStandStateSnapshot {
        let container = self.container.lock();
        BrewingStandStateSnapshot {
            identity: self.identity,
            items: container.items.clone(),
            brew_time: container.brew_time,
            recipe_brew_time: container.recipe_brew_time,
            fuel: container.fuel,
            custom_name: container.custom_name.clone(),
            lock: container.lock.clone(),
        }
    }

    /// Reads one live slot only while this exact placed instance is current.
    #[must_use]
    pub fn live_item(&self, identity: u64, slot: usize) -> Option<ItemStack> {
        if identity != self.identity || slot >= BREWING_STAND_SLOTS {
            return None;
        }
        let world = self.get_level()?;
        if !self.is_current_in(&world, self.get_block_pos()) {
            return None;
        }
        Some(self.container.lock().items[slot].clone())
    }

    /// Writes one live slot only while this exact placed instance is current.
    #[must_use]
    pub fn set_live_item(&self, identity: u64, slot: usize, item: ItemStack) -> bool {
        if identity != self.identity || slot >= BREWING_STAND_SLOTS {
            return false;
        }
        let Some(world) = self.get_level() else {
            return false;
        };
        if !self.is_current_in(&world, self.get_block_pos()) {
            return false;
        }
        {
            let mut container = self.container.lock();
            container.items[slot] = item;
        }
        self.set_changed();
        world.send_block_updated(self.get_block_pos());
        true
    }

    /// Applies one complete Bukkit snapshot under the block entity's state lock.
    #[must_use]
    pub fn apply_state_snapshot(&self, snapshot: BrewingStandStateSnapshot) -> bool {
        if snapshot.identity != self.identity
            || snapshot.items.len() != BREWING_STAND_SLOTS
            || snapshot.recipe_brew_time <= 0
        {
            return false;
        }
        let Some(world) = self.get_level() else {
            return false;
        };
        if !self.is_current_in(&world, self.get_block_pos()) {
            return false;
        }
        {
            let mut container = self.container.lock();
            container.items = snapshot.items;
            container.brew_time = snapshot.brew_time;
            container.recipe_brew_time = snapshot.recipe_brew_time;
            container.fuel = snapshot.fuel;
            container.custom_name = snapshot.custom_name;
            container.lock = snapshot.lock;
            container.brewing_ingredient =
                (container.brew_time > 0).then(|| container.items[SLOT_INGREDIENT].clone());
            self.publish_data(&container);
        }
        self.set_changed();
        world.send_block_updated(self.get_block_pos());
        true
    }

    /// Returns the name an anvil gave this brewing stand, if any.
    ///
    /// Vanilla parity: `Nameable.getCustomName`.
    #[must_use]
    pub fn custom_name(&self) -> Option<TextComponent> {
        self.container.lock().custom_name.clone()
    }

    fn is_current_in(&self, world: &World, pos: BlockPos) -> bool {
        world
            .get_block_entity(pos)
            .is_some_and(|current| ptr::eq(current.base(), self.base()))
    }

    /// Runs one vanilla brewing tick with an optional owner callback.
    ///
    /// The ordinary path keeps the existing direct mutex and allocation-free
    /// tick. Only a completed, owned brew enters `ContainerLockGuard`, whose
    /// unlocked callback boundary lets listeners inspect the same stand.
    fn tick_with_brew_dispatch<H, D>(&self, world: &Arc<World>, mut has_owner: H, mut dispatch: D)
    where
        H: FnMut() -> bool,
        D: FnMut(&mut BrewEvent),
    {
        let pos = self.get_block_pos();
        let mut container = self.container.lock();

        // Vanilla parity: fuel is taken the moment the stand runs dry, whether
        // or not there is anything to brew, which is why a stand refills itself
        // from a hopper before a player ever opens it.
        if container.fuel <= 0 && potion_brewing::is_brewing_fuel(&container.items[SLOT_FUEL]) {
            container.fuel = potion_brewing::FUEL_USES;
            let remaining = container.items[SLOT_FUEL].count() - 1;
            container.items[SLOT_FUEL].set_count(remaining);
        }

        let brewable = container.is_brewable();
        let mut remainder = None;
        let mut completed = false;
        let mut pending_completion = None;

        if container.brew_time > 0 {
            container.brew_time -= 1;
            let ingredient_swapped = container
                .brewing_ingredient
                .as_ref()
                .is_none_or(|started| !container.items[SLOT_INGREDIENT].is(started.item()));

            if container.brew_time == 0 && brewable {
                if has_owner() {
                    pending_completion = Some(BrewCompletionSnapshot::capture(&container));
                } else {
                    remainder = container.brew();
                    completed = true;
                }
            } else if !brewable || ingredient_swapped {
                container.brew_time = 0;
            }
        } else if brewable && container.fuel > 0 {
            container.fuel -= 1;
            container.brew_time = container.recipe_brew_time;
            container.brewing_ingredient = Some(container.items[SLOT_INGREDIENT].clone());
        }

        let mut flags = container.bottle_flags();
        let mut flags_changed = false;
        if pending_completion.is_none() {
            flags_changed = flags != container.last_bottle_flags;
            if flags_changed {
                container.last_bottle_flags = flags;
            }
        }
        self.publish_data(&container);
        drop(container);

        if let Some(snapshot) = pending_completion {
            let Some(effects) = self.finish_owned_brew(world, pos, snapshot, &mut dispatch) else {
                return;
            };
            remainder = effects.remainder;
            completed = effects.completed;
            flags = effects.flags;
            flags_changed = effects.flags_changed;
        }

        if let Some(remainder) = remainder {
            #[cfg(test)]
            BREW_WORLD_EFFECTS.with(|count| count.set(count.get() + 1));
            world.drop_item_stack(pos, remainder);
        }
        if completed {
            #[cfg(test)]
            BREW_WORLD_EFFECTS.with(|count| count.set(count.get() + 1));
            world.level_event(SOUND_BREWING_STAND_BREW, pos, 0, None);
        }
        if flags_changed {
            #[cfg(test)]
            BREW_WORLD_EFFECTS.with(|count| count.set(count.get() + 1));
            let mut state = self.get_block_state();
            for (property, filled) in HAS_BOTTLE.iter().zip(flags) {
                state = state.set_value(*property, filled);
            }
            world.set_block(pos, state, UpdateFlags::UPDATE_CLIENTS);
        }
    }

    fn publish_data(&self, container: &BrewingStandContainer) {
        self.data
            .brew_time
            .store(container.brew_time, Ordering::Relaxed);
        self.data.fuel.store(container.fuel, Ordering::Relaxed);
    }

    fn finish_owned_brew<D>(
        &self,
        world: &World,
        pos: BlockPos,
        snapshot: BrewCompletionSnapshot,
        dispatch: &mut D,
    ) -> Option<BrewCompletionEffects>
    where
        D: FnMut(&mut BrewEvent),
    {
        let mut guard = ContainerLockGuard::lock_all(&[&self.container_ref]);
        let id = self.container_ref.container_id();
        if !guard
            .get_typed::<BrewingStandContainer>(id)
            .is_some_and(|current| snapshot.still_matches(current))
        {
            return None;
        }
        let results = (0..BOTTLE_SLOTS)
            .map(|slot| {
                potion_brewing::mix_with(&snapshot.items[SLOT_INGREDIENT], &snapshot.items[slot])
            })
            .collect();
        let mut event = BrewEvent::new(
            world.key.to_string(),
            pos,
            snapshot.items.clone(),
            results,
            snapshot.fuel,
        );
        guard.run_unlocked(|| dispatch(&mut event));
        drop(guard);

        // A listener may remove or replace the stand while no container lock is
        // held. The detached Rust object must never commit into that position.
        if !self.is_current_in(world, pos) {
            return None;
        }

        let mut guard = ContainerLockGuard::lock_all(&[&self.container_ref]);
        let snapshot_is_current = guard
            .get_typed::<BrewingStandContainer>(id)
            .is_some_and(|current| snapshot.still_matches(current));
        if snapshot_is_current && event.is_cancelled() {
            if let Some(current) = guard.get_typed::<BrewingStandContainer>(id) {
                self.publish_data(current);
            }
            return None;
        }
        let mut remainder = None;
        let mut completed = false;
        if snapshot_is_current
            && event.contents().len() == BREWING_STAND_SLOTS
            && let Some(current) = guard.get_typed_mut::<BrewingStandContainer>(id)
        {
            current.items.clone_from_slice(event.contents());
            for slot in 0..BOTTLE_SLOTS {
                current.items[slot] = event
                    .results()
                    .get(slot)
                    .cloned()
                    .unwrap_or_else(ItemStack::empty);
            }
            remainder = current.consume_ingredient();
            completed = true;
        }

        let current = guard.get_typed_mut::<BrewingStandContainer>(id)?;
        self.publish_data(current);
        let flags = current.bottle_flags();
        let flags_changed = flags != current.last_bottle_flags;
        if flags_changed {
            current.last_bottle_flags = flags;
        }
        Some(BrewCompletionEffects {
            remainder,
            completed,
            flags,
            flags_changed,
        })
    }
}

impl BlockEntity for BrewingStandBlockEntity {
    fn base(&self) -> &BlockEntityBase {
        &self.base
    }

    fn tick(&self, world: &Arc<World>) {
        self.tick_with_brew_dispatch(
            world,
            || {
                world
                    .server()
                    .is_some_and(|server| server.events().listener_count::<BrewEvent>() > 0)
            },
            |event| world.fire_event(event),
        );
    }

    fn pre_remove_side_effects(&self, pos: BlockPos, _state: BlockStateId) {
        let items = {
            let mut container = self.container.lock();
            mem::replace(
                &mut container.items,
                vec![ItemStack::empty(); BREWING_STAND_SLOTS],
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
        let mut container = self.container.lock();
        container.items.fill(ItemStack::empty());
        container.custom_name = nbt_view
            .get(CUSTOM_NAME_TAG)
            .map(|tag| tag.to_owned())
            .as_ref()
            .and_then(TextComponent::from_nbt);
        container.lock = nbt_view
            .get(LOCK_TAG)
            .and_then(LockCode::from_nbt_tag)
            .unwrap_or(LockCode::NO_LOCK);

        if let Some(items_list) = nbt_view.list("Items")
            && let Some(compounds) = items_list.compounds()
        {
            for compound in compounds {
                if let Some(slot) = compound.byte("Slot") {
                    let slot = slot as usize;
                    if slot < BREWING_STAND_SLOTS
                        && let Some(item) = ItemStack::from_borrowed_compound(&compound)
                    {
                        container.items[slot] = item;
                    }
                }
            }
        }

        container.brew_time = nbt_view.short("BrewTime").unwrap_or(0).into();
        container.fuel = nbt_view.byte("Fuel").unwrap_or(0).into();
        container.recipe_brew_time = DEFAULT_RECIPE_BREW_TIME;
        // Vanilla reloads the in-progress ingredient from the slot, so a brew
        // interrupted by a save resumes instead of cancelling on the next tick.
        container.brewing_ingredient =
            (container.brew_time > 0).then(|| container.items[SLOT_INGREDIENT].clone());
        container.last_bottle_flags = container.bottle_flags();
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        let container = self.container.lock();
        while nbt.remove(CUSTOM_NAME_TAG).is_some() {}
        if let Some(name) = &container.custom_name {
            nbt.insert(CUSTOM_NAME_TAG, name.to_codec_nbt());
        }
        while nbt.remove(LOCK_TAG).is_some() {}
        if container.lock != LockCode::NO_LOCK {
            nbt.insert(LOCK_TAG, container.lock.clone().to_nbt_tag());
        }
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
        nbt.insert("BrewTime", container.brew_time as i16);
        nbt.insert("Fuel", container.fuel as i8);
    }

    fn get_update_tag(&self) -> Option<NbtCompound> {
        None
    }

    fn container_ref(&self) -> Option<ContainerRef> {
        Some(self.container_ref.clone())
    }

    /// Vanilla parity: `BaseContainerBlockEntity.getName`, which falls back to
    /// the block's own name.
    fn display_name(&self, default_name: TextComponent) -> TextComponent {
        self.custom_name().unwrap_or(default_name)
    }

    /// Vanilla parity: the name and lock fields of
    /// `BaseContainerBlockEntity.collectImplicitComponents`.
    fn collect_implicit_components(&self, components: &mut DataComponentMap) {
        let container = self.container.lock();
        components.set(CUSTOM_NAME, container.custom_name.clone());
        components.set(
            LOCK,
            (container.lock != LockCode::NO_LOCK).then(|| container.lock.clone()),
        );
    }

    /// Vanilla parity: the name and lock fields of
    /// `BaseContainerBlockEntity.applyImplicitComponents`.
    fn apply_implicit_components(&self, input: &ImplicitComponentInput<'_>) {
        let mut container = self.container.lock();
        container.custom_name = input.get(CUSTOM_NAME);
        container.lock = input.get(LOCK).unwrap_or(LockCode::NO_LOCK);
    }
}

/// Slots a hopper above a brewing stand may fill.
///
/// Vanilla parity: `BrewingStandBlockEntity.SLOTS_FOR_UP`.
static SLOTS_FOR_UP: [usize; 1] = [SLOT_INGREDIENT];

/// Slots a hopper below a brewing stand may drain.
///
/// Vanilla parity: `BrewingStandBlockEntity.SLOTS_FOR_DOWN`.
static SLOTS_FOR_DOWN: [usize; 4] = [0, 1, 2, SLOT_INGREDIENT];

/// Slots a hopper at the side of a brewing stand may reach.
///
/// Vanilla parity: `BrewingStandBlockEntity.SLOTS_FOR_SIDES`. Fuel goes in from
/// the side, which is why an automatic stand feeds blaze powder sideways and
/// ingredients from above.
static SLOTS_FOR_SIDES: [usize; 4] = [0, 1, 2, SLOT_FUEL];

impl Container for BrewingStandContainer {
    fn items(&self) -> &[ItemStack] {
        &self.items
    }

    fn items_mut(&mut self) -> &mut [ItemStack] {
        &mut self.items
    }

    fn get_container_size(&self) -> usize {
        BREWING_STAND_SLOTS
    }

    /// Vanilla parity: `BrewingStandBlockEntity.canPlaceItem`.
    fn can_place_item(&self, slot: usize, stack: &ItemStack) -> bool {
        match slot {
            SLOT_INGREDIENT => potion_brewing::is_ingredient(stack),
            SLOT_FUEL => potion_brewing::is_brewing_fuel(stack),
            // A bottle slot takes one bottle and only while it is empty, which
            // is what stops a hopper stacking three potions into one slot.
            _ => {
                (stack.is(&vanilla_items::POTION)
                    || stack.is(&vanilla_items::SPLASH_POTION)
                    || stack.is(&vanilla_items::LINGERING_POTION)
                    || stack.is(&vanilla_items::GLASS_BOTTLE))
                    && self.items[slot].is_empty()
            }
        }
    }

    /// Vanilla parity: `BrewingStandBlockEntity.getSlotsForFace`.
    fn slots_for_face(&self, direction: Direction) -> SlotsForFace {
        match direction {
            Direction::Up => SlotsForFace::Explicit(&SLOTS_FOR_UP),
            Direction::Down => SlotsForFace::Explicit(&SLOTS_FOR_DOWN),
            _ => SlotsForFace::Explicit(&SLOTS_FOR_SIDES),
        }
    }

    /// Vanilla parity: `BrewingStandBlockEntity.canTakeItemThroughFace`. Only an
    /// emptied bottle leaves the ingredient slot, so a hopper cannot steal the
    /// nether wart a stand is about to brew with.
    fn can_take_item_through_face(
        &self,
        slot: usize,
        stack: &ItemStack,
        _direction: Direction,
    ) -> bool {
        if slot == SLOT_INGREDIENT {
            return stack.is(&vanilla_items::GLASS_BOTTLE);
        }
        true
    }

    fn get_max_stack_size(&self) -> i32 {
        64
    }

    fn set_changed(&mut self) {}
}

#[cfg(test)]
mod tests {
    use crate::behavior::init_behaviors;
    use crate::block_entity::{SharedBlockEntity, init_block_entities};
    use crate::chunk::Chunk;
    use crate::chunk::chunk_holder::ChunkHolder;
    use crate::chunk::status::ChunkStatus;
    use crate::event::BrewEvent;
    use crate::test_support::{fresh_test_world, insert_ready_full_chunk};
    use foton_registry::potion_brewing::potion_item;
    use foton_registry::{init_vanilla_registry, vanilla_blocks, vanilla_potions};

    use super::*;
    use foton_registry::data_components::PotionContents;
    use foton_registry::data_components::vanilla_components::POTION_CONTENTS;
    use foton_utils::ChunkPos;

    fn container() -> BrewingStandContainer {
        init_vanilla_registry();
        BrewingStandContainer {
            items: vec![ItemStack::empty(); BREWING_STAND_SLOTS],
            brew_time: 0,
            fuel: 0,
            recipe_brew_time: DEFAULT_RECIPE_BREW_TIME,
            custom_name: None,
            lock: LockCode::NO_LOCK,
            brewing_ingredient: None,
            last_bottle_flags: [false; BOTTLE_SLOTS],
        }
    }

    fn water_bottle() -> ItemStack {
        potion_item(&vanilla_items::POTION, &vanilla_potions::WATER)
    }

    fn ready_stand(
        key: &'static str,
    ) -> (Arc<World>, Arc<BrewingStandBlockEntity>, Arc<ChunkHolder>) {
        init_vanilla_registry();
        init_behaviors();
        init_block_entities();
        let world = fresh_test_world(key);
        let pos = BlockPos::new(3, 64, -2);
        let holder = insert_ready_full_chunk(&world, ChunkPos::from_block_pos(pos));
        assert!(world.set_block(
            pos,
            vanilla_blocks::BREWING_STAND.default_state(),
            UpdateFlags::UPDATE_NONE,
        ));
        let stand = Arc::new(BrewingStandBlockEntity::new(
            Arc::downgrade(&world),
            pos,
            vanilla_blocks::BREWING_STAND.default_state(),
        ));
        {
            let mut container = stand.container.lock();
            container.items[0] = water_bottle();
            container.items[1] = water_bottle();
            container.items[2] = water_bottle();
            container.items[SLOT_INGREDIENT] = ItemStack::new(&vanilla_items::NETHER_WART);
            container.items[SLOT_FUEL] = ItemStack::new(&vanilla_items::BLAZE_POWDER);
            container.brew_time = 1;
            container.fuel = 7;
            container.brewing_ingredient = Some(container.items[SLOT_INGREDIENT].clone());
        }
        let entity: SharedBlockEntity = stand.clone();
        assert!(world.set_block_entity(entity));
        (world, stand, holder)
    }

    fn reset_brew_test_counters() {
        BREW_SNAPSHOT_CAPTURES.with(|count| count.set(0));
        BREW_WORLD_EFFECTS.with(|count| count.set(0));
    }

    fn brew_test_counters() -> (usize, usize) {
        (
            BREW_SNAPSHOT_CAPTURES.with(Cell::get),
            BREW_WORLD_EFFECTS.with(Cell::get),
        )
    }

    fn assert_stack_matches(actual: &ItemStack, expected: &ItemStack) {
        assert!(
            ItemStack::matches(actual, expected),
            "expected {:?} x{}, got {:?} x{}",
            expected.item().key,
            expected.count(),
            actual.item().key,
            actual.count()
        );
    }

    #[test]
    fn brew_event_exposes_mutable_snapshot_results_fuel_and_cancellation() {
        let contents = vec![
            water_bottle(),
            water_bottle(),
            water_bottle(),
            ItemStack::new(&vanilla_items::NETHER_WART),
            ItemStack::new(&vanilla_items::BLAZE_POWDER),
        ];
        let results = vec![
            potion_item(&vanilla_items::POTION, &vanilla_potions::AWKWARD),
            potion_item(&vanilla_items::POTION, &vanilla_potions::AWKWARD),
            potion_item(&vanilla_items::POTION, &vanilla_potions::AWKWARD),
        ];
        let mut event = BrewEvent::new(
            "minecraft:overworld".to_owned(),
            BlockPos::new(3, 64, -2),
            contents,
            results,
            7,
        );

        event.contents_mut()[SLOT_FUEL] = ItemStack::empty();
        event.results_mut().truncate(1);
        event.set_cancelled(true);

        assert_eq!(event.contents().len(), BREWING_STAND_SLOTS);
        assert!(event.contents()[SLOT_FUEL].is_empty());
        assert_eq!(event.results().len(), 1);
        assert_eq!(event.fuel_level(), 7);
        assert!(event.is_cancelled());
    }

    #[test]
    fn cancelled_completion_preserves_inventory_fuel_and_can_restart_next_tick() {
        let (world, stand, _) = ready_stand("cancelled_brew_event");
        let before = stand.container.lock().items.clone();
        let unlocked_container = Arc::clone(&stand.container);
        let mut listener = |event: &mut BrewEvent| {
            assert!(
                unlocked_container.try_lock().is_some(),
                "the brewing container lock must be released during listeners"
            );
            event.contents_mut()[SLOT_FUEL] = ItemStack::empty();
            event.results_mut().clear();
            event.set_cancelled(true);
        };

        stand.tick_with_brew_dispatch(&world, || true, &mut listener);

        {
            let container = stand.container.lock();
            for (actual, expected) in container.items.iter().zip(&before) {
                assert_stack_matches(actual, expected);
            }
            assert_eq!(container.fuel, 7);
            assert_eq!(container.brew_time, 0);
        }

        stand.tick_with_brew_dispatch(&world, || false, |_| {});
        let container = stand.container.lock();
        assert_eq!(container.fuel, 6);
        assert_eq!(container.brew_time, potion_brewing::BREWING_TIME_TICKS);
    }

    #[test]
    fn successful_completion_applies_snapshot_mutations_and_short_results() {
        let (world, stand, _) = ready_stand("mutable_brew_event");
        let mut listener = |event: &mut BrewEvent| {
            let mut ingredient = ItemStack::new(&vanilla_items::REDSTONE);
            ingredient.set_count(2);
            event.contents_mut()[SLOT_INGREDIENT] = ingredient;
            event.contents_mut()[SLOT_FUEL] = ItemStack::new(&vanilla_items::DIRT);
            event.results_mut()[0] = ItemStack::new(&vanilla_items::SUGAR);
            event.results_mut().truncate(1);
        };

        stand.tick_with_brew_dispatch(&world, || true, &mut listener);

        let container = stand.container.lock();
        assert!(container.items[0].is(&vanilla_items::SUGAR));
        assert!(container.items[1].is_empty());
        assert!(container.items[2].is_empty());
        assert!(container.items[SLOT_INGREDIENT].is(&vanilla_items::REDSTONE));
        assert_eq!(container.items[SLOT_INGREDIENT].count(), 1);
        assert!(container.items[SLOT_FUEL].is(&vanilla_items::DIRT));
        assert_eq!(
            container.fuel, 7,
            "completion does not consume another fuel use"
        );
    }

    #[test]
    fn reentrant_inventory_change_is_not_overwritten_by_stale_completion() {
        let (world, stand, _) = ready_stand("stale_brew_event");
        let reentrant_container = Arc::clone(&stand.container);
        let mut listener = move |_event: &mut BrewEvent| {
            reentrant_container.lock().items[SLOT_FUEL] = ItemStack::new(&vanilla_items::DIAMOND);
        };

        stand.tick_with_brew_dispatch(&world, || true, &mut listener);

        let container = stand.container.lock();
        assert!(container.items[SLOT_FUEL].is(&vanilla_items::DIAMOND));
        for slot in 0..BOTTLE_SLOTS {
            let potion = container.items[slot]
                .get(POTION_CONTENTS)
                .and_then(PotionContents::potion)
                .expect("stale completion should leave each water bottle intact");
            assert!(ptr::eq(potion.value(), &raw const vanilla_potions::WATER));
        }
        assert!(container.items[SLOT_INGREDIENT].is(&vanilla_items::NETHER_WART));
    }

    #[test]
    fn ownerless_completion_commits_vanilla_without_dispatch() {
        let (world, stand, _) = ready_stand("ownerless_brew_event");

        reset_brew_test_counters();
        let mut dispatches = 0;

        stand.tick_with_brew_dispatch(&world, || false, |_| dispatches += 1);

        let container = stand.container.lock();
        for slot in 0..BOTTLE_SLOTS {
            let potion = container.items[slot]
                .get(POTION_CONTENTS)
                .and_then(PotionContents::potion)
                .expect("brewed bottle should retain potion identity");
            assert!(ptr::eq(potion.value(), &raw const vanilla_potions::AWKWARD));
        }
        assert!(container.items[SLOT_INGREDIENT].is_empty());
        assert_eq!(container.fuel, 7);
        assert_eq!(dispatches, 0);
        assert_eq!(
            brew_test_counters().0,
            0,
            "the direct path must not allocate an event snapshot"
        );
    }

    #[test]
    fn listener_removing_stand_aborts_detached_commit_and_world_effects() {
        let (world, stand, _) = ready_stand("removed_during_brew_event");
        let pos = stand.get_block_pos();
        reset_brew_test_counters();

        stand.tick_with_brew_dispatch(
            &world,
            || true,
            |_| {
                assert!(world.remove_block_entity(pos));
            },
        );

        let recreated = world
            .get_block_entity(pos)
            .expect("the live brewing-stand block recreates its missing block entity");
        assert!(!ptr::eq(recreated.base(), stand.base()));
        assert_eq!(brew_test_counters(), (1, 0));
        let container = stand.container.lock();
        assert!(container.items[0].is(&vanilla_items::POTION));
        assert!(container.items[SLOT_INGREDIENT].is(&vanilla_items::NETHER_WART));
    }

    #[test]
    fn listener_replacing_stand_cannot_commit_or_touch_replacement_block() {
        let (world, stand, _) = ready_stand("replaced_during_brew_event");
        let pos = stand.get_block_pos();
        let replacement = Arc::new(BrewingStandBlockEntity::new(
            Arc::downgrade(&world),
            pos,
            vanilla_blocks::BREWING_STAND.default_state(),
        ));
        let replacement_entity: SharedBlockEntity = replacement.clone();
        reset_brew_test_counters();

        stand.tick_with_brew_dispatch(
            &world,
            || true,
            |_| {
                assert!(world.set_block_entity(replacement_entity.clone()));
            },
        );

        let current = world
            .get_block_entity(pos)
            .expect("replacement should remain installed");
        assert!(Arc::ptr_eq(&current, &replacement_entity));
        assert_eq!(brew_test_counters(), (1, 0));
        assert!(
            replacement
                .container
                .lock()
                .items
                .iter()
                .all(ItemStack::is_empty)
        );
        let detached = stand.container.lock();
        assert!(detached.items[0].is(&vanilla_items::POTION));
        assert!(detached.items[SLOT_INGREDIENT].is(&vanilla_items::NETHER_WART));
    }

    #[test]
    fn stale_identity_cannot_read_write_or_apply_after_replacement() {
        let (world, stand, _) = ready_stand("stale_plugin_snapshot");
        let pos = stand.get_block_pos();
        let snapshot = stand.state_snapshot();
        let replacement = Arc::new(BrewingStandBlockEntity::new(
            Arc::downgrade(&world),
            pos,
            vanilla_blocks::BREWING_STAND.default_state(),
        ));
        let replacement_entity: SharedBlockEntity = replacement.clone();
        assert!(world.set_block_entity(replacement_entity));

        assert!(stand.live_item(snapshot.identity, 0).is_none());
        assert!(!stand.set_live_item(
            snapshot.identity,
            0,
            ItemStack::new(&vanilla_items::DIAMOND),
        ));
        assert!(!stand.apply_state_snapshot(snapshot));
        assert!(
            replacement
                .live_item(replacement.identity, 0)
                .is_some_and(|item| item.is_empty())
        );
    }

    #[test]
    fn live_slot_write_marks_storage_dirty_and_queues_client_update() {
        let (_world, stand, holder) = ready_stand("live_plugin_inventory_write");
        holder
            .try_chunk(ChunkStatus::Full)
            .expect("brewing stand chunk should remain loaded")
            .clear_dirty();
        let _ = holder.take_changed_blocks();
        holder.clear_broadcast_queued();
        let identity = stand.state_snapshot().identity;

        assert!(stand.set_live_item(identity, SLOT_FUEL, ItemStack::new(&vanilla_items::DIAMOND),));

        assert!(
            holder
                .try_chunk(ChunkStatus::Full)
                .is_some_and(Chunk::is_dirty),
            "a live inventory mutation must be persisted"
        );
        assert!(
            holder.has_changes_to_broadcast(),
            "a live inventory mutation must notify tracking clients"
        );
    }

    #[test]
    fn applied_inventory_keeps_bottle_flags_pending_until_the_world_updates() {
        let (world, stand, _) = ready_stand("plugin_snapshot_bottle_flags");
        let pos = stand.get_block_pos();
        let mut filled_state = vanilla_blocks::BREWING_STAND.default_state();
        for property in HAS_BOTTLE {
            filled_state = filled_state.set_value(property, true);
        }
        assert!(world.set_block(pos, filled_state, UpdateFlags::UPDATE_NONE));
        stand.container.lock().last_bottle_flags = [true; BOTTLE_SLOTS];
        let mut snapshot = stand.state_snapshot();
        snapshot.items[..BOTTLE_SLOTS].fill(ItemStack::empty());
        snapshot.brew_time = 0;

        assert!(stand.apply_state_snapshot(snapshot));
        stand.tick_with_brew_dispatch(&world, || false, |_| {});

        let updated = world.get_block_state(pos);
        assert!(
            HAS_BOTTLE
                .iter()
                .all(|property| !updated.get_value(*property)),
            "the next tick must publish bottle occupancy from the applied inventory"
        );
    }

    #[test]
    fn one_ingredient_converts_every_bottle_it_applies_to() {
        let mut stand = container();
        stand.items[0] = water_bottle();
        stand.items[1] = water_bottle();
        stand.items[2] = ItemStack::empty();
        stand.items[SLOT_INGREDIENT] = ItemStack::new(&vanilla_items::NETHER_WART);

        assert!(stand.is_brewable());
        assert!(stand.brew().is_none());

        // Both filled bottles converted; the ingredient was spent once.
        for slot in 0..2 {
            let contents = stand.items[slot]
                .get(POTION_CONTENTS)
                .and_then(PotionContents::potion)
                .expect("brewed bottle holds a potion");
            assert!(ptr::eq(
                contents.value(),
                &raw const vanilla_potions::AWKWARD
            ));
        }
        assert!(stand.items[SLOT_INGREDIENT].is_empty());
    }

    #[test]
    fn a_bottle_the_ingredient_does_not_touch_is_left_alone() {
        let mut stand = container();
        stand.items[0] = water_bottle();
        // Awkward potion takes nothing from a second nether wart.
        stand.items[1] = potion_item(&vanilla_items::POTION, &vanilla_potions::AWKWARD);
        stand.items[SLOT_INGREDIENT] = ItemStack::new(&vanilla_items::NETHER_WART);

        assert!(stand.is_brewable());
        stand.brew();

        let untouched = stand.items[1]
            .get(POTION_CONTENTS)
            .and_then(PotionContents::potion)
            .expect("holds a potion");
        assert!(ptr::eq(
            untouched.value(),
            &raw const vanilla_potions::AWKWARD
        ));
    }

    #[test]
    fn nothing_brews_without_a_bottle_the_ingredient_applies_to() {
        let mut stand = container();
        stand.items[SLOT_INGREDIENT] = ItemStack::new(&vanilla_items::NETHER_WART);
        assert!(!stand.is_brewable());

        stand.items[0] = ItemStack::new(&vanilla_items::GLASS_BOTTLE);
        assert!(
            !stand.is_brewable(),
            "an empty glass bottle holds no potion to convert"
        );
    }

    #[test]
    fn only_blaze_powder_goes_in_the_fuel_slot() {
        let stand = container();
        assert!(stand.can_place_item(SLOT_FUEL, &ItemStack::new(&vanilla_items::BLAZE_POWDER)));
        assert!(!stand.can_place_item(SLOT_FUEL, &ItemStack::new(&vanilla_items::REDSTONE)));
    }

    #[test]
    fn a_bottle_slot_takes_one_bottle_and_no_more() {
        let mut stand = container();
        assert!(stand.can_place_item(0, &water_bottle()));
        stand.items[0] = water_bottle();
        assert!(
            !stand.can_place_item(0, &water_bottle()),
            "a filled bottle slot refuses a second"
        );
    }

    #[test]
    fn the_ingredient_slot_refuses_what_brews_nothing() {
        let stand = container();
        assert!(stand.can_place_item(
            SLOT_INGREDIENT,
            &ItemStack::new(&vanilla_items::NETHER_WART)
        ));
        assert!(!stand.can_place_item(SLOT_INGREDIENT, &ItemStack::new(&vanilla_items::DIRT)));
    }

    #[test]
    fn each_face_exposes_the_slots_vanilla_gives_it() {
        let stand = container();
        // Ingredients from above, fuel from the side, output from below.
        assert_eq!(
            stand
                .slots_for_face(Direction::Up)
                .into_iter()
                .collect::<Vec<_>>(),
            vec![SLOT_INGREDIENT]
        );
        assert!(
            stand
                .slots_for_face(Direction::North)
                .into_iter()
                .any(|slot| slot == SLOT_FUEL)
        );
        assert!(
            !stand
                .slots_for_face(Direction::Down)
                .into_iter()
                .any(|slot| slot == SLOT_FUEL),
            "a hopper below must not drain the blaze powder"
        );
    }

    #[test]
    fn only_an_emptied_bottle_leaves_the_ingredient_slot() {
        let stand = container();
        assert!(stand.can_take_item_through_face(
            SLOT_INGREDIENT,
            &ItemStack::new(&vanilla_items::GLASS_BOTTLE),
            Direction::Down
        ));
        assert!(!stand.can_take_item_through_face(
            SLOT_INGREDIENT,
            &ItemStack::new(&vanilla_items::NETHER_WART),
            Direction::Down
        ));
    }

    #[test]
    fn a_dragons_breath_bottle_comes_back() {
        let mut stand = container();
        stand.items[0] = potion_item(&vanilla_items::SPLASH_POTION, &vanilla_potions::HEALING);
        let mut breath = ItemStack::new(&vanilla_items::DRAGON_BREATH);
        breath.set_count(1);
        stand.items[SLOT_INGREDIENT] = breath;

        assert!(stand.is_brewable());
        let dropped = stand.brew();

        assert!(stand.items[0].is(&vanilla_items::LINGERING_POTION));
        // The last dragon's breath leaves its empty bottle in the slot rather
        // than on the floor.
        assert!(dropped.is_none());
        assert!(stand.items[SLOT_INGREDIENT].is(&vanilla_items::GLASS_BOTTLE));
    }
}
