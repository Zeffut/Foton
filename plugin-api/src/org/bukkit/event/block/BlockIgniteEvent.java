package org.bukkit.event.block;

import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;
import org.bukkit.entity.Player;
import org.bukkit.block.Block;

/** Fired before a block is ignited. */
public class BlockIgniteEvent extends BlockEvent implements Cancellable {
    public enum IgniteCause { FLINT_AND_STEEL, FIREBALL, LAVA, LIGHTNING, SPREAD, ENDER_CRYSTAL, VOID, OTHER }
    private boolean cancelled;
    private static final HandlerList HANDLERS = new HandlerList();
    public BlockIgniteEvent(org.bukkit.block.Block block) { this(block, IgniteCause.FLINT_AND_STEEL, null); }
    public BlockIgniteEvent(org.bukkit.block.Block block, IgniteCause cause) { this(block, cause, null); }
    public BlockIgniteEvent(org.bukkit.block.Block block, IgniteCause cause, Player player) {
        this(block, cause, player, null);
    }
    public BlockIgniteEvent(org.bukkit.block.Block block, IgniteCause cause, Player player, Block ignitingBlock) {
        super(block);
        this.cause = cause == null ? IgniteCause.OTHER : cause;
        this.player = player;
        this.ignitingBlock = ignitingBlock;
    }
    private final IgniteCause cause;
    private final Player player;
    private final Block ignitingBlock;
    public IgniteCause getCause() { return cause; }
    public Player getPlayer() { return player; }
    public org.bukkit.entity.Entity getIgnitingEntity() { return player; }
    public Block getIgnitingBlock() { return ignitingBlock; }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean value) { cancelled = value; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
