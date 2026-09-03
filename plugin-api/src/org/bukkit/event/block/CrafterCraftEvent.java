package org.bukkit.event.block;

import java.util.ArrayList;
import java.util.List;
import org.bukkit.block.Block;
import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;
import org.bukkit.inventory.CraftingRecipe;
import org.bukkit.inventory.ItemStack;

public class CrafterCraftEvent extends BlockEvent implements Cancellable {
    private static final HandlerList HANDLERS = new HandlerList();
    private final CraftingRecipe recipe;
    private ItemStack result;
    private final List<ItemStack> remainingItems;
    private boolean cancelled;

    public CrafterCraftEvent(Block block, CraftingRecipe recipe, ItemStack result, List<ItemStack> remainingItems) {
        super(block);
        this.recipe = recipe;
        this.result = result == null ? null : result.clone();
        this.remainingItems = remainingItems == null ? new ArrayList<>() : remainingItems;
    }
    public ItemStack getResult() { return result == null ? null : result.clone(); }
    public void setResult(ItemStack result) { this.result = result == null ? null : result.clone(); }
    public List<ItemStack> getRemainingItems() { return remainingItems; }
    public CraftingRecipe getRecipe() { return recipe; }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean cancelled) { this.cancelled = cancelled; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
