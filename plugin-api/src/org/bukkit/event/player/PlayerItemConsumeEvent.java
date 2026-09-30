package org.bukkit.event.player;

import org.bukkit.Material;
import org.bukkit.entity.Player;
import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;
import org.bukkit.inventory.EquipmentSlot;
import org.bukkit.inventory.ItemStack;

/** A player is about to finish eating or drinking. The item consumed and what
 * the hand holds afterwards may both be changed; cancelling leaves the item. */
public class PlayerItemConsumeEvent extends PlayerEvent implements Cancellable {
    private static final HandlerList HANDLERS = new HandlerList();
    private final EquipmentSlot hand;
    private ItemStack item;
    private ItemStack replacement;
    private boolean cancelled;

    public PlayerItemConsumeEvent(Player player, ItemStack item, EquipmentSlot hand) {
        super(player);
        this.item = item;
        this.hand = hand;
    }

    @Deprecated
    public PlayerItemConsumeEvent(Player player, ItemStack item) { this(player, item, EquipmentSlot.HAND); }

    public ItemStack getItem() { return item.clone(); }
    public void setItem(ItemStack item) { this.item = item == null ? new ItemStack(Material.AIR) : item; }
    public EquipmentSlot getHand() { return hand; }
    /** What the hand holds afterwards, or null for the item's own leftover. */
    public ItemStack getReplacement() { return replacement; }
    public void setReplacement(ItemStack replacement) { this.replacement = replacement; }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean cancel) { cancelled = cancel; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
