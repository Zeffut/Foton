package org.bukkit.event.hanging;

import org.bukkit.entity.Hanging;
import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;

/** Fired before a hanging entity is broken. */
public class HangingBreakEvent extends HangingEvent implements Cancellable {
    public enum RemoveCause { ENTITY, EXPLOSION, OBSTRUCTION, PHYSICS, DEFAULT }
    private final RemoveCause cause;
    private boolean cancelled;
    private static final HandlerList HANDLERS = new HandlerList();

    public HangingBreakEvent(Hanging entity, RemoveCause cause) {
        super(entity);
        this.cause = cause == null ? RemoveCause.DEFAULT : cause;
    }

    public RemoveCause getCause() { return cause; }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean value) { cancelled = value; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
