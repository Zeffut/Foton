//! Shared vanilla `AbstractChestedHorse` state and hooks.
//!
//! Vanilla parity: `AbstractChestedHorse`. The chest is what separates a donkey
//! from a horse: strapping one on grows the mob's own inventory instead of
//! opening a block, so the container has to survive a save on the entity and be
//! rebuilt whenever the chest comes or goes.

use simdnbt::ToNbtTag as _;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::{NbtCompound, NbtList, NbtTag};
use steel_registry::item_stack::ItemStack;
use steel_registry::{sound_events, vanilla_items};
use steel_utils::types::InteractionHand;

use crate::behavior::InteractionResult;
use crate::entity::Mob;
use crate::entity::equine::AbstractHorse;
use crate::inventory::container::Container as _;
use crate::player::Player;

/// Inventory columns a chested horse gains from its chest.
///
/// Vanilla parity: the `hasChest() ? 5 : 0` of `AbstractChestedHorse.getInventoryColumns`.
const CHESTED_INVENTORY_COLUMNS: usize = 5;

/// Vanilla-shaped behavior shared by entities that extend `AbstractChestedHorse`.
pub trait AbstractChestedHorse: AbstractHorse {
    /// Returns the synchronized `AbstractChestedHorse.DATA_ID_CHEST` flag.
    fn has_chest(&self) -> bool;

    /// Sets the synchronized `AbstractChestedHorse.DATA_ID_CHEST` flag.
    fn set_chest(&self, has_chest: bool);

    /// Returns vanilla `AbstractChestedHorse.getInventoryColumns`.
    fn chested_horse_inventory_columns(&self) -> usize {
        if self.has_chest() {
            CHESTED_INVENTORY_COLUMNS
        } else {
            0
        }
    }

    /// Applies vanilla `AbstractChestedHorse.playChestEquipsSound`.
    fn play_chest_equips_sound(&self) {
        let pitch = (rand::random::<f32>() - rand::random::<f32>()).mul_add(0.2, 1.0);
        self.play_sound(&sound_events::ENTITY_DONKEY_CHEST, 1.0, pitch);
    }

    /// Applies vanilla `AbstractChestedHorse.equipChest`.
    fn equip_chest(&self, player: &Player, hand: InteractionHand) {
        self.set_chest(true);
        self.play_chest_equips_sound();
        Mob::use_player_item(self, player, hand);
        self.create_horse_inventory();
    }

    /// Applies vanilla `AbstractChestedHorse.mobInteract`.
    fn chested_horse_mob_interact(
        &self,
        player: &Player,
        hand: InteractionHand,
    ) -> InteractionResult {
        if self.skips_feeding_interact(player, Some(&vanilla_items::GOLDEN_DANDELION)) {
            return self.abstract_horse_mob_interact(player, hand);
        }

        if let Some(result) = self.try_feed_or_anger(player, hand) {
            return result;
        }

        let holds_chest = {
            let inventory = player.inventory.lock();
            inventory.get_item_in_hand(hand).is(&vanilla_items::CHEST)
        };
        if !self.has_chest() && holds_chest {
            self.equip_chest(player, hand);
            return InteractionResult::Success;
        }

        self.abstract_horse_mob_interact(player, hand)
    }

    /// Drops the chest a chested horse was carrying.
    ///
    /// Vanilla parity: the chest half of `AbstractChestedHorse.dropEquipment`.
    fn drop_chested_horse_chest(&self) {
        if !self.has_chest() {
            return;
        }
        self.spawn_at_location(ItemStack::new(&vanilla_items::CHEST), 0.0);
        self.set_chest(false);
    }

    /// Saves vanilla `AbstractChestedHorse` fields.
    fn save_chested_horse(&self, nbt: &mut NbtCompound) {
        nbt.insert("ChestedHorse", i8::from(self.has_chest()));
        if !self.has_chest() {
            return;
        }

        let inventory = self.abstract_horse_base().inventory();
        let container = inventory.lock();
        let mut items: Vec<NbtCompound> = Vec::new();
        for (slot, item) in container.items().iter().enumerate() {
            if !item.is_empty()
                && let NbtTag::Compound(mut item_nbt) = item.clone().to_nbt_tag()
            {
                item_nbt.insert("Slot", slot as i8);
                items.push(item_nbt);
            }
        }
        drop(container);
        nbt.insert("Items", NbtList::Compound(items));
    }

    /// Loads vanilla `AbstractChestedHorse` fields.
    fn load_chested_horse(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.set_chest(nbt.byte("ChestedHorse").is_some_and(|value| value != 0));
        self.create_horse_inventory();
        if !self.has_chest() {
            return;
        }

        let inventory = self.abstract_horse_base().inventory();
        let mut container = inventory.lock();
        let size = container.get_container_size();
        let Some(items_list) = nbt.list("Items") else {
            return;
        };
        let Some(compounds) = items_list.compounds() else {
            return;
        };
        for compound in compounds {
            let Some(slot) = compound.byte("Slot") else {
                continue;
            };
            let slot = slot as usize;
            if slot < size
                && let Some(item) = ItemStack::from_borrowed_compound(&compound)
            {
                container.items_mut()[slot] = item;
            }
        }
    }
}
