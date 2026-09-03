package org.bukkit.block;

import org.bukkit.Location;
import org.bukkit.inventory.Inventory;

/** Snapshot of a hopper block. */
public interface Hopper extends TileState, org.bukkit.inventory.BlockInventoryHolder {
    @Override Location getLocation();
    default String getCustomName() { return null; }
    default void setCustomName(String name) { }
    Inventory getInventory();
}
