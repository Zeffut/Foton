package org.bukkit.event.entity;
import org.bukkit.entity.Player;
import org.bukkit.event.Event;
import org.bukkit.event.HandlerList;
public class PlayerDeathEvent extends Event {
 private final Player player; private static final HandlerList HANDLERS = new HandlerList();
 public PlayerDeathEvent(Player player){this.player=player;}
 public Player getEntity(){return player;}
 public Player getPlayer(){return player;}
 public HandlerList getHandlers(){return HANDLERS;}
 public static HandlerList getHandlerList(){return HANDLERS;}
}
