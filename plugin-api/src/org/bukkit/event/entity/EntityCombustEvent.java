package org.bukkit.event.entity;

import org.bukkit.entity.Entity;
import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;

/** Fired when an entity is set on fire. */
public class EntityCombustEvent extends EntityEvent implements Cancellable {
    private int duration;
    private boolean cancelled;
    private static final HandlerList HANDLERS = new HandlerList();
    public EntityCombustEvent(Entity entity, int duration) { super(entity); this.duration = Math.max(0, duration); }
    public int getDuration() { return duration; }
    public void setDuration(int duration) { this.duration = Math.max(0, duration); }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean value) { cancelled = value; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
