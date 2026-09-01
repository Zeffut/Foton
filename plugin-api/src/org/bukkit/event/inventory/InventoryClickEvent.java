package org.bukkit.event.inventory;

import org.bukkit.entity.HumanEntity;
import org.bukkit.event.Cancellable;
import org.bukkit.event.Event;
import org.bukkit.event.HandlerList;

/** Fired before a player container click is applied. */
public final class InventoryClickEvent extends Event implements Cancellable {
    private final HumanEntity whoClicked;
    private final org.bukkit.inventory.ItemStack currentItem;
    private boolean cancelled;
    private static final HandlerList HANDLERS = new HandlerList();

    public InventoryClickEvent(HumanEntity whoClicked) { this(whoClicked, null); }
    public InventoryClickEvent(HumanEntity whoClicked, org.bukkit.inventory.ItemStack currentItem) {
        this.whoClicked = whoClicked;
        this.currentItem = currentItem == null ? null : currentItem.clone();
    }
    public HumanEntity getWhoClicked() { return whoClicked; }
    public org.bukkit.inventory.ItemStack getCurrentItem() {
        return currentItem == null ? null : currentItem.clone();
    }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean cancelled) { this.cancelled = cancelled; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
