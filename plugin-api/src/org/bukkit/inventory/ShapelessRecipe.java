package org.bukkit.inventory;

import java.util.ArrayList;
import java.util.Collections;
import java.util.List;
import org.bukkit.Material;
import org.bukkit.NamespacedKey;

/** A shapeless crafting recipe definition. */
public class ShapelessRecipe extends CraftingRecipe {
    private final List<RecipeChoice> choices = new ArrayList<>();
    public ShapelessRecipe(NamespacedKey key, ItemStack result) { super(key, result); }
    /** Legacy Bukkit constructor for recipes that are assigned a key on registration. */
    public ShapelessRecipe(ItemStack result) { this(null, result); }
    public ShapelessRecipe addIngredient(RecipeChoice choice) { if (choice != null) choices.add(choice); return this; }
    public ShapelessRecipe addIngredient(Material material) { return addIngredient(1, material); }
    /** Adds one live material choice for each requested ingredient. */
    public ShapelessRecipe addIngredient(int count, Material material) {
        if (choices.size() + count > 9) {
            throw new IllegalArgumentException("Shapeless recipes cannot have more than 9 ingredients");
        }
        if (count > 0 && (material == null || material.isAir())) {
            throw new IllegalArgumentException("Cannot have empty/air material choice");
        }
        for (int remaining = count; remaining > 0; remaining--) {
            choices.add(new RecipeChoice.MaterialChoice(material));
        }
        return this;
    }
    /** A stack's amount is the number of slots it occupies in a shapeless recipe. */
    public ShapelessRecipe addIngredient(ItemStack stack) {
        return stack == null ? this : addIngredient(stack.getAmount(), stack);
    }
    /** Adds one exact choice for each requested ingredient. */
    public ShapelessRecipe addIngredient(int count, ItemStack stack) {
        for (int remaining = count; remaining > 0; remaining--)
            addIngredient(new RecipeChoice.ExactChoice(stack));
        return this;
    }
    public List<RecipeChoice> getChoiceList() { return Collections.unmodifiableList(choices); }
    /** Bukkit compatibility name for the ingredient choices. */
    public List<RecipeChoice> getIngredientList() { return getChoiceList(); }
}
