package org.bukkit.event.entity;

import org.bukkit.entity.Entity;
import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;

/** Fired when a mob changes its selected target. */
public class EntityTargetEvent extends EntityEvent implements Cancellable {
    private Entity target;
    private boolean cancelled;
    private static final HandlerList HANDLERS = new HandlerList();
    public EntityTargetEvent(Entity entity, Entity target) { super(entity); this.target = target; }
    public Entity getTarget() { return target; }
    public void setTarget(Entity target) { this.target = target; }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean value) { cancelled = value; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
