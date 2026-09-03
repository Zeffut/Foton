package org.bukkit.inventory;

public interface GrindstoneInventory extends Inventory {
    ItemStack getUpperItem();
    void setUpperItem(ItemStack item);
    ItemStack getLowerItem();
    void setLowerItem(ItemStack item);
    ItemStack getResult();
    void setResult(ItemStack result);
}
