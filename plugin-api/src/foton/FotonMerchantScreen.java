package foton;

import java.util.Collections;
import java.util.List;
import org.bukkit.inventory.Merchant;
import org.bukkit.inventory.MerchantRecipe;

/** A merchant with no body, as its trading screen showed it when an event
 * was raised: the trades it offered then. */
final class FotonMerchantScreen implements Merchant {
    private final List<MerchantRecipe> recipes;

    FotonMerchantScreen(List<MerchantRecipe> recipes) { this.recipes = List.copyOf(recipes); }

    @Override public List<MerchantRecipe> getRecipes() { return Collections.unmodifiableList(recipes); }
}
