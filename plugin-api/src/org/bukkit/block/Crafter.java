package org.bukkit.block;

import org.bukkit.inventory.Inventory;

/** Live inventory view of a vanilla crafter block. */
public interface Crafter extends TileState, org.bukkit.inventory.BlockInventoryHolder {
    @Override Inventory getInventory();
    default Inventory getSnapshotInventory() { return getInventory(); }
}
