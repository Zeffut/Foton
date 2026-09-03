package org.bukkit.event.inventory;

import org.bukkit.entity.HumanEntity;
import org.bukkit.event.Cancellable;
import org.bukkit.event.Event;
import org.bukkit.event.HandlerList;

/** Fired when a player opens an external inventory view. */
public class InventoryOpenEvent extends InventoryEvent implements Cancellable {
    private final HumanEntity player;
    private boolean cancelled;
    private static final HandlerList HANDLERS = new HandlerList();
    public InventoryOpenEvent(HumanEntity player) {
        super(player instanceof org.bukkit.entity.Player ? ((org.bukkit.entity.Player) player).getOpenInventory() : null);
        this.player = player;
    }
    public HumanEntity getPlayer() { return player; }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean value) { cancelled = value; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
