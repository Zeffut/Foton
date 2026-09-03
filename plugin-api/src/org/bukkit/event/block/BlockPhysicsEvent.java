package org.bukkit.event.block;
import org.bukkit.Material;
import org.bukkit.block.Block;
import org.bukkit.block.data.BlockData;
import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;
public class BlockPhysicsEvent extends BlockEvent implements Cancellable {
 private static final HandlerList HANDLERS=new HandlerList(); private final BlockData changed; private boolean cancelled;
 public BlockPhysicsEvent(Block block, BlockData changed){super(block);this.changed=changed;}
 public BlockPhysicsEvent(Block block, BlockData changed, Block source){this(block,changed);}
 public BlockPhysicsEvent(Block block, BlockData changed,int x,int y,int z){this(block,changed);}
 public Material getChangedType(){return changed==null?null:changed.getMaterial();}
 public boolean isCancelled(){return cancelled;} public void setCancelled(boolean cancel){cancelled=cancel;} public HandlerList getHandlers(){return HANDLERS;} public static HandlerList getHandlerList(){return HANDLERS;}
}
