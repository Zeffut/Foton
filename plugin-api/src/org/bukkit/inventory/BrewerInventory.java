package org.bukkit.inventory;

import org.bukkit.block.BrewingStand;
import org.jetbrains.annotations.Nullable;

/** Five-slot inventory owned by a brewing stand. */
public interface BrewerInventory extends Inventory {
    @Nullable ItemStack getIngredient();
    void setIngredient(@Nullable ItemStack ingredient);
    @Nullable ItemStack getFuel();
    void setFuel(@Nullable ItemStack fuel);
    @Override @Nullable BrewingStand getHolder();
}
