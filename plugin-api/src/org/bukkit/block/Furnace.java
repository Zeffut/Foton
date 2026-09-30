package org.bukkit.block;

import org.bukkit.inventory.FurnaceInventory;

/** A furnace, smoker or blast furnace as a block state. The times are read
 * when the snapshot is taken and written back by {@code update}. */
public interface Furnace extends TileState, org.bukkit.inventory.BlockInventoryHolder {
    short getBurnTime();
    void setBurnTime(short burnTime);
    short getCookTime();
    void setCookTime(short cookTime);
    int getCookTimeTotal();
    void setCookTimeTotal(int cookTimeTotal);
    @Override FurnaceInventory getInventory();
    FurnaceInventory getSnapshotInventory();
}
