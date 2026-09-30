package org.bukkit.event.inventory;

import java.util.List;
import org.bukkit.block.Block;
import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;
import org.bukkit.event.block.BlockEvent;
import org.bukkit.inventory.BrewerInventory;
import org.bukkit.inventory.ItemStack;
import org.jetbrains.annotations.NotNull;

/** Fired when a brewing stand has calculated a completed brew. */
public class BrewEvent extends BlockEvent implements Cancellable {
    private static final HandlerList HANDLERS = new HandlerList();
    private final BrewerInventory contents;
    private final List<ItemStack> results;
    private final int fuelLevel;
    private boolean cancelled;

    @org.jetbrains.annotations.ApiStatus.Internal
    public BrewEvent(@NotNull Block brewer, @NotNull BrewerInventory contents,
            @NotNull List<ItemStack> results, int fuelLevel) {
        super(brewer);
        this.contents = contents;
        this.results = results;
        this.fuelLevel = fuelLevel;
    }

    @NotNull public BrewerInventory getContents() { return contents; }
    @NotNull public List<ItemStack> getResults() { return results; }
    public int getFuelLevel() { return fuelLevel; }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean cancelled) { this.cancelled = cancelled; }
    @Override @NotNull public HandlerList getHandlers() { return HANDLERS; }
    @NotNull public static HandlerList getHandlerList() { return HANDLERS; }
}
