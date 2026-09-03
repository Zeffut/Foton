package org.bukkit.event.entity;

import org.bukkit.entity.Creeper;
import org.bukkit.entity.LightningStrike;
import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;

/** Called when a creeper changes powered state. */
public class CreeperPowerEvent extends EntityEvent implements Cancellable {
    private static final HandlerList HANDLER_LIST = new HandlerList();
    private final PowerCause cause;
    private LightningStrike bolt;
    private boolean cancelled;
    public CreeperPowerEvent(Creeper creeper, LightningStrike bolt, PowerCause cause) { this(creeper, cause); this.bolt = bolt; }
    public CreeperPowerEvent(Creeper creeper, PowerCause cause) { super(creeper); this.cause = cause; }
    @Override public Creeper getEntity() { return (Creeper) super.getEntity(); }
    public LightningStrike getLightning() { return bolt; }
    public PowerCause getCause() { return cause; }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean cancel) { cancelled = cancel; }
    @Override public HandlerList getHandlers() { return HANDLER_LIST; }
    public static HandlerList getHandlerList() { return HANDLER_LIST; }
    public enum PowerCause { LIGHTNING, SET_ON, SET_OFF }
}
