package org.bukkit.inventory;

public interface CraftingInventory extends Inventory {
    ItemStack[] getMatrix();
    void setMatrix(ItemStack[] matrix);
    ItemStack getResult();
    void setResult(ItemStack result);
}
