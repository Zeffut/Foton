package org.bukkit.event.vehicle;
import org.bukkit.entity.LivingEntity;
import org.bukkit.entity.Vehicle;
import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;
public class VehicleExitEvent extends VehicleEvent implements Cancellable {
 private static final HandlerList HANDLERS=new HandlerList(); private final LivingEntity exited; private boolean cancelled;
 public VehicleExitEvent(Vehicle vehicle, LivingEntity exited){this(vehicle,exited,true);} public VehicleExitEvent(Vehicle vehicle, LivingEntity exited, boolean cancellable){super(vehicle);this.exited=exited;this.cancelled=!cancellable;}
 public LivingEntity getExited(){return exited;} public boolean isCancelled(){return cancelled;} public void setCancelled(boolean cancel){cancelled=cancel;}
 public HandlerList getHandlers(){return HANDLERS;} public static HandlerList getHandlerList(){return HANDLERS;}
}
