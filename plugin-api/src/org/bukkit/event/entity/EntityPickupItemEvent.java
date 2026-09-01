package org.bukkit.event.entity;

import org.bukkit.entity.Item;
import org.bukkit.entity.LivingEntity;
import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;

/** Fired before a living entity picks up an item. */
public final class EntityPickupItemEvent extends org.bukkit.event.Event implements Cancellable {
    private final LivingEntity entity; private final Item item; private boolean cancelled;
    private static final HandlerList HANDLERS = new HandlerList();
    public EntityPickupItemEvent(LivingEntity entity, Item item) { this.entity = entity; this.item = item; }
    public LivingEntity getEntity() { return entity; } public Item getItem() { return item; }
    @Override public boolean isCancelled() { return cancelled; } @Override public void setCancelled(boolean value) { cancelled = value; }
    @Override public HandlerList getHandlers() { return HANDLERS; } public static HandlerList getHandlerList() { return HANDLERS; }
}
