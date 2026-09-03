package org.bukkit.event.inventory;

import org.bukkit.event.Cancellable;
import org.bukkit.event.Event;
import org.bukkit.event.HandlerList;
import org.bukkit.inventory.Inventory;

/** Fired when a container moves an item into another container. */
public class InventoryMoveItemEvent extends Event implements Cancellable {
    private final Inventory source;
    private final Inventory destination;
    private final org.bukkit.inventory.ItemStack item;
    private final boolean didSourceInitiate;
    private boolean cancelled;
    private static final HandlerList HANDLERS = new HandlerList();
    public InventoryMoveItemEvent(Inventory source, org.bukkit.inventory.ItemStack item, Inventory destination) { this(source, item, destination, true); }
    public InventoryMoveItemEvent(Inventory source, org.bukkit.inventory.ItemStack item, Inventory destination, boolean didSourceInitiate) {
        this.source = source; this.item = item == null ? null : item.clone(); this.destination = destination; this.didSourceInitiate = didSourceInitiate;
    }
    public Inventory getSource() { return source; }
    public Inventory getDestination() { return destination; }
    public Inventory getInitiator() { return didSourceInitiate ? source : destination; }
    public org.bukkit.inventory.ItemStack getItem() { return item == null ? null : item.clone(); }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean value) { cancelled = value; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
