package org.bukkit.event.player;

import java.net.InetAddress;
import java.util.UUID;
import org.bukkit.event.Event;
import org.bukkit.event.HandlerList;

/** Admission event fired before a Player object exists. */
public class AsyncPlayerPreLoginEvent extends Event {
    public enum Result { ALLOWED, KICK_FULL, KICK_BANNED, KICK_WHITELIST, KICK_OTHER }
    private final String name; private final UUID uuid; private final InetAddress address;
    private Result result = Result.ALLOWED; private String message;
    private static final HandlerList HANDLERS = new HandlerList();
    public AsyncPlayerPreLoginEvent(String name, UUID uuid, InetAddress address) { this.name = name; this.uuid = uuid; this.address = address; }
    public String getName() { return name; }
    public UUID getUniqueId() { return uuid; }
    public InetAddress getAddress() { return address; }
    /** Profile available before a Player instance is created. */
    public com.destroystokyo.paper.profile.PlayerProfile getPlayerProfile() {
        return new foton.FotonPlayerProfile(uuid, name);
    }
    public Result getLoginResult() { return result; }
    public void setLoginResult(Result result) { this.result = result == null ? Result.KICK_OTHER : result; }
    public String getKickMessage() { return message; }
    public void setKickMessage(String message) { this.message = message; }
    public void disallow(Result result, String message) { this.result = result; this.message = message; }
    public void allow() { result = Result.ALLOWED; message = null; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
