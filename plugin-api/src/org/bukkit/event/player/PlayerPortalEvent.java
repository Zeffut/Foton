package org.bukkit.event.player;

import org.bukkit.Location;
import org.bukkit.entity.Player;

/** Fired before a player travels through a portal. */
public class PlayerPortalEvent extends PlayerTeleportEvent {
    private int searchRadius = 128;
    private boolean canCreatePortal = true;
    public PlayerPortalEvent(Player player, Location from, Location to, TeleportCause cause) {
        super(player, from, to, cause);
    }
    public int getSearchRadius() { return searchRadius; }
    public void setSearchRadius(int radius) { searchRadius = radius; }
    public boolean getCanCreatePortal() { return canCreatePortal; }
    public void setCanCreatePortal(boolean value) { canCreatePortal = value; }
}
