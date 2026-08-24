//! The two per-tick passes a player runs over the stacks they carry.
//!
//! Vanilla splits them: `Inventory.tick` and `EntityEquipment.tick` drive
//! `Item.inventoryTick`, while `ServerPlayer.doTick` walks the same slots again
//! to flush map updates. Steel keeps the split, because the second pass must
//! run after the first has redrawn the pixels.

use std::mem;

use steel_registry::data_components::components::MapId;
use steel_registry::data_components::vanilla_components::MAP_ID;
use steel_registry::equipment::EquipmentSlot;
use steel_registry::item_stack::ItemStack;

use crate::behavior::ITEM_BEHAVIORS;
use crate::behavior::items::saved_map_data;
use crate::inventory::container::Container as _;
use crate::player::Player;
use crate::player::player_inventory::PlayerInventory;

/// The equipment slot a container index stands for, if any.
///
/// Vanilla parity: the `i == this.selected ? MAINHAND : null` of
/// `Inventory.tick` merged with `EntityEquipment.tick`'s own slot keys.
const fn equipment_slot_for(slot: usize, selected: usize) -> Option<EquipmentSlot> {
    if slot == selected {
        return Some(EquipmentSlot::MainHand);
    }
    match slot {
        36 => Some(EquipmentSlot::Feet),
        37 => Some(EquipmentSlot::Legs),
        38 => Some(EquipmentSlot::Chest),
        39 => Some(EquipmentSlot::Head),
        40 => Some(EquipmentSlot::OffHand),
        41 => Some(EquipmentSlot::Body),
        42 => Some(EquipmentSlot::Saddle),
        _ => None,
    }
}

impl Player {
    /// Runs every carried stack's per-tick behavior.
    ///
    /// Vanilla parity: `Inventory.tick`, called from `Player.aiStep`, plus
    /// `EntityEquipment.tick` from `LivingEntity.tick`. Steel runs both in one
    /// pass because a player's worn slots live in the same container.
    ///
    /// The stack is lifted out of its slot for the duration of the call, the
    /// way `updating_using_item` does: a behavior is free to lock the
    /// inventory again, and Steel's mutex is not reentrant.
    pub(crate) fn tick_inventory_items(&self) {
        let world = self.get_world();
        let selected = usize::from(self.inventory.lock().get_selected_slot());

        for slot in 0..PlayerInventory::CONTAINER_SIZE {
            let mut stack = {
                let mut inventory = self.inventory.lock();
                if inventory.get_item(slot).is_empty() {
                    continue;
                }
                mem::replace(inventory.get_item_mut(slot), ItemStack::empty())
            };

            ITEM_BEHAVIORS.get_behavior(stack.item()).inventory_tick(
                &mut stack,
                &world,
                self,
                equipment_slot_for(slot, selected),
            );

            let mut inventory = self.inventory.lock();
            *inventory.get_item_mut(slot) = stack;
        }
    }

    /// Sends every carried map whatever it still owes this client.
    ///
    /// Vanilla parity: the `synchronizeSpecialItemUpdates` loop of
    /// `ServerPlayer.doTick`.
    pub(crate) fn sync_map_item_updates(&self) {
        let world = self.get_world();
        let map_ids: Vec<MapId> = {
            let inventory = self.inventory.lock();
            (0..PlayerInventory::CONTAINER_SIZE)
                .filter_map(|slot| inventory.get_item(slot).get(MAP_ID).copied())
                .collect()
        };

        for map_id in map_ids {
            let Some(data) = saved_map_data(&world, self, map_id) else {
                continue;
            };
            let packet = data.lock().update_packet(map_id.id(), self.gameprofile.id);
            if let Some(packet) = packet {
                self.send_packet(packet);
            }
        }
    }
}
