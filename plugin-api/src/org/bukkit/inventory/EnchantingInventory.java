package org.bukkit.inventory;

/** The item and lapis slots of an enchanting table. */
public interface EnchantingInventory extends Inventory {
    void setItem(ItemStack item);
    ItemStack getItem();
    void setSecondary(ItemStack item);
    ItemStack getSecondary();
}
