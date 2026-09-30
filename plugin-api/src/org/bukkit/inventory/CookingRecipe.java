package org.bukkit.inventory;

import org.bukkit.Keyed;
import org.bukkit.Material;
import org.bukkit.NamespacedKey;

/** A recipe cooked over time: in a furnace, a blast furnace, a smoker or on a
 * campfire. */
public abstract class CookingRecipe<T extends CookingRecipe> implements Recipe, Keyed {
    private final NamespacedKey key;
    private final ItemStack result;
    private RecipeChoice input;
    private float experience;
    private int cookingTime;

    public CookingRecipe(NamespacedKey key, ItemStack result, Material source, float experience, int cookingTime) {
        this(key, result, new RecipeChoice.MaterialChoice(source), experience, cookingTime);
    }

    public CookingRecipe(NamespacedKey key, ItemStack result, RecipeChoice input, float experience, int cookingTime) {
        this.key = key;
        this.result = result == null ? new ItemStack(Material.AIR) : result.clone();
        this.input = input == null ? new RecipeChoice.MaterialChoice(Material.AIR) : input;
        this.experience = experience;
        this.cookingTime = cookingTime;
    }

    @Override public NamespacedKey getKey() { return key; }
    @Override public ItemStack getResult() { return result.clone(); }
    public ItemStack getInput() { return input.getItemStack(); }
    public CookingRecipe setInput(Material input) { this.input = new RecipeChoice.MaterialChoice(input); return this; }
    public RecipeChoice getInputChoice() { return input; }
    @SuppressWarnings("unchecked")
    public T setInputChoice(RecipeChoice input) { this.input = input; return (T) this; }
    public float getExperience() { return experience; }
    public void setExperience(float experience) { this.experience = experience; }
    public int getCookingTime() { return cookingTime; }
    public void setCookingTime(int cookingTime) {
        if (cookingTime < 0) throw new IllegalArgumentException("cookingTime must be >= 0");
        this.cookingTime = cookingTime;
    }
}
