package io.papermc.paper.event.connection.configuration;

import io.papermc.paper.connection.PlayerConfigurationConnection;
import org.bukkit.event.Event;
import org.bukkit.event.HandlerList;

public class AsyncPlayerConnectionConfigureEvent extends Event {
    private static final HandlerList HANDLERS = new HandlerList();
    private final PlayerConfigurationConnection connection;
    public AsyncPlayerConnectionConfigureEvent(PlayerConfigurationConnection connection) {
        super(true);
        this.connection = connection;
    }
    public PlayerConfigurationConnection getConnection() { return connection; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
