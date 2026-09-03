package org.bukkit.event.player;

import org.bukkit.entity.Item;
import org.bukkit.entity.Player;
import org.bukkit.event.Cancellable;
import org.bukkit.event.Event;
import org.bukkit.event.HandlerList;

/** Fired when a player drops an item. */
public final class PlayerDropItemEvent extends Event implements Cancellable {
    private final Player player;
    private final Item itemDrop;
    private boolean cancelled;
    private static final HandlerList HANDLERS = new HandlerList();
    public PlayerDropItemEvent(Player player, Item itemDrop) { this.player = player; this.itemDrop = itemDrop; }
    public Player getPlayer() { return player; }
    public Item getItemDrop() { return itemDrop; }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean value) { cancelled = value; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
