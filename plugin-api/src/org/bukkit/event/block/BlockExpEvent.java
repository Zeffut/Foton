package org.bukkit.event.block;

import org.bukkit.block.Block;
import org.bukkit.event.HandlerList;

/** Fired when a block is about to drop experience. */
public class BlockExpEvent extends BlockEvent {
    private int expToDrop;
    private boolean cancelled;
    private static final HandlerList HANDLERS = new HandlerList();
    public BlockExpEvent(Block block, int expToDrop) { super(block); this.expToDrop = Math.max(0, expToDrop); }
    public int getExpToDrop() { return expToDrop; }
    public void setExpToDrop(int value) { expToDrop = Math.max(0, value); }
    public boolean isCancelled() { return cancelled; }
    public void setCancelled(boolean value) { cancelled = value; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
