package org.bukkit.event.block;

import org.bukkit.block.Block;
import org.bukkit.block.BlockState;
import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;

/** Called when the moisture level of a soil block changes. */
public class MoistureChangeEvent extends BlockEvent implements Cancellable {
    private static final HandlerList HANDLERS = new HandlerList();
    private final BlockState newState;
    private boolean cancelled;
    public MoistureChangeEvent(Block block, BlockState newState) { super(block); this.newState = newState; }
    public BlockState getNewState() { return newState; }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean cancel) { cancelled = cancel; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
