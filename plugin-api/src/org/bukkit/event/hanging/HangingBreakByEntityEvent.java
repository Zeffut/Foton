package org.bukkit.event.hanging;

import org.bukkit.entity.Entity;
import org.bukkit.entity.Hanging;
import org.bukkit.event.HandlerList;

/** Fired when an entity breaks a hanging entity. */
public class HangingBreakByEntityEvent extends HangingBreakEvent {
    private final Entity remover;
    private static final HandlerList HANDLERS = new HandlerList();

    public HangingBreakByEntityEvent(Hanging entity, Entity remover) {
        super(entity, RemoveCause.ENTITY);
        this.remover = remover;
    }

    public Entity getRemover() { return remover; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
