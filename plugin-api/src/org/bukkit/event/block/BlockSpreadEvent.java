package org.bukkit.event.block;

import org.bukkit.block.Block;
import org.bukkit.block.BlockState;
import org.bukkit.event.HandlerList;

/** Fired when a block spreads into a neighboring position. */
public class BlockSpreadEvent extends BlockFormEvent {
    private final Block source;
    private final BlockState newState;
    private static final HandlerList HANDLERS = new HandlerList();
    public BlockSpreadEvent(Block block, Block source, BlockState newState) {
        super(block); this.source = source; this.newState = newState;
    }
    public Block getSource() { return source; }
    public BlockState getNewState() { return newState; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
