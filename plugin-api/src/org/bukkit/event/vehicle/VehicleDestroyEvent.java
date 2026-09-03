package org.bukkit.event.vehicle;
import org.bukkit.damage.DamageSource;
import org.bukkit.entity.Entity;
import org.bukkit.entity.Vehicle;
import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;
public class VehicleDestroyEvent extends VehicleEvent implements Cancellable {
 private static final HandlerList HANDLERS=new HandlerList(); private final DamageSource damageSource; private final Entity attacker; private boolean cancelled;
 public VehicleDestroyEvent(Vehicle vehicle, DamageSource source, Entity attacker){super(vehicle);this.damageSource=source;this.attacker=attacker;}
 public DamageSource getDamageSource(){return damageSource;} public Entity getAttacker(){return attacker;} public boolean isCancelled(){return cancelled;} public void setCancelled(boolean cancel){cancelled=cancel;}
 public HandlerList getHandlers(){return HANDLERS;} public static HandlerList getHandlerList(){return HANDLERS;}
}
