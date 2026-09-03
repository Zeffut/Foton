package org.bukkit.event.entity;

import org.bukkit.entity.Entity;
import org.bukkit.event.HandlerList;

/** Fired when an entity is detached while its chunk unloads. */
public class EntityRemoveFromWorldEvent extends EntityEvent {
    private static final HandlerList HANDLERS = new HandlerList();
    public EntityRemoveFromWorldEvent(Entity entity) { super(entity); }
    @Override public HandlerList getHandlers() { return HANDLERS; } public static HandlerList getHandlerList() { return HANDLERS; }
}
