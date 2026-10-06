package org.bukkit.event.entity;

import org.bukkit.entity.Entity;
import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;

/** Fired when an entity is set on fire, for a duration in seconds. */
public class EntityCombustEvent extends EntityEvent implements Cancellable {
    private float duration;
    private boolean cancelled;
    private static final HandlerList HANDLERS = new HandlerList();
    public EntityCombustEvent(Entity entity, int duration) { this(entity, (float) duration); }
    public EntityCombustEvent(Entity entity, float duration) { super(entity); this.duration = duration; }
    public float getDuration() { return duration; }
    public void setDuration(float duration) { this.duration = duration; }
    public void setDuration(int duration) { this.duration = duration; }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean value) { cancelled = value; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
