package org.bukkit.event.block;
import org.bukkit.block.Block; import org.bukkit.event.Cancellable; import org.bukkit.event.HandlerList;
public class BlockFromToEvent extends BlockEvent implements Cancellable {
 private final Block toBlock; private boolean cancelled; private static final HandlerList HANDLERS=new HandlerList();
 public BlockFromToEvent(Block block, Block toBlock){super(block);this.toBlock=toBlock;}
 public Block getToBlock(){return toBlock;} public boolean isCancelled(){return cancelled;} public void setCancelled(boolean value){cancelled=value;}
 public HandlerList getHandlers(){return HANDLERS;} public static HandlerList getHandlerList(){return HANDLERS;}
}
