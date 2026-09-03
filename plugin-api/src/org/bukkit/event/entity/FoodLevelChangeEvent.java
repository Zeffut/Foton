package org.bukkit.event.entity;

import org.bukkit.entity.Player;
import org.bukkit.entity.HumanEntity;
import org.bukkit.event.Cancellable;
import org.bukkit.event.Event;
import org.bukkit.event.HandlerList;

/** Fired before a player's food level changes. */
public final class FoodLevelChangeEvent extends Event implements Cancellable {
    private final HumanEntity entity;
    private int foodLevel;
    private final org.bukkit.inventory.ItemStack item;
    private boolean cancelled;
    private static final HandlerList HANDLERS = new HandlerList();
    public FoodLevelChangeEvent(Player player, int foodLevel) { this((HumanEntity) player, foodLevel, null); }
    public FoodLevelChangeEvent(HumanEntity entity, int foodLevel) { this(entity, foodLevel, null); }
    public FoodLevelChangeEvent(HumanEntity entity, int foodLevel, org.bukkit.inventory.ItemStack item) { this.entity = entity; this.foodLevel = foodLevel; this.item = item == null ? null : item.clone(); }
    public HumanEntity getEntity() { return entity; }
    public Player getPlayer() { return entity instanceof Player player ? player : null; }
    public org.bukkit.entity.EntityType getEntityType() { return entity == null ? null : entity.getType(); }
    public org.bukkit.inventory.ItemStack getItem() { return item == null ? null : item.clone(); }
    public int getFoodLevel() { return foodLevel; }
    public void setFoodLevel(int value) { foodLevel = value; }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean value) { cancelled = value; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
