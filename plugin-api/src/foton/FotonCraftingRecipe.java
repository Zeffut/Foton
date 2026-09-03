package foton;

import org.bukkit.NamespacedKey;
import org.bukkit.inventory.CraftingRecipe;
import org.bukkit.inventory.ItemStack;

/** Vanilla crafting recipe exposed to a CrafterCraftEvent. */
public final class FotonCraftingRecipe extends CraftingRecipe {
    public FotonCraftingRecipe(NamespacedKey key, ItemStack result) {
        super(key, result);
    }
}
