//! Enchanting table menu.
//!
//! Vanilla parity: `EnchantmentMenu`. Two slots -- the item and the lapis --
//! and ten data slots: the three level costs, the seed the offers were drawn
//! from, and a clue for each offer so the client can show one enchantment name
//! before the player commits.

use std::sync::Arc;

use foton_registry::blocks::block_state_ext::BlockStateExt as _;
use foton_registry::{RegistryEntry as _, vanilla_blocks, vanilla_items, vanilla_menu_types};
use foton_utils::random::Random as _;
use foton_utils::random::legacy_random::LegacyRandom;
use foton_utils::{
    BlockPos,
    locks::{IntoShared, Shared},
};

use crate::behavior::blocks::count_enchanting_power;
use crate::enchantment_selection::{EnchantmentInstance, apply_enchantments};
use crate::enchantment_selection::{
    OFFER_COUNT, enchanting_table_candidates, enchantment_cost, select_enchantment,
};
use crate::entity::Entity as _;
use crate::event::{EnchantmentOffer, EnchantmentViewState, PrepareItemEnchantEvent};
use crate::inventory::container::SimpleContainer;
use crate::inventory::lock::ContainerId;
use crate::inventory::prelude::*;
use crate::player::player_inventory::PlayerInventory;
use crate::world::{LevelReader as _, World};
use foton_registry::item_stack::ItemStack;

/// Slot holding the item being enchanted.
const SLOT_ITEM: usize = 0;
/// Slot holding the lapis lazuli.
const SLOT_LAPIS: usize = 1;

/// Builds the enchanting table menu.
///
/// Vanilla parity: `EnchantmentMenu`.
#[must_use]
pub fn enchantment(
    inventory: Shared<PlayerInventory>,
    container_id: u8,
    pos: BlockPos,
    world: &Arc<World>,
) -> Menu {
    let enchant_slots = SimpleContainer::new(2).into_shared();
    let mut builder = MenuBuilder::new(&vanilla_menu_types::ENCHANTMENT, container_id);

    // Vanilla parity: `EnchantmentMenu`'s second slot overrides `mayPlace` to
    // `itemStack.is(Items.LAPIS_LAZULI)`. Without it the currency check below --
    // which only ever counted -- accepted three of anything, and enchanting cost
    // no lapis at all.
    let input = builder.section_with(
        &enchant_slots,
        2,
        // Vanilla parity: the item slot overrides `getMaxStackSize()` to one, so
        // a stack can never sit there and shift-clicking sixty-four books moves
        // exactly one. The lapis slot keeps the default -- the top offer costs
        // three lapis, and capping that slot too would make it unpayable.
        SectionKind::capped(
            |slot, stack| slot != SLOT_LAPIS || stack.is(&vanilla_items::LAPIS_LAZULI),
            |slot| (slot == SLOT_ITEM).then_some(1),
        ),
    );
    let player = builder.player_inventory(&inventory);

    // Vanilla parity: costs, then the seed, then the three enchantment clues,
    // then the three level clues -- the order the client reads them in.
    let costs = [
        builder.data_slot(0),
        builder.data_slot(0),
        builder.data_slot(0),
    ];
    let seed = builder.data_slot(0);
    let enchant_clues = [
        builder.data_slot(-1),
        builder.data_slot(-1),
        builder.data_slot(-1),
    ];
    let level_clues = [
        builder.data_slot(-1),
        builder.data_slot(-1),
        builder.data_slot(-1),
    ];

    builder.route(input, player.all(), FillDirection::Backward);
    builder.route(player.all(), input, FillDirection::Forward);
    builder.drain(input);

    builder.build(EnchantmentKind {
        enchant_slots: ContainerId::from_arc(&enchant_slots),
        block_pos: pos,
        world: Arc::clone(world),
        costs,
        seed,
        enchant_clues,
        level_clues,
        state: EnchantmentViewState {
            seed: 0,
            costs: [0; OFFER_COUNT],
            offers: [None; OFFER_COUNT],
        },
        seed_initialized: false,
    })
}

/// Per-menu enchanting table state.
pub struct EnchantmentKind {
    /// The two slots the table itself owns.
    enchant_slots: ContainerId,
    block_pos: BlockPos,
    world: Arc<World>,
    /// The three level costs shown to the client.
    costs: [DataSlot; OFFER_COUNT],
    /// The seed the offers were drawn from.
    seed: DataSlot,
    /// Registry id of one enchantment per offer, or -1 when there is none.
    enchant_clues: [DataSlot; OFFER_COUNT],
    /// Level of that enchantment per offer, or -1.
    level_clues: [DataSlot; OFFER_COUNT],
    /// Server-side copy of the costs, which the data slots only mirror.
    state: EnchantmentViewState,
    seed_initialized: bool,
}

// SAFETY: This Foton-owned key uniquely identifies the concrete menu kind
// within the process.
unsafe impl foton_utils::DowncastType for EnchantmentKind {
    const TYPE_KEY: foton_utils::DowncastTypeKey =
        foton_utils::DowncastTypeKey::new("foton:menu/enchantment");
}

impl EnchantmentKind {
    /// Recomputes the three offers for whatever is in the item slot.
    ///
    /// Vanilla parity: `EnchantmentMenu.slotsChanged`.
    fn recompute_offers(
        &mut self,
        behavior: &mut MenuBehavior,
        guard: &mut ContainerLockGuard,
        player: &Player,
    ) {
        let Some(slots) = guard.get(self.enchant_slots) else {
            return;
        };
        let items = [
            slots.get_item(SLOT_ITEM).clone(),
            slots.get_item(SLOT_LAPIS).clone(),
        ];
        let item = &items[SLOT_ITEM];

        if item.is_empty() {
            self.clear_offers(behavior);
            return;
        }

        let bookshelves = self.count_bookshelves();
        if !self.seed_initialized {
            self.state.seed = player.enchantment_seed();
            self.seed_initialized = true;
        }
        let seed = self.state.seed;

        // Vanilla reseeds one shared random from the player's seed and reads all
        // three costs from it in order, so the same seed always yields the same
        // three offers for the same item and shelf count.
        let mut random = LegacyRandom::from_seed(seed as u64);
        for slot in 0..OFFER_COUNT {
            let mut cost = enchantment_cost(&mut random, slot, bookshelves, item);
            // An offer that cannot even reach its own slot number is not shown.
            if cost < i32::try_from(slot).unwrap_or(i32::MAX) + 1 {
                cost = 0;
            }
            self.state.costs[slot] = cost;
            self.state.offers[slot] = None;
        }

        for slot in 0..OFFER_COUNT {
            if self.state.costs[slot] <= 0 {
                continue;
            }
            let (rolled, mut random) = Self::roll_offer(seed, slot, self.state.costs[slot], item);
            if rolled.is_empty() {
                continue;
            }
            let selected = random.next_i32_bounded(i32::try_from(rolled.len()).unwrap_or(i32::MAX));
            let Some(clue) = usize::try_from(selected)
                .ok()
                .and_then(|index| rolled.get(index))
            else {
                continue;
            };
            self.state.offers[slot] = Some(EnchantmentOffer {
                enchantment: clue.enchantment,
                level: i32::try_from(clue.level).unwrap_or(i32::MAX),
                cost: self.state.costs[slot],
            });
        }
        let cancelled = !item.is_enchantable();
        let mut event = PrepareItemEnchantEvent {
            menu_id: behavior.instance_id(),
            player_id: player.uuid(),
            world: self.world.key.to_string(),
            position: self.block_pos,
            items,
            bonus: bookshelves,
            state: self.state,
            cancelled,
        };
        // Java can read/write the player's real inventory during dispatch.
        // The menu itself is detached by Player's existing callback protocol.
        guard.run_unlocked(|| player.fire_event(&mut event));
        self.state = event.state;
        if event.cancelled {
            self.state.clear();
        }
        if let Some(slots) = guard.get_mut(self.enchant_slots) {
            for (index, item) in event.items.into_iter().enumerate() {
                slots.set_item(index, item);
            }
        }
        self.synchronize(behavior);
    }

    /// Blanks every offer.
    fn clear_offers(&mut self, behavior: &mut MenuBehavior) {
        self.state.clear();
        self.synchronize(behavior);
    }

    /// Returns the full-width values exposed by Bukkit's enchantment view.
    #[must_use]
    pub const fn view_state(&self) -> EnchantmentViewState {
        self.state
    }

    /// The table backing this menu, including its world key.
    #[must_use]
    pub fn table(&self) -> (BlockPos, String) {
        (self.block_pos, self.world.key.to_string())
    }

    /// Replaces only view state, without consuming RNG or recalculating offers.
    pub fn set_view_state(&mut self, behavior: &mut MenuBehavior, state: EnchantmentViewState) {
        self.state = state;
        self.seed_initialized = true;
        self.synchronize(behavior);
    }

    fn synchronize(&self, behavior: &mut MenuBehavior) {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "Vanilla narrows data slot values to signed shorts on the wire"
        )]
        self.seed.set(behavior, self.state.seed as i16);
        for slot in 0..OFFER_COUNT {
            self.costs[slot].set(behavior, data_slot_short(self.state.costs[slot]));
            let offer = self.state.offers[slot];
            let id = offer
                .and_then(|offer| offer.enchantment.try_id())
                .and_then(|id| i16::try_from(id).ok())
                .unwrap_or(-1);
            self.enchant_clues[slot].set(behavior, id);
            self.level_clues[slot].set(
                behavior,
                offer.map_or(-1, |offer| data_slot_short(offer.level)),
            );
        }
    }

    /// Rolls what one offer would grant.
    ///
    /// Vanilla parity: `EnchantmentMenu.getEnchantmentList`, including the
    /// offset seed per slot, which is what lets the clue shown before the click
    /// match what the click produces.
    fn roll_offer(
        seed: i32,
        slot: usize,
        cost: i32,
        item: &ItemStack,
    ) -> (Vec<EnchantmentInstance>, LegacyRandom) {
        // Java adds the slot as an int before widening to RandomSource.setSeed(long).
        let offset = i64::from(seed.wrapping_add(i32::try_from(slot).unwrap_or(0)));
        #[expect(
            clippy::cast_sign_loss,
            reason = "the seed is reinterpreted, matching Java's setSeed(long)"
        )]
        let mut random = LegacyRandom::from_seed(offset as u64);

        let mut rolled = select_enchantment(&mut random, item, cost, enchanting_table_candidates());

        // Vanilla drops one enchantment at random from a book's offer, so a book
        // is not simply the best of every item.
        if item.is(&vanilla_items::BOOK) && rolled.len() > 1 {
            let dropped = random.next_i32_bounded(i32::try_from(rolled.len()).unwrap_or(i32::MAX));
            rolled.remove(usize::try_from(dropped).unwrap_or(0));
        }

        (rolled, random)
    }

    /// Counts the shelves powering this table.
    ///
    /// Vanilla parity: the `BOOKSHELF_OFFSETS` walk of `EnchantmentMenu`.
    fn count_bookshelves(&self) -> i32 {
        count_enchanting_power(&self.world, self.block_pos)
    }
}

/// Narrows a value to the `i16` a data slot carries.
const fn data_slot_short(value: i32) -> i16 {
    value as i16
}

impl MenuKind for EnchantmentKind {
    fn still_valid(&self, _behavior: &MenuBehavior, player: &Player) -> bool {
        let state = self.world.get_block_state(self.block_pos);
        state.get_block() == &vanilla_blocks::ENCHANTING_TABLE
            && player.is_within_block_interaction_range_with_buffer(self.block_pos, 4.0)
    }

    fn on_open(
        &mut self,
        behavior: &mut MenuBehavior,
        guard: &mut ContainerLockGuard,
        player: &Player,
    ) {
        self.state.seed = player.enchantment_seed();
        self.seed_initialized = true;
        self.recompute_offers(behavior, guard, player);
    }

    fn slots_changed(
        &mut self,
        behavior: &mut MenuBehavior,
        guard: &mut ContainerLockGuard,
        player: &Player,
    ) {
        self.recompute_offers(behavior, guard, player);
    }

    /// Applies one of the three offers.
    ///
    /// Vanilla parity: `EnchantmentMenu.clickMenuButton`.
    fn on_button_click(
        &mut self,
        behavior: &mut MenuBehavior,
        guard: &mut ContainerLockGuard,
        player: &Player,
        button: i32,
    ) -> bool {
        let Ok(slot) = usize::try_from(button) else {
            return false;
        };
        if slot >= OFFER_COUNT {
            return false;
        }

        let (item, lapis) = {
            let Some(slots) = guard.get(self.enchant_slots) else {
                return false;
            };
            (
                slots.get_item(SLOT_ITEM).clone(),
                slots.get_item(SLOT_LAPIS).clone(),
            )
        };

        // Vanilla parity: `int enchantmentCost = buttonId + 1`, which is both the
        // lapis charged *and* the levels charged -- one, two or three. The row's
        // `costs[buttonId]` is only ever a gate: the player must have that many
        // levels to click the row, but it is never what they pay. Charging it
        // made the bottom offer cost thirty levels instead of three.
        let enchantment_cost = i32::try_from(slot).unwrap_or(0) + 1;
        let cost = self.state.costs[slot];
        let free = player.has_infinite_materials();

        if !free && (lapis.is_empty() || lapis.count() < enchantment_cost) {
            return false;
        }
        if cost <= 0 || item.is_empty() {
            return false;
        }
        let level = player.experience.lock().level();
        if !free && (level < enchantment_cost || level < cost) {
            return false;
        }

        let (rolled, _) = Self::roll_offer(self.state.seed, slot, cost, &item);
        if rolled.is_empty() {
            return false;
        }

        let enchanted = apply_enchantments(&item, &rolled);

        {
            let Some(slots) = guard.get_mut(self.enchant_slots) else {
                return false;
            };
            slots.set_item(SLOT_ITEM, enchanted);
            let mut remaining = lapis;
            if !free {
                remaining.set_count(remaining.count() - enchantment_cost);
            }
            slots.set_item(SLOT_LAPIS, remaining);
        }

        // Vanilla charges levels and rerolls the player seed even in creative;
        // infinite materials only waive the lapis and level requirement.
        player.on_enchantment_performed(enchantment_cost);

        self.state.seed = player.enchantment_seed();
        self.recompute_offers(behavior, guard, player);
        true
    }
}

#[cfg(test)]
mod tests;
