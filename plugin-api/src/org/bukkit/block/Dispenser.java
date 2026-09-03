package org.bukkit.block;

import org.bukkit.inventory.Inventory;

/** Snapshot of a dispenser block. */
public interface Dispenser extends TileState {
    default Inventory getInventory() {
        throw new UnsupportedOperationException("Dispenser inventories are not exposed by Steel yet");
    }
    default Inventory getSnapshotInventory() {
        throw new UnsupportedOperationException("Dispenser inventories are not exposed by Steel yet");
    }
    default boolean update(boolean force, boolean applyPhysics) {
        throw new UnsupportedOperationException("Dispenser updates are not exposed by Steel yet");
    }
}
