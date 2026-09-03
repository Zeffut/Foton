package org.bukkit.inventory;

import org.bukkit.NamespacedKey;

public class TransmuteRecipe extends CraftingRecipe {
    private final RecipeChoice input;
    private final RecipeChoice material;
    public TransmuteRecipe(NamespacedKey key, ItemStack result, RecipeChoice input, RecipeChoice material) {
        super(key, result);
        this.input = input;
        this.material = material;
    }
    public RecipeChoice getInput() { return input; }
    public RecipeChoice getMaterial() { return material; }
}
