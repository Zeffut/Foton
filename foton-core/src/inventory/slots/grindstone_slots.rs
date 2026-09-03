//! The grindstone's two inputs and its result.
//!
//! Vanilla parity: the anonymous slots of `GrindstoneMenu`. A grindstone does
//! two unrelated things through one pair of slots -- it strips enchantments
//! off a single item, and it welds two damaged copies of the same item into
//! one -- and in both cases it keeps the curses, which is the whole reason a
//! cursed item cannot be laundered.

use foton_registry::data_components::components::ItemEnchantments;
use foton_registry::data_components::vanilla_components::{MAX_DAMAGE, REPAIR_COST};
use foton_registry::item_stack::ItemStack;
use foton_registry::vanilla_enchantment_tags::EnchantmentTag;
use foton_registry::{REGISTRY, RegistryExt as _, TaggedRegistryExt as _, vanilla_items};
use foton_utils::Identifier;
use foton_utils::locks::Shared;

use crate::inventory::container::{Container as _, ResultContainer, SimpleContainer};
use crate::inventory::lock::{ContainerId, ContainerLockGuard, ContainerRef};
use crate::inventory::slots::ResultHandler;
use crate::player::Player;

/// The first input slot.
pub const GRINDSTONE_INPUT: usize = 0;
/// The second input slot.
pub const GRINDSTONE_ADDITIONAL: usize = 1;

/// How much of an item's durability welding two of them gives back.
///
/// Vanilla parity: the `durability * 5 / 100` bonus of
/// `GrindstoneMenu.mergeItems`.
const WELD_BONUS_PERCENT: i32 = 5;

/// Keeps a grindstone's result in step with its two inputs.
#[derive(Clone)]
pub struct GrindstoneHandler {
    input_container: Shared<SimpleContainer>,
    result_container: Shared<ResultContainer>,
}

impl GrindstoneHandler {
    /// Creates a handler over the grindstone's containers.
    #[must_use]
    pub const fn new(
        input_container: Shared<SimpleContainer>,
        result_container: Shared<ResultContainer>,
    ) -> Self {
        Self {
            input_container,
            result_container,
        }
    }

    fn input_id(&self) -> ContainerId {
        ContainerId::from_arc(&self.input_container)
    }

    fn result_id(&self) -> ContainerId {
        ContainerId::from_arc(&self.result_container)
    }

    /// Returns both inputs.
    #[must_use]
    pub fn input_snapshot(&self, guard: &ContainerLockGuard) -> Option<(ItemStack, ItemStack)> {
        let container = guard.get(self.input_id())?;
        Some((
            container.get_item(GRINDSTONE_INPUT).clone(),
            container.get_item(GRINDSTONE_ADDITIONAL).clone(),
        ))
    }

    /// The currently previewed result, empty when the inputs make nothing.
    pub fn result_snapshot(&self, guard: &ContainerLockGuard) -> ItemStack {
        guard
            .get(self.result_id())
            .map_or_else(ItemStack::empty, |c| c.get_item(0).clone())
    }

    /// Writes a plugin's edited inputs and result back under the existing menu
    /// lock.
    ///
    /// Takes the guard the caller already holds, so the whole preview-edit-write
    /// cycle happens without the containers being unlocked in between.
    pub fn apply_snapshot(
        &self,
        guard: &mut ContainerLockGuard,
        upper: ItemStack,
        lower: ItemStack,
        result: ItemStack,
    ) {
        if let Some(container) = guard.get_mut(self.input_id()) {
            container.set_item(GRINDSTONE_INPUT, upper);
            container.set_item(GRINDSTONE_ADDITIONAL, lower);
            container.set_changed();
        }
        if let Some(container) = guard.get_typed_mut::<ResultContainer>(self.result_id()) {
            container.set_item(0, result);
            container.set_changed();
        }
    }

    /// Returns the experience the two inputs are worth.
    ///
    /// Vanilla parity: `getExperienceAmount`. Half the enchantments' minimum
    /// cost, rounded up, plus a random amount up to that again -- so grinding
    /// the same item twice does not give the same experience.
    #[must_use]
    pub fn experience(&self, guard: &ContainerLockGuard) -> i32 {
        let Some((first, second)) = self.input_snapshot(guard) else {
            return 0;
        };
        let total = experience_from(&first) + experience_from(&second);
        if total <= 0 {
            return 0;
        }
        // `div_ceil` is unstable for i32 here, so the round-up is written out.
        let half = (total + 1) / 2;
        half + (rand::random::<u32>() % half.max(1) as u32) as i32
    }

    /// Returns whether there is nothing to take.
    #[must_use]
    pub fn result_is_empty(&self, guard: &ContainerLockGuard) -> bool {
        let result_id = self.result_id();
        guard
            .get(result_id)
            .is_none_or(|container| container.get_item(0).is_empty())
    }

    /// Empties both inputs, which is what taking the result costs.
    pub fn clear_inputs(&self, guard: &mut ContainerLockGuard) {
        let input_id = self.input_id();
        let Some(container) = guard.get_mut(input_id) else {
            return;
        };
        container.set_item(GRINDSTONE_INPUT, ItemStack::empty());
        container.set_item(GRINDSTONE_ADDITIONAL, ItemStack::empty());
        container.set_changed();
    }
}

/// Returns the experience one item's non-curse enchantments are worth.
fn experience_from(item: &ItemStack) -> i32 {
    let Some(enchantments) = item.get_enchantments_for_crafting() else {
        return 0;
    };
    enchantments
        .iter()
        .filter(|(id, _)| !is_curse(id))
        .filter_map(|(id, level)| {
            let enchantment = REGISTRY.enchantments.by_key(id)?;
            let level = *level as i32;
            // Vanilla parity: `Enchantment.getMinCost(level)`.
            Some(
                enchantment.min_cost.base
                    + (level - 1) * enchantment.min_cost.per_level_above_first,
            )
        })
        .sum()
}

/// Returns whether an enchantment is a curse, and so survives the grindstone.
fn is_curse(id: &Identifier) -> bool {
    REGISTRY.enchantments.by_key(id).is_some_and(|enchantment| {
        REGISTRY
            .enchantments
            .is_in_tag(enchantment, &EnchantmentTag::CURSE)
    })
}

/// Strips everything but the curses, and prices what is left.
///
/// Vanilla parity: `GrindstoneMenu.removeNonCursesFrom`. An enchanted book
/// with nothing cursed left on it becomes a plain book, because an enchanted
/// book with no enchantments is not a thing.
fn remove_non_curses(mut item: ItemStack) -> ItemStack {
    let kept: Vec<(Identifier, u32)> = item
        .get_enchantments_for_crafting()
        .map(|enchantments| {
            enchantments
                .iter()
                .filter(|(id, _)| is_curse(id))
                .map(|(id, level)| (id.clone(), *level))
                .collect()
        })
        .unwrap_or_default();

    item.replace_enchantments(&kept);

    if item.is(&vanilla_items::ENCHANTED_BOOK) && kept.is_empty() {
        let mut book = ItemStack::with_count(&vanilla_items::BOOK, item.count());
        book.set(REPAIR_COST, 0);
        return book;
    }

    // Vanilla parity: one doubling of the repair cost per surviving
    // enchantment, so a cursed item stays expensive on an anvil afterwards.
    let mut repair_cost = 0i32;
    for _ in 0..kept.len() {
        repair_cost = repair_cost.saturating_mul(2).saturating_add(1);
    }
    item.set(REPAIR_COST, repair_cost);
    item
}

/// Welds two of the same item into one.
///
/// Vanilla parity: `GrindstoneMenu.mergeItems`.
fn merge_items(first: &ItemStack, second: &ItemStack) -> ItemStack {
    if !ItemStack::is_same_item(first, second) {
        return ItemStack::empty();
    }

    let durability = first.get_max_damage().max(second.get_max_damage());
    let remaining = (first.get_max_damage() - first.get_damage_value())
        + (second.get_max_damage() - second.get_damage_value())
        + durability * WELD_BONUS_PERCENT / 100;

    let mut count = 1;
    if !first.is_damageable_item() {
        // Two undamageable items only stack; anything that cannot stack to two
        // has nothing to weld.
        if first.max_stack_size() < 2 || !ItemStack::is_same_item_same_components(first, second) {
            return ItemStack::empty();
        }
        count = 2;
    }

    let mut welded = first.copy_with_count(count);
    if welded.is_damageable_item() {
        welded.set(MAX_DAMAGE, durability);
        welded.set_damage_value((durability - remaining).max(0));
    }

    merge_enchantments_from(&mut welded, second);
    remove_non_curses(welded)
}

/// Carries `source`'s enchantments onto `target` before the strip.
///
/// Vanilla parity: `mergeEnchantsFrom`. Curses are only carried when the
/// target does not already have them, which stops two cursed items stacking
/// the same curse to a higher level.
fn merge_enchantments_from(target: &mut ItemStack, source: &ItemStack) {
    let Some(source_enchantments) = source.get_enchantments_for_crafting() else {
        return;
    };
    let mut merged = target
        .get_enchantments_for_crafting()
        .cloned()
        .unwrap_or_else(ItemEnchantments::empty);

    for (id, level) in source_enchantments.iter() {
        if !is_curse(id) || merged.get_level(id) == 0 {
            merged.upgrade(id.clone(), *level);
        }
    }

    let levels: Vec<(Identifier, u32)> = merged
        .iter()
        .map(|(id, level)| (id.clone(), *level))
        .collect();
    target.replace_enchantments(&levels);
}

/// Returns what the grindstone would make of these two inputs.
///
/// Vanilla parity: `GrindstoneMenu.computeResult`.
#[must_use]
pub fn grindstone_result(first: &ItemStack, second: &ItemStack) -> ItemStack {
    if first.is_empty() && second.is_empty() {
        return ItemStack::empty();
    }
    // A grindstone works on single items; a stack in either slot does nothing.
    if first.count() > 1 || second.count() > 1 {
        return ItemStack::empty();
    }

    if first.is_empty() || second.is_empty() {
        let lone = if first.is_empty() { second } else { first };
        let has_enchantments = lone
            .get_enchantments_for_crafting()
            .is_some_and(|enchantments| !enchantments.is_empty());
        if !has_enchantments {
            return ItemStack::empty();
        }
        return remove_non_curses(lone.clone());
    }

    merge_items(first, second)
}

impl ResultHandler for GrindstoneHandler {
    fn result_container(&self) -> ContainerRef {
        ContainerRef::from(self.result_container.clone())
    }

    fn dependencies(&self) -> Vec<ContainerRef> {
        vec![ContainerRef::from(self.input_container.clone())]
    }

    fn update_result(&self, guard: &mut ContainerLockGuard) {
        let result = self
            .input_snapshot(guard)
            .map_or_else(ItemStack::empty, |(first, second)| {
                grindstone_result(&first, &second)
            });

        let result_id = self.result_id();
        let Some(container) = guard.get_typed_mut::<ResultContainer>(result_id) else {
            return;
        };
        container.set_item(0, result);
        container.set_changed();
    }

    /// Vanilla parity: the `onTake` of the result slot, which empties both
    /// inputs -- a grindstone consumes what it grinds whole.
    fn on_result_taken(
        &self,
        guard: &mut ContainerLockGuard,
        _player: &Player,
    ) -> Option<ItemStack> {
        self.clear_inputs(guard);
        self.update_result(guard);
        None
    }

    fn is_result_valid(&self, guard: &ContainerLockGuard, _player: &Player) -> bool {
        self.input_snapshot(guard)
            .is_some_and(|(first, second)| !grindstone_result(&first, &second).is_empty())
    }
}

/// A grindstone's inputs only take things it can work on.
///
/// Vanilla parity: the `mayPlace` of both input slots -- a slot that refuses
/// what it cannot grind is what stops a player parking a stack of cobblestone
/// in one.
#[must_use]
pub fn grindstone_accepts(item: &ItemStack) -> bool {
    item.is_damageable_item()
        || item
            .get_enchantments_for_crafting()
            .is_some_and(|enchantments| !enchantments.is_empty())
}

#[cfg(test)]
mod tests {
    use foton_registry::data_components::vanilla_components::{MAX_DAMAGE, REPAIR_COST};
    use foton_registry::item_stack::ItemStack;
    use foton_registry::{init_vanilla_registry, vanilla_items};
    use foton_utils::Identifier;

    use foton_registry::data_components::components::ItemEnchantments;
    use foton_registry::items::Item;

    use super::{grindstone_accepts, grindstone_result};

    /// Builds an enchanted item.
    fn enchanted(item: &'static Item, enchantment: &'static str, level: u32) -> ItemStack {
        let mut stack = ItemStack::new(item);
        stack.set_enchantments(&[(Identifier::vanilla_static(enchantment), level)], false);
        stack
    }

    /// The curse tag really has something in it.
    ///
    /// Every test below that distinguishes a curse from an ordinary enchantment is
    /// meaningless if this is empty, and it comes from extracted data rather than
    /// anything in this file.
    #[test]
    fn binding_curse_is_a_curse_and_efficiency_is_not() {
        init_vanilla_registry();
        let cursed = enchanted(&vanilla_items::DIAMOND_PICKAXE, "binding_curse", 1);
        let ordinary = enchanted(&vanilla_items::DIAMOND_PICKAXE, "efficiency", 3);

        // Grinding strips one and keeps the other, which is the observable form of
        // the same question.
        let ground_cursed = grindstone_result(&cursed, &ItemStack::empty());
        let ground_ordinary = grindstone_result(&ordinary, &ItemStack::empty());

        assert_eq!(
            ground_cursed
                .get_enchantments_for_crafting()
                .map_or(0, ItemEnchantments::len),
            1,
            "a curse survives the grindstone"
        );
        assert_eq!(
            ground_ordinary
                .get_enchantments_for_crafting()
                .map_or(0, ItemEnchantments::len),
            0,
            "an ordinary enchantment does not"
        );
    }

    /// An unenchanted item alone gives nothing.
    ///
    /// Vanilla parity: `computeResult` returns empty for a lone item with no
    /// enchantments -- there is nothing to strip, so the grindstone has no work.
    #[test]
    fn a_plain_item_alone_makes_nothing() {
        init_vanilla_registry();
        let result = grindstone_result(
            &ItemStack::new(&vanilla_items::DIAMOND_PICKAXE),
            &ItemStack::empty(),
        );
        assert!(result.is_empty());
    }

    /// Two empty slots make nothing.
    #[test]
    fn nothing_in_makes_nothing_out() {
        init_vanilla_registry();
        assert!(grindstone_result(&ItemStack::empty(), &ItemStack::empty()).is_empty());
    }

    /// A stack in either slot makes nothing.
    ///
    /// Vanilla parity: the `count <= 1` guard. A grindstone works on one item at a
    /// time, and this is what stops a stack of enchanted books being stripped in
    /// one go.
    #[test]
    fn a_stack_is_refused() {
        init_vanilla_registry();
        let mut stacked = enchanted(&vanilla_items::ENCHANTED_BOOK, "efficiency", 1);
        stacked.set_count(2);

        assert!(grindstone_result(&stacked, &ItemStack::empty()).is_empty());
    }

    /// An enchanted book with nothing cursed on it comes out a plain book.
    ///
    /// Vanilla parity: the `transmuteCopy(Items.BOOK)` of `removeNonCursesFrom`.
    #[test]
    fn an_enchanted_book_becomes_a_book() {
        init_vanilla_registry();
        let book = enchanted(&vanilla_items::ENCHANTED_BOOK, "efficiency", 3);

        let result = grindstone_result(&book, &ItemStack::empty());

        assert!(
            result.is(&vanilla_items::BOOK),
            "an enchanted book with no enchantments left is just a book"
        );
    }

    /// A book that keeps a curse stays an enchanted book.
    #[test]
    fn a_cursed_book_stays_enchanted() {
        init_vanilla_registry();
        let book = enchanted(&vanilla_items::ENCHANTED_BOOK, "vanishing_curse", 1);

        let result = grindstone_result(&book, &ItemStack::empty());

        assert!(
            result.is(&vanilla_items::ENCHANTED_BOOK),
            "the curse is still on it, so it is still an enchanted book"
        );
    }

    /// Welding two damaged tools gives back more durability than either had.
    ///
    /// Vanilla parity: `mergeItems`, including the five percent bonus that makes
    /// grinding two half-broken tools worth doing.
    #[test]
    fn two_damaged_tools_weld_into_a_better_one() {
        init_vanilla_registry();
        let max = ItemStack::new(&vanilla_items::DIAMOND_PICKAXE).get_max_damage();
        assert!(max > 0, "a diamond pickaxe is damageable");

        let mut first = ItemStack::new(&vanilla_items::DIAMOND_PICKAXE);
        first.set_damage_value(max / 2);
        let mut second = ItemStack::new(&vanilla_items::DIAMOND_PICKAXE);
        second.set_damage_value(max / 2);

        let welded = grindstone_result(&first, &second);

        assert!(welded.is(&vanilla_items::DIAMOND_PICKAXE));
        assert!(
            welded.get_damage_value() < max / 2,
            "welding two half-worn pickaxes should give back more than either had, \
             got damage {} of {max}",
            welded.get_damage_value()
        );
    }

    /// Two different items do not weld.
    #[test]
    fn different_items_do_not_weld() {
        init_vanilla_registry();
        let result = grindstone_result(
            &ItemStack::new(&vanilla_items::DIAMOND_PICKAXE),
            &ItemStack::new(&vanilla_items::IRON_PICKAXE),
        );
        assert!(result.is_empty());
    }

    /// Grinding clears the repair cost an anvil would have charged.
    #[test]
    fn grinding_prices_the_result_from_scratch() {
        init_vanilla_registry();
        let mut expensive = enchanted(&vanilla_items::DIAMOND_PICKAXE, "efficiency", 3);
        expensive.set(REPAIR_COST, 31);

        let result = grindstone_result(&expensive, &ItemStack::empty());

        assert_eq!(
            result.get(REPAIR_COST).copied().unwrap_or(0),
            0,
            "nothing enchanted survived, so the anvil price resets"
        );
    }

    /// A grindstone slot only takes what it can work on.
    #[test]
    fn the_slots_refuse_what_cannot_be_ground() {
        init_vanilla_registry();
        assert!(
            grindstone_accepts(&ItemStack::new(&vanilla_items::DIAMOND_PICKAXE)),
            "a tool can be ground for its durability"
        );
        assert!(
            grindstone_accepts(&enchanted(&vanilla_items::ENCHANTED_BOOK, "efficiency", 1)),
            "an enchanted book can be ground for its enchantments"
        );
        assert!(
            !grindstone_accepts(&ItemStack::new(&vanilla_items::COBBLESTONE)),
            "a block of cobblestone has nothing to grind"
        );
    }

    /// The welded tool's durability ceiling is the larger of the two.
    #[test]
    fn welding_keeps_the_larger_durability_ceiling() {
        init_vanilla_registry();
        let mut first = ItemStack::new(&vanilla_items::DIAMOND_PICKAXE);
        let ceiling = first.get_max_damage();
        first.set(MAX_DAMAGE, ceiling - 100);
        let second = ItemStack::new(&vanilla_items::DIAMOND_PICKAXE);

        let welded = grindstone_result(&first, &second);

        assert_eq!(
            welded.get_max_damage(),
            ceiling,
            "the better of the two ceilings wins"
        );
    }
}
