package org.bukkit.event.inventory;

import org.bukkit.entity.HumanEntity;
import org.bukkit.event.Event;
import org.bukkit.event.HandlerList;

/** Fired after a player closes an external inventory view. */
public class InventoryCloseEvent extends InventoryEvent {
    private final HumanEntity player;
    private static final HandlerList HANDLERS = new HandlerList();

    public InventoryCloseEvent(HumanEntity player) {
        super(player instanceof org.bukkit.entity.Player ? ((org.bukkit.entity.Player) player).getOpenInventory() : null);
        this.player = player;
    }
    public HumanEntity getPlayer() { return player; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
