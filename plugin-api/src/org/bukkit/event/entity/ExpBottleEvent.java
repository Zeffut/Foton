package org.bukkit.event.entity;
import org.bukkit.entity.ThrownExpBottle;
import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;
public class ExpBottleEvent extends EntityEvent implements Cancellable {
 private int experience; private boolean cancelled; private static final HandlerList HANDLERS=new HandlerList();
 public ExpBottleEvent(ThrownExpBottle entity,int experience){super(entity);this.experience=Math.max(0,experience);}
 @Override public ThrownExpBottle getEntity(){return (ThrownExpBottle)super.getEntity();}
 public int getExperience(){return experience;} public void setExperience(int value){experience=Math.max(0,value);}
 public boolean isCancelled(){return cancelled;} public void setCancelled(boolean value){cancelled=value;}
 public HandlerList getHandlers(){return HANDLERS;} public static HandlerList getHandlerList(){return HANDLERS;}
}
