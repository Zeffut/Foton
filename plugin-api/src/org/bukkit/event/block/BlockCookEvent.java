package org.bukkit.event.block;

import org.bukkit.block.Block;
import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;
import org.bukkit.inventory.CookingRecipe;
import org.bukkit.inventory.ItemStack;

/** A block finished cooking an item. The result may be changed; cancelling
 * consumes nothing. */
public class BlockCookEvent extends BlockEvent implements Cancellable {
    private static final HandlerList HANDLERS = new HandlerList();
    private final ItemStack source;
    private ItemStack result;
    private final CookingRecipe<?> recipe;
    private boolean cancelled;

    @Deprecated
    public BlockCookEvent(Block block, ItemStack source, ItemStack result) { this(block, source, result, null); }

    public BlockCookEvent(Block block, ItemStack source, ItemStack result, CookingRecipe<?> recipe) {
        super(block);
        this.source = source;
        this.result = result;
        this.recipe = recipe;
    }

    public ItemStack getSource() { return source; }
    public ItemStack getResult() { return result; }
    public void setResult(ItemStack result) { this.result = result; }
    public CookingRecipe<?> getRecipe() { return recipe; }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean cancel) { cancelled = cancel; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
