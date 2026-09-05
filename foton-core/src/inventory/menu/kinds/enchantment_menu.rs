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
use crate::inventory::container::SimpleContainer;
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
        enchant_slots,
        block_pos: pos,
        world: Arc::clone(world),
        costs,
        seed,
        enchant_clues,
        level_clues,
        cost_values: [0; OFFER_COUNT],
    })
}

/// Per-menu enchanting table state.
pub struct EnchantmentKind {
    /// The two slots the table itself owns.
    enchant_slots: Shared<SimpleContainer>,
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
    cost_values: [i32; OFFER_COUNT],
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
    fn recompute_offers(&mut self, behavior: &mut MenuBehavior, player: &Player) {
        let item = self.enchant_slots.lock().get_item(SLOT_ITEM).clone();

        if item.is_empty() || !item.is_enchantable() {
            self.clear_offers(behavior);
            return;
        }

        let bookshelves = self.count_bookshelves();
        let seed = player.enchantment_seed();
        // The wire carries a short, and vanilla casts rather than clamps. The
        // difference matters here and nowhere else: a seed is a full-range int,
        // so clamping would send the same value for almost every player and the
        // client would draw the same scribbles on every table.
        #[expect(
            clippy::cast_possible_truncation,
            reason = "matching the (short) cast vanilla sends this seed through"
        )]
        self.seed.set(behavior, seed as i16);

        // Vanilla reseeds one shared random from the player's seed and reads all
        // three costs from it in order, so the same seed always yields the same
        // three offers for the same item and shelf count.
        let mut random = LegacyRandom::from_seed(seed as u64);
        for slot in 0..OFFER_COUNT {
            let mut cost = enchantment_cost(&mut random, slot, bookshelves, &item);
            // An offer that cannot even reach its own slot number is not shown.
            if cost < i32::try_from(slot).unwrap_or(i32::MAX) + 1 {
                cost = 0;
            }
            self.cost_values[slot] = cost;
            self.costs[slot].set(behavior, clamp_to_i16(cost));
            self.enchant_clues[slot].set(behavior, -1);
            self.level_clues[slot].set(behavior, -1);
        }

        for slot in 0..OFFER_COUNT {
            if self.cost_values[slot] <= 0 {
                continue;
            }
            let rolled = Self::roll_offer(seed, slot, self.cost_values[slot], &item);
            let Some(first) = rolled.first() else {
                continue;
            };
            // The clue is the registry id, which is what the client looks the
            // name up by.
            let id = first
                .enchantment
                .try_id()
                .and_then(|id| i16::try_from(id).ok())
                .unwrap_or(-1);
            self.enchant_clues[slot].set(behavior, id);
            let level = i16::try_from(first.level).unwrap_or(i16::MAX);
            self.level_clues[slot].set(behavior, level);
        }
    }

    /// Blanks every offer.
    fn clear_offers(&mut self, behavior: &mut MenuBehavior) {
        for slot in 0..OFFER_COUNT {
            self.cost_values[slot] = 0;
            self.costs[slot].set(behavior, 0);
            self.enchant_clues[slot].set(behavior, -1);
            self.level_clues[slot].set(behavior, -1);
        }
    }

    /// Rolls what one offer would grant.
    ///
    /// Vanilla parity: `EnchantmentMenu.getEnchantmentList`, including the
    /// offset seed per slot, which is what lets the clue shown before the click
    /// match what the click produces.
    fn roll_offer(seed: i32, slot: usize, cost: i32, item: &ItemStack) -> Vec<EnchantmentInstance> {
        let offset = i64::from(seed) + i64::try_from(slot).unwrap_or(0);
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

        rolled
    }

    /// Counts the shelves powering this table.
    ///
    /// Vanilla parity: the `BOOKSHELF_OFFSETS` walk of `EnchantmentMenu`.
    fn count_bookshelves(&self) -> i32 {
        count_enchanting_power(&self.world, self.block_pos)
    }
}

/// Narrows a value to the `i16` a data slot carries.
fn clamp_to_i16(value: i32) -> i16 {
    i16::try_from(value).unwrap_or(i16::MAX)
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
        _guard: &mut ContainerLockGuard,
        player: &Player,
    ) {
        self.recompute_offers(behavior, player);
    }

    fn slots_changed(
        &mut self,
        behavior: &mut MenuBehavior,
        _guard: &mut ContainerLockGuard,
        player: &Player,
    ) {
        self.recompute_offers(behavior, player);
    }

    /// Applies one of the three offers.
    ///
    /// Vanilla parity: `EnchantmentMenu.clickMenuButton`.
    fn on_button_click(
        &mut self,
        behavior: &mut MenuBehavior,
        _guard: &mut ContainerLockGuard,
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
            let slots = self.enchant_slots.lock();
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
        let cost = self.cost_values[slot];
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

        let seed = player.enchantment_seed();
        let rolled = Self::roll_offer(seed, slot, cost, &item);
        if rolled.is_empty() {
            return false;
        }

        let enchanted = apply_enchantments(&item, &rolled);

        {
            let mut slots = self.enchant_slots.lock();
            slots.set_item(SLOT_ITEM, enchanted);
            let mut remaining = lapis;
            if !free {
                remaining.set_count(remaining.count() - enchantment_cost);
            }
            slots.set_item(SLOT_LAPIS, remaining);
        }

        if !free {
            player.on_enchantment_performed(enchantment_cost);
        }

        self.recompute_offers(behavior, player);
        true
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use foton_registry::item_stack::ItemStack;
    use foton_registry::{init_vanilla_registry, vanilla_items};
    use foton_utils::BlockPos;

    use super::{SLOT_ITEM, SLOT_LAPIS, enchantment};
    use crate::player::Player;
    use crate::test_support::{TestPlayerBuilder, fresh_test_world};

    /// The two slot rules vanilla puts on the enchanting table.
    ///
    /// Both were missing, and each cost something different: with no `mayPlace`
    /// on the currency slot, three of any item bought an enchantment and lapis
    /// left the economy entirely; with no ceiling on the item slot, a
    /// shift-clicked stack of sixty-four books went in and came back as one.
    ///
    /// The ceiling is asserted on *both* slots on purpose. Capping the section
    /// rather than the slot is an easy mistake and a quiet one -- the lapis slot
    /// still looks right, and only the three-lapis offer silently becomes
    /// unpayable. A test that checked the item slot alone would not catch it.
    #[test]
    fn the_table_takes_lapis_only_and_one_item_at_a_time() {
        init_vanilla_registry();
        let world = fresh_test_world("enchantment_slot_rules");
        let player: Arc<Player> =
            TestPlayerBuilder::new(Arc::clone(&world), "EnchantTester", 1).build();

        let menu = enchantment(
            Arc::clone(&player.inventory),
            1,
            BlockPos::new(0, 64, 0),
            &world,
        );
        let slots = menu.behavior().slots();

        // Vanilla: `mayPlace` returns `itemStack.is(Items.LAPIS_LAZULI)`.
        assert!(
            slots[SLOT_LAPIS].may_place(&ItemStack::new(&vanilla_items::LAPIS_LAZULI)),
            "the currency slot must accept lapis"
        );
        assert!(
            !slots[SLOT_LAPIS].may_place(&ItemStack::new(&vanilla_items::DIRT)),
            "the currency slot must not accept anything else"
        );

        // Vanilla: only the item slot overrides `getMaxStackSize()` to one.
        let guard = menu.behavior().lock_all_containers();
        assert_eq!(
            slots[SLOT_ITEM].get_max_stack_size(&guard),
            1,
            "a stack must never sit in the item slot"
        );
        assert!(
            slots[SLOT_LAPIS].get_max_stack_size(&guard) >= 3,
            "the lapis slot has to hold the three the top offer costs"
        );
    }
}
