package org.bukkit.event.entity;

import org.bukkit.Location;
import org.bukkit.entity.Item;
import org.bukkit.event.Cancellable;
import org.bukkit.event.Event;
import org.bukkit.event.HandlerList;

/** Fired before an item entity is inserted into a world. */
public final class ItemSpawnEvent extends Event implements Cancellable {
    private final Item entity;
    private final Location location;
    private boolean cancelled;
    private static final HandlerList HANDLERS = new HandlerList();

    public ItemSpawnEvent(Item entity, Location location) {
        this.entity = entity;
        this.location = location;
    }
    public Item getEntity() { return entity; }
    public Item getItemEntity() { return entity; }
    public Location getLocation() { return location; }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean value) { cancelled = value; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
