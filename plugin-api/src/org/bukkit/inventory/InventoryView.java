package org.bukkit.inventory;

import org.bukkit.entity.HumanEntity;

/** A read/write view of the inventories participating in a player's menu. */
public interface InventoryView {
    default org.bukkit.event.inventory.InventoryType getType() { return getTopInventory().getType(); }
    public abstract Inventory getTopInventory();
    public abstract Inventory getBottomInventory();
    public abstract HumanEntity getPlayer();
    public abstract String getTitle();

    /** Closes this view for its owning human entity when supported. */
    default void close() {
        HumanEntity owner = getPlayer();
        if (owner instanceof org.bukkit.entity.Player player) player.closeInventory();
    }

    default int countSlots() {
        return getTopInventory().getSize() + getBottomInventory().getSize();
    }

    default Inventory getInventory(int rawSlot) {
        if (rawSlot < 0) return null;
        int top = getTopInventory().getSize();
        if (rawSlot < top) return getTopInventory();
        if (rawSlot < top + getBottomInventory().getSize()) return getBottomInventory();
        return null;
    }

    default int convertSlot(int rawSlot) {
        int top = getTopInventory().getSize();
        if (rawSlot < 0) return -1;
        if (rawSlot < top) return rawSlot;
        int bottom = rawSlot - top;
        if (bottom < 0 || bottom >= 36) return -1;
        // Bukkit's visible bottom inventory is main storage (9..35), then hotbar (0..8).
        return bottom < 27 ? bottom + 9 : bottom - 27;
    }

    default ItemStack getItem(int rawSlot) {
        Inventory inventory = getInventory(rawSlot);
        int slot = convertSlot(rawSlot);
        return inventory == null || slot < 0 ? null : inventory.getItem(slot);
    }

    default void setItem(int rawSlot, ItemStack item) {
        Inventory inventory = getInventory(rawSlot);
        int slot = convertSlot(rawSlot);
        if (inventory != null && slot >= 0) inventory.setItem(slot, item);
    }
}
