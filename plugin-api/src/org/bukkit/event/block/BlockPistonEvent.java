package org.bukkit.event.block;

import org.bukkit.block.Block;
import org.bukkit.block.BlockFace;
import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;

public class BlockPistonEvent extends BlockEvent implements Cancellable {
    private final BlockFace direction; private boolean cancelled;
    private static final HandlerList HANDLERS = new HandlerList();
    public BlockPistonEvent(Block block, BlockFace direction) { super(block); this.direction = direction; }
    public BlockFace getDirection() { return direction; }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean value) { cancelled = value; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
