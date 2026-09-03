package org.bukkit.event.player;

import org.bukkit.Location;
import org.bukkit.entity.Player;
import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;

/** Fired after a player movement is accepted by the server's collision checks. */
public class PlayerMoveEvent extends PlayerEvent implements Cancellable {
    private final Location from;
    private Location to;
    private boolean cancelled;
    private static final HandlerList HANDLERS = new HandlerList();

    public PlayerMoveEvent(Player player, Location from, Location to) {
        super(player);
        this.from = from;
        this.to = to;
    }

    public Location getFrom() { return from; }
    public Location getTo() { return to; }
    public void setTo(Location to) { this.to = to; }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean cancelled) { this.cancelled = cancelled; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
