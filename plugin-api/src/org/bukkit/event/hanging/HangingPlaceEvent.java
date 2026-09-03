package org.bukkit.event.hanging;

import org.bukkit.entity.Hanging;
import org.bukkit.entity.Player;
import org.bukkit.block.Block;
import org.bukkit.block.BlockFace;
import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;

/** Fired before a hanging entity is placed. */
public class HangingPlaceEvent extends HangingEvent implements Cancellable {
    private final Player player;
    private final Block block;
    private final BlockFace blockFace;
    private boolean cancelled;
    private static final HandlerList HANDLERS = new HandlerList();
    public HangingPlaceEvent(Hanging entity, Player player, Block block, BlockFace blockFace) {
        super(entity); this.player = player; this.block = block; this.blockFace = blockFace;
    }
    public Player getPlayer() { return player; }
    public Block getBlock() { return block; }
    public BlockFace getBlockFace() { return blockFace; }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean value) { cancelled = value; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
