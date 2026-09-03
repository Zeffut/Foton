package org.bukkit.event.block;

import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;

/** Base event for a block forming naturally. */
public class BlockFormEvent extends BlockEvent implements Cancellable {
    private boolean cancelled;
    private final org.bukkit.block.BlockState newState;
    private static final HandlerList HANDLERS = new HandlerList();
    public BlockFormEvent(org.bukkit.block.Block block) { this(block, block == null ? null : block.getState()); }
    public BlockFormEvent(org.bukkit.block.Block block, org.bukkit.block.BlockState newState) { super(block); this.newState = newState; }
    public org.bukkit.block.BlockState getNewState() { return newState; }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean value) { cancelled = value; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
