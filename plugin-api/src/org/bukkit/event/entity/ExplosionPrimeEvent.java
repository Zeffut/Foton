package org.bukkit.event.entity;
import org.bukkit.entity.Entity;
import org.bukkit.entity.Explosive;
import org.bukkit.entity.EntityType;
import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;
public class ExplosionPrimeEvent extends EntityEvent implements Cancellable {
 private static final HandlerList HANDLERS=new HandlerList(); private float radius; private boolean fire; private boolean cancelled;
 public ExplosionPrimeEvent(Entity entity,float radius,boolean fire){super(entity);this.radius=radius;this.fire=fire;}
 public ExplosionPrimeEvent(Explosive explosive){this(explosive,3.0f,false);}
 public EntityType getEntityType(){return getEntity().getType();} public float getRadius(){return radius;} public void setRadius(float value){radius=value;} public boolean getFire(){return fire;} public void setFire(boolean value){fire=value;}
 public boolean isCancelled(){return cancelled;} public void setCancelled(boolean cancel){cancelled=cancel;} public HandlerList getHandlers(){return HANDLERS;} public static HandlerList getHandlerList(){return HANDLERS;}
}
