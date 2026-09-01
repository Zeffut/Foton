package org.bukkit.event.world;
import org.bukkit.World;
import org.bukkit.event.Event;
import org.bukkit.event.HandlerList;
public class WorldUnloadEvent extends Event {
 private final World world; private static final HandlerList HANDLERS = new HandlerList();
 public WorldUnloadEvent(World world){this.world=world;}
 public World getWorld(){return world;}
 public HandlerList getHandlers(){return HANDLERS;}
 public static HandlerList getHandlerList(){return HANDLERS;}
}
