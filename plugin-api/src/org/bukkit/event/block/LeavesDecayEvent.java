package org.bukkit.event.block;

import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;

/** Fired before leaves naturally decay. */
public class LeavesDecayEvent extends BlockEvent implements Cancellable {
    private boolean cancelled;
    private static final HandlerList HANDLERS = new HandlerList();
    public LeavesDecayEvent(org.bukkit.block.Block block) { super(block); }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean value) { cancelled = value; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
