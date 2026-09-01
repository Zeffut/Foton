package org.bukkit.event.entity;

import org.bukkit.entity.Entity;
import org.bukkit.event.HandlerList;

/** Fired when an entity is detached while its chunk unloads. */
public final class EntityRemoveFromWorldEvent extends org.bukkit.event.Event {
    private final Entity entity; private static final HandlerList HANDLERS = new HandlerList();
    public EntityRemoveFromWorldEvent(Entity entity) { this.entity = entity; }
    public Entity getEntity() { return entity; }
    @Override public HandlerList getHandlers() { return HANDLERS; } public static HandlerList getHandlerList() { return HANDLERS; }
}
