package org.bukkit.event.entity;

import org.bukkit.entity.Entity;
import org.bukkit.event.Event;

/** Common base for events associated with one entity. */
public abstract class EntityEvent extends Event {
    private final Entity entity;
    protected EntityEvent(Entity entity) { this.entity = entity; }
    public Entity getEntity() { return entity; }
}
