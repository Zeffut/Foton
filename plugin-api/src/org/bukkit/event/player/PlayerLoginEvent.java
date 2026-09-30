package org.bukkit.event.player;

import net.kyori.adventure.text.Component;
import org.bukkit.entity.Player;
import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;

/** A player is being admitted; a result other than {@code ALLOWED} turns
 * them away with {@link #kickMessage()}. */
public class PlayerLoginEvent extends PlayerEvent implements Cancellable {
    public enum Result { ALLOWED, KICK_FULL, KICK_BANNED, KICK_WHITELIST, KICK_OTHER }
    private Result result = Result.ALLOWED;
    private Component kickMessage;
    private final java.net.InetAddress address;
    private final String hostname;
    private static final HandlerList HANDLERS = new HandlerList();

    public PlayerLoginEvent(Player player) { this(player, player == null || player.getAddress() == null ? null : player.getAddress().getAddress(), null); }
    public PlayerLoginEvent(Player player, java.net.InetAddress address) { this(player, address, null); }
    public PlayerLoginEvent(Player player, java.net.InetAddress address, String hostname) { super(player); this.address = address; this.hostname = hostname; }
    public java.net.InetAddress getAddress() { return address; }
    public String getHostname() { return hostname; }
    public Result getResult() { return result; }
    public void setResult(Result result) { this.result = result; }
    public Component kickMessage() { return kickMessage; }
    public void kickMessage(Component message) { this.kickMessage = message; }
    @Deprecated public String getKickMessage() { return PlayerJoinEvent.toLegacy(kickMessage); }
    @Deprecated public void setKickMessage(String message) { this.kickMessage = PlayerJoinEvent.fromLegacy(message); }
    public void allow() { result = Result.ALLOWED; kickMessage = Component.empty(); }
    @Deprecated public void disallow(Result result, String message) { this.result = result; this.kickMessage = PlayerJoinEvent.fromLegacy(message); }
    public void disallow(Result result, Component message) { this.result = result; this.kickMessage = message; }
    @Override public boolean isCancelled() { return result != Result.ALLOWED; }
    @Override public void setCancelled(boolean cancelled) { if (!cancelled) allow(); else disallow(Result.KICK_OTHER, "You have been denied access to this server"); }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
