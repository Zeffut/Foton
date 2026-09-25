package org.bukkit.inventory;

import org.bukkit.Material;
import org.bukkit.NamespacedKey;

/** A recipe a furnace cooks. */
public class FurnaceRecipe extends CookingRecipe<FurnaceRecipe> {
    public FurnaceRecipe(NamespacedKey key, ItemStack result, Material input, float experience, int cookingTime) {
        super(key, result, input, experience, cookingTime);
    }
    public FurnaceRecipe(NamespacedKey key, ItemStack result, RecipeChoice input, float experience, int cookingTime) {
        super(key, result, input, experience, cookingTime);
    }
}
