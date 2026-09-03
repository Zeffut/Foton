package org.bukkit.event.player;

import org.bukkit.entity.Item;
import org.bukkit.entity.Player;
import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;

/** Legacy player-specific pickup event retained for plugin compatibility. */
@Deprecated
public class PlayerPickupItemEvent extends PlayerEvent implements Cancellable {
    private final Item item;
    private boolean cancelled;
    private static final HandlerList HANDLERS = new HandlerList();
    public PlayerPickupItemEvent(Player player, Item item) { super(player); this.item = item; }
    public Item getItem() { return item; }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean value) { cancelled = value; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
