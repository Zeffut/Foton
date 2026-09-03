package org.bukkit.event.block;
import java.util.List;
import org.bukkit.block.Block;
import org.bukkit.block.BlockFace;
import org.bukkit.event.HandlerList;
public class BlockPistonExtendEvent extends PistonEvent {
    private static final HandlerList HANDLERS = new HandlerList();
    public BlockPistonExtendEvent(Block block, BlockFace direction, List<Block> blocks) { super(block, direction, blocks); }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
