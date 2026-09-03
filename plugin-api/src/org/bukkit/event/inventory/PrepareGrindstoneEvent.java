package org.bukkit.event.inventory;

import org.bukkit.event.Event;
import org.bukkit.event.HandlerList;
import org.bukkit.inventory.GrindstoneInventory;
import org.bukkit.inventory.ItemStack;
import org.bukkit.inventory.InventoryView;

/** Fired after a grindstone preview is computed and before it is shown. */
public class PrepareGrindstoneEvent extends InventoryEvent {
    private final GrindstoneInventory inventory;
    private ItemStack result;
    private static final HandlerList HANDLERS = new HandlerList();

    public PrepareGrindstoneEvent(InventoryView view, GrindstoneInventory inventory, ItemStack result) {
        super(view);
        this.inventory = inventory;
        this.result = result == null ? null : result.clone();
    }
    @Override public GrindstoneInventory getInventory() { return inventory; }
    public ItemStack getResult() { return result == null ? null : result.clone(); }
    public void setResult(ItemStack result) { this.result = result == null ? null : result.clone(); }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
