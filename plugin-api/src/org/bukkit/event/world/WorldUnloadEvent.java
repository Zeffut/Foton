package org.bukkit.event.world;
import org.bukkit.World;
import org.bukkit.event.Event;
import org.bukkit.event.HandlerList;
import org.bukkit.event.Cancellable;
public class WorldUnloadEvent extends Event implements Cancellable {
 private final World world; private boolean cancelled; private static final HandlerList HANDLERS = new HandlerList();
 public WorldUnloadEvent(World world){this.world=world;}
 public World getWorld(){return world;}
 public boolean isCancelled(){return cancelled;}
 public void setCancelled(boolean value){cancelled=value;}
 public HandlerList getHandlers(){return HANDLERS;}
 public static HandlerList getHandlerList(){return HANDLERS;}
}
