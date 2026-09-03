package org.bukkit.event.entity;

import org.bukkit.entity.LivingEntity;
import org.bukkit.event.Event;
import org.bukkit.event.HandlerList;

/** Fired when a living entity dies. */
public final class EntityDeathEvent extends Event {
    private final LivingEntity entity;
    private int droppedExp;
    private static final HandlerList HANDLERS = new HandlerList();
    public EntityDeathEvent(LivingEntity entity) { this(entity, 0); }
    public EntityDeathEvent(LivingEntity entity, int droppedExp) { this.entity = entity; this.droppedExp = Math.max(0, droppedExp); }
    public LivingEntity getEntity() { return entity; }
    public int getDroppedExp() { return droppedExp; }
    public void setDroppedExp(int value) { droppedExp = Math.max(0, value); }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
