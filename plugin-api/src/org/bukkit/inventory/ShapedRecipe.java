package org.bukkit.inventory;

import java.util.LinkedHashMap;
import java.util.Map;
import org.bukkit.Material;
import org.bukkit.NamespacedKey;

/** A shaped crafting recipe definition. */
public class ShapedRecipe extends CraftingRecipe {
    private String[] shape = new String[0];
    private final Map<Character, RecipeChoice> choices = new LinkedHashMap<>();
    public ShapedRecipe(NamespacedKey key, ItemStack result) { super(key, result); }
    public ShapedRecipe shape(String... rows) { shape = rows == null ? new String[0] : rows.clone(); return this; }
    public String[] getShape() { return shape.clone(); }
    public ShapedRecipe setIngredient(char key, RecipeChoice choice) { if (choice != null) choices.put(key, choice); return this; }
    public ShapedRecipe setIngredient(char key, Material material) { return setIngredient(key, new RecipeChoice.MaterialChoice(material)); }
    public ShapedRecipe setIngredient(char key, ItemStack item) {
        return setIngredient(key, new RecipeChoice.ExactChoice(item));
    }
    @Deprecated
    public ShapedRecipe setIngredient(char key, org.bukkit.material.MaterialData data) {
        return setIngredient(key, data == null ? null : data.getItemType());
    }
    public Map<Character, RecipeChoice> getChoiceMap() { return java.util.Collections.unmodifiableMap(choices); }
    /** Bukkit compatibility view exposing the representative item per key. */
    public Map<Character, ItemStack> getIngredientMap() {
        Map<Character, ItemStack> result = new LinkedHashMap<>();
        for (Map.Entry<Character, RecipeChoice> entry : choices.entrySet()) {
            result.put(entry.getKey(), entry.getValue().getItemStack());
        }
        return java.util.Collections.unmodifiableMap(result);
    }
}
