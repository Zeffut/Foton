package org.bukkit.event.player;

import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;

/** Fired before a plugin-requested player kick is applied. */
public class PlayerKickEvent extends PlayerEvent implements Cancellable {
    public enum Cause { UNKNOWN, BANNED, KICK_COMMAND, TIMEOUT, EXPIRED, ILLEGAL_ACTION, INVALID_VEHICLE_MOVEMENT, FLYING_PLAYER, SELF, PLUGIN }
    private String reason;
    private String leaveMessage;
    private boolean cancelled;
    private static final HandlerList HANDLERS = new HandlerList();

    public PlayerKickEvent(org.bukkit.entity.Player player, String reason) {
        super(player); this.reason = reason; this.leaveMessage = reason;
    }
    public String getReason() { return reason; }
    public void setReason(String reason) { this.reason = reason; }
    public String getLeaveMessage() { return leaveMessage; }
    public void setLeaveMessage(String message) { leaveMessage = message; }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean value) { cancelled = value; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
