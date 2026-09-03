package org.bukkit.block;

import org.bukkit.inventory.Inventory;

/** Chest block state and its live inventory. */
public interface Chest extends BlockState, org.bukkit.inventory.InventoryHolder {
    default Inventory getBlockInventory() { return getInventory(); }
}
