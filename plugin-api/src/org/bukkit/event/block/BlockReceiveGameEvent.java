package org.bukkit.event.block;

import org.bukkit.block.Block;
import org.bukkit.entity.Entity;
import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;

/** Fired when a block receives a vibration/game event from an entity. */
public class BlockReceiveGameEvent extends BlockEvent implements Cancellable {
    private final Entity entity;
    private boolean cancelled;
    private static final HandlerList HANDLERS = new HandlerList();

    public BlockReceiveGameEvent(Block block, Entity entity) {
        super(block);
        this.entity = entity;
    }
    public Entity getEntity() { return entity; }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean value) { cancelled = value; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
