package org.bukkit.event.block;
import org.bukkit.block.Block;
import org.bukkit.event.HandlerList;
public class BlockRedstoneEvent extends BlockEvent {
 private static final HandlerList HANDLERS=new HandlerList(); private final int oldCurrent; private int newCurrent;
 public BlockRedstoneEvent(Block block,int oldCurrent,int newCurrent){super(block);this.oldCurrent=oldCurrent;this.newCurrent=newCurrent;}
 public int getOldCurrent(){return oldCurrent;} public int getNewCurrent(){return newCurrent;} public void setNewCurrent(int value){newCurrent=Math.max(0,Math.min(15,value));}
 public HandlerList getHandlers(){return HANDLERS;} public static HandlerList getHandlerList(){return HANDLERS;}
}
