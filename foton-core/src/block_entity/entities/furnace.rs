//! Furnace, smoker and blast furnace block entity.
//!
//! Vanilla parity: `AbstractFurnaceBlockEntity`. The three variants share this
//! implementation and differ only by which recipe family they look up and by a
//! burn-duration multiplier, exactly as the vanilla subclasses do.

use std::{
    mem,
    sync::{
        Arc, Weak,
        atomic::{AtomicI32, Ordering},
    },
};

use foton_registry::blocks::block_state_ext::BlockStateExt;
use foton_registry::blocks::properties::BlockStateProperties;
use foton_registry::recipe::CookingKind;
use foton_registry::{
    REGISTRY, block_entity_type::BlockEntityType, fuel, item_stack::ItemStack,
    vanilla_block_entity_types, vanilla_items,
};
use foton_utils::types::UpdateFlags;
use foton_utils::{
    BlockPos, BlockStateId, Direction, DowncastType, DowncastTypeKey, Identifier, locks::SyncMutex,
};
use rustc_hash::FxHashMap;
use simdnbt::ToNbtTag;
use simdnbt::borrow::{BaseNbtCompound as BorrowedNbtCompound, NbtCompound as NbtCompoundView};
use simdnbt::owned::{NbtCompound, NbtList, NbtTag};

use crate::block_entity::{BlockEntity, BlockEntityBase, BlockEntityName, ImplicitComponentInput};
use crate::event::{Event as _, FurnaceBurnEvent, FurnaceSmeltEvent, FurnaceStartSmeltEvent};
use crate::inventory::container::{Container, SlotsForFace};
use crate::inventory::lock::{ContainerRef, SharedContainer};
use crate::world::World;
use foton_registry::data_components::DataComponentMap;
use text_components::TextComponent;

/// Slot holding the item being cooked.
pub const SLOT_INPUT: usize = 0;
/// Slot holding the fuel.
pub const SLOT_FUEL: usize = 1;
/// Slot holding the finished item.
pub const SLOT_RESULT: usize = 2;
/// Total slots in a furnace.
pub const FURNACE_SLOTS: usize = 3;

/// Ticks the cooking timer falls back by each tick when the furnace goes out.
///
/// Vanilla parity: `AbstractFurnaceBlockEntity.BURN_COOL_SPEED`.
const BURN_COOL_SPEED: i32 = 2;

/// Cooking time used when the input matches no recipe.
const DEFAULT_COOKING_TIME: i32 = 200;

/// Cooking progress mirrored to every open menu.
///
/// Vanilla parity: the four-entry `ContainerData` of `AbstractFurnaceBlockEntity`.
/// Vanilla shares the block entity object itself with the menu; Foton keeps the
/// block entity's state behind its own lock and publishes these four values so a
/// menu never has to take that lock while rendering.
#[derive(Debug, Default)]
pub struct FurnaceDataSlots {
    /// Ticks of fuel left in the current burn.
    pub lit_time_remaining: AtomicI32,
    /// Ticks the current burn started with.
    pub lit_total_time: AtomicI32,
    /// Ticks of progress on the current item.
    pub cooking_timer: AtomicI32,
    /// Ticks the current item needs in total.
    pub cooking_total_time: AtomicI32,
}

impl FurnaceDataSlots {
    /// Reads the four values in the order the vanilla protocol expects.
    #[must_use]
    pub fn snapshot(&self) -> [i16; 4] {
        [
            clamp_to_i16(self.lit_time_remaining.load(Ordering::Relaxed)),
            clamp_to_i16(self.lit_total_time.load(Ordering::Relaxed)),
            clamp_to_i16(self.cooking_timer.load(Ordering::Relaxed)),
            clamp_to_i16(self.cooking_total_time.load(Ordering::Relaxed)),
        ]
    }
}

/// Narrows a tick counter to the `i16` the protocol carries.
///
/// A lava bucket burns for 20 000 ticks, which fits; the clamp only guards
/// against future values that would silently wrap.
fn clamp_to_i16(value: i32) -> i16 {
    i16::try_from(value).unwrap_or(i16::MAX)
}

/// Furnace block entity.
pub struct FurnaceBlockEntity {
    base: Arc<BlockEntityBase>,
    container: Arc<SyncMutex<FurnaceContainer>>,
    container_ref: ContainerRef,
    data: Arc<FurnaceDataSlots>,
    /// Vanilla parity: the `name` of `BaseContainerBlockEntity`, the anvil
    /// name this block was placed with.
    name: BlockEntityName,
}

/// Items and cooking state, held under one lock because a tick mutates both.
struct FurnaceContainer {
    items: Vec<ItemStack>,
    /// Which recipe family this furnace accepts.
    kind: CookingKind,
    /// Ticks of fuel left in the current burn.
    lit_time_remaining: i32,
    /// Ticks the current burn started with, for the flame gauge.
    lit_total_time: i32,
    /// Ticks of progress on the current item.
    cooking_timer: i32,
    /// Ticks the current item needs in total.
    cooking_total_time: i32,
    /// Recipes cooked since a player last collected the output, for experience.
    recipes_used: FxHashMap<Identifier, i32>,
}

// SAFETY: This key is owned by Foton and uniquely identifies `FurnaceBlockEntity`.
unsafe impl DowncastType for FurnaceBlockEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:block_entity/furnace");
}

// SAFETY: This key is owned by Foton and uniquely identifies the independently
// lockable inventory data used by a furnace block entity.
unsafe impl DowncastType for FurnaceContainer {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("foton:container/furnace");
}

impl FurnaceContainer {
    /// Returns how long `fuel` burns in this furnace.
    ///
    /// Vanilla parity: `AbstractFurnaceBlockEntity.getBurnDuration`, halved by the
    /// smoker and blast furnace overrides.
    fn burn_duration(&self, fuel: &ItemStack) -> i32 {
        let base = fuel::burn_duration(fuel);
        match self.kind {
            CookingKind::Smelting => base,
            CookingKind::Blasting | CookingKind::Smoking => base / 2,
        }
    }

    /// Returns the result the current input would produce, if any.
    fn cooking_result(&self) -> Option<(Identifier, ItemStack, i32)> {
        let input = self.items.get(SLOT_INPUT)?;
        if input.is_empty() {
            return None;
        }
        let recipe = REGISTRY.recipes.find_cooking_recipe(self.kind, input)?;
        let result = recipe.assemble_result(1, false);
        if result.is_empty() {
            return None;
        }
        Some((recipe.id.clone(), result, recipe.cooking_time))
    }

    /// Returns whether the result slot can accept `burn_result`.
    ///
    /// Vanilla parity: `AbstractFurnaceBlockEntity.canBurn`.
    fn can_burn(&self, burn_result: &ItemStack) -> bool {
        let current = &self.items[SLOT_RESULT];
        if current.is_empty() {
            return true;
        }
        if !ItemStack::is_same_item_same_components(current, burn_result) {
            return false;
        }
        let combined = current.count() + burn_result.count();
        combined <= self.get_max_stack_size().min(burn_result.max_stack_size())
    }

    /// Moves the finished item into the result slot and consumes one input.
    /// Returns whether it did.
    ///
    /// Vanilla parity: `AbstractFurnaceBlockEntity.burn`, with Paper's
    /// allowance for a `FurnaceSmeltEvent` result: an empty one consumes the
    /// input and adds nothing, and one the result slot cannot take consumes
    /// nothing. The input is read again because a listener ran since the
    /// cook finished and may have taken it.
    fn burn(&mut self, burn_result: &ItemStack) -> bool {
        if self.items[SLOT_INPUT].is_empty() {
            return false;
        }
        if burn_result.is_empty() {
            // Nothing comes out.
        } else if self.items[SLOT_RESULT].is_empty() {
            self.items[SLOT_RESULT] = burn_result.clone();
        } else if ItemStack::is_same_item_same_components(&self.items[SLOT_RESULT], burn_result) {
            let grown = self.items[SLOT_RESULT].count() + burn_result.count();
            self.items[SLOT_RESULT].set_count(grown);
        } else {
            return false;
        }

        // Vanilla turns the bucket in the fuel slot back into a water bucket when a
        // wet sponge dries out.
        if self.items[SLOT_INPUT].is(&vanilla_items::WET_SPONGE)
            && self.items[SLOT_FUEL].is(&vanilla_items::BUCKET)
        {
            self.items[SLOT_FUEL] = ItemStack::new(&vanilla_items::WATER_BUCKET);
        }

        let remaining = self.items[SLOT_INPUT].count() - 1;
        self.items[SLOT_INPUT].set_count(remaining);
        true
    }

    /// Consumes one fuel item, leaving its crafting remainder behind.
    ///
    /// Vanilla parity: `AbstractFurnaceBlockEntity.consumeFuel`. This is what turns
    /// a lava bucket into an empty bucket.
    fn consume_fuel(&mut self) {
        let fuel_item = self.items[SLOT_FUEL].item();
        let remaining = self.items[SLOT_FUEL].count() - 1;
        self.items[SLOT_FUEL].set_count(remaining);

        if self.items[SLOT_FUEL].is_empty() {
            self.items[SLOT_FUEL] = fuel_item.get_crafting_remainder();
        }
    }

    /// Recomputes the cooking time for the current input.
    ///
    /// Vanilla parity: `AbstractFurnaceBlockEntity.getTotalCookTime`.
    fn total_cook_time(&self) -> i32 {
        self.cooking_result()
            .map_or(DEFAULT_COOKING_TIME, |(_, _, time)| time)
    }
}

/// What one tick of a lit or lightable furnace works on, read under the lock
/// so the events can run without it.
struct Cooking {
    fuel: ItemStack,
    burn_duration: i32,
    source: ItemStack,
    recipe: Identifier,
    result: ItemStack,
    cooking_time: i32,
    /// Whether no cook is under way, which is when one starts.
    starting: bool,
}

impl FurnaceBlockEntity {
    /// Lights the furnace from its fuel, after plugins had their say.
    ///
    /// Paper parity: `FurnaceBurnEvent`. Returns whether the furnace is lit
    /// afterwards, or `None` when a plugin refused.
    fn light(&self, world: &Arc<World>, pos: BlockPos, cooking: &Cooking) -> Option<bool> {
        let mut event = FurnaceBurnEvent::new(
            world.key.to_string(),
            pos,
            cooking.fuel.clone(),
            cooking.burn_duration,
        );
        world.fire_event(&mut event);
        if event.is_cancelled() {
            return None;
        }
        let mut container = self.container.lock();
        container.lit_time_remaining = event.burn_time();
        container.lit_total_time = event.burn_time();
        let lit = container.lit_time_remaining > 0;
        // A furnace a plugin lit without burning keeps its fuel item.
        if lit && event.burning() && event.consume_fuel() && !container.items[SLOT_FUEL].is_empty()
        {
            container.consume_fuel();
        }
        Some(lit)
    }

    /// Advances the cook one tick, and finishes it when it is done.
    ///
    /// Paper parity: `FurnaceStartSmeltEvent` when a cook begins, which may
    /// change its length, and `FurnaceSmeltEvent` when one ends, which may
    /// change or refuse the result.
    fn cook(&self, world: &Arc<World>, pos: BlockPos, cooking: Cooking) {
        if cooking.starting {
            let mut event = FurnaceStartSmeltEvent::new(
                world.key.to_string(),
                pos,
                cooking.source.clone(),
                cooking.recipe.clone(),
                cooking.cooking_time,
            );
            world.fire_event(&mut event);
            self.container.lock().cooking_total_time = event.total_cook_time();
        }
        let finished = {
            let mut container = self.container.lock();
            container.cooking_timer += 1;
            let finished = container.cooking_timer >= container.cooking_total_time;
            if finished {
                container.cooking_timer = 0;
                container.cooking_total_time = cooking.cooking_time;
            }
            finished
        };
        if !finished {
            return;
        }
        let mut event = FurnaceSmeltEvent::new(
            world.key.to_string(),
            pos,
            cooking.source,
            cooking.result,
            cooking.recipe.clone(),
        );
        world.fire_event(&mut event);
        if event.is_cancelled() {
            return;
        }
        let mut container = self.container.lock();
        if container.burn(event.result()) {
            *container.recipes_used.entry(cooking.recipe).or_insert(0) += 1;
        }
    }

    /// Mirrors the progress into the data slots the open menus read.
    fn publish_data(&self) {
        let container = self.container.lock();
        self.data
            .lit_time_remaining
            .store(container.lit_time_remaining, Ordering::Relaxed);
        self.data
            .lit_total_time
            .store(container.lit_total_time, Ordering::Relaxed);
        self.data
            .cooking_timer
            .store(container.cooking_timer, Ordering::Relaxed);
        self.data
            .cooking_total_time
            .store(container.cooking_total_time, Ordering::Relaxed);
    }

    /// Creates a furnace block entity of the given cooking family.
    #[must_use]
    pub fn new_of_kind(
        block_entity_type: &'static BlockEntityType,
        kind: CookingKind,
        level: Weak<World>,
        pos: BlockPos,
        state: BlockStateId,
    ) -> Self {
        let base = Arc::new(BlockEntityBase::new(block_entity_type, level, pos, state));
        let container = Arc::new(SyncMutex::new(FurnaceContainer {
            items: vec![ItemStack::empty(); FURNACE_SLOTS],
            kind,
            lit_time_remaining: 0,
            lit_total_time: 0,
            cooking_timer: 0,
            cooking_total_time: 0,
            recipes_used: FxHashMap::default(),
        }));
        let shared_container: SharedContainer = container.clone();
        Self {
            container_ref: ContainerRef::owned_by_block_entity(shared_container, Arc::clone(&base)),
            base,
            container,
            data: Arc::new(FurnaceDataSlots::default()),
            name: BlockEntityName::new(),
        }
    }

    /// Returns the progress values shared with open menus.
    #[must_use]
    pub fn data(&self) -> Arc<FurnaceDataSlots> {
        Arc::clone(&self.data)
    }

    /// Creates a furnace.
    #[must_use]
    pub fn new(level: Weak<World>, pos: BlockPos, state: BlockStateId) -> Self {
        Self::new_of_kind(
            &vanilla_block_entity_types::FURNACE,
            CookingKind::Smelting,
            level,
            pos,
            state,
        )
    }

    /// Creates a blast furnace.
    #[must_use]
    pub fn new_blast_furnace(level: Weak<World>, pos: BlockPos, state: BlockStateId) -> Self {
        Self::new_of_kind(
            &vanilla_block_entity_types::BLAST_FURNACE,
            CookingKind::Blasting,
            level,
            pos,
            state,
        )
    }

    /// Creates a smoker.
    #[must_use]
    pub fn new_smoker(level: Weak<World>, pos: BlockPos, state: BlockStateId) -> Self {
        Self::new_of_kind(
            &vanilla_block_entity_types::SMOKER,
            CookingKind::Smoking,
            level,
            pos,
            state,
        )
    }

    /// Ticks of fuel left, ticks into the current cook, and ticks the cook
    /// takes in all.
    #[must_use]
    pub fn times(&self) -> [i32; 3] {
        let container = self.container.lock();
        [
            container.lit_time_remaining,
            container.cooking_timer,
            container.cooking_total_time,
        ]
    }

    /// Sets what [`Self::times`] reads.
    ///
    /// Bukkit's `Furnace.update` writes these back from a snapshot. The burn
    /// also becomes the burn's full length, which is what the flame gauge is
    /// drawn against.
    pub fn set_times(&self, [burn, cook, cook_total]: [i32; 3]) {
        let mut container = self.container.lock();
        container.lit_time_remaining = burn;
        container.lit_total_time = burn;
        container.cooking_timer = cook;
        container.cooking_total_time = cook_total;
    }

    /// Returns whether the furnace is currently burning fuel.
    #[must_use]
    pub fn is_lit(&self) -> bool {
        self.container.lock().lit_time_remaining > 0
    }

    /// Takes the experience owed for everything cooked since the last collection.
    ///
    /// Vanilla parity: the `recipesUsed` half of
    /// `AbstractFurnaceBlockEntity.awardUsedRecipesAndPopExperience`.
    #[must_use]
    pub fn take_earned_experience(&self) -> f32 {
        let used = mem::take(&mut self.container.lock().recipes_used);
        used.iter()
            .filter_map(|(id, count)| {
                REGISTRY
                    .recipes
                    .find_cooking_recipe_by_id(id)
                    .map(|recipe| recipe.experience * *count as f32)
            })
            .sum()
    }

    /// Returns the name an anvil gave this furnace, if any.
    ///
    /// Vanilla parity: `Nameable.getCustomName`.
    #[must_use]
    pub fn custom_name(&self) -> Option<TextComponent> {
        self.name.custom_name()
    }
}

impl BlockEntity for FurnaceBlockEntity {
    fn base(&self) -> &BlockEntityBase {
        &self.base
    }

    /// Vanilla parity: `AbstractFurnaceBlockEntity.serverTick`, with Paper's
    /// three events fired where Paper fires them. The container is unlocked
    /// while each runs, since a listener reading this furnace is the norm.
    fn tick(&self, world: &Arc<World>) {
        let pos = self.get_block_pos();
        let (was_lit, mut is_lit, cooking) = {
            let mut container = self.container.lock();
            let was_lit = container.lit_time_remaining > 0;
            if was_lit {
                container.lit_time_remaining -= 1;
            }
            let is_lit = container.lit_time_remaining > 0;

            let has_fuel = !container.items[SLOT_FUEL].is_empty();
            let has_ingredient = !container.items[SLOT_INPUT].is_empty();

            let cooking = if is_lit || (has_fuel && has_ingredient) {
                match container.cooking_result() {
                    Some((recipe, result, cooking_time)) if container.can_burn(&result) => {
                        let fuel = container.items[SLOT_FUEL].clone();
                        Some(Cooking {
                            burn_duration: container.burn_duration(&fuel),
                            fuel,
                            source: container.items[SLOT_INPUT].clone(),
                            recipe,
                            result,
                            cooking_time,
                            starting: container.cooking_timer == 0,
                        })
                    }
                    _ => {
                        container.cooking_timer = 0;
                        None
                    }
                }
            } else {
                if container.cooking_timer > 0 {
                    let cooled = container.cooking_timer - BURN_COOL_SPEED;
                    container.cooking_timer = cooled.clamp(0, container.cooking_total_time);
                }
                None
            };
            (was_lit, is_lit, cooking)
        };

        if let Some(cooking) = cooking {
            if !is_lit {
                let Some(lit) = self.light(world, pos, &cooking) else {
                    // Paper returns from the whole tick on a refused light.
                    self.publish_data();
                    return;
                };
                is_lit = lit;
            }
            if is_lit {
                self.cook(world, pos, cooking);
            } else {
                self.container.lock().cooking_timer = 0;
            }
        }

        self.publish_data();

        if was_lit != is_lit {
            let state = self.get_block_state();
            world.set_block(
                pos,
                state.set_value(&BlockStateProperties::LIT, is_lit),
                UpdateFlags::UPDATE_ALL,
            );
        }
    }

    fn pre_remove_side_effects(&self, pos: BlockPos, _state: BlockStateId) {
        let items = {
            let mut container = self.container.lock();
            mem::replace(
                &mut container.items,
                vec![ItemStack::empty(); FURNACE_SLOTS],
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
        self.name.load(&nbt_view);
        let mut container = self.container.lock();
        container.items.fill(ItemStack::empty());

        if let Some(items_list) = nbt_view.list("Items")
            && let Some(compounds) = items_list.compounds()
        {
            for compound in compounds {
                if let Some(slot) = compound.byte("Slot") {
                    let slot = slot as usize;
                    if slot < FURNACE_SLOTS
                        && let Some(item) = ItemStack::from_borrowed_compound(&compound)
                    {
                        container.items[slot] = item;
                    }
                }
            }
        }

        container.lit_time_remaining = nbt_view.short("lit_time_remaining").unwrap_or(0).into();
        container.lit_total_time = nbt_view.short("lit_total_time").unwrap_or(0).into();
        container.cooking_timer = nbt_view.short("cooking_time_spent").unwrap_or(0).into();
        container.cooking_total_time = nbt_view.short("cooking_total_time").unwrap_or(0).into();
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.name.save(nbt);
        let container = self.container.lock();
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
        nbt.insert("lit_time_remaining", container.lit_time_remaining as i16);
        nbt.insert("lit_total_time", container.lit_total_time as i16);
        nbt.insert("cooking_time_spent", container.cooking_timer as i16);
        nbt.insert("cooking_total_time", container.cooking_total_time as i16);
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
        self.name.display_name(default_name)
    }

    /// Vanilla parity: the `CUSTOM_NAME` half of
    /// `BaseContainerBlockEntity.collectImplicitComponents`. `CONTAINER` and
    /// `LOCK` are not collected: no vanilla loot table asks this block for
    /// either, and Foton has no lock on a container yet.
    fn collect_implicit_components(&self, components: &mut DataComponentMap) {
        self.name.collect_implicit_components(components);
    }

    /// Vanilla parity: the `CUSTOM_NAME` half of
    /// `BaseContainerBlockEntity.applyImplicitComponents`.
    fn apply_implicit_components(&self, input: &ImplicitComponentInput<'_>) {
        self.name.apply_implicit_components(input);
    }
}

/// Slots a hopper above a furnace may fill.
///
/// Vanilla parity: `AbstractFurnaceBlockEntity.SLOTS_FOR_UP`.
static SLOTS_FOR_UP: [usize; 1] = [SLOT_INPUT];

/// Slots a hopper below a furnace may drain, result first.
///
/// Vanilla parity: `AbstractFurnaceBlockEntity.SLOTS_FOR_DOWN`. The fuel slot
/// comes second so a hopper empties the output before it ever reaches the
/// bucket a lava bucket left behind.
static SLOTS_FOR_DOWN: [usize; 2] = [SLOT_RESULT, SLOT_FUEL];

/// Slots a hopper at the side of a furnace may reach.
///
/// Vanilla parity: `AbstractFurnaceBlockEntity.SLOTS_FOR_SIDES`.
static SLOTS_FOR_SIDES: [usize; 1] = [SLOT_FUEL];

impl Container for FurnaceContainer {
    fn items(&self) -> &[ItemStack] {
        &self.items
    }

    fn items_mut(&mut self) -> &mut [ItemStack] {
        &mut self.items
    }

    fn get_container_size(&self) -> usize {
        FURNACE_SLOTS
    }

    fn set_item(&mut self, slot: usize, mut stack: ItemStack) {
        if slot >= FURNACE_SLOTS {
            return;
        }
        let same =
            !stack.is_empty() && ItemStack::is_same_item_same_components(&self.items[slot], &stack);

        let max_stack_size = self.get_max_stack_size_for_item(&stack);
        if !stack.is_empty() && stack.count() > max_stack_size {
            stack.set_count(max_stack_size);
        }
        self.items[slot] = stack;

        // Vanilla restarts the progress bar whenever the input becomes a different item.
        if slot == SLOT_INPUT && !same {
            self.cooking_total_time = self.total_cook_time();
            self.cooking_timer = 0;
        }
    }

    /// Vanilla parity: `AbstractFurnaceBlockEntity.canPlaceItem`.
    fn can_place_item(&self, slot: usize, stack: &ItemStack) -> bool {
        match slot {
            SLOT_RESULT => false,
            SLOT_FUEL => {
                fuel::is_fuel(stack)
                    || (stack.is(&vanilla_items::BUCKET)
                        && !self.items[SLOT_FUEL].is(&vanilla_items::BUCKET))
            }
            _ => true,
        }
    }

    /// Vanilla parity: `AbstractFurnaceBlockEntity.getSlotsForFace`.
    fn slots_for_face(&self, direction: Direction) -> SlotsForFace {
        match direction {
            Direction::Down => SlotsForFace::Explicit(&SLOTS_FOR_DOWN),
            Direction::Up => SlotsForFace::Explicit(&SLOTS_FOR_UP),
            _ => SlotsForFace::Explicit(&SLOTS_FOR_SIDES),
        }
    }

    /// Vanilla parity: `AbstractFurnaceBlockEntity.canTakeItemThroughFace`. A
    /// hopper underneath may only take the empty bucket back, never the fuel
    /// the furnace is about to burn.
    fn can_take_item_through_face(
        &self,
        slot: usize,
        stack: &ItemStack,
        direction: Direction,
    ) -> bool {
        if direction == Direction::Down && slot == SLOT_FUEL {
            return stack.is(&vanilla_items::WATER_BUCKET) || stack.is(&vanilla_items::BUCKET);
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
    use foton_registry::init_vanilla_registry;

    use super::*;

    fn container(kind: CookingKind) -> FurnaceContainer {
        init_vanilla_registry();
        FurnaceContainer {
            items: vec![ItemStack::empty(); FURNACE_SLOTS],
            kind,
            lit_time_remaining: 0,
            lit_total_time: 0,
            cooking_timer: 0,
            cooking_total_time: 0,
            recipes_used: FxHashMap::default(),
        }
    }

    #[test]
    fn smokers_and_blast_furnaces_burn_fuel_twice_as_fast() {
        let coal = ItemStack::new(&vanilla_items::COAL);

        assert_eq!(container(CookingKind::Smelting).burn_duration(&coal), 1600);
        assert_eq!(container(CookingKind::Blasting).burn_duration(&coal), 800);
        assert_eq!(container(CookingKind::Smoking).burn_duration(&coal), 800);
    }

    #[test]
    fn the_fuel_slot_takes_fuel_and_buckets_only() {
        let furnace = container(CookingKind::Smelting);

        assert!(furnace.can_place_item(SLOT_FUEL, &ItemStack::new(&vanilla_items::COAL)));
        assert!(furnace.can_place_item(SLOT_FUEL, &ItemStack::new(&vanilla_items::BUCKET)));
        assert!(!furnace.can_place_item(SLOT_FUEL, &ItemStack::new(&vanilla_items::STONE)));
    }

    #[test]
    fn the_result_slot_never_accepts_items() {
        let furnace = container(CookingKind::Smelting);
        assert!(!furnace.can_place_item(SLOT_RESULT, &ItemStack::new(&vanilla_items::IRON_INGOT)));
        assert!(furnace.can_place_item(SLOT_INPUT, &ItemStack::new(&vanilla_items::RAW_IRON)));
    }

    #[test]
    fn changing_the_input_restarts_the_progress_bar() {
        let mut furnace = container(CookingKind::Smelting);
        furnace.set_item(SLOT_INPUT, ItemStack::new(&vanilla_items::RAW_IRON));
        furnace.cooking_timer = 50;

        furnace.set_item(SLOT_INPUT, ItemStack::new(&vanilla_items::RAW_GOLD));

        assert_eq!(furnace.cooking_timer, 0);
        assert_eq!(furnace.cooking_total_time, 200);
    }

    #[test]
    fn a_lava_bucket_leaves_an_empty_bucket_behind() {
        let mut furnace = container(CookingKind::Smelting);
        furnace.set_item(SLOT_FUEL, ItemStack::new(&vanilla_items::LAVA_BUCKET));

        furnace.consume_fuel();

        assert!(furnace.items[SLOT_FUEL].is(&vanilla_items::BUCKET));
    }

    #[test]
    fn the_result_slot_only_stacks_matching_items() {
        let mut furnace = container(CookingKind::Smelting);
        let ingot = ItemStack::new(&vanilla_items::IRON_INGOT);

        assert!(
            furnace.can_burn(&ingot),
            "an empty result slot accepts anything"
        );

        furnace.items[SLOT_RESULT] = ItemStack::new(&vanilla_items::GOLD_INGOT);
        assert!(
            !furnace.can_burn(&ingot),
            "a different item must block the burn"
        );

        furnace.items[SLOT_RESULT] = ItemStack::with_count(&vanilla_items::IRON_INGOT, 64);
        assert!(
            !furnace.can_burn(&ingot),
            "a full stack must block the burn"
        );
    }

    #[test]
    fn drying_a_sponge_refills_the_bucket_in_the_fuel_slot() {
        let mut furnace = container(CookingKind::Smelting);
        furnace.items[SLOT_INPUT] = ItemStack::new(&vanilla_items::WET_SPONGE);
        furnace.items[SLOT_FUEL] = ItemStack::new(&vanilla_items::BUCKET);

        furnace.burn(&ItemStack::new(&vanilla_items::SPONGE));

        assert!(furnace.items[SLOT_FUEL].is(&vanilla_items::WATER_BUCKET));
        assert!(furnace.items[SLOT_RESULT].is(&vanilla_items::SPONGE));
    }
    /// Vanilla parity: `AbstractFurnaceBlockEntity.getSlotsForFace`. This is
    /// what makes a hopper on top load the input, one at the side load the fuel,
    /// and one underneath drain the output.
    #[test]
    fn each_face_exposes_the_slots_vanilla_gives_it() {
        let furnace = container(CookingKind::Smelting);

        let up: Vec<usize> = furnace.slots_for_face(Direction::Up).into_iter().collect();
        assert_eq!(up, vec![SLOT_INPUT]);

        let down: Vec<usize> = furnace
            .slots_for_face(Direction::Down)
            .into_iter()
            .collect();
        assert_eq!(down, vec![SLOT_RESULT, SLOT_FUEL]);

        for side in [
            Direction::North,
            Direction::South,
            Direction::East,
            Direction::West,
        ] {
            let slots: Vec<usize> = furnace.slots_for_face(side).into_iter().collect();
            assert_eq!(slots, vec![SLOT_FUEL], "wrong slots for {side:?}");
        }
    }

    /// Vanilla parity: `AbstractFurnaceBlockEntity.canTakeItemThroughFace`. A
    /// hopper underneath may take the empty bucket back but never the coal.
    #[test]
    fn only_buckets_leave_the_fuel_slot_downwards() {
        let furnace = container(CookingKind::Smelting);
        let coal = ItemStack::new(&vanilla_items::COAL);
        let bucket = ItemStack::new(&vanilla_items::BUCKET);

        assert!(!furnace.can_take_item_through_face(SLOT_FUEL, &coal, Direction::Down));
        assert!(furnace.can_take_item_through_face(SLOT_FUEL, &bucket, Direction::Down));
        assert!(furnace.can_take_item_through_face(SLOT_RESULT, &coal, Direction::Down));
        assert!(furnace.can_take_item_through_face(SLOT_FUEL, &coal, Direction::North));
    }
}
