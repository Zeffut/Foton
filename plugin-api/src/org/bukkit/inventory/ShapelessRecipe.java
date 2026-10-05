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
        if (choices.size() + 1 > 9) {
            throw new IllegalArgumentException("Shapeless recipes cannot have more than 9 ingredients");
        }
        choices.add(choice.validate(false).clone());
        return this;
    }
    public ShapelessRecipe addIngredient(Material material) { return addIngredient(1, material); }
    /** Adds one live material choice for each requested ingredient. */
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
    /** A stack's amount is the number of slots it occupies in a shapeless recipe. */
    public ShapelessRecipe addIngredient(ItemStack stack) {
        return addIngredient(stack.getAmount(), stack);
    }
    /** Adds one exact choice for each requested ingredient. */
    public ShapelessRecipe addIngredient(int count, ItemStack stack) {
        if (choices.size() + count > 9) {
            throw new IllegalArgumentException("Shapeless recipes cannot have more than 9 ingredients");
        }
        if (stack.getType().isAir()) {
            throw new IllegalArgumentException("Cannot have empty/air item stack choice");
        }
        stack = stack.clone();
        for (int remaining = count; remaining > 0; remaining--)
            choices.add(new RecipeChoice.ExactChoice(stack));
        return this;
    }
    public List<RecipeChoice> getChoiceList() {
        List<RecipeChoice> copied = new ArrayList<>(choices.size());
        for (RecipeChoice choice : choices) copied.add(choice.clone());
        return copied;
    }
    /** Bukkit compatibility list of representative ingredient stacks. */
    public List<ItemStack> getIngredientList() {
        List<ItemStack> copied = new ArrayList<>(choices.size());
        for (RecipeChoice choice : choices) copied.add(choice.getItemStack().clone());
        return copied;
    }
}
