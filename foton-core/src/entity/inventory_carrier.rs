//! Mobs that carry a container of their own.
//!
//! Vanilla parity: `net.minecraft.world.entity.npc.InventoryCarrier`. The
//! piglin and the villager already carried one each with their own copy of the
//! save code; this is the shared interface behind them, and it is what lets a
//! brain behavior reach an inventory whose shape it does not know.

use std::sync::Arc;

use foton_protocol::packets::game::CTakeItemEntity;
use foton_registry::item_stack::ItemStack;
use foton_utils::ChunkPos;
use foton_utils::locks::SyncMutex;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::{NbtCompound, NbtList, NbtTag};

use crate::entity::entities::ItemEntity;
use crate::entity::{Mob, RemovalReason, SharedEntity};
use crate::inventory::container::{Container as _, SimpleContainer};
use crate::world::World;

/// The NBT key a carried inventory is stored under.
///
/// Vanilla parity: `InventoryCarrier.TAG_INVENTORY`.
pub const TAG_INVENTORY: &str = "Inventory";

/// A mob with a container of its own.
///
/// Vanilla parity: the `InventoryCarrier` interface.
pub trait InventoryCarrier: Mob {
    /// Vanilla parity: `InventoryCarrier.getInventory`.
    fn carried_inventory(&self) -> &SyncMutex<SimpleContainer>;
}

/// Writes the carried inventory.
///
/// Vanilla parity: `InventoryCarrier.writeInventoryToTag`.
pub fn save_inventory(inventory: &SimpleContainer, nbt: &mut NbtCompound) {
    let items: Vec<NbtCompound> = inventory
        .items()
        .iter()
        .filter(|item| !item.is_empty())
        .filter_map(|item| match item.to_nbt_tag_ref() {
            NbtTag::Compound(compound) => Some(compound),
            _ => None,
        })
        .collect();
    nbt.insert(TAG_INVENTORY, NbtList::Compound(items));
}

/// Reads the carried inventory back.
///
/// Vanilla parity: `InventoryCarrier.readInventoryFromTag`.
pub fn load_inventory(inventory: &mut SimpleContainer, nbt: BorrowedNbtCompoundView<'_, '_>) {
    let Some(list) = nbt.list(TAG_INVENTORY).and_then(|list| list.compounds()) else {
        return;
    };
    for compound in list {
        let Some(mut item) = ItemStack::from_borrowed_compound(&compound) else {
            continue;
        };
        inventory.add(&mut item);
    }
}

/// Whether `stack` would fit in this container at all.
///
/// Vanilla parity: `SimpleContainer.canAddItem`, which asks only whether some
/// slot could take part of it -- an item that half fits still counts.
#[must_use]
pub fn can_add_item(inventory: &SimpleContainer, stack: &ItemStack) -> bool {
    inventory.items().iter().any(|slot| {
        slot.is_empty()
            || (ItemStack::is_same_item_same_components(slot, stack)
                && slot.count() < slot.max_stack_size())
    })
}

/// Takes a dropped item into the carrier's own container.
///
/// Vanilla parity: the static `InventoryCarrier.pickUpItem`. The count is read
/// before the add so the pickup animation shows how many actually went in, and
/// an item that only half fits is left on the ground with the remainder.
pub fn pick_up_item(
    world: &Arc<World>,
    carrier: &dyn InventoryCarrier,
    item_entity: &SharedEntity,
) {
    use foton_utils::Downcast as _;

    let Some(item) = item_entity.downcast_ref::<ItemEntity>() else {
        return;
    };
    let stack = item.get_item();
    if !carrier.wants_to_pick_up(world, &stack) {
        return;
    }

    let mut remainder = stack.copy_with_count(stack.count());
    let count = remainder.count();
    {
        let mut inventory = carrier.carried_inventory().lock();
        if !can_add_item(&inventory, &remainder) {
            return;
        }
        inventory.add(&mut remainder);
    }

    // Vanilla parity: `mob.take(itemEntity, count - remainder.getCount())`,
    // which is the pickup animation every client draws.
    let taken = count - remainder.count();
    if taken > 0 {
        world.broadcast_to_nearby(
            ChunkPos::from_entity_pos(item_entity.position()),
            CTakeItemEntity::new(item_entity.id(), carrier.id(), taken),
            None,
        );
    }

    if remainder.is_empty() {
        item_entity.set_removed(RemovalReason::Discarded);
    } else {
        item.set_item(remainder);
    }
}
