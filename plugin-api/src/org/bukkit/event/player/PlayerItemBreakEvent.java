package org.bukkit.event.player;

import org.bukkit.event.HandlerList;
import org.bukkit.inventory.ItemStack;

/** Fired when a player's equipped item breaks. */
public final class PlayerItemBreakEvent extends PlayerEvent {
    private final ItemStack brokenItem;
    private static final HandlerList HANDLERS = new HandlerList();
    public PlayerItemBreakEvent(org.bukkit.entity.Player player, ItemStack brokenItem) {
        super(player); this.brokenItem = brokenItem == null ? null : brokenItem.clone();
    }
    public ItemStack getBrokenItem() { return brokenItem == null ? null : brokenItem.clone(); }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
