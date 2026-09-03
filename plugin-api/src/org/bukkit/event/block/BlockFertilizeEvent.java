package org.bukkit.event.block;
import org.bukkit.entity.Player;
import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;
/** Fired before bonemeal grows a block. */
public final class BlockFertilizeEvent extends BlockEvent implements Cancellable {
    private final Player player; private final java.util.List<org.bukkit.block.BlockState> blocks; private boolean cancelled; private static final HandlerList HANDLERS = new HandlerList();
    public BlockFertilizeEvent(org.bukkit.block.Block block, Player player) { this(block, player, block == null ? java.util.Collections.emptyList() : java.util.Collections.singletonList(block.getState())); }
    public BlockFertilizeEvent(org.bukkit.block.Block block, Player player, java.util.List<org.bukkit.block.BlockState> blocks) { super(block); this.player = player; this.blocks = blocks == null ? java.util.Collections.emptyList() : blocks; }
    public Player getPlayer() { return player; }
    public java.util.List<org.bukkit.block.BlockState> getBlocks() { return blocks; }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean value) { cancelled = value; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
