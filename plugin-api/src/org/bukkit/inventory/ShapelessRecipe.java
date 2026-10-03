package org.bukkit.inventory;

import java.util.ArrayList;
import java.util.List;
import org.bukkit.Material;
import org.bukkit.NamespacedKey;

/** A shapeless crafting recipe definition. */
public class ShapelessRecipe extends CraftingRecipe {
    private final List<RecipeChoice> choices = new ArrayList<>();
    public ShapelessRecipe(NamespacedKey key, ItemStack result) { super(key, result); }
    /** Legacy Bukkit constructor for recipes that are assigned a key on registration. */
    public ShapelessRecipe(ItemStack result) { this(null, result); }
    public ShapelessRecipe addIngredient(RecipeChoice choice) {
        if (choices.size() >= 9) {
            throw new IllegalArgumentException("Shapeless recipes cannot have more than 9 ingredients");
        }
        choices.add(java.util.Objects.requireNonNull(choice, "choice").clone());
        return this;
    }
    public ShapelessRecipe addIngredient(Material material) { return addIngredient(1, material); }
    /** Adds one grid ingredient per material, as in the Paper 1.21.11 API. */
    public ShapelessRecipe addIngredient(int count, Material material) {
        if (choices.size() + count > 9) {
            throw new IllegalArgumentException("Shapeless recipes cannot have more than 9 ingredients");
        }
        for (int i = 0; i < count; i++) {
            if (material == null) throw new IllegalArgumentException("Cannot have null choice");
            if (material.isAir()) throw new IllegalArgumentException("Cannot have empty/air choice");
            if (!material.isItem()) throw new IllegalArgumentException("Cannot have non-item choice " + material);
            choices.add(new RecipeChoice.MaterialChoice(material));
        }
        return this;
    }
    public ShapelessRecipe addIngredient(ItemStack stack) { return addIngredient(new RecipeChoice.ExactChoice(stack)); }
    public List<RecipeChoice> getChoiceList() {
        List<RecipeChoice> result = new ArrayList<>(choices.size());
        for (RecipeChoice choice : choices) result.add(choice.clone());
        return result;
    }
    /** Bukkit's representative item stacks, detached from the recipe. */
    public List<ItemStack> getIngredientList() {
        List<ItemStack> result = new ArrayList<>(choices.size());
        for (RecipeChoice choice : choices) result.add(choice.getItemStack().clone());
        return result;
    }
}
