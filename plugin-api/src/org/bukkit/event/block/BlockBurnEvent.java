package org.bukkit.event.block;

import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;

/** Fired before fire consumes a flammable block. */
public class BlockBurnEvent extends BlockEvent implements Cancellable {
    private boolean cancelled;
    private static final HandlerList HANDLERS = new HandlerList();
    public BlockBurnEvent(org.bukkit.block.Block block) { super(block); }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean value) { cancelled = value; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
