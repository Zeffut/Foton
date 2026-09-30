package org.bukkit.event.inventory;

import org.bukkit.inventory.InventoryView;
import org.bukkit.inventory.ItemStack;
import org.bukkit.inventory.SmithingInventory;

/** A smithing table has worked out what its inputs make; the result left
 * here is the one shown. */
public class PrepareSmithingEvent extends com.destroystokyo.paper.event.inventory.PrepareResultEvent {
    public PrepareSmithingEvent(InventoryView inventory, ItemStack result) {
        super(inventory, result);
    }

    @Override public SmithingInventory getInventory() { return (SmithingInventory) super.getInventory(); }
}
