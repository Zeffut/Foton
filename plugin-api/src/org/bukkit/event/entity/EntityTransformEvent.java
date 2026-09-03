package org.bukkit.event.entity;

import org.bukkit.entity.Entity;
import org.bukkit.event.Cancellable;
import org.bukkit.event.Event;
import org.bukkit.event.HandlerList;

/** Fired before an entity conversion is inserted into the world. */
public class EntityTransformEvent extends EntityEvent implements Cancellable {
    public enum TransformReason { CURED, DROWNED, FROZEN, INFECTION, LIGHTNING, PIGLIN_ZOMBIFICATION, POISON, SPLIT, UNKNOWN }
    private final Entity transformed;
    private final TransformReason reason;
    private boolean cancelled;
    private static final HandlerList HANDLERS = new HandlerList();
    public EntityTransformEvent(Entity entity, Entity transformed) { this(entity, transformed, TransformReason.UNKNOWN); }
    public EntityTransformEvent(Entity entity, Entity transformed, TransformReason reason) { super(entity); this.transformed = transformed; this.reason = reason == null ? TransformReason.UNKNOWN : reason; }
    public Entity getTransformedEntity() { return transformed; }
    public TransformReason getTransformReason() { return reason; }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean value) { cancelled = value; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
