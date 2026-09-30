package org.bukkit.event.player;

import net.kyori.adventure.text.Component;
import org.bukkit.entity.Player;
import org.bukkit.event.HandlerList;

/** A player left; what is announced is {@link #quitMessage()}, and null
 * announces nothing. */
public class PlayerQuitEvent extends PlayerEvent {
    private static final HandlerList HANDLERS = new HandlerList();
    private Component quitMessage;

    public PlayerQuitEvent(Player player, Component quitMessage) {
        super(player);
        this.quitMessage = quitMessage;
    }

    @Deprecated
    public PlayerQuitEvent(Player player, String quitMessage) {
        this(player, PlayerJoinEvent.fromLegacy(quitMessage));
    }

    public Component quitMessage() { return quitMessage; }
    public void quitMessage(Component quitMessage) { this.quitMessage = quitMessage; }
    @Deprecated public String getQuitMessage() { return PlayerJoinEvent.toLegacy(quitMessage); }
    @Deprecated public void setQuitMessage(String quitMessage) { this.quitMessage = PlayerJoinEvent.fromLegacy(quitMessage); }

    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
