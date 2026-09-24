package org.bukkit.inventory;

import java.util.ArrayList;
import java.util.List;
import org.bukkit.entity.HumanEntity;

/** Something a player can trade with: a villager, a wandering trader, or a
 * merchant a plugin made with {@code Bukkit.createMerchant}. */
public interface Merchant {
    List<MerchantRecipe> getRecipes();
    void setRecipes(List<MerchantRecipe> recipes);

    default MerchantRecipe getRecipe(int i) { return getRecipes().get(i); }

    default void setRecipe(int i, MerchantRecipe recipe) {
        ArrayList<MerchantRecipe> recipes = new ArrayList<>(getRecipes());
        recipes.set(i, recipe);
        setRecipes(recipes);
    }

    default int getRecipeCount() { return getRecipes().size(); }

    default boolean isTrading() { return getTrader() != null; }

    /** The player trading with this merchant now, or null. */
    default HumanEntity getTrader() {
        if (!(this instanceof org.bukkit.entity.Entity entity)) return null;
        String trader = foton.Native.merchantTrader(entity.getUniqueId().toString());
        return trader == null ? null : org.bukkit.Bukkit.getPlayer(java.util.UUID.fromString(trader));
    }
}
