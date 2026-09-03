package org.bukkit.event.entity;
import org.bukkit.entity.AnimalTamer;
import org.bukkit.entity.LivingEntity;
import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;
public class EntityTameEvent extends EntityEvent implements Cancellable {
 private static final HandlerList HANDLERS=new HandlerList(); private final AnimalTamer owner; private boolean cancelled;
 public EntityTameEvent(LivingEntity entity, AnimalTamer owner){super(entity);this.owner=owner;}
 public LivingEntity getEntity(){return (LivingEntity)super.getEntity();} public AnimalTamer getOwner(){return owner;}
 public boolean isCancelled(){return cancelled;} public void setCancelled(boolean cancel){cancelled=cancel;} public HandlerList getHandlers(){return HANDLERS;} public static HandlerList getHandlerList(){return HANDLERS;}
}
