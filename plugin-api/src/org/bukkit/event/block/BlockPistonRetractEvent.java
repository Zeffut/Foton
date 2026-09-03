package org.bukkit.event.block;
import java.util.List;
import org.bukkit.block.Block;
import org.bukkit.block.BlockFace;
import org.bukkit.event.HandlerList;
public class BlockPistonRetractEvent extends PistonEvent {
    private static final HandlerList HANDLERS = new HandlerList();
    public BlockPistonRetractEvent(Block block, BlockFace direction, List<Block> blocks) { super(block, direction, blocks); }
    public boolean isSticky() {
        return getBlock().getType() == org.bukkit.Material.STICKY_PISTON;
    }
    public org.bukkit.Location getRetractLocation() {
        return getBlock().getLocation().add(getDirection().getModX(), getDirection().getModY(), getDirection().getModZ());
    }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
