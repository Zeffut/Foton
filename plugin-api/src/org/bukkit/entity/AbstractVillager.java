package org.bukkit.entity;

import java.util.List;
import org.bukkit.inventory.MerchantRecipe;

/** A living entity that exposes merchant offers. */
public interface AbstractVillager extends LivingEntity, org.bukkit.inventory.Merchant {
    @Override List<MerchantRecipe> getRecipes();
}
