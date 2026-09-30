package org.bukkit.inventory;

/** A smithing table's template, base and addition, and its result. */
public interface SmithingInventory extends Inventory {
    ItemStack getResult();
    void setResult(ItemStack newResult);
    /** The smithing recipe the inputs match, or null. */
    Recipe getRecipe();
    default ItemStack getInputTemplate() { return getItem(0); }
    default void setInputTemplate(ItemStack itemStack) { setItem(0, itemStack); }
    default ItemStack getInputEquipment() { return getItem(1); }
    default void setInputEquipment(ItemStack itemStack) { setItem(1, itemStack); }
    default ItemStack getInputMineral() { return getItem(2); }
    default void setInputMineral(ItemStack itemStack) { setItem(2, itemStack); }
}
