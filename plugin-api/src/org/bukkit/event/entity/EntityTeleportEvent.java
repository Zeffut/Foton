package org.bukkit.event.entity;

import org.bukkit.Location;
import org.bukkit.entity.Entity;
import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;

/** Fired when an entity is teleported. */
public class EntityTeleportEvent extends EntityEvent implements Cancellable {
    protected Location from; protected Location to; private boolean cancelled;
    private static final HandlerList HANDLERS = new HandlerList();
    public EntityTeleportEvent(Entity entity, Location from, Location to) { super(entity); this.from=from; this.to=to; }
    public Location getFrom() { return from; }
    public void setFrom(Location from) { this.from=from; }
    public Location getTo() { return to; }
    public void setTo(Location to) { this.to=to; }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean value) { cancelled=value; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
