package org.bukkit.event.vehicle;
import org.bukkit.entity.Vehicle;
import org.bukkit.event.Event;
public abstract class VehicleEvent extends Event { private final Vehicle vehicle; protected VehicleEvent(Vehicle vehicle){this.vehicle=vehicle;} public Vehicle getVehicle(){return vehicle;} }
