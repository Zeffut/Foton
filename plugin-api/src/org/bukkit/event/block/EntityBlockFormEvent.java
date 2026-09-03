package org.bukkit.event.block;

import org.bukkit.block.Block;
import org.bukkit.block.BlockState;
import org.bukkit.entity.Entity;
import org.bukkit.event.HandlerList;

/** Fired when an entity causes a block to form. */
public class EntityBlockFormEvent extends BlockFormEvent {
    private final Entity entity;
    private final BlockState newState;
    private static final HandlerList HANDLERS = new HandlerList();
    public EntityBlockFormEvent(Block block, Entity entity, BlockState newState) {
        super(block); this.entity = entity; this.newState = newState;
    }
    public Entity getEntity() { return entity; }
    public BlockState getNewState() { return newState; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
