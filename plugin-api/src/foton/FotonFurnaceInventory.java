package foton;

import org.bukkit.block.Furnace;
import org.bukkit.inventory.FurnaceInventory;
import org.bukkit.inventory.ItemStack;

/** The live slots of a furnace block entity: input, fuel, result. */
final class FotonFurnaceInventory extends FotonHopperInventory implements FurnaceInventory {
    private final FotonFurnace furnace;

    FotonFurnaceInventory(FotonFurnace furnace) {
        super(furnace, 3);
        this.furnace = furnace;
    }

    @Override public org.bukkit.event.inventory.InventoryType getType() { return org.bukkit.event.inventory.InventoryType.FURNACE; }
    @Override public ItemStack getSmelting() { return getItem(0); }
    @Override public void setSmelting(ItemStack stack) { setItem(0, stack); }
    @Override public ItemStack getFuel() { return getItem(1); }
    @Override public void setFuel(ItemStack stack) { setItem(1, stack); }
    @Override public ItemStack getResult() { return getItem(2); }
    @Override public void setResult(ItemStack stack) { setItem(2, stack); }
    @Override public boolean isFuel(ItemStack item) { return FotonFurnaceMenuInventory.fuel(item); }
    @Override public boolean isSmeltable(ItemStack item) { return FotonFurnaceMenuInventory.smeltable(furnace.getType(), item); }
    @Override public Furnace getHolder() { return furnace; }
}
