package org.bukkit.event.player;

import org.bukkit.entity.Arrow;
import org.bukkit.entity.AbstractArrow;
import org.bukkit.entity.Player;
import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;
import org.bukkit.inventory.ItemStack;

/** Fired when a player picks up an arrow entity. */
public class PlayerPickupArrowEvent extends PlayerEvent implements Cancellable {
    private final Arrow arrow; private final org.bukkit.entity.Item itemEntity; private final ItemStack itemStack; private boolean cancelled;
    private static final HandlerList HANDLERS = new HandlerList();
    public PlayerPickupArrowEvent(Player player, Arrow arrow, ItemStack item) { super(player); this.arrow = arrow; this.itemEntity = null; this.itemStack = item == null ? null : item.clone(); }
    public PlayerPickupArrowEvent(Player player, Arrow arrow, org.bukkit.entity.Item item) { super(player); this.arrow = arrow; this.itemEntity = item; this.itemStack = null; }
    public AbstractArrow getArrow() { return arrow; }
    public org.bukkit.entity.Item getItem() { return itemEntity; }
    public ItemStack getItemStack() { return itemStack == null ? null : itemStack.clone(); }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean value) { cancelled = value; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
