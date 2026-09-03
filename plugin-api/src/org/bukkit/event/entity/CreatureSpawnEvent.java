package org.bukkit.event.entity;

import org.bukkit.Location;
import org.bukkit.entity.LivingEntity;
import org.bukkit.event.Cancellable;
import org.bukkit.event.Event;
import org.bukkit.event.HandlerList;

/** Fired before a creature is inserted into a world. */
public class CreatureSpawnEvent extends EntityEvent implements Cancellable {
    public enum SpawnReason { NATURAL, SPAWNER, SPAWNER_EGG, DISPENSE_EGG, EGG, BREEDING, COMMAND, CUSTOM, DEFAULT }
    private final Location location;
    private final SpawnReason reason;
    private boolean cancelled;
    private static final HandlerList HANDLERS = new HandlerList();

    public CreatureSpawnEvent(LivingEntity entity, Location location, SpawnReason reason) {
        super(entity); this.location = location; this.reason = reason;
    }
    @Override public LivingEntity getEntity() { return (LivingEntity) super.getEntity(); }
    public org.bukkit.entity.EntityType getEntityType() { return getEntity() == null ? null : getEntity().getType(); }
    public Location getLocation() { return location; }
    public SpawnReason getSpawnReason() { return reason; }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean value) { cancelled = value; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
