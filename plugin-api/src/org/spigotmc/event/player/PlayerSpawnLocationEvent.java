package org.spigotmc.event.player;

import org.bukkit.Location;
import org.bukkit.entity.Player;
import org.bukkit.event.Event;
import org.bukkit.event.HandlerList;

/** Spigot's initial spawn location event. */
public final class PlayerSpawnLocationEvent extends Event {
    private final Player player;
    private Location spawnLocation;
    private static final HandlerList HANDLERS = new HandlerList();
    public PlayerSpawnLocationEvent(Player player, Location spawnLocation) {
        this.player = player; this.spawnLocation = spawnLocation;
    }
    public Player getPlayer() { return player; }
    public Location getSpawnLocation() { return spawnLocation; }
    public void setSpawnLocation(Location location) { spawnLocation = location; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
