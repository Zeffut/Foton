package org.bukkit.event.block;
import org.bukkit.block.Block;
import org.bukkit.block.BlockState;
import org.bukkit.block.data.Levelled;
import org.bukkit.entity.Entity;
import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;
public class CauldronLevelChangeEvent extends BlockEvent implements Cancellable {
 private static final HandlerList HANDLERS=new HandlerList(); private final Entity entity; private final ChangeReason reason; private final BlockState newState; private boolean cancelled;
 public CauldronLevelChangeEvent(Block block, Entity entity, ChangeReason reason, BlockState newState){super(block);this.entity=entity;this.reason=reason;this.newState=newState;}
 public Entity getEntity(){return entity;} public ChangeReason getReason(){return reason;} public BlockState getNewState(){return newState;}
 public int getOldLevel(){return level(getBlock()==null?null:getBlock().getBlockData());} public int getNewLevel(){return level(newState==null?null:newState.getBlockData());}
 private static int level(org.bukkit.block.data.BlockData data){return data instanceof Levelled l?l.getLevel():0;}
 public boolean isCancelled(){return cancelled;} public void setCancelled(boolean cancel){cancelled=cancel;} public HandlerList getHandlers(){return HANDLERS;} public static HandlerList getHandlerList(){return HANDLERS;}
 public enum ChangeReason { BUCKET_FILL, BUCKET_EMPTY, BOTTLE_FILL, BOTTLE_EMPTY, BANNER_WASH, ARMOR_WASH, SHULKER_WASH, EXTINGUISH, EVAPORATE, NATURAL_FILL, UNKNOWN }
}
