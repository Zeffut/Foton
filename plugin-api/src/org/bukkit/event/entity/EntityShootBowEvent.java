package org.bukkit.event.entity;
import org.bukkit.entity.*; import org.bukkit.inventory.ItemStack; import org.bukkit.event.Cancellable; import org.bukkit.event.HandlerList;
public class EntityShootBowEvent extends EntityEvent implements Cancellable {
 private final LivingEntity shooter; private final ItemStack bow; private final Entity projectile; private boolean cancelled; private static final HandlerList HANDLERS=new HandlerList();
 public EntityShootBowEvent(LivingEntity shooter,ItemStack bow,Entity projectile){super(shooter);this.shooter=shooter;this.bow=bow;this.projectile=projectile;} public LivingEntity getEntity(){return shooter;} public ItemStack getBow(){return bow;} public Entity getProjectile(){return projectile;}
 public boolean isCancelled(){return cancelled;} public void setCancelled(boolean value){cancelled=value;} public HandlerList getHandlers(){return HANDLERS;} public static HandlerList getHandlerList(){return HANDLERS;}
}
