package org.bukkit.event.entity;

import org.bukkit.Location;
import org.bukkit.PortalType;
import org.bukkit.entity.Entity;
import org.bukkit.event.HandlerList;

/** Fired before an entity travels through a portal. */
public class EntityPortalEvent extends EntityTeleportEvent {
    private static final HandlerList HANDLER_LIST = new HandlerList();
    private final PortalType type; private int searchRadius; private boolean canCreatePortal; private int creationRadius;
    public EntityPortalEvent(Entity entity, Location from, Location to) { this(entity,from,to,128,true,16,null); }
    public EntityPortalEvent(Entity entity, Location from, Location to, int searchRadius) { this(entity,from,to,searchRadius,true,16,null); }
    public EntityPortalEvent(Entity entity, Location from, Location to, int searchRadius, boolean canCreatePortal, int creationRadius) { this(entity,from,to,searchRadius,canCreatePortal,creationRadius,null); }
    public EntityPortalEvent(Entity entity, Location from, Location to, int searchRadius, boolean canCreatePortal, int creationRadius, PortalType type) { super(entity,from,to); this.searchRadius=searchRadius; this.canCreatePortal=canCreatePortal; this.creationRadius=creationRadius; this.type=type; }
    @Override public Location getTo() { return to; }
    @Override public void setTo(Location to) { this.to=to; }
    public PortalType getPortalType() { return type; }
    public void setSearchRadius(int value) { searchRadius=value; } public int getSearchRadius() { return searchRadius; }
    public boolean getCanCreatePortal() { return canCreatePortal; } public void setCanCreatePortal(boolean value) { canCreatePortal=value; }
    public void setCreationRadius(int value) { creationRadius=value; } public int getCreationRadius() { return creationRadius; }
    @Override public HandlerList getHandlers() { return HANDLER_LIST; }
    public static HandlerList getHandlerList() { return HANDLER_LIST; }
}
