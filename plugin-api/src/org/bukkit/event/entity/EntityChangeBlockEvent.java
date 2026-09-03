package org.bukkit.event.entity;

import org.bukkit.Material;
import org.bukkit.block.Block;
import org.bukkit.entity.Entity;
import org.bukkit.event.Cancellable;
import org.bukkit.event.Event;
import org.bukkit.event.HandlerList;

/** Fired before an entity changes a block. */
public class EntityChangeBlockEvent extends EntityEvent implements Cancellable {
    private final Block block;
    private final Material to;
    private boolean cancelled;
    private static final HandlerList HANDLERS = new HandlerList();
    public EntityChangeBlockEvent(Entity entity, Block block, Material to) {
        super(entity); this.block = block; this.to = to;
    }
    public Block getBlock() { return block; }
    public Material getTo() { return to; }
    public org.bukkit.block.data.BlockData getBlockData() {
        return to == null ? null : new org.bukkit.block.data.SimpleBlockData("minecraft:" + to.getKeyName());
    }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean value) { cancelled = value; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
