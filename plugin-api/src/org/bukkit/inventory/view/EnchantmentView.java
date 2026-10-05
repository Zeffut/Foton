package org.bukkit.inventory.view;

import org.bukkit.enchantments.EnchantmentOffer;
import org.bukkit.inventory.EnchantingInventory;
import org.bukkit.inventory.InventoryView;

/** The server's current enchanting offers and per-menu random seed. */
public interface EnchantmentView extends InventoryView {
    @Override EnchantingInventory getTopInventory();
    int getEnchantmentSeed();
    void setEnchantmentSeed(int seed);
    EnchantmentOffer[] getOffers();
    void setOffers(EnchantmentOffer[] offers);
}
