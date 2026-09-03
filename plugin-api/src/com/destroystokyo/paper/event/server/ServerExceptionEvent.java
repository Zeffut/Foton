package com.destroystokyo.paper.event.server;

import com.destroystokyo.paper.exception.ServerException;
import org.bukkit.event.Event;
import org.bukkit.event.HandlerList;

/** Event describing a server-side plugin exception. */
public class ServerExceptionEvent extends Event {
    private static final HandlerList HANDLERS = new HandlerList();
    private final ServerException exception;
    public ServerExceptionEvent(ServerException exception) { this.exception = exception; }
    public ServerException getException() { return exception; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
