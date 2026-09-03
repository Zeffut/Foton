package org.bukkit.inventory;

/** Inventory exposed by a chested llama. */
public interface LlamaInventory extends HorseInventory {
    default ItemStack getDecor() { return getArmor(); }
    default void setDecor(ItemStack decor) { setArmor(decor); }
}
