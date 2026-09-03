package com.destroystokyo.paper.event.block;

import org.bukkit.block.Block;
import org.bukkit.entity.Entity;
import org.bukkit.event.Cancellable;
import org.bukkit.event.Event;
import org.bukkit.event.HandlerList;

/** Legacy Paper TNT priming event. */
public class TNTPrimeEvent extends Event implements Cancellable {
    public enum PrimeReason { BLOCK, FIRE, EXPLOSION, PROJECTILE, REDSTONE, ENTITY }
    public enum PrimeCause { BLOCK, FIRE, EXPLOSION, PROJECTILE, REDSTONE, ENTITY }
    private final Block block; private final Entity primerEntity;
    private final PrimeReason reason; private final PrimeCause cause; private boolean cancelled;
    private static final HandlerList HANDLERS = new HandlerList();
    public TNTPrimeEvent(Block block, Entity primerEntity, PrimeReason reason) {
        this.block = block; this.primerEntity = primerEntity; this.reason = reason; this.cause = PrimeCause.valueOf(reason.name());
    }
    public Block getBlock() { return block; }
    public Entity getPrimerEntity() { return primerEntity; }
    public PrimeReason getReason() { return reason; }
    public PrimeCause getCause() { return cause; }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean value) { cancelled = value; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
