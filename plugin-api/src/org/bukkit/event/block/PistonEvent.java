package org.bukkit.event.block;

import java.util.ArrayList;
import java.util.List;
import org.bukkit.block.Block;
import org.bukkit.block.BlockFace;
import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;
import org.bukkit.event.Event;

/** Common state for piston extension and retraction events. */
public abstract class PistonEvent extends Event implements Cancellable {
    private final Block block;
    private final BlockFace direction;
    private final List<Block> blocks;
    private boolean cancelled;
    protected PistonEvent(Block block, BlockFace direction, List<Block> blocks) {
        this.block = block; this.direction = direction; this.blocks = new ArrayList<>(blocks);
    }
    public Block getBlock() { return block; }
    public BlockFace getDirection() { return direction; }
    public List<Block> getBlocks() { return blocks; }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean value) { cancelled = value; }
}
