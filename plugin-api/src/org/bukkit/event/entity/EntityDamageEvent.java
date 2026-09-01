package org.bukkit.event.entity;

import org.bukkit.entity.Entity;
import org.bukkit.event.Cancellable;
import org.bukkit.event.Event;
import org.bukkit.event.HandlerList;

/** Base event for damage applied to an entity. */
public class EntityDamageEvent extends Event implements Cancellable {
    public enum DamageCause { ENTITY_ATTACK, PROJECTILE, SUFFOCATION, FALL, FIRE, FIRE_TICK, LAVA, DROWNING, BLOCK_EXPLOSION, ENTITY_EXPLOSION, VOID, CUSTOM }
    private final Entity entity;
    private boolean cancelled;
    private final DamageCause cause;
    private static final HandlerList HANDLERS = new HandlerList();
    protected EntityDamageEvent(Entity entity) { this(entity, DamageCause.CUSTOM); }
    protected EntityDamageEvent(Entity entity, DamageCause cause) { this.entity = entity; this.cause = cause == null ? DamageCause.CUSTOM : cause; }
    public Entity getEntity() { return entity; }
    public DamageCause getCause() { return cause; }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean cancelled) { this.cancelled = cancelled; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
