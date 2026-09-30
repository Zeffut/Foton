package org.bukkit.event.inventory;

import java.util.List;
import org.bukkit.block.Block;
import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;
import org.bukkit.event.block.BlockEvent;
import org.bukkit.inventory.BrewerInventory;
import org.bukkit.inventory.ItemStack;

/** A brewing stand finished a brew. The three results may be changed;
 * cancelling leaves the ingredient and the bottles as they were. */
public class BrewEvent extends BlockEvent implements Cancellable {
    private static final HandlerList HANDLERS = new HandlerList();
    private final BrewerInventory contents;
    private final List<ItemStack> results;
    private final int fuelLevel;
    private boolean cancelled;

    public BrewEvent(Block brewer, BrewerInventory contents, List<ItemStack> results, int fuelLevel) {
        super(brewer);
        this.contents = contents;
        this.results = results;
        this.fuelLevel = fuelLevel;
    }

    public BrewerInventory getContents() { return contents; }
    public List<ItemStack> getResults() { return results; }
    public int getFuelLevel() { return fuelLevel; }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean cancel) { cancelled = cancel; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
