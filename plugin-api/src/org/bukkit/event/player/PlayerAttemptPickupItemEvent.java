package org.bukkit.event.player;

import org.bukkit.entity.Item;
import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;

/** Fired before a player attempts to pick up an item. */
public final class PlayerAttemptPickupItemEvent extends PlayerEvent implements Cancellable {
    private final Item item;
    private boolean cancelled;
    private static final HandlerList HANDLERS = new HandlerList();

    public PlayerAttemptPickupItemEvent(org.bukkit.entity.Player player, Item item) {
        super(player);
        this.item = item;
    }

    public Item getItem() { return item; }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean value) { cancelled = value; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
