package org.bukkit.event.entity;

import org.bukkit.entity.LivingEntity;
import org.bukkit.event.Cancellable;
import org.bukkit.event.Event;
import org.bukkit.event.HandlerList;

/** Fired before a living entity regains health. */
public final class EntityRegainHealthEvent extends EntityEvent implements Cancellable {
    public enum RegainReason { REGEN, SATIATED, EATING, ENDER_CRYSTAL, MAGIC, MAGIC_REGEN, WITHER_SPAWN, WITHER, CUSTOM, UNKNOWN }
    private float amount;
    private final RegainReason reason;
    private boolean cancelled;
    private static final HandlerList HANDLERS = new HandlerList();

    public EntityRegainHealthEvent(LivingEntity entity, double amount) {
        this(entity, amount, RegainReason.CUSTOM);
    }

    public EntityRegainHealthEvent(org.bukkit.entity.Entity entity, double amount, RegainReason reason) {
        super(entity);
        this.amount = (float) amount;
        this.reason = reason == null ? RegainReason.CUSTOM : reason;
    }

    public EntityRegainHealthEvent(LivingEntity entity, double amount, RegainReason reason) {
        super(entity);
        this.amount = (float) amount;
        this.reason = reason == null ? RegainReason.CUSTOM : reason;
    }

    public double getAmount() { return amount; }
    public void setAmount(double amount) { this.amount = (float) amount; }
    public RegainReason getRegainReason() { return reason; }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean value) { cancelled = value; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
