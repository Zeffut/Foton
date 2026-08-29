//! Brewing stand block entity.
//!
//! Vanilla parity: `BrewingStandBlockEntity`. Three bottles brew at once from
//! one ingredient, which is what makes brewing worth the walk to the nether:
//! the ingredient is spent once and every bottle it applies to is converted.
//! Bottles it does not apply to are left alone rather than blocking the brew.

use std::{
    mem,
    sync::{
        Arc, Weak,
        atomic::{AtomicI32, Ordering},
    },
};

use foton_registry::blocks::block_state_ext::BlockStateExt;
use foton_registry::blocks::properties::{BlockStateProperties, BoolProperty};
use foton_registry::item_stack::ItemStack;
use foton_registry::level_events::SOUND_BREWING_STAND_BREW;
use foton_registry::{potion_brewing, vanilla_block_entity_types, vanilla_items};
use foton_utils::types::UpdateFlags;
use foton_utils::{
    BlockPos, BlockStateId, Direction, DowncastType, DowncastTypeKey, locks::SyncMutex,
};
use simdnbt::ToNbtTag;
use simdnbt::borrow::{BaseNbtCompound as BorrowedNbtCompound, NbtCompound as NbtCompoundView};
use simdnbt::owned::{NbtCompound, NbtList, NbtTag};

use crate::block_entity::{BlockEntity, BlockEntityBase, BlockEntityName, ImplicitComponentInput};
use crate::inventory::container::{Container, SlotsForFace};
use crate::inventory::lock::{ContainerRef, SharedContainer};
use crate::world::World;
use foton_registry::data_components::DataComponentMap;
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
    container: Arc<SyncMutex<BrewingStandContainer>>,
    container_ref: ContainerRef,
    data: Arc<BrewingStandDataSlots>,
    /// Vanilla parity: the `name` of `BaseContainerBlockEntity`, the anvil
    /// name this block was placed with.
    name: BlockEntityName,
}

/// Items and brewing state, held under one lock because a tick mutates both.
struct BrewingStandContainer {
    items: Vec<ItemStack>,
    /// Ticks left on the current brew.
    brew_time: i32,
    /// Brews left in the blaze powder already consumed.
    fuel: i32,
    /// The ingredient the current brew started with.
    ///
    /// Vanilla keeps this to notice a player swapping the ingredient mid-brew,
    /// which cancels the brew rather than quietly producing the wrong potion.
    brewing_ingredient: Option<ItemStack>,
    /// Which bottle slots were filled when the block state was last written.
    last_bottle_flags: [bool; BOTTLE_SLOTS],
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
            brewing_ingredient: None,
            last_bottle_flags: [false; BOTTLE_SLOTS],
        }));
        let shared_container: SharedContainer = container.clone();
        Self {
            container_ref: ContainerRef::owned_by_block_entity(shared_container, Arc::clone(&base)),
            base,
            container,
            data: Arc::new(BrewingStandDataSlots::default()),
            name: BlockEntityName::new(),
        }
    }

    /// Returns the progress values shared with open menus.
    #[must_use]
    pub fn data(&self) -> Arc<BrewingStandDataSlots> {
        Arc::clone(&self.data)
    }

    /// Returns the name an anvil gave this brewing stand, if any.
    ///
    /// Vanilla parity: `Nameable.getCustomName`.
    #[must_use]
    pub fn custom_name(&self) -> Option<TextComponent> {
        self.name.custom_name()
    }
}

impl BlockEntity for BrewingStandBlockEntity {
    fn base(&self) -> &BlockEntityBase {
        &self.base
    }

    fn tick(&self, world: &Arc<World>) {
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

        if container.brew_time > 0 {
            container.brew_time -= 1;
            let ingredient_swapped = container
                .brewing_ingredient
                .as_ref()
                .is_none_or(|started| !container.items[SLOT_INGREDIENT].is(started.item()));

            if container.brew_time == 0 && brewable {
                remainder = container.brew();
                world.level_event(SOUND_BREWING_STAND_BREW, pos, 0, None);
            } else if !brewable || ingredient_swapped {
                container.brew_time = 0;
            }
        } else if brewable && container.fuel > 0 {
            container.fuel -= 1;
            container.brew_time = potion_brewing::BREWING_TIME_TICKS;
            container.brewing_ingredient = Some(container.items[SLOT_INGREDIENT].clone());
        }

        self.data
            .brew_time
            .store(container.brew_time, Ordering::Relaxed);
        self.data.fuel.store(container.fuel, Ordering::Relaxed);

        let flags = container.bottle_flags();
        let flags_changed = flags != container.last_bottle_flags;
        if flags_changed {
            container.last_bottle_flags = flags;
        }
        drop(container);

        if let Some(remainder) = remainder {
            world.drop_item_stack(pos, remainder);
        }

        if flags_changed {
            let mut state = self.get_block_state();
            for (property, filled) in HAS_BOTTLE.iter().zip(flags) {
                state = state.set_value(*property, filled);
            }
            world.set_block(pos, state, UpdateFlags::UPDATE_CLIENTS);
        }
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
        self.name.load(&nbt_view);
        let mut container = self.container.lock();
        container.items.fill(ItemStack::empty());

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
        // Vanilla reloads the in-progress ingredient from the slot, so a brew
        // interrupted by a save resumes instead of cancelling on the next tick.
        container.brewing_ingredient =
            (container.brew_time > 0).then(|| container.items[SLOT_INGREDIENT].clone());
        container.last_bottle_flags = container.bottle_flags();
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
    use foton_registry::potion_brewing::potion_item;
    use foton_registry::{init_vanilla_registry, vanilla_potions};

    use super::*;
    use foton_registry::data_components::PotionContents;
    use foton_registry::data_components::vanilla_components::POTION_CONTENTS;
    use std::ptr;

    fn container() -> BrewingStandContainer {
        init_vanilla_registry();
        BrewingStandContainer {
            items: vec![ItemStack::empty(); BREWING_STAND_SLOTS],
            brew_time: 0,
            fuel: 0,
            brewing_ingredient: None,
            last_bottle_flags: [false; BOTTLE_SLOTS],
        }
    }

    fn water_bottle() -> ItemStack {
        potion_item(&vanilla_items::POTION, &vanilla_potions::WATER)
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
