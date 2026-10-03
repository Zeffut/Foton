package org.bukkit.event.enchantment;

import org.bukkit.block.Block;
import org.bukkit.enchantments.EnchantmentOffer;
import org.bukkit.entity.Player;
import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;
import org.bukkit.event.inventory.InventoryEvent;
import org.bukkit.inventory.ItemStack;
import org.bukkit.inventory.view.EnchantmentView;

/** Fired after offers are rolled and before the client can use them. */
public class PrepareItemEnchantEvent extends InventoryEvent implements Cancellable {
    private static final HandlerList HANDLERS = new HandlerList();
    private final Player enchanter;
    private final Block table;
    private final ItemStack item;
    private final EnchantmentOffer[] offers;
    private final int bonus;
    private boolean cancelled;

    public PrepareItemEnchantEvent(Player enchanter, EnchantmentView view, Block table,
            ItemStack item, EnchantmentOffer[] offers, int bonus) {
        super(view);
        this.enchanter = enchanter;
        this.table = table;
        this.item = item;
        this.offers = offers;
        this.bonus = bonus;
    }

    @Override public EnchantmentView getView() { return (EnchantmentView) super.getView(); }
    public Player getEnchanter() { return enchanter; }
    public Block getEnchantBlock() { return table; }
    public ItemStack getItem() { return item; }
    public EnchantmentOffer[] getOffers() { return offers; }
    public int getEnchantmentBonus() { return bonus; }
    @Deprecated(since = "1.20.5")
    public int[] getExpLevelCostsOffered() {
        int[] result = new int[offers.length];
        for (int index = 0; index < offers.length; index++)
            result[index] = offers[index] == null ? 0 : offers[index].getCost();
        return result;
    }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean cancelled) { this.cancelled = cancelled; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
