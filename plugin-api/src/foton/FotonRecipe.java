package foton;

import org.bukkit.NamespacedKey;
import org.bukkit.inventory.ItemStack;
import org.bukkit.inventory.Recipe;

/** A vanilla recipe result exposed through Bukkit. */
public final class FotonRecipe implements Recipe, org.bukkit.Keyed {
    private final NamespacedKey key;
    private final ItemStack result;
    public FotonRecipe(NamespacedKey key, ItemStack result) {
        this.key = key; this.result = result.clone();
    }
    @Override public NamespacedKey getKey() { return key; }
    @Override public ItemStack getResult() { return result.clone(); }
}
