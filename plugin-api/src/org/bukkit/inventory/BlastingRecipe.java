package org.bukkit.inventory;

import org.bukkit.Material;
import org.bukkit.NamespacedKey;

/** A recipe a blast furnace cooks. */
public class BlastingRecipe extends CookingRecipe<BlastingRecipe> {
    public BlastingRecipe(NamespacedKey key, ItemStack result, Material source, float experience, int cookingTime) {
        super(key, result, source, experience, cookingTime);
    }
    public BlastingRecipe(NamespacedKey key, ItemStack result, RecipeChoice input, float experience, int cookingTime) {
        super(key, result, input, experience, cookingTime);
    }
}
