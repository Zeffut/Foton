package org.bukkit.event.entity;
import org.bukkit.block.Block;
import org.bukkit.block.BlockFace;
import org.bukkit.entity.Entity;
import org.bukkit.entity.Projectile;
import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;
public class ProjectileHitEvent extends EntityEvent implements Cancellable {
 private static final HandlerList HANDLERS=new HandlerList(); private final Entity hitEntity; private final Block hitBlock; private final BlockFace hitFace; private boolean cancelled;
 public ProjectileHitEvent(Projectile projectile){this(projectile,null,null,null);} public ProjectileHitEvent(Projectile projectile,Entity hitEntity){this(projectile,hitEntity,null,null);} public ProjectileHitEvent(Projectile projectile,Block hitBlock){this(projectile,null,hitBlock,null);} public ProjectileHitEvent(Projectile projectile,Entity hitEntity,Block hitBlock){this(projectile,hitEntity,hitBlock,null);} public ProjectileHitEvent(Projectile projectile,Entity hitEntity,Block hitBlock,BlockFace hitFace){super(projectile);this.hitEntity=hitEntity;this.hitBlock=hitBlock;this.hitFace=hitFace;}
 public Projectile getEntity(){return (Projectile)super.getEntity();} public Entity getHitEntity(){return hitEntity;} public Block getHitBlock(){return hitBlock;} public BlockFace getHitBlockFace(){return hitFace;}
 public boolean isCancelled(){return cancelled;} public void setCancelled(boolean cancel){cancelled=cancel;} public HandlerList getHandlers(){return HANDLERS;} public static HandlerList getHandlerList(){return HANDLERS;}
}
