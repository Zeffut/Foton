package org.bukkit.event.player;

import org.bukkit.entity.Player;
import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;
import org.bukkit.util.Vector;

/** The server is about to send a player a velocity of its own making. The
 * velocity may be changed; cancelling sends none. */
public class PlayerVelocityEvent extends PlayerEvent implements Cancellable {
    private static final HandlerList HANDLERS = new HandlerList();
    private Vector velocity;
    private boolean cancelled;

    public PlayerVelocityEvent(Player player, Vector velocity) {
        super(player);
        this.velocity = velocity;
    }

    public Vector getVelocity() { return velocity; }
    public void setVelocity(Vector velocity) { this.velocity = velocity.clone(); }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean cancel) { cancelled = cancel; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
