package org.bukkit.event.entity;

import org.bukkit.entity.LivingEntity;
import org.bukkit.event.Cancellable;
import org.bukkit.event.Event;
import org.bukkit.event.HandlerList;

/** Fired before a living entity regains health. */
public final class EntityRegainHealthEvent extends Event implements Cancellable {
    private final LivingEntity entity;
    private final float amount;
    private boolean cancelled;
    private static final HandlerList HANDLERS = new HandlerList();

    public EntityRegainHealthEvent(LivingEntity entity, double amount) {
        this.entity = entity;
        this.amount = (float) amount;
    }

    public LivingEntity getEntity() { return entity; }
    public double getAmount() { return amount; }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean value) { cancelled = value; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
