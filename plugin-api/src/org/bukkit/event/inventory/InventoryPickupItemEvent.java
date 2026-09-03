package org.bukkit.event.inventory;

import org.bukkit.entity.Item;
import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;
import org.bukkit.inventory.Inventory;

/** Fired when a hopper or similar inventory picks up an item entity. */
public class InventoryPickupItemEvent extends org.bukkit.event.Event implements Cancellable {
    private final Inventory inventory; private final Item item; private boolean cancelled;
    private static final HandlerList HANDLERS = new HandlerList();
    public InventoryPickupItemEvent(Inventory inventory, Item item) { this.inventory = inventory; this.item = item; }
    public Inventory getInventory() { return inventory; }
    public Item getItem() { return item; }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean value) { cancelled = value; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
