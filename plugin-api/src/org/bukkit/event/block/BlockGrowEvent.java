package org.bukkit.event.block;
import org.bukkit.block.Block;
import org.bukkit.block.BlockState;
import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;
public class BlockGrowEvent extends BlockEvent implements Cancellable {
 private static final HandlerList HANDLERS=new HandlerList(); private final BlockState newState; private boolean cancelled;
 public BlockGrowEvent(Block block, BlockState newState){super(block);this.newState=newState;}
 public BlockState getNewState(){return newState;} public boolean isCancelled(){return cancelled;} public void setCancelled(boolean cancel){cancelled=cancel;}
 public HandlerList getHandlers(){return HANDLERS;} public static HandlerList getHandlerList(){return HANDLERS;}
}
