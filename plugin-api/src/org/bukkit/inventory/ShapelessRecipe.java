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
    public ShapelessRecipe addIngredient(Material material) { return addIngredient(new RecipeChoice.MaterialChoice(material)); }
    public ShapelessRecipe addIngredient(ItemStack stack) { return addIngredient(new RecipeChoice.ExactChoice(stack)); }
    public List<RecipeChoice> getChoiceList() { return Collections.unmodifiableList(choices); }
    /** Bukkit compatibility name for the ingredient choices. */
    public List<RecipeChoice> getIngredientList() { return getChoiceList(); }
}
