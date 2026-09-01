package org.bukkit.event.player;
import org.bukkit.entity.Player;
import org.bukkit.event.Event;
import org.bukkit.event.HandlerList;
public class PlayerRespawnEvent extends Event {
 private final Player player; private static final HandlerList HANDLERS = new HandlerList();
 public PlayerRespawnEvent(Player player){this.player=player;}
 public Player getPlayer(){return player;}
 public HandlerList getHandlers(){return HANDLERS;}
 public static HandlerList getHandlerList(){return HANDLERS;}
}
