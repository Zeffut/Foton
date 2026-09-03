package org.bukkit.event.block;

import org.bukkit.block.Block;
import org.bukkit.entity.Player;
import org.bukkit.event.Event;
import org.bukkit.event.HandlerList;

/** Fired when a player stops the progressive damage animation for a block. */
public class BlockDamageAbortEvent extends Event {
    private final Player player;
    private final Block block;
    private static final HandlerList HANDLERS = new HandlerList();
    public BlockDamageAbortEvent(Player player, Block block) { this.player = player; this.block = block; }
    public Player getPlayer() { return player; }
    public Block getBlock() { return block; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
