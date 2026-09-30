package org.bukkit.event.entity;

import org.bukkit.entity.Entity;
import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;

/** An entity is getting off what it rides. Cancelling keeps it on, except
 * when the dismount comes from either side being removed. */
public class EntityDismountEvent extends EntityEvent implements Cancellable {
    private static final HandlerList HANDLERS = new HandlerList();
    private final Entity dismounted;
    private final boolean isCancellable;
    private boolean cancelled;

    public EntityDismountEvent(Entity entity, Entity dismounted) { this(entity, dismounted, true); }

    public EntityDismountEvent(Entity entity, Entity dismounted, boolean isCancellable) {
        super(entity);
        this.dismounted = dismounted;
        this.isCancellable = isCancellable;
    }

    /** What was being ridden. */
    public Entity getDismounted() { return dismounted; }
    public boolean isCancellable() { return isCancellable; }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean cancel) {
        if (cancel && !isCancellable) return;
        cancelled = cancel;
    }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
