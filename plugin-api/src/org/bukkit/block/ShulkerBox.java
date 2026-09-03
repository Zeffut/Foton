package org.bukkit.block;

import org.bukkit.inventory.Inventory;

/** Shulker-box block state snapshot. */
public interface ShulkerBox extends BlockState {
    Inventory getInventory();
    default Inventory getSnapshotInventory() { return getInventory(); }
}
