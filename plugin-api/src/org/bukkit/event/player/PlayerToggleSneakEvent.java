package org.bukkit.event.player;
import org.bukkit.entity.Player; import org.bukkit.event.Event; import org.bukkit.event.HandlerList;
public final class PlayerToggleSneakEvent extends PlayerEvent { private final boolean sneaking; private static final HandlerList HANDLERS=new HandlerList(); public PlayerToggleSneakEvent(Player p,boolean s){super(p);sneaking=s;} public boolean isSneaking(){return sneaking;} public HandlerList getHandlers(){return HANDLERS;} public static HandlerList getHandlerList(){return HANDLERS;} }
