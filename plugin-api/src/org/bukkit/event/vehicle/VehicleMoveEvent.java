package org.bukkit.event.vehicle;
import org.bukkit.Location;
import org.bukkit.entity.Vehicle;
import org.bukkit.event.HandlerList;
public class VehicleMoveEvent extends VehicleEvent {
 private static final HandlerList HANDLERS = new HandlerList();
 private final Location from,to;
 public VehicleMoveEvent(Vehicle vehicle, Location from, Location to){super(vehicle);this.from=from;this.to=to;}
 public Location getFrom(){return from;} public Location getTo(){return to;}
 public HandlerList getHandlers(){return HANDLERS;} public static HandlerList getHandlerList(){return HANDLERS;}
}
