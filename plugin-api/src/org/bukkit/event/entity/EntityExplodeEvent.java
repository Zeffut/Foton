package org.bukkit.event.entity;

import java.util.ArrayList;
import java.util.List;
import org.bukkit.block.Block;
import org.bukkit.entity.Entity;
import org.bukkit.event.Cancellable;
import org.bukkit.event.Event;
import org.bukkit.event.HandlerList;

/** Fired before an entity explosion destroys its block list. */
public class EntityExplodeEvent extends EntityEvent implements Cancellable {
    private final List<Block> blocks;
    private float yield;
    private final org.bukkit.ExplosionResult explosionResult;
    private boolean cancelled;
    private static final HandlerList HANDLERS = new HandlerList();

    public EntityExplodeEvent(Entity entity, List<Block> blocks, float yield) {
        this(entity, blocks, yield, org.bukkit.ExplosionResult.BLOCK);
    }
    public EntityExplodeEvent(Entity entity, List<Block> blocks, float yield, org.bukkit.ExplosionResult result) {
        super(entity); this.blocks = new ArrayList<>(blocks); this.yield = yield;
        this.explosionResult = result == null ? org.bukkit.ExplosionResult.BLOCK : result;
    }
    public org.bukkit.ExplosionResult getExplosionResult() { return explosionResult; }
    public org.bukkit.Location getLocation() { return getEntity() == null ? null : getEntity().getLocation(); }
    /** Returns the Bukkit entity type of the exploding entity. */
    public org.bukkit.entity.EntityType getEntityType() {
        return getEntity() == null ? null : getEntity().getType();
    }
    public List<Block> blockList() { return blocks; }
    public float getYield() { return yield; }
    public void setYield(float value) { yield = value; }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean value) { cancelled = value; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
