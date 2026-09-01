package org.bukkit.event.player;

import org.bukkit.Location;
import org.bukkit.entity.Player;
import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;

/** Fired before a player teleport requested by a plugin is applied. */
public final class PlayerTeleportEvent extends PlayerEvent implements Cancellable {
    public enum TeleportCause { UNKNOWN, COMMAND, PLUGIN, NETHER_PORTAL, END_PORTAL, SPECTATE, CHORUS_FRUIT, ENDER_PEARL, EXIT_END_PORTAL, DISMOUNT, END_GATEWAY, SHULKER }
    private final Location from; private Location to; private final TeleportCause cause; private boolean cancelled;
    private static final HandlerList HANDLERS = new HandlerList();
    public PlayerTeleportEvent(Player player, Location from, Location to, TeleportCause cause) { super(player); this.from = from; this.to = to; this.cause = cause; }
    public Location getFrom() { return from; } public Location getTo() { return to; } public void setTo(Location to) { this.to = to; } public TeleportCause getCause() { return cause; }
    @Override public boolean isCancelled() { return cancelled; } @Override public void setCancelled(boolean cancelled) { this.cancelled = cancelled; }
    @Override public HandlerList getHandlers() { return HANDLERS; } public static HandlerList getHandlerList() { return HANDLERS; }
}
