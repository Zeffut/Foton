package org.bukkit.event.entity;

import org.bukkit.entity.Entity;
import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;

/** Base event for entities entering a world. */
public class EntitySpawnEvent extends EntityEvent implements Cancellable {
    private boolean cancelled;
    private static final HandlerList HANDLERS = new HandlerList();
    public EntitySpawnEvent(Entity entity) { super(entity); }
    public org.bukkit.entity.EntityType getEntityType() { return getEntity() == null ? null : getEntity().getType(); }
    public org.bukkit.Location getLocation() { return getEntity() == null ? null : getEntity().getLocation(); }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean value) { cancelled = value; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
