package org.bukkit.inventory;

import org.bukkit.block.Furnace;

/** The three slots of a furnace, smoker or blast furnace. */
public interface FurnaceInventory extends Inventory {
    ItemStack getResult();
    ItemStack getFuel();
    ItemStack getSmelting();
    void setFuel(ItemStack stack);
    void setResult(ItemStack stack);
    void setSmelting(ItemStack stack);
    /** Whether the stack can go in the fuel slot. */
    boolean isFuel(ItemStack item);
    /** Whether some recipe of this furnace cooks the stack. */
    boolean isSmeltable(ItemStack item);
    @Override Furnace getHolder();
}
