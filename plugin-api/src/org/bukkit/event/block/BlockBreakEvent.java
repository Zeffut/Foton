package org.bukkit.event.block;

import org.bukkit.block.Block;
import org.bukkit.entity.Player;
import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;

/** A player is about to break a block. The experience it drops and whether
 * its items drop may both be changed; cancelling leaves the block standing. */
public class BlockBreakEvent extends BlockExpEvent implements Cancellable {
    private static final HandlerList HANDLERS = new HandlerList();
    private final Player player;
    private boolean dropItems = true;
    private boolean cancelled;

    public BlockBreakEvent(Block block, Player player) {
        super(block, 0);
        this.player = player;
    }

    public Player getPlayer() { return player; }

    /** Whether the block's normal drops are spawned. */
    public boolean isDropItems() { return dropItems; }
    public void setDropItems(boolean value) { this.dropItems = value; }

    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean value) { this.cancelled = value; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
