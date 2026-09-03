package com.destroystokyo.paper.event.entity;

import org.bukkit.Location;
import org.bukkit.entity.EntityType;
import org.bukkit.event.Cancellable;
import org.bukkit.event.Event;
import org.bukkit.event.HandlerList;
import org.bukkit.event.entity.CreatureSpawnEvent;

/** Fired before a creature spawn is inserted into a world. */
public final class PreCreatureSpawnEvent extends Event implements Cancellable {
    private static final HandlerList HANDLER_LIST = new HandlerList();
    private final Location location;
    private final EntityType type;
    private final CreatureSpawnEvent.SpawnReason reason;
    private boolean shouldAbortSpawn;
    private boolean cancelled;

    public PreCreatureSpawnEvent(Location location, EntityType type, CreatureSpawnEvent.SpawnReason reason) {
        this.location = location; this.type = type; this.reason = reason;
    }
    public Location getSpawnLocation() { return location; }
    public EntityType getType() { return type; }
    public CreatureSpawnEvent.SpawnReason getReason() { return reason; }
    public boolean shouldAbortSpawn() { return shouldAbortSpawn; }
    public void setShouldAbortSpawn(boolean value) { shouldAbortSpawn = value; }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean value) { cancelled = value; }
    @Override public HandlerList getHandlers() { return HANDLER_LIST; }
    public static HandlerList getHandlerList() { return HANDLER_LIST; }
}
