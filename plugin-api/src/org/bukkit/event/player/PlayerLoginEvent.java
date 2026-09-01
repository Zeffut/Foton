package org.bukkit.event.player;

import org.bukkit.entity.Player;
import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;

public class PlayerLoginEvent extends PlayerEvent implements Cancellable {
    public enum Result { ALLOWED, KICK_FULL, KICK_BANNED, KICK_WHITELIST, KICK_OTHER }
    private Result result = Result.ALLOWED;
    private String kickMessage;
    private static final HandlerList HANDLERS = new HandlerList();

    public PlayerLoginEvent(Player player) { super(player); }
    public Result getResult() { return result; }
    public void setResult(Result result) { this.result = result; }
    public String getKickMessage() { return kickMessage; }
    public void setKickMessage(String message) { this.kickMessage = message; }
    public void allow() { result = Result.ALLOWED; kickMessage = null; }
    public void disallow(Result result, String message) { this.result = result; this.kickMessage = message; }
    @Override public boolean isCancelled() { return result != Result.ALLOWED; }
    @Override public void setCancelled(boolean cancelled) { if (!cancelled) allow(); else disallow(Result.KICK_OTHER, "You have been denied access to this server"); }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
