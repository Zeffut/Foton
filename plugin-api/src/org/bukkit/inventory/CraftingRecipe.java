package org.bukkit.inventory;

import org.bukkit.Keyed;
import org.bukkit.NamespacedKey;

/** Common base for keyed crafting recipes. */
public abstract class CraftingRecipe implements Recipe, Keyed {
    private final NamespacedKey key;
    private final ItemStack result;
    protected CraftingRecipe(NamespacedKey key, ItemStack result) {
        this.key = key; this.result = result == null ? new ItemStack(org.bukkit.Material.AIR) : result.clone();
    }
    @Override public NamespacedKey getKey() { return key; }
    @Override public ItemStack getResult() { return result.clone(); }
}
