package org.bukkit.event.entity;

import org.bukkit.entity.Entity;
import org.bukkit.event.HandlerList;

/** Fired when one entity ignites another. */
public class EntityCombustByEntityEvent extends EntityCombustEvent {
    private final Entity combuster;
    private static final HandlerList HANDLERS = new HandlerList();
    public EntityCombustByEntityEvent(Entity combuster, Entity combustee, int duration) {
        super(combustee, duration); this.combuster = combuster;
    }
    public Entity getCombuster() { return combuster; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
