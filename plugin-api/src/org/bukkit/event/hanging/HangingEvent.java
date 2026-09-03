package org.bukkit.event.hanging;

import org.bukkit.entity.Hanging;
import org.bukkit.event.Event;

/** Base event for an entity attached to a block face. */
public abstract class HangingEvent extends Event {
    private final Hanging entity;
    protected HangingEvent(Hanging entity) { this.entity = entity; }
    public Hanging getEntity() { return entity; }
}
