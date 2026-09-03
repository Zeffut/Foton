package io.papermc.paper.event.connection;

import io.papermc.paper.connection.PlayerConnection;
import org.bukkit.event.Event;
import org.bukkit.event.HandlerList;

/** Paper login validation event. */
public final class PlayerConnectionValidateLoginEvent extends Event {
    private final PlayerConnection connection;
    private boolean allowed = true;
    private static final HandlerList HANDLERS = new HandlerList();
    public PlayerConnectionValidateLoginEvent(PlayerConnection connection) { this.connection = connection; }
    public PlayerConnection getConnection() { return connection; }
    public boolean isAllowed() { return allowed; }
    public void setAllowed(boolean allowed) { this.allowed = allowed; }
    public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
