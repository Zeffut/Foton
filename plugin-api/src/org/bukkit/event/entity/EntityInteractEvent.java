package org.bukkit.event.entity;

import java.util.Objects;
import org.bukkit.block.Block;
import org.bukkit.entity.Entity;
import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;

/** Fired when an entity interacts with a block. */
public class EntityInteractEvent extends EntityEvent implements Cancellable {
    private static final HandlerList HANDLERS = new HandlerList();
    private final Block block;
    private boolean cancelled;

    public EntityInteractEvent(Entity entity, Block block) {
        super(Objects.requireNonNull(entity, "entity"));
        this.block = Objects.requireNonNull(block, "block");
    }
    public Block getBlock() { return block; }
    public org.bukkit.entity.EntityType getEntityType() { return getEntity() == null ? null : getEntity().getType(); }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean cancelled) { this.cancelled = cancelled; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
