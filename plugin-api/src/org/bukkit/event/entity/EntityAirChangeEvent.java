package org.bukkit.event.entity;
import org.bukkit.entity.Entity; import org.bukkit.event.Cancellable; import org.bukkit.event.HandlerList;
public class EntityAirChangeEvent extends EntityEvent implements Cancellable {
 private int amount; private boolean cancelled; private static final HandlerList HANDLERS=new HandlerList();
 public EntityAirChangeEvent(Entity entity,int amount){super(entity);this.amount=amount;} public int getAmount(){return amount;} public void setAmount(int amount){this.amount=amount;}
 public boolean isCancelled(){return cancelled;} public void setCancelled(boolean value){cancelled=value;} public HandlerList getHandlers(){return HANDLERS;} public static HandlerList getHandlerList(){return HANDLERS;}
}
