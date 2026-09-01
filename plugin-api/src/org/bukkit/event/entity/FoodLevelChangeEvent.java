package org.bukkit.event.entity;

import org.bukkit.entity.Player;
import org.bukkit.event.Cancellable;
import org.bukkit.event.Event;
import org.bukkit.event.HandlerList;

/** Fired before a player's food level changes. */
public final class FoodLevelChangeEvent extends Event implements Cancellable {
    private final Player player;
    private int foodLevel;
    private boolean cancelled;
    private static final HandlerList HANDLERS = new HandlerList();
    public FoodLevelChangeEvent(Player player, int foodLevel) { this.player = player; this.foodLevel = foodLevel; }
    public Player getEntity() { return player; }
    public Player getPlayer() { return player; }
    public int getFoodLevel() { return foodLevel; }
    public void setFoodLevel(int value) { foodLevel = value; }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean value) { cancelled = value; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
