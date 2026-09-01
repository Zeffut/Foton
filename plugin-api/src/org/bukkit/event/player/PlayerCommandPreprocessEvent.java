package org.bukkit.event.player;

import org.bukkit.entity.Player;
import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;

/** Fired before a player command is dispatched. */
public final class PlayerCommandPreprocessEvent extends PlayerEvent implements Cancellable {
    private String message;
    private boolean cancelled;
    private static final HandlerList HANDLERS = new HandlerList();
    public PlayerCommandPreprocessEvent(Player player, String message) {
        super(player); this.message = message;
    }
    public String getMessage() { return message; }
    public void setMessage(String message) { this.message = message; }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean cancelled) { this.cancelled = cancelled; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
