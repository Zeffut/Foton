package org.bukkit.event.block;

import org.bukkit.block.Block;
import org.bukkit.entity.Entity;
import org.bukkit.event.Cancellable;
import org.bukkit.event.Event;
import org.bukkit.event.HandlerList;

/** Fired when TNT is about to be primed. */
public class TNTPrimeEvent extends Event implements Cancellable {
    public enum PrimeReason {
        BLOCK, FIRE, EXPLOSION, PROJECTILE, REDSTONE, ENTITY
    }
    public enum PrimeCause { BLOCK, BLOCK_BREAK, FIRE, EXPLOSION, PROJECTILE, REDSTONE, ENTITY, UNKNOWN }
    private final Block block;
    private final Entity primingEntity;
    private final PrimeReason reason;
    private boolean cancelled;
    private static final HandlerList HANDLERS = new HandlerList();
    public TNTPrimeEvent(Block block, Entity primingEntity, PrimeReason reason) {
        this.block = block; this.primingEntity = primingEntity; this.reason = reason;
    }
    public Block getBlock() { return block; }
    public Entity getPrimingEntity() { return primingEntity; }
    /** Legacy Bukkit spelling retained for binary compatibility. */
    public Entity getPrimerEntity() { return primingEntity; }
    public PrimeReason getReason() { return reason; }
    public PrimeCause getCause() { return PrimeCause.valueOf(reason.name()); }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean value) { cancelled = value; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
