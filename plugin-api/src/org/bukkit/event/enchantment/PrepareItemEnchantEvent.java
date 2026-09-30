package org.bukkit.event.enchantment;

import org.bukkit.block.Block;
import org.bukkit.enchantments.EnchantmentOffer;
import org.bukkit.entity.Player;
import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;
import org.bukkit.event.inventory.InventoryEvent;
import org.bukkit.inventory.InventoryView;
import org.bukkit.inventory.ItemStack;

/** An enchanting table has rolled its offers for the item put in it. The
 * offers left in {@link #getOffers()} are what the screen shows and what a
 * click costs; cancelling blanks them. */
public class PrepareItemEnchantEvent extends InventoryEvent implements Cancellable {
    private static final HandlerList HANDLER_LIST = new HandlerList();
    private final Player enchanter;
    private final Block table;
    private final ItemStack item;
    private final EnchantmentOffer[] offers;
    private final int bonus;
    private boolean cancelled;

    public PrepareItemEnchantEvent(Player enchanter, InventoryView view, Block table, ItemStack item,
            EnchantmentOffer[] offers, int bonus) {
        super(view);
        this.enchanter = enchanter;
        this.table = table;
        this.item = item;
        this.offers = offers;
        this.bonus = bonus;
    }

    public Player getEnchanter() { return enchanter; }
    public Block getEnchantBlock() { return table; }
    public ItemStack getItem() { return item; }
    @Deprecated(since = "1.20.5")
    public int[] getExpLevelCostsOffered() {
        int[] levelOffers = new int[offers.length];
        for (int i = 0; i < offers.length; i++) levelOffers[i] = offers[i] != null ? offers[i].getCost() : 0;
        return levelOffers;
    }
    public EnchantmentOffer[] getOffers() { return offers; }
    public int getEnchantmentBonus() { return bonus; }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean cancel) { cancelled = cancel; }
    @Override public HandlerList getHandlers() { return HANDLER_LIST; }
    public static HandlerList getHandlerList() { return HANDLER_LIST; }
}
