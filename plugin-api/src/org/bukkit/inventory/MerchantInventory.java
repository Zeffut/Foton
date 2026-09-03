package org.bukkit.inventory;

/** Inventory view associated with a merchant. */
public interface MerchantInventory extends Inventory {
    Merchant getMerchant();
    default MerchantRecipe getSelectedRecipe() { return null; }
}
