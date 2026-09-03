package org.bukkit.event.block;

import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;

/** Fired before a block naturally fades away. */
public class BlockFadeEvent extends BlockEvent implements Cancellable {
    private final org.bukkit.block.BlockState newState;
    private boolean cancelled;
    private static final HandlerList HANDLERS = new HandlerList();
    public BlockFadeEvent(org.bukkit.block.Block block) { this(block, null); }
    public BlockFadeEvent(org.bukkit.block.Block block, org.bukkit.block.BlockState newState) { super(block); this.newState = newState; }
    public org.bukkit.block.BlockState getNewState() { return newState; }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean value) { cancelled = value; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
