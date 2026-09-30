package org.bukkit.event.block;

import java.util.List;
import org.bukkit.block.Block;
import org.bukkit.block.BlockState;
import org.bukkit.entity.Item;
import org.bukkit.entity.Player;
import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;

/** A block a player broke has dropped its items. Removing an item from the
 * list, or cancelling, takes it back; the items themselves may be changed. */
public class BlockDropItemEvent extends BlockEvent implements Cancellable {
    private static final HandlerList HANDLERS = new HandlerList();
    private final Player player;
    private final BlockState blockState;
    private final List<Item> items;
    private boolean cancelled;

    public BlockDropItemEvent(Block block, BlockState blockState, Player player, List<Item> items) {
        super(block);
        this.blockState = blockState;
        this.player = player;
        this.items = items;
    }

    public Player getPlayer() { return player; }
    /** The block as it stood before it broke. */
    public BlockState getBlockState() { return blockState; }
    public List<Item> getItems() { return items; }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean cancel) { cancelled = cancel; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
