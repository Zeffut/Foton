package org.bukkit.block;

import org.bukkit.inventory.BrewerInventory;
import org.jetbrains.annotations.NotNull;
import org.jetbrains.annotations.Range;

/** Snapshot of a brewing stand block entity. */
public interface BrewingStand extends Container {
    int getBrewingTime();
    void setBrewingTime(int brewingTime);
    void setRecipeBrewTime(@Range(from = 1, to = Integer.MAX_VALUE) int brewTime);
    int getRecipeBrewTime();
    int getFuelLevel();
    void setFuelLevel(int level);
    @Override @NotNull BrewerInventory getInventory();
    @Override @NotNull BrewerInventory getSnapshotInventory();
}
