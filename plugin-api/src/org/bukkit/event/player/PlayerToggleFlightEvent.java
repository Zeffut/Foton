package org.bukkit.event.player;

import org.bukkit.entity.Player;
import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;

/** A player allowed to fly is starting or stopping. Cancelling keeps them as
 * they were, and their client is told so. */
public class PlayerToggleFlightEvent extends PlayerEvent implements Cancellable {
    private static final HandlerList HANDLERS = new HandlerList();
    private final boolean isFlying;
    private boolean cancelled;

    public PlayerToggleFlightEvent(Player player, boolean isFlying) {
        super(player);
        this.isFlying = isFlying;
    }

    /** Whether the player is starting to fly. */
    public boolean isFlying() { return isFlying; }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean cancel) { cancelled = cancel; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
