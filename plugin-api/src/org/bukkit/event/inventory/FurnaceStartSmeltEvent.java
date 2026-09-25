package org.bukkit.event.inventory;

import org.bukkit.block.Block;
import org.bukkit.event.block.InventoryBlockStartEvent;
import org.bukkit.inventory.CookingRecipe;
import org.bukkit.inventory.ItemStack;

/** A furnace starts cooking an item. The time it takes may be changed. */
public class FurnaceStartSmeltEvent extends InventoryBlockStartEvent {
    private final CookingRecipe<?> recipe;
    private int totalCookTime;

    @Deprecated
    public FurnaceStartSmeltEvent(Block furnace, ItemStack source, CookingRecipe<?> recipe) {
        this(furnace, source, recipe, recipe.getCookingTime());
    }

    public FurnaceStartSmeltEvent(Block furnace, ItemStack source, CookingRecipe<?> recipe, int cookingTime) {
        super(furnace, source);
        this.recipe = recipe;
        this.totalCookTime = cookingTime;
    }

    public CookingRecipe<?> getRecipe() { return recipe; }
    public int getTotalCookTime() { return totalCookTime; }
    public void setTotalCookTime(int cookTime) { this.totalCookTime = cookTime; }
}
