package org.bukkit.event.weather;
import org.bukkit.World;
import org.bukkit.event.Event;
public abstract class WeatherEvent extends Event {
    private final World world;
    protected WeatherEvent(World world) { this.world = world; }
    public World getWorld() { return world; }
}
