package org.bukkit.event.entity;
import org.bukkit.entity.LivingEntity;
import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;
import org.bukkit.util.Vector;
public class EntityKnockbackEvent extends EntityEvent implements Cancellable {
 private static final HandlerList HANDLER_LIST=new HandlerList(); private final KnockbackCause cause; private final double force; private final Vector raw; private Vector finalKnockback; private boolean cancelled;
 public EntityKnockbackEvent(LivingEntity entity,KnockbackCause cause,double force,Vector raw,Vector knockback){super(entity);this.cause=cause;this.force=force;this.raw=raw.clone();this.finalKnockback=knockback.clone();}
 @Override public LivingEntity getEntity(){return (LivingEntity)super.getEntity();} public KnockbackCause getCause(){return cause;} public double getForce(){return force;} public Vector getKnockback(){return raw.clone();} public Vector getFinalKnockback(){return finalKnockback.clone();} public void setFinalKnockback(Vector value){if(value==null)throw new IllegalArgumentException("Knockback cannot be null");finalKnockback=value.clone();}
 public boolean isCancelled(){return cancelled;} public void setCancelled(boolean value){cancelled=value;} public HandlerList getHandlers(){return HANDLER_LIST;} public static HandlerList getHandlerList(){return HANDLER_LIST;}
 public enum KnockbackCause { DAMAGE, ENTITY_ATTACK, EXPLOSION, SHIELD_BLOCK, SWEEP_ATTACK, UNKNOWN }
}
