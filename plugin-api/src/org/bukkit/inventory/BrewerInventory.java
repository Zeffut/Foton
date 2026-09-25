package org.bukkit.inventory;

/** The five slots of a brewing stand: three bottles, the ingredient, the fuel. */
public interface BrewerInventory extends Inventory {
    ItemStack getIngredient();
    void setIngredient(ItemStack ingredient);
    ItemStack getFuel();
    void setFuel(ItemStack fuel);
}
