package org.bukkit.event.inventory;

import org.bukkit.event.HandlerList;
import org.bukkit.inventory.InventoryView;
import org.bukkit.inventory.ItemStack;

/** A workstation screen has worked out its result. Its handler list is the
 * one every such event shares, as in Paper. */
@Deprecated
public class PrepareInventoryResultEvent extends InventoryEvent {
    private static final HandlerList HANDLER_LIST = new HandlerList();
    private ItemStack result;

    public PrepareInventoryResultEvent(InventoryView inventory, ItemStack result) {
        super(inventory);
        this.result = result;
    }

    public ItemStack getResult() { return result; }
    public void setResult(ItemStack result) { this.result = result; }
    @Override public HandlerList getHandlers() { return HANDLER_LIST; }
    public static HandlerList getHandlerList() { return HANDLER_LIST; }
}
