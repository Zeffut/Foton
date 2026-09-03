package org.bukkit.event.weather;

import org.bukkit.World;
import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;
import org.bukkit.event.world.WorldEvent;

/** Fired when precipitation starts or stops. */
public class WeatherChangeEvent extends WorldEvent implements Cancellable {
    private final boolean raining;
    private boolean cancelled;
    private static final HandlerList HANDLERS = new HandlerList();
    public WeatherChangeEvent(World world, boolean raining) { super(world); this.raining = raining; }
    public boolean toWeatherState() { return raining; }
    public boolean isRaining() { return raining; }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean value) { cancelled = value; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
