package org.bukkit.event.vehicle;
import org.bukkit.entity.Entity;
import org.bukkit.entity.Vehicle;
import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;
public class VehicleEnterEvent extends VehicleEvent implements Cancellable {
 private static final HandlerList HANDLERS=new HandlerList(); private final Entity entered; private boolean cancelled;
 public VehicleEnterEvent(Vehicle vehicle, Entity entered){super(vehicle);this.entered=entered;}
 public Entity getEntered(){return entered;} public boolean isCancelled(){return cancelled;} public void setCancelled(boolean cancel){cancelled=cancel;}
 public HandlerList getHandlers(){return HANDLERS;} public static HandlerList getHandlerList(){return HANDLERS;}
}
