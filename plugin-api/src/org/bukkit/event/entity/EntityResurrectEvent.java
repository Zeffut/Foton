package org.bukkit.event.entity;
import org.bukkit.entity.LivingEntity;
import org.bukkit.event.Event;
import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;

/** Fired when a living entity is saved from death by death protection. */
public final class EntityResurrectEvent extends Event implements Cancellable {
    private final LivingEntity entity; private boolean cancelled; private static final HandlerList HANDLERS = new HandlerList();
    public EntityResurrectEvent(LivingEntity entity) { this.entity = entity; }
    public LivingEntity getEntity() { return entity; }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean cancelled) { this.cancelled = cancelled; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
