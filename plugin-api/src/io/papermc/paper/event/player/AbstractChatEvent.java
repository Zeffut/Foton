package io.papermc.paper.event.player;

import org.bukkit.event.Cancellable;
import org.bukkit.event.Event;
import org.bukkit.event.HandlerList;

/** Base type exposed by Paper for chat events. */
public abstract class AbstractChatEvent extends Event implements Cancellable {
    private boolean cancelled;
    protected AbstractChatEvent() { super(); }
    protected AbstractChatEvent(boolean asynchronous) { super(asynchronous); }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean value) { cancelled = value; }
    private static final HandlerList HANDLERS = new HandlerList();
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
