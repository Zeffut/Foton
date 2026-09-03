package org.bukkit.event.entity;

import org.bukkit.entity.Entity;
import org.bukkit.entity.Projectile;
import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;

/** Fired before a player-created projectile is added to a world. */
public class ProjectileLaunchEvent extends EntityEvent implements Cancellable {
    private final Entity shooter;
    private boolean cancelled;
    private static final HandlerList HANDLERS = new HandlerList();
    public ProjectileLaunchEvent(Projectile entity, Entity shooter) { super(entity); this.shooter = shooter; }
    @Override public Projectile getEntity() { return (Projectile) super.getEntity(); }
    public Entity getShooter() { return shooter; }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean value) { cancelled = value; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
