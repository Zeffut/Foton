//! Campfire block entity implementation.
//!
//! A campfire holds four items on its edge and cooks each one on its own
//! timer. It is deliberately not a [`Container`](crate::inventory::container::Container):
//! vanilla's `CampfireBlockEntity` implements `Clearable` and nothing else, so
//! a hopper cannot feed it and a comparator cannot read it. Food goes on one
//! item at a time by hand, and comes off as a dropped item entity.

use std::mem;
use std::sync::{Arc, Weak};

use simdnbt::ToNbtTag as _;
use simdnbt::borrow::{BaseNbtCompound as BorrowedNbtCompound, NbtCompound as NbtCompoundView};
use simdnbt::owned::{NbtCompound, NbtList, NbtTag};
use steel_registry::item_stack::ItemStack;
use steel_registry::{REGISTRY, vanilla_block_entity_types, vanilla_game_events};
use steel_utils::locks::SyncMutex;
use steel_utils::{BlockPos, BlockStateId, Downcast as _, DowncastType, DowncastTypeKey};

use crate::block_entity::{BlockEntity, BlockEntityBase};
use crate::entity::Entity;
use crate::world::World;
use crate::world::game_event::GameEventContext;

/// Vanilla `CampfireBlockEntity.NUM_SLOTS`.
pub const CAMPFIRE_SLOTS: usize = 4;

/// How much progress an unlit campfire loses each tick.
///
/// Vanilla parity: `CampfireBlockEntity.BURN_COOL_SPEED`.
const BURN_COOL_SPEED: i32 = 2;

const ITEMS_NBT_KEY: &str = "Items";
const ITEM_SLOT_NBT_KEY: &str = "Slot";
const COOKING_TIMES_NBT_KEY: &str = "CookingTimes";
const COOKING_TOTAL_TIMES_NBT_KEY: &str = "CookingTotalTimes";

/// What is on the fire and how far along each piece is.
struct CampfireCooking {
    items: Vec<ItemStack>,
    /// Vanilla `cookingProgress`, saved under `CookingTimes`.
    progress: [i32; CAMPFIRE_SLOTS],
    /// Vanilla `cookingTime`, saved under `CookingTotalTimes`.
    total: [i32; CAMPFIRE_SLOTS],
}

/// Four-slot cooking storage for a campfire or soul campfire.
pub struct CampfireBlockEntity {
    base: BlockEntityBase,
    cooking: SyncMutex<CampfireCooking>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `CampfireBlockEntity`.
unsafe impl DowncastType for CampfireBlockEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:block_entity/campfire");
}

impl CampfireBlockEntity {
    /// Creates a campfire block entity.
    #[must_use]
    pub fn new(level: Weak<World>, pos: BlockPos, state: BlockStateId) -> Self {
        Self {
            base: BlockEntityBase::new(&vanilla_block_entity_types::CAMPFIRE, level, pos, state),
            cooking: SyncMutex::new(CampfireCooking {
                items: vec![ItemStack::empty(); CAMPFIRE_SLOTS],
                progress: [0; CAMPFIRE_SLOTS],
                total: [0; CAMPFIRE_SLOTS],
            }),
        }
    }

    /// Returns a copy of the item cooking in `slot`.
    #[must_use]
    pub fn item(&self, slot: usize) -> ItemStack {
        self.cooking
            .lock()
            .items
            .get(slot)
            .map_or_else(ItemStack::empty, Clone::clone)
    }

    /// Returns how many ticks `slot` has been cooking for.
    #[must_use]
    pub fn cooking_progress(&self, slot: usize) -> i32 {
        self.cooking.lock().progress.get(slot).copied().unwrap_or(0)
    }

    /// Returns how many ticks `slot` needs in total.
    #[must_use]
    pub fn cooking_total_time(&self, slot: usize) -> i32 {
        self.cooking.lock().total.get(slot).copied().unwrap_or(0)
    }

    /// Advances every occupied slot and drops whatever finished.
    ///
    /// Vanilla parity: `CampfireBlockEntity.cookTick`. Vanilla's
    /// `result.isItemEnabled` guard has no counterpart: Steel has no feature
    /// flags, so every recipe result is always enabled.
    pub(crate) fn cook_tick(
        world: &Arc<World>,
        pos: BlockPos,
        state: BlockStateId,
        block_entity: &dyn BlockEntity,
    ) {
        let Some(campfire) = block_entity.downcast_ref::<Self>() else {
            return;
        };

        // The world callbacks below take locks of their own, so the finished
        // items leave the cooking lock before anything is dropped.
        let (changed, finished) = {
            let mut cooking = campfire.cooking.lock();
            let mut changed = false;
            let mut finished: Vec<ItemStack> = Vec::new();

            for slot in 0..CAMPFIRE_SLOTS {
                if cooking.items[slot].is_empty() {
                    continue;
                }
                changed = true;
                cooking.progress[slot] += 1;
                if cooking.progress[slot] < cooking.total[slot] {
                    continue;
                }
                let input = cooking.items[slot].clone();
                let result = REGISTRY
                    .recipes
                    .find_campfire_recipe(&input)
                    .map_or(input, |recipe| recipe.result.to_item_stack());
                cooking.items[slot] = ItemStack::empty();
                finished.push(result);
            }

            (changed, finished)
        };

        for cooked in finished {
            world.drop_item_stack(pos, cooked);
            world.send_block_updated(pos);
            world.game_event(
                &vanilla_game_events::BLOCK_CHANGE,
                pos,
                &GameEventContext::new(None, Some(state)),
            );
        }

        if changed {
            BlockEntity::set_changed(campfire);
        }
    }

    /// Walks every slot's progress back down while the fire is out.
    ///
    /// Vanilla parity: `CampfireBlockEntity.cooldownTick`.
    pub(crate) fn cooldown_tick(
        _world: &Arc<World>,
        _pos: BlockPos,
        _state: BlockStateId,
        block_entity: &dyn BlockEntity,
    ) {
        let Some(campfire) = block_entity.downcast_ref::<Self>() else {
            return;
        };

        let changed = {
            let mut cooking = campfire.cooking.lock();
            let mut changed = false;
            for slot in 0..CAMPFIRE_SLOTS {
                if cooking.progress[slot] <= 0 {
                    continue;
                }
                changed = true;
                cooking.progress[slot] =
                    (cooking.progress[slot] - BURN_COOL_SPEED).clamp(0, cooking.total[slot]);
            }
            changed
        };

        if changed {
            BlockEntity::set_changed(campfire);
        }
    }

    /// Puts one item on the fire, returning whether a slot took it.
    ///
    /// Vanilla parity: `CampfireBlockEntity.placeFood`. Vanilla consumes from
    /// the held stack here through `ItemStack.consumeAndReturn`; Steel's caller
    /// owns the player's inventory lock, so this only copies the single item it
    /// stores and leaves the shrink to the caller.
    pub(crate) fn place_food(
        &self,
        world: &Arc<World>,
        source: Option<&dyn Entity>,
        held: &ItemStack,
    ) -> bool {
        let placed = {
            let mut cooking = self.cooking.lock();
            let Some(slot) = (0..CAMPFIRE_SLOTS).find(|&slot| cooking.items[slot].is_empty())
            else {
                return false;
            };
            // Vanilla looks the recipe up only once it has an empty slot, and
            // gives up on the whole interaction when there is none.
            let Some(recipe) = REGISTRY.recipes.find_campfire_recipe(held) else {
                return false;
            };
            cooking.total[slot] = recipe.cooking_time;
            cooking.progress[slot] = 0;
            cooking.items[slot] = held.copy_with_count(1);
            true
        };

        if !placed {
            return false;
        }

        world.game_event(
            &vanilla_game_events::BLOCK_CHANGE,
            self.base.pos(),
            &GameEventContext::new(source, Some(self.get_block_state())),
        );
        // Vanilla `markUpdated`: dirty for the save, and pushed to every client
        // that can see the campfire, because the food is visible on the block.
        BlockEntity::set_changed(self);
        world.send_block_updated(self.base.pos());
        true
    }

    fn write_items(&self, nbt: &mut NbtCompound) {
        let cooking = self.cooking.lock();
        let mut items: Vec<NbtCompound> = Vec::new();
        for (slot, item) in cooking.items.iter().enumerate() {
            if item.is_empty() {
                continue;
            }
            if let NbtTag::Compound(mut item_nbt) = item.clone().to_nbt_tag() {
                item_nbt.insert(ITEM_SLOT_NBT_KEY, slot as i8);
                items.push(item_nbt);
            }
        }
        nbt.insert(ITEMS_NBT_KEY, NbtList::Compound(items));
    }
}

impl BlockEntity for CampfireBlockEntity {
    fn base(&self) -> &BlockEntityBase {
        &self.base
    }

    /// Vanilla parity: `CampfireBlockEntity.preRemoveSideEffects`, which drops
    /// everything still on the fire when the block goes.
    fn pre_remove_side_effects(&self, pos: BlockPos, _state: BlockStateId) {
        let items = {
            let mut cooking = self.cooking.lock();
            mem::replace(&mut cooking.items, vec![ItemStack::empty(); CAMPFIRE_SLOTS])
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
        let mut cooking = self.cooking.lock();
        cooking.items.fill(ItemStack::empty());

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
                if slot < CAMPFIRE_SLOTS
                    && let Some(item) = ItemStack::from_borrowed_compound(&compound)
                {
                    cooking.items[slot] = item;
                }
            }
        }

        // Vanilla fills both arrays with zeroes when the tag is absent, and
        // copies only as many entries as the shorter of the two lengths.
        cooking.progress = [0; CAMPFIRE_SLOTS];
        cooking.total = [0; CAMPFIRE_SLOTS];
        if let Some(times) = nbt_view.int_array(COOKING_TIMES_NBT_KEY) {
            for (slot, time) in times.iter().take(CAMPFIRE_SLOTS).enumerate() {
                cooking.progress[slot] = *time;
            }
        }
        if let Some(times) = nbt_view.int_array(COOKING_TOTAL_TIMES_NBT_KEY) {
            for (slot, time) in times.iter().take(CAMPFIRE_SLOTS).enumerate() {
                cooking.total[slot] = *time;
            }
        }
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.write_items(nbt);
        let (progress, total) = {
            let cooking = self.cooking.lock();
            (cooking.progress, cooking.total)
        };
        nbt.insert(COOKING_TIMES_NBT_KEY, NbtTag::IntArray(progress.to_vec()));
        nbt.insert(
            COOKING_TOTAL_TIMES_NBT_KEY,
            NbtTag::IntArray(total.to_vec()),
        );
    }

    /// Vanilla parity: `CampfireBlockEntity.getUpdateTag`, which sends the
    /// items and deliberately not the timers -- the client animates smoke from
    /// what it can see on the block, and never needs the progress.
    fn get_update_tag(&self) -> Option<NbtCompound> {
        let mut nbt = NbtCompound::new();
        self.write_items(&mut nbt);
        Some(nbt)
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use simdnbt::borrow::read_compound as read_borrowed_compound;
    use steel_registry::{init_vanilla_registry, vanilla_blocks, vanilla_items};

    use super::*;

    fn test_campfire() -> CampfireBlockEntity {
        init_vanilla_registry();
        CampfireBlockEntity::new(
            Weak::new(),
            BlockPos::new(1, 2, 3),
            vanilla_blocks::CAMPFIRE.default_state(),
        )
    }

    /// A round trip has to carry both timers, not just the food. Losing
    /// `CookingTotalTimes` would leave a reloaded campfire with a total of
    /// zero, and every item on it would finish on the very next lit tick.
    #[test]
    fn cooking_state_survives_a_save_and_load_round_trip() {
        let campfire = test_campfire();
        {
            let mut cooking = campfire.cooking.lock();
            cooking.items[2] = ItemStack::new(&vanilla_items::PORKCHOP);
            cooking.progress[2] = 37;
            cooking.total[2] = 600;
        }

        let mut nbt = NbtCompound::new();
        campfire.save_additional(&mut nbt);
        let mut bytes = Vec::new();
        nbt.write(&mut bytes);

        let borrowed = read_borrowed_compound(&mut Cursor::new(bytes.as_slice()))
            .expect("campfire NBT should re-read");
        let restored = test_campfire();
        restored.load_additional(&borrowed);

        assert!(restored.item(2).is(&vanilla_items::PORKCHOP));
        assert_eq!(restored.cooking_progress(2), 37);
        assert_eq!(restored.cooking_total_time(2), 600);
        assert!(restored.item(0).is_empty());
        assert_eq!(restored.cooking_total_time(0), 0);
    }

    /// The update tag is what a client sees, so it must carry the food. It
    /// must also stay free of the timers: vanilla's client never reads them,
    /// and sending them would make every cooking tick a packet.
    #[test]
    fn the_update_tag_carries_the_food_and_not_the_timers() {
        let campfire = test_campfire();
        {
            let mut cooking = campfire.cooking.lock();
            cooking.items[0] = ItemStack::new(&vanilla_items::BEEF);
            cooking.progress[0] = 12;
            cooking.total[0] = 600;
        }

        let tag = campfire
            .get_update_tag()
            .expect("campfires sync their food");

        assert!(tag.get(ITEMS_NBT_KEY).is_some());
        assert!(tag.get(COOKING_TIMES_NBT_KEY).is_none());
        assert!(tag.get(COOKING_TOTAL_TIMES_NBT_KEY).is_none());
    }
}
