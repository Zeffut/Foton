package org.bukkit.block;

import org.bukkit.Location;
import org.bukkit.inventory.Inventory;
import org.bukkit.inventory.InventoryHolder;

/** Combined holder for a double chest. */
interface DoubleChestSides { InventoryHolder getLeftSide(); InventoryHolder getRightSide(); }

public final class DoubleChest implements InventoryHolder, DoubleChestSides {
    private final Inventory inventory; private final Location location;
    public DoubleChest(Inventory inventory, Location location) { this.inventory = inventory; this.location = location; }
    @Override public Inventory getInventory() { return inventory; }
    public Location getLocation() { return location; }
    public Chest getLeftSide() { return inventory instanceof Chest chest ? chest : null; }
    public Chest getRightSide() { return inventory instanceof Chest chest ? chest : null; }
    public InventoryHolder getLeftSide(boolean useSnapshot) { return getLeftSide(); }
    public InventoryHolder getRightSide(boolean useSnapshot) { return getRightSide(); }
}
