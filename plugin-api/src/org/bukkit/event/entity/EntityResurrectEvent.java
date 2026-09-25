package org.bukkit.event.entity;

import org.bukkit.entity.LivingEntity;
import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;
import org.bukkit.inventory.EquipmentSlot;

/** A living entity is about to be saved from death. Fired even with no totem,
 * starting cancelled; un-cancelling it saves the entity anyway. */
public class EntityResurrectEvent extends EntityEvent implements Cancellable {
    private static final HandlerList HANDLERS = new HandlerList();
    private final EquipmentSlot hand;
    private boolean cancelled;

    public EntityResurrectEvent(LivingEntity entity, EquipmentSlot hand) {
        super(entity);
        this.hand = hand;
    }

    @Deprecated
    public EntityResurrectEvent(LivingEntity entity) { this(entity, null); }

    @Override public LivingEntity getEntity() { return (LivingEntity) super.getEntity(); }
    /** The hand holding the totem, or null when there is none. */
    public EquipmentSlot getHand() { return hand; }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean cancelled) { this.cancelled = cancelled; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
