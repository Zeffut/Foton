package org.bukkit.event.entity;

import org.bukkit.entity.Entity;
import org.bukkit.event.Cancellable;
import org.bukkit.event.Event;
import org.bukkit.event.HandlerList;

/** Fired before one entity damages another. */
public final class EntityDamageByEntityEvent extends Event implements Cancellable {
    private final Entity damager;
    private final Entity entity;
    private boolean cancelled;
    private static final HandlerList HANDLERS = new HandlerList();
    public EntityDamageByEntityEvent(Entity damager, Entity entity) {
        this.damager = damager; this.entity = entity;
    }
    public Entity getDamager() { return damager; }
    public Entity getEntity() { return entity; }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean cancelled) { this.cancelled = cancelled; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
