package com.destroystokyo.paper.event.entity;

import org.bukkit.entity.Entity;

/** Paper-compatible alias for the entity removal event. */
public final class EntityRemoveFromWorldEvent extends org.bukkit.event.entity.EntityRemoveFromWorldEvent {
    public EntityRemoveFromWorldEvent(Entity entity) { super(entity); }
}
