package org.bukkit.inventory;

import org.bukkit.Material;
import org.bukkit.NamespacedKey;

/** A recipe a smoker cooks. */
public class SmokingRecipe extends CookingRecipe<SmokingRecipe> {
    public SmokingRecipe(NamespacedKey key, ItemStack result, Material source, float experience, int cookingTime) {
        super(key, result, source, experience, cookingTime);
    }
    public SmokingRecipe(NamespacedKey key, ItemStack result, RecipeChoice input, float experience, int cookingTime) {
        super(key, result, input, experience, cookingTime);
    }
}
