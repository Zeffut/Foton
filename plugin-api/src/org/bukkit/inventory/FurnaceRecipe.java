package org.bukkit.inventory;

import org.bukkit.Keyed;
import org.bukkit.Material;
import org.bukkit.NamespacedKey;

/** Bukkit furnace recipe with a representative input stack. */
public class FurnaceRecipe implements Recipe, Keyed {
    private final NamespacedKey key;
    private final ItemStack result;
    private final RecipeChoice input;
    private final float experience;
    private final int cookingTime;

    public FurnaceRecipe(NamespacedKey key, ItemStack result, Material input, float experience, int cookingTime) {
        this(key, result, new RecipeChoice.MaterialChoice(input), experience, cookingTime);
    }
    public FurnaceRecipe(NamespacedKey key, ItemStack result, RecipeChoice input, float experience, int cookingTime) {
        this.key = key;
        this.result = result == null ? new ItemStack(Material.AIR) : result.clone();
        this.input = input == null ? new RecipeChoice.MaterialChoice(Material.AIR) : input;
        this.experience = experience;
        this.cookingTime = cookingTime;
    }
    @Override public NamespacedKey getKey() { return key; }
    @Override public ItemStack getResult() { return result.clone(); }
    public ItemStack getInput() { return input.getItemStack(); }
    public RecipeChoice getInputChoice() { return input; }
    public float getExperience() { return experience; }
    public int getCookingTime() { return cookingTime; }
}
