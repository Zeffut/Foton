package org.bukkit.event.inventory;

import org.bukkit.block.Block;
import org.bukkit.event.block.BlockCookEvent;
import org.bukkit.inventory.CookingRecipe;
import org.bukkit.inventory.ItemStack;

/** A furnace, smoker or blast furnace finished cooking an item. */
public class FurnaceSmeltEvent extends BlockCookEvent {
    @Deprecated
    public FurnaceSmeltEvent(Block furnace, ItemStack source, ItemStack result) { super(furnace, source, result); }

    public FurnaceSmeltEvent(Block furnace, ItemStack source, ItemStack result, CookingRecipe<?> recipe) {
        super(furnace, source, result, recipe);
    }
}
