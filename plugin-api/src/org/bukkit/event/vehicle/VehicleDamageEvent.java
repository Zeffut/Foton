package org.bukkit.event.vehicle;

import org.bukkit.damage.DamageSource;
import org.bukkit.entity.Entity;
import org.bukkit.entity.Vehicle;
import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;

/** Raised when a vehicle receives damage. */
public class VehicleDamageEvent extends VehicleEvent implements Cancellable {
    private static final HandlerList HANDLER_LIST = new HandlerList();
    private final DamageSource damageSource;
    private final Entity attacker;
    private double damage;
    private boolean cancelled;

    public VehicleDamageEvent(Vehicle vehicle, DamageSource damageSource, Entity attacker, double damage) {
        super(vehicle);
        this.damageSource = damageSource;
        this.attacker = attacker;
        this.damage = damage;
    }
    public DamageSource getDamageSource() { return damageSource; }
    public Entity getAttacker() { return attacker; }
    public double getDamage() { return damage; }
    public void setDamage(double damage) { this.damage = damage; }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean cancel) { cancelled = cancel; }
    @Override public HandlerList getHandlers() { return HANDLER_LIST; }
    public static HandlerList getHandlerList() { return HANDLER_LIST; }
}
