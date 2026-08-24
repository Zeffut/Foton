//! Placing rolled loot into a container.
//!
//! Vanilla parity: `LootTable.fill` and the two helpers it is built on,
//! `shuffleAndSplitItems` and `getAvailableSlots`. Between them they are what
//! makes a dungeon chest look like a dungeon chest: the stacks are broken up
//! and scattered over the free slots instead of being packed into the first
//! few.

use std::mem;

use super::{ItemStack, LootContext, LootTable, RngExt, uniform_int};

/// The container surface [`LootTable::fill`] needs.
///
/// Vanilla fills a `net.minecraft.world.Container`. Steel's `Container` trait
/// lives in `steel-core`, which depends on this crate, so `fill` asks only for
/// the three operations vanilla performs through it.
pub trait LootFillContainer {
    /// Vanilla parity: `Container.getContainerSize`.
    fn get_container_size(&self) -> usize;

    /// Vanilla parity: `Container.getItem`.
    fn get_item(&self, slot: usize) -> &ItemStack;

    /// Vanilla parity: `Container.setItem`.
    fn set_item(&mut self, slot: usize, stack: ItemStack);
}

impl LootTable {
    /// Rolls this table and scatters the result over the container's free slots.
    ///
    /// Vanilla parity: `LootTable.fill`. The roll and the scatter share one
    /// random source, in that order, so a container filled from a fixed loot
    /// table seed always ends up with the same items in the same slots.
    ///
    /// Slots that already hold something are left alone, matching vanilla's
    /// `getAvailableSlots`.
    pub fn fill<C, R>(&self, container: &mut C, ctx: &mut LootContext<'_, R>)
    where
        C: LootFillContainer + ?Sized,
        R: rand::Rng,
    {
        let mut item_stacks = self.get_random_items(ctx);
        let mut available_slots = available_slots(container, ctx.rng);
        shuffle_and_split_items(&mut item_stacks, available_slots.len(), ctx.rng);

        for item_stack in item_stacks {
            let Some(slot) = available_slots.pop() else {
                log::warn!("Tried to over-fill a container");
                return;
            };
            container.set_item(slot, item_stack);
        }
    }
}

/// Breaks large stacks apart until the loot spreads over the free slots.
///
/// Vanilla parity: `LootTable.shuffleAndSplitItems`. Only stacks of more than
/// one are candidates, and a split half is offered up for splitting again on a
/// coin flip, which is why a chest ends up with uneven little piles rather than
/// with every stack cut exactly in two.
fn shuffle_and_split_items<R: rand::Rng>(
    result: &mut Vec<ItemStack>,
    available_slots: usize,
    rng: &mut R,
) {
    let mut splittable_items: Vec<ItemStack> = Vec::new();
    result.retain_mut(|item_stack| {
        if item_stack.is_empty() {
            return false;
        }
        if item_stack.count() > 1 {
            splittable_items.push(mem::take(item_stack));
            return false;
        }
        true
    });

    while !splittable_items.is_empty() && available_slots > result.len() + splittable_items.len() {
        let index = uniform_int(rng, 0, splittable_items.len() as i32 - 1) as usize;
        let mut item_stack = splittable_items.remove(index);
        let remove = uniform_int(rng, 1, item_stack.count() / 2);
        let copy = item_stack.split(remove);

        if item_stack.count() > 1 && rng.random::<bool>() {
            splittable_items.push(item_stack);
        } else {
            result.push(item_stack);
        }

        if copy.count() > 1 && rng.random::<bool>() {
            splittable_items.push(copy);
        } else {
            result.push(copy);
        }
    }

    result.append(&mut splittable_items);
    shuffle(result, rng);
}

/// Returns the empty slots of `container`, in a random order.
///
/// Vanilla parity: `LootTable.getAvailableSlots`. `fill` takes them off the
/// end, so the shuffle here is what decides where each stack lands.
fn available_slots<C, R>(container: &C, rng: &mut R) -> Vec<usize>
where
    C: LootFillContainer + ?Sized,
    R: rand::Rng,
{
    let mut slots: Vec<usize> = (0..container.get_container_size())
        .filter(|&slot| container.get_item(slot).is_empty())
        .collect();
    shuffle(&mut slots, rng);
    slots
}

/// Vanilla parity: `Util.shuffle`, a Fisher-Yates pass walking from the end
/// down to the second element.
fn shuffle<T, R: rand::Rng>(list: &mut [T], rng: &mut R) {
    for i in (2..=list.len()).rev() {
        let swap_to = rng.random_range(0..i);
        list.swap(i - 1, swap_to);
    }
}

#[cfg(test)]
mod tests {
    use rand::SeedableRng as _;
    use rand::rngs::StdRng;

    use super::{LootFillContainer, shuffle_and_split_items};
    use crate::item_stack::ItemStack;
    use crate::loot_table::LootContext;
    use crate::{init_vanilla_registry, vanilla_items, vanilla_loot_tables};

    /// A plain array of slots, which is all `fill` ever asks a container for.
    struct TestContainer {
        items: Vec<ItemStack>,
    }

    impl TestContainer {
        fn new(size: usize) -> Self {
            Self {
                items: vec![ItemStack::empty(); size],
            }
        }

        fn occupied_slots(&self) -> Vec<usize> {
            (0..self.items.len())
                .filter(|&slot| !self.items[slot].is_empty())
                .collect()
        }

        fn contents(&self) -> Vec<(usize, String, i32)> {
            (0..self.items.len())
                .filter(|&slot| !self.items[slot].is_empty())
                .map(|slot| {
                    (
                        slot,
                        self.items[slot].item.key.to_string(),
                        self.items[slot].count(),
                    )
                })
                .collect()
        }
    }

    impl LootFillContainer for TestContainer {
        fn get_container_size(&self) -> usize {
            self.items.len()
        }

        fn get_item(&self, slot: usize) -> &ItemStack {
            &self.items[slot]
        }

        fn set_item(&mut self, slot: usize, stack: ItemStack) {
            self.items[slot] = stack;
        }
    }

    fn fill_dungeon_chest(seed: u64) -> TestContainer {
        init_vanilla_registry();
        let mut container = TestContainer::new(27);
        let mut rng = StdRng::seed_from_u64(seed);
        let mut ctx = LootContext::new(&mut rng);
        vanilla_loot_tables::CHESTS_SIMPLE_DUNGEON.fill(&mut container, &mut ctx);
        container
    }

    #[test]
    fn the_same_seed_fills_a_chest_the_same_way_twice() {
        assert_eq!(
            fill_dungeon_chest(1234).contents(),
            fill_dungeon_chest(1234).contents()
        );
    }

    #[test]
    fn a_different_seed_fills_a_chest_differently() {
        assert_ne!(
            fill_dungeon_chest(1234).contents(),
            fill_dungeon_chest(9876).contents()
        );
    }

    /// The point of the scatter: a dungeon chest must not be a solid block of
    /// items in the first N slots.
    #[test]
    fn loot_lands_on_scattered_slots_rather_than_the_first_ones() {
        let occupied = fill_dungeon_chest(1234).occupied_slots();

        assert!(!occupied.is_empty(), "simple_dungeon should roll something");
        let contiguous_from_zero: Vec<usize> = (0..occupied.len()).collect();
        assert_ne!(
            occupied, contiguous_from_zero,
            "loot was packed into the first slots instead of being scattered"
        );
    }

    /// Vanilla never overwrites what is already in the container.
    #[test]
    fn filling_leaves_occupied_slots_alone() {
        init_vanilla_registry();
        let mut container = TestContainer::new(27);
        let kept = ItemStack::with_count(&vanilla_items::DIAMOND, 7);
        for slot in 0..26 {
            container.set_item(slot, kept.clone());
        }

        let mut rng = StdRng::seed_from_u64(4242);
        let mut ctx = LootContext::new(&mut rng);
        vanilla_loot_tables::CHESTS_SIMPLE_DUNGEON.fill(&mut container, &mut ctx);

        for slot in 0..26 {
            assert!(
                container.get_item(slot).is(&vanilla_items::DIAMOND),
                "slot {slot} was overwritten"
            );
        }
    }

    /// Splitting stops once there are as many piles as free slots, and every
    /// item that went in comes back out.
    #[test]
    fn splitting_preserves_every_item_and_respects_the_free_slot_count() {
        init_vanilla_registry();
        let mut rng = StdRng::seed_from_u64(7);
        let mut result = vec![
            ItemStack::with_count(&vanilla_items::STONE, 32),
            ItemStack::with_count(&vanilla_items::DIRT, 9),
            ItemStack::empty(),
        ];

        shuffle_and_split_items(&mut result, 8, &mut rng);

        assert!(
            result.len() <= 8,
            "split into more piles than there are slots"
        );
        assert!(result.len() > 2, "nothing was split apart");
        let stone: i32 = result
            .iter()
            .filter(|stack| stack.is(&vanilla_items::STONE))
            .map(ItemStack::count)
            .sum();
        let dirt: i32 = result
            .iter()
            .filter(|stack| stack.is(&vanilla_items::DIRT))
            .map(ItemStack::count)
            .sum();
        assert_eq!((stone, dirt), (32, 9));
        assert!(result.iter().all(|stack| !stack.is_empty()));
    }
}
