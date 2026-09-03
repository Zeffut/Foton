package org.bukkit.event.player;
import org.bukkit.entity.Player;
import org.bukkit.Location;
import org.bukkit.event.Event;
import org.bukkit.event.HandlerList;
public class PlayerRespawnEvent extends Event {
 private final Player player; private Location respawnLocation; private final boolean bedSpawn; private final boolean anchorSpawn; private static final HandlerList HANDLERS = new HandlerList();
 public PlayerRespawnEvent(Player player){this(player, null, false);}
 public PlayerRespawnEvent(Player player, Location location){this(player, location, false, false);}
 public PlayerRespawnEvent(Player player, Location location, boolean bedSpawn){this(player, location, bedSpawn, false);}
 public PlayerRespawnEvent(Player player, Location location, boolean bedSpawn, boolean anchorSpawn){this.player=player; this.respawnLocation=location; this.bedSpawn=bedSpawn; this.anchorSpawn=anchorSpawn;}
 public Player getPlayer(){return player;}
 public Location getRespawnLocation(){return respawnLocation;}
 public void setRespawnLocation(Location location){if(location != null) respawnLocation=location;}
 public boolean isBedSpawn(){return bedSpawn;}
 public boolean isAnchorSpawn(){return anchorSpawn;}
 public HandlerList getHandlers(){return HANDLERS;}
 public static HandlerList getHandlerList(){return HANDLERS;}
}
