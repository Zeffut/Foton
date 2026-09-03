package org.bukkit.event.block;

import java.util.ArrayList;
import java.util.List;
import org.bukkit.block.Block;
import org.bukkit.event.Cancellable;
import org.bukkit.event.Event;
import org.bukkit.event.HandlerList;

/** Fired before a block-originated explosion destroys its block list. */
public class BlockExplodeEvent extends BlockEvent implements Cancellable {
    private final List<Block> blocks;
    private float yield;
    private boolean cancelled;
    private final org.bukkit.ExplosionResult explosionResult;
    private static final HandlerList HANDLERS = new HandlerList();
    public BlockExplodeEvent(Block block, List<Block> blocks, float yield) {
        this(block, blocks, yield, org.bukkit.ExplosionResult.BLOCK);
    }
    public BlockExplodeEvent(Block block, List<Block> blocks, float yield, org.bukkit.ExplosionResult result) {
        super(block); this.blocks = new ArrayList<>(blocks); this.yield = yield;
        this.explosionResult = result == null ? org.bukkit.ExplosionResult.BLOCK : result;
    }
    public org.bukkit.block.BlockState getExplodedBlockState() { return block == null ? null : block.getState(); }
    public List<Block> blockList() { return blocks; }
    public float getYield() { return yield; }
    public void setYield(float value) { yield = value; }
    public org.bukkit.ExplosionResult getExplosionResult() { return explosionResult; }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean value) { cancelled = value; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
