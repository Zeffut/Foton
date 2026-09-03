package io.papermc.paper.event.player;

import net.kyori.adventure.text.Component;
import org.bukkit.entity.Player;
import org.bukkit.event.Cancellable;
import org.bukkit.event.Event;
import org.bukkit.event.HandlerList;

/** Paper chat event view backed by the same serialized chat dispatch as Bukkit. */
public class AsyncChatEvent extends Event implements Cancellable {
    private final Player player;
    private Component message;
    private io.papermc.paper.chat.ChatRenderer renderer;
    private final java.util.Set<net.kyori.adventure.audience.Audience> viewers =
        new java.util.LinkedHashSet<>();
    private boolean cancelled;
    private static final HandlerList HANDLERS = new HandlerList();

    public AsyncChatEvent(Player player, Component message) {
        this.player = player;
        this.message = message;
    }
    public Player getPlayer() { return player; }
    public Component message() { return message; }
    public void message(Component value) { message = value; }
    public io.papermc.paper.chat.ChatRenderer renderer() { return renderer; }
    public void renderer(io.papermc.paper.chat.ChatRenderer value) { renderer = value; }
    /** Mutable recipients set, matching Paper's per-message viewer contract. */
    public java.util.Set<net.kyori.adventure.audience.Audience> viewers() { return viewers; }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean value) { cancelled = value; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
