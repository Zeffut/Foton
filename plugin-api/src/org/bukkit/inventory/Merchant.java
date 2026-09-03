package org.bukkit.inventory;

import java.util.List;

/** Source of villager trading offers. */
public interface Merchant {
    List<MerchantRecipe> getRecipes();
    default void setRecipes(List<MerchantRecipe> recipes) { }
}
