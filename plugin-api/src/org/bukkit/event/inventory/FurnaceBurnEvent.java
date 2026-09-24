package org.bukkit.event.inventory;

import org.bukkit.block.Block;
import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;
import org.bukkit.event.block.BlockEvent;
import org.bukkit.inventory.ItemStack;

/** A furnace is about to take a fuel item and light. */
public class FurnaceBurnEvent extends BlockEvent implements Cancellable {
    private static final HandlerList HANDLERS = new HandlerList();
    private final ItemStack fuel;
    private int burnTime;
    private boolean burning = true;
    private boolean consumeFuel = true;
    private boolean cancelled;

    public FurnaceBurnEvent(Block furnace, ItemStack fuel, int burnTime) {
        super(furnace);
        this.fuel = fuel;
        this.burnTime = burnTime;
    }

    public ItemStack getFuel() { return fuel; }
    public int getBurnTime() { return burnTime; }
    public void setBurnTime(int burnTime) { this.burnTime = Math.clamp(burnTime, Short.MIN_VALUE, Short.MAX_VALUE); }
    public boolean isBurning() { return burning; }
    public void setBurning(boolean burning) { this.burning = burning; }
    public boolean willConsumeFuel() { return consumeFuel; }
    public void setConsumeFuel(boolean consumeFuel) { this.consumeFuel = consumeFuel; }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean cancel) { cancelled = cancel; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
